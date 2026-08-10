param(
    [int]$NodeCount = 3,
    [int]$Requests = 100000,
    [int]$Keys = 50000,
    [int]$ValueSize = 128,
    [int]$Seed = 42,
    [string]$Out = "$PSScriptRoot\results\recovery.csv"
)
$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$server = Join-Path $root "target\debug\cachex-server.exe"
$bench = Join-Path $root "target\debug\cachex-bench.exe"
if (-not (Test-Path $server)) { throw "build the server first: cargo build -p cachex-server" }
if (-not (Test-Path $bench)) { throw "build the bench first: cargo build -p cachex-bench" }

$portBase = 7400 + 100 * ($NodeCount - 2)
$metricsBase = 9400 + 100 * ($NodeCount - 2)
$nodes = 1..$NodeCount | ForEach-Object { "127.0.0.1:$($portBase + $_)" }
$members = ($nodes | ForEach-Object { "`"$_`"" }) -join ", "

function New-NodeConfig {
    param([int]$Index)
    $path = Join-Path $env:TEMP "cachex-exp-recovery-n$Index.toml"
    $metricsPort = $metricsBase + $Index
    $port = $portBase + $Index
    @"
[node]
id = $Index
host = "127.0.0.1"
port = $port

[cluster]
nodes = [
    $members
]

[cache]
max_memory_bytes = 104857600
eviction_policy = "lru"
ttl_purge_interval_secs = 1

[aof]
enabled = true
path = "cachex-exp-recovery-n$Index.aof"
fsync = "interval"
fsync_interval_secs = 1
rewrite_threshold_bytes = 67108864

[hashing]
vnodes = 100

[replication]
enabled = false
factor = 1

[heartbeat]
interval_secs = 1
timeout_ms = 500
miss_threshold = 2

[metrics]
enabled = true
host = "127.0.0.1"
port = $metricsPort
"@ | Set-Content -Path $path -Encoding ascii
    $path
}

function Start-Node {
    param([int]$Index)
    Start-Process -FilePath $server -ArgumentList (New-NodeConfig -Index $Index) `
        -WorkingDirectory $env:TEMP `
        -RedirectStandardOutput (Join-Path $env:TEMP "cachex-exp-recovery-out$Index.txt") `
        -RedirectStandardError (Join-Path $env:TEMP "cachex-exp-recovery-err$Index.txt") -PassThru
}

function Get-Metrics {
    param([int]$Index)
    $metricsPort = $metricsBase + $Index
    curl.exe -s "http://127.0.0.1:$metricsPort/metrics" | ConvertFrom-Json
}

function Wait-For-Metrics {
    param([int]$Index, [int]$TimeoutMs = 15000)
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    while ($sw.ElapsedMilliseconds -lt $TimeoutMs) {
        try {
            $null = Get-Metrics -Index $Index
            return $sw.ElapsedMilliseconds
        }
        catch { }
        Start-Sleep -Milliseconds 100
    }
    return $sw.ElapsedMilliseconds
}

function Run-Bench {
    param($NodeList, [int]$Count, [double]$Ratio, [int]$ClientSeed, $OutFile)
    & $bench --nodes $NodeList --router consistent --clients 8 `
        --requests $Count --keys $Keys --value-size $ValueSize --get-ratio $Ratio `
        --key-order uniform --seed $ClientSeed --output $OutFile | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "bench failed" }
    $row = Get-Content $OutFile | Select-Object -Last 1
    $f = $row.Split(",")
    [int64]$f[14], [int64]$f[15], [int64]$f[16]   
}

