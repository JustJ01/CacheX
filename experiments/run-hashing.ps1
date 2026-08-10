

param(
    [int]$Keys = 100000,
    [int]$Seed = 42,
    [string]$Out = "$PSScriptRoot\results\hashing.csv"
)
$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$server = Join-Path $root "target\debug\cachex-server.exe"
$bench = Join-Path $root "target\debug\cachex-bench.exe"
if (-not (Test-Path $server)) { throw "build the server first: cargo build -p cachex-server" }
if (-not (Test-Path $bench)) { throw "build the bench first: cargo build -p cachex-bench" }

$ports = 7201, 7202, 7203, 7204
$nodes = $ports | ForEach-Object { "127.0.0.1:$_" }

function New-NodeConfig {
    param([int]$Index, [string[]]$Members)
    $path = Join-Path $env:TEMP "cachex-exp-hash-n$Index.toml"
    $members = ($Members | ForEach-Object { "`"$_`"" }) -join ", "
    $public = $ports[$Index - 1]
    $metricsPort = 9200 + $Index
    @"
[node]
id = $Index
host = "127.0.0.1"
port = $public

[cluster]
nodes = [
    $members
]

[cache]
max_memory_bytes = 104857600
eviction_policy = "lru"
ttl_purge_interval_secs = 1

[aof]
enabled = false
path = "cachex-exp-hash-n$Index.aof"
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

function Start-Nodes {
    param([int]$Count, [string[]]$Members)
    $procs = @()
    for ($i = 1; $i -le $Count; $i++) {
        $config = New-NodeConfig -Index $i -Members $Members
        $procs += Start-Process -FilePath $server -ArgumentList $config -WorkingDirectory $env:TEMP `
            -RedirectStandardOutput (Join-Path $env:TEMP "cachex-exp-out$i.txt") `
            -RedirectStandardError (Join-Path $env:TEMP "cachex-exp-err$i.txt") -PassThru
    }
    Start-Sleep -Milliseconds 1800
    $procs
}

function Stop-Nodes {
    param($Procs)
    foreach ($proc in $Procs) {
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    }
    Start-Sleep -Milliseconds 400
}

function Get-Metrics {
    param([int]$Index)
    $metricsPort = 9200 + $Index
    curl.exe -s "http://127.0.0.1:$metricsPort/metrics" | ConvertFrom-Json
}

function Load-Distribution {
    param($Count, [string]$Counter)
    $counts = @()
    for ($i = 1; $i -le $Count; $i++) {
        $m = Get-Metrics -Index $i
        $counts += $m.requests.$Counter
    }
    $total = ($counts | Measure-Object -Sum).Sum
    $pcts = $counts | ForEach-Object { [math]::Round(100.0 * $_ / $total, 2) }
    $max = ($pcts | Measure-Object -Maximum).Maximum
    $min = ($pcts | Measure-Object -Minimum).Minimum
    @{
        Total = $total
        Pcts  = $pcts
        Max   = $max
        Min   = $min
        Ratio = if ($min -gt 0) { [math]::Round($max / $min, 2) } else { 0 }
    }
}

Set-Content -Path $Out -Value "router,initial_nodes,final_nodes,keys,keys_moved,movement_pct,load3_pcts,load3_max_min_ratio,load4_pcts,load4_max_min_ratio,ops_per_sec,avg_us,p50_us,p95_us,p99_us,max_us"

$routers = "modulo", "consistent"
foreach ($router in $routers) {
    $procs = Start-Nodes -Count 3 -Members $nodes[0..2]
    try {
        
        & $bench --nodes ($nodes[0..2] -join ",") --router $router --clients 1 --requests $Keys --keys $Keys `
            --value-size 32 --get-ratio 0.0 --key-order sequential --seed $Seed `
            --output (Join-Path $env:TEMP "cachex-exp-hash-load.csv") | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "load phase failed" }

        $load3 = Load-Distribution -Count 3 -Counter "sets"
        $load3Pcts = $load3.Pcts -join "/"

        
        $procs += Start-Process -FilePath $server -ArgumentList (New-NodeConfig -Index 4 -Members $nodes) `
            -WorkingDirectory $env:TEMP `
            -RedirectStandardOutput (Join-Path $env:TEMP "cachex-exp-out4.txt") `
            -RedirectStandardError (Join-Path $env:TEMP "cachex-exp-err4.txt") -PassThru
        Start-Sleep -Milliseconds 1000

        
        & $bench --nodes ($nodes -join ",") --router $router --clients 1 --requests $Keys --keys $Keys `
            --value-size 32 --get-ratio 1.0 --key-order sequential --seed $Seed `
            --output (Join-Path $env:TEMP "cachex-exp-hash-scan.csv") | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "scan phase failed" }

        $row = Get-Content (Join-Path $env:TEMP "cachex-exp-hash-scan.csv") | Select-Object -Last 1
        $f = $row.Split(",")
        $ops = [double]$f[11]; $avg = [int64]$f[17]; $p50 = [int64]$f[18]
        $p95 = [int64]$f[19]; $p99 = [int64]$f[20]; $max = [int64]$f[21]
        $hits = [int64]$f[14]; $misses = [int64]$f[15]
        $movedPct = [math]::Round(100.0 * $misses / ($hits + $misses), 2)

        $load4 = Load-Distribution -Count 4 -Counter "gets"
        $load4Pcts = $load4.Pcts -join "/"

        "{0},{1},{2},{3},{4},{5:0.00},{6},{7},{8},{9},{10:0.00},{11},{12},{13},{14},{15}" -f `
            $router, 3, 4, $Keys, $misses, $movedPct, `
            $load3Pcts, $load3.Ratio, $load4Pcts, $load4.Ratio, `
            $ops, $avg, $p50, $p95, $p99, $max | Add-Content -Path $Out

        Write-Output ("{0,-10} moved {1,6} keys ({2,5:P1})  3-node load {3}  4-node load {4}  p99 {5}us" -f `
            $router, $misses, ($misses / $Keys), ($load3Pcts -replace "/", " "), ($load4Pcts -replace "/", " "), $p99)
    }
    finally {
        Stop-Nodes -Procs $procs
    }
}
Write-Output "hashing results written to $Out"

