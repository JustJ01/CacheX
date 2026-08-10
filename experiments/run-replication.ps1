param(
    [int]$Requests = 200000,
    [int]$Keys = 50000,
    [int]$Clients = 8,
    [double]$GetRatio = 0.8,
    [int]$ValueSize = 128,
    [int]$Seed = 42,
    [string]$Out = "$PSScriptRoot\results\replication.csv"
)
$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$server = Join-Path $root "target\debug\cachex-server.exe"
$bench = Join-Path $root "target\debug\cachex-bench.exe"
if (-not (Test-Path $server)) { throw "build the server first: cargo build -p cachex-server" }
if (-not (Test-Path $bench)) { throw "build the bench first: cargo build -p cachex-bench" }

$ports = 7301, 7302, 7303
$nodes = $ports | ForEach-Object { "127.0.0.1:$_" }
$members = ($nodes | ForEach-Object { "`"$_`"" }) -join ", "

function New-NodeConfig {
    param([int]$Index, [int]$ReplicationFactor)
    $path = Join-Path $env:TEMP "cachex-exp-repl-n$Index-f$ReplicationFactor.toml"
    $replEnabled = if ($ReplicationFactor -gt 1) { "true" } else { "false" }
    $metricsPort = 9300 + $Index
$port = $ports[$Index - 1]
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
path = "cachex-exp-repl-n$Index.aof"
fsync = "interval"
fsync_interval_secs = 1
rewrite_threshold_bytes = 67108864

[hashing]
vnodes = 100

[replication]
enabled = $replEnabled
factor = $ReplicationFactor

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

function Get-Metrics {
    param([int]$Index)
    $metricsPort = 9300 + $Index
    curl.exe -s "http://127.0.0.1:$metricsPort/metrics" | ConvertFrom-Json
}

Set-Content -Path $Out -Value "rf,clients,requests,keys,value_size,get_ratio,ops_per_sec,avg_us,p50_us,p95_us,p99_us,max_us,hits,misses,errors,repl_sent,repl_failed,repl_received,sent_received_gap,total_keys,total_used_bytes"

foreach ($rf in 1, 2) {
    $procs = @()
    for ($i = 1; $i -le 3; $i++) {
        $config = New-NodeConfig -Index $i -ReplicationFactor $rf
        $procs += Start-Process -FilePath $server -ArgumentList $config -WorkingDirectory $env:TEMP `
            -RedirectStandardOutput (Join-Path $env:TEMP "cachex-exp-repl-out$i.txt") `
            -RedirectStandardError (Join-Path $env:TEMP "cachex-exp-repl-err$i.txt") -PassThru
    }
    Start-Sleep -Milliseconds 1800
    try {
        & $bench --nodes ($nodes -join ",") --router consistent --clients $Clients `
            --requests $Requests --keys $Keys --value-size $ValueSize --get-ratio $GetRatio `
            --key-order uniform --seed $Seed --output (Join-Path $env:TEMP "cachex-exp-repl.csv") | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "bench failed" }

        $row = Get-Content (Join-Path $env:TEMP "cachex-exp-repl.csv") | Select-Object -Last 1
        $f = $row.Split(",")
        $ops = [double]$f[11]; $avg = [int64]$f[17]; $p50 = [int64]$f[18]
        $p95 = [int64]$f[19]; $p99 = [int64]$f[20]; $max = [int64]$f[21]
        $hits = [int64]$f[14]; $misses = [int64]$f[15]; $errors = [int64]$f[16]

        $sent = 0; $failed = 0; $received = 0; $keysSum = 0; $used = 0
        for ($i = 1; $i -le 3; $i++) {
            $m = Get-Metrics -Index $i
            $sent += $m.replication.sent
            $failed += $m.replication.failed
            $received += $m.replication.received
            $keysSum += $m.storage.keys
            $used += $m.storage.used_bytes
        }
        $gap = $sent - $received

        "{0},{1},{2},{3},{4},{5},{6:0.00},{7},{8},{9},{10},{11},{12},{13},{14},{15},{16},{17},{18},{19},{20:0}" -f `
            $rf, $Clients, $Requests, $Keys, $ValueSize, $GetRatio, `
            $ops, $avg, $p50, $p95, $p99, $max, $hits, $misses, $errors, `
            $sent, $failed, $received, $gap, $keysSum, $used | Add-Content -Path $Out

        Write-Output ("RF={0}: {1,7:N0} ops/s  avg {2}us  p99 {3}us  repl sent {4} recv {5} gap {6}  keys {7}  used {8:N0}B" -f `
            $rf, $ops, $avg, $p99, $sent, $received, $gap, $keysSum, $used)
    }
    finally {
        foreach ($proc in $procs) {
            Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
        }
        Start-Sleep -Milliseconds 500
    }
}
Write-Output "replication results written to $Out"