function Get-BenchStats {
    param($NodeList, [int]$Count, [double]$Ratio, [int]$ClientSeed)
    $r = Run-Bench -NodeList $NodeList -Count $Count -Ratio $Ratio -ClientSeed $ClientSeed `
        -OutFile (Join-Path $env:TEMP "cachex-exp-recovery.csv")
    $r
}

Remove-Item (Join-Path $env:TEMP "cachex-exp-recovery-n*.aof") -ErrorAction SilentlyContinue

$procs = @()
for ($i = 1; $i -le $NodeCount; $i++) { $procs += Start-Node -Index $i }
Start-Sleep -Milliseconds 2000

Set-Content -Path $Out -Value "phase,metric,value"
"cluster,nodes,$NodeCount" | Add-Content $Out
$allNodes = $nodes -join ","

try {
    
    $null = Run-Bench -NodeList $allNodes -Count $Requests -Ratio 0.5 -ClientSeed $Seed `
        -OutFile (Join-Path $env:TEMP "cachex-exp-recovery-baseline.csv")
    
    
    Start-Sleep -Seconds 2
    $scan = Run-Bench -NodeList $allNodes -Count $Keys -Ratio 1.0 -ClientSeed 999 `
        -OutFile (Join-Path $env:TEMP "cachex-exp-recovery-scan.csv")
    "baseline,scan_hits,$($scan[0])" | Add-Content $Out
    "baseline,scan_misses,$($scan[1])" | Add-Content $Out
    "baseline,scan_errors,$($scan[2])" | Add-Content $Out

    
    $killTime = [System.Diagnostics.Stopwatch]::StartNew()
    Stop-Process -Id $procs[1].Id -Force -ErrorAction SilentlyContinue
    $detectedMs = -1
    while ($killTime.ElapsedMilliseconds -lt 15000) {
        try {
            $m = Get-Metrics -Index 1
            if ($m.peers.failed -ge 1) {
                $detectedMs = $killTime.ElapsedMilliseconds
                break
            }
        }
        catch { }
        Start-Sleep -Milliseconds 100
    }
    "detection,failed_detection_ms,$detectedMs" | Add-Content $Out

    
    
    
    $down = Get-BenchStats -NodeList $allNodes -Count 900 -Ratio 0.5 -ClientSeed ($Seed + 1)
    $downErrors = $down[2]
    $downShare = [math]::Round(100.0 * $downErrors / 900, 2)
    "failure,failed_requests,$downErrors" | Add-Content $Out
    "failure,failed_request_share,$downShare" | Add-Content $Out

    
    $restartTime = [System.Diagnostics.Stopwatch]::StartNew()
    $procs[1] = Start-Node -Index 2
    $upMs = Wait-For-Metrics -Index 2
    "recovery,restart_to_metrics_ms,$upMs" | Add-Content $Out

    $replayMs = -1
    $outFile2 = Join-Path $env:TEMP "cachex-exp-recovery-out2.txt"
    while ($restartTime.ElapsedMilliseconds -lt 15000) {
        $line = Get-Content $outFile2 -ErrorAction SilentlyContinue | Select-String "AOF replay"
        if ($line) {
            if ($line -match "in (\d+)ms") { $replayMs = [int]$Matches[1] }
            break
        }
        Start-Sleep -Milliseconds 100
    }
    "recovery,aof_replay_ms,$replayMs" | Add-Content $Out

    $m2 = Get-Metrics -Index 2
    "recovery,node2_recovery_ms_metric,$($m2.recovery_ms)" | Add-Content $Out

    $healthyMs = -1
    while ($restartTime.ElapsedMilliseconds -lt 15000) {
        try {
            $m = Get-Metrics -Index 1
            if ($m.peers.failed -eq 0) {
                $healthyMs = $restartTime.ElapsedMilliseconds
                break
            }
        }
        catch { }
        Start-Sleep -Milliseconds 100
    }
    "recovery,time_to_healthy_ms,$healthyMs" | Add-Content $Out

    
    $scan2 = Run-Bench -NodeList $allNodes -Count $Keys -Ratio 1.0 -ClientSeed 999 `
        -OutFile (Join-Path $env:TEMP "cachex-exp-recovery-scan2.csv")
    "verification,scan_hits,$($scan2[0])" | Add-Content $Out
    "verification,scan_misses,$($scan2[1])" | Add-Content $Out
    "verification,scan_errors,$($scan2[2])" | Add-Content $Out

    Write-Output ("detection_ms   = {0}" -f $detectedMs)
    Write-Output ("down errors    = {0}" -f $downErrors)
    Write-Output ("restart ms     = {0}  replay ms = {1}  healthy ms = {2}" -f $upMs, $replayMs, $healthyMs)
    Write-Output ("post-recovery  hits={0} misses={1} errors={2}" -f $scan2[0], $scan2[1], $scan2[2])
}
finally {
    foreach ($proc in $procs) {
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    }
}
Write-Output "recovery results written to $Out"

