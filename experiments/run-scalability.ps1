param(
    [int]$Requests = 200000,
    [int]$Keys = 50000,
    [double]$GetRatio = 0.8,
    [int]$ValueSize = 128,
    [int]$Seed = 42,
    [string]$Out = "$PSScriptRoot\results\scalability.csv"
)
$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$server = Join-Path $root "target\debug\cachex-server.exe"
$bench = Join-Path $root "target\debug\cachex-bench.exe"
if (-not (Test-Path $server)) { throw "build the server first: cargo build -p cachex-server" }
if (-not (Test-Path $bench)) { throw "build the bench first: cargo build -p cachex-bench" }

function New-NodeConfig {
    param([int]$Index, [string[]]$Members)
    $path = Join-Path $env:TEMP "cachex-exp-scale-n$Index.toml"
    $members = ($Members | ForEach-Object { "`"$_`"" }) -join ", "
    $metricsPort = 9400 + $Index
$port = 7400 + $Index
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
enabled = false
path = "cachex-exp-scale-n$Index.aof"
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

function Start-Cluster {
    param([int]$Count)
    $procs = @()
    $members = 1..$Count | ForEach-Object { "127.0.0.1:$(7400 + $_)" }
    for ($i = 1; $i -le $Count; $i++) {
        $config = New-NodeConfig -Index $i -Members $members
        $procs += Start-Process -FilePath $server -ArgumentList $config -WorkingDirectory $env:TEMP `
            -RedirectStandardOutput (Join-Path $env:TEMP "cachex-exp-scale-out$i.txt") `
            -RedirectStandardError (Join-Path $env:TEMP "cachex-exp-scale-err$i.txt") -PassThru
    }
    Start-Sleep -Milliseconds 1800
    $procs
}

function Stop-Cluster {
    param($Procs)
    foreach ($proc in $Procs) {
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    }
    Start-Sleep -Milliseconds 400
}

function Run-Bench {
    param([int]$NodeCount, [int]$ClientCount)
    $nodes = (1..$NodeCount | ForEach-Object { "127.0.0.1:$(7400 + $_)" }) -join ","
    & $bench --nodes $nodes --router consistent --clients $ClientCount `
        --requests $Requests --keys $Keys --value-size $ValueSize --get-ratio $GetRatio `
        --key-order uniform --seed $Seed --output (Join-Path $env:TEMP "cachex-exp-scale.csv") | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "bench failed (nodes=$NodeCount clients=$ClientCount)" }

    $row = Get-Content (Join-Path $env:TEMP "cachex-exp-scale.csv") | Select-Object -Last 1
    $f = $row.Split(",")
    [double]$f[11], [int64]$f[17], [int64]$f[18], [int64]$f[19], [int64]$f[20], [int64]$f[21], `
        [int64]$f[14], [int64]$f[15], [int64]$f[16]
}

function Write-Row {
    param($Dimension, $Scale, $NodeCount, $ClientCount, $Result)
    "{0},{1},{2},{3},{4},{5:0.00},{6},{7},{8},{9},{10},{11},{12},{13}" -f `
        $Dimension, $Scale, $NodeCount, $ClientCount, $Requests, `
        $Result[0], $Result[1], $Result[2], $Result[3], $Result[4], $Result[5], `
        $Result[6], $Result[7], $Result[8] | Add-Content -Path $Out
}

Set-Content -Path $Out -Value "dimension,scale,nodes,clients,requests,ops_per_sec,avg_us,p50_us,p95_us,p99_us,max_us,hits,misses,errors"

foreach ($nodeCount in 1, 2, 3, 4, 5) {
    $procs = Start-Cluster -Count $nodeCount
    try {
        $r = Run-Bench -NodeCount $nodeCount -ClientCount 8
        Write-Row "nodes" $nodeCount $nodeCount 8 $r
        Write-Output ("nodes={0}: {1,7:N0} ops/s  avg {2}us  p99 {3}us" -f $nodeCount, $r[0], $r[1], $r[4])
    }
    finally {
        Stop-Cluster -Procs $procs
    }
}

$procs = Start-Cluster -Count 3
try {
    foreach ($clients in 10, 50, 100, 250, 500, 1000) {
        $r = Run-Bench -NodeCount 3 -ClientCount $clients
        Write-Row "clients" $clients 3 $clients $r
        Write-Output ("clients={0,4}: {1,7:N0} ops/s  avg {2}us  p99 {3}us" -f $clients, $r[0], $r[1], $r[4])
    }
}
finally {
    Stop-Cluster -Procs $procs
}
Write-Output "scalability results written to $Out"

