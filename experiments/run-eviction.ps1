

param(
    [int]$CacheMB = 1,
    [int]$ValueSize = 128,
    [string]$Out = "$PSScriptRoot\results\eviction.csv"
)
$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$server = Join-Path $root "target\debug\cachex-server.exe"
$bench = Join-Path $root "target\debug\cachex-bench.exe"
if (-not (Test-Path $server)) { throw "build the server first: cargo build -p cachex-server" }
if (-not (Test-Path $bench)) { throw "build the bench first: cargo build -p cachex-bench" }

$publicPort = 28001
$metricsPort = 28002
$cacheBytes = $CacheMB * 1024 * 1024

function New-NodeConfig {
    $path = Join-Path $env:TEMP "cachex-exp-eviction.toml"
    @"
[node]
id = 1
host = "127.0.0.1"
port = $publicPort

[cluster]
nodes = [
    "127.0.0.1:$publicPort",
]

[cache]
max_memory_bytes = $cacheBytes
eviction_policy = "lru"
ttl_purge_interval_secs = 1

[aof]
enabled = false
path = "cachex-exp-eviction.aof"
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
    $config = New-NodeConfig
    Start-Process -FilePath $server -ArgumentList $config -WorkingDirectory $env:TEMP `
        -RedirectStandardOutput (Join-Path $env:TEMP "cachex-exp-out.txt") `
        -RedirectStandardError (Join-Path $env:TEMP "cachex-exp-err.txt") -PassThru
}

$capacityKeys = [math]::Floor($cacheBytes / (64 + $ValueSize))
$keyspaces = @(1000, 3000, 5000, 7000, 10000, 15000, 20000)

Set-Content -Path $Out -Value "cache_mb,value_size,capacity_keys,keyspace,requests,hit_rate,ops_per_sec,hits,misses,evictions"

foreach ($keyspace in $keyspaces) {
    $node = Start-Node
    Start-Sleep -Milliseconds 1500
    try {
        
        & $bench --nodes "127.0.0.1:$publicPort" --clients 1 --requests $keyspace --keys $keyspace `
            --value-size $ValueSize --get-ratio 0.0 --key-order sequential --seed 1 `
            --output (Join-Path $env:TEMP "cachex-exp-load.csv") | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "load phase failed" }

        
        & $bench --nodes "127.0.0.1:$publicPort" --clients 1 --requests $keyspace --keys $keyspace `
            --value-size $ValueSize --get-ratio 1.0 --key-order sequential --seed 2 `
            --output (Join-Path $env:TEMP "cachex-exp-measure.csv") | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "measure phase failed" }

        $row = Get-Content (Join-Path $env:TEMP "cachex-exp-measure.csv") | Select-Object -Last 1
        $fields = $row.Split(",")
        
        $ops = [double]$fields[11]
        $hits = [int64]$fields[14]
        $misses = [int64]$fields[15]
        $hitRate = if (($hits + $misses) -gt 0) { $hits / ($hits + $misses) } else { 0.0 }

        $metrics = curl.exe -s "http://127.0.0.1:$metricsPort/metrics" | ConvertFrom-Json
        $evictions = $metrics.storage.evictions

        "{0},{1},{2},{3},{4},{5:0.0000},{6:0.00},{7},{8},{9}" -f `
            $CacheMB, $ValueSize, $capacityKeys, $keyspace, $keyspace, `
            $hitRate, $ops, $hits, $misses, $evictions | Add-Content -Path $Out

        Write-Output ("keyspace {0,6}: hit_rate {1,6:P1}  evictions {2}" -f $keyspace, $hitRate, $evictions)
    }
    finally {
        Stop-Process -Id $node.Id -Force -ErrorAction SilentlyContinue
        Start-Sleep -Milliseconds 300
    }
}
Write-Output "eviction results written to $Out"

