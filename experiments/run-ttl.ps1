

param(
    [int]$TTL = 10,
    [int]$Keys = 5000,
    [int]$IntervalSecs = 2,
    [string]$Out = "$PSScriptRoot\results\ttl.csv"
)
$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$server = Join-Path $root "target\debug\cachex-server.exe"
$bench = Join-Path $root "target\debug\cachex-bench.exe"
if (-not (Test-Path $server)) { throw "build the server first: cargo build -p cachex-server" }
if (-not (Test-Path $bench)) { throw "build the bench first: cargo build -p cachex-bench" }

$publicPort = 28201
$metricsPort = 28202

function New-NodeConfig {
    $path = Join-Path $env:TEMP "cachex-exp-ttl.toml"
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
max_memory_bytes = 104857600
eviction_policy = "lru"
ttl_purge_interval_secs = 1

[aof]
enabled = false
path = "cachex-exp-ttl.aof"
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

$node = Start-Process -FilePath $server -ArgumentList (New-NodeConfig) -WorkingDirectory $env:TEMP `
    -RedirectStandardOutput (Join-Path $env:TEMP "cachex-exp-out.txt") `
    -RedirectStandardError (Join-Path $env:TEMP "cachex-exp-err.txt") -PassThru
Start-Sleep -Milliseconds 1500

Set-Content -Path $Out -Value "ttl_secs,elapsed_secs,requests,hit_rate,ops_per_sec,hits,misses,errors"

try {
    
    & $bench --nodes "127.0.0.1:$publicPort" --clients 1 --requests $Keys --keys $Keys `
        --value-size 32 --get-ratio 0.0 --key-order sequential --seed 1 --ttl $TTL `
        --output (Join-Path $env:TEMP "cachex-exp-ttl-load.csv") | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "load phase failed" }

    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $real = 0.0
    while ($real -le ($TTL + $IntervalSecs * 2)) {
        & $bench --nodes "127.0.0.1:$publicPort" --clients 1 --requests $Keys --keys $Keys `
            --value-size 32 --get-ratio 1.0 --key-order sequential --seed 2 `
            --output (Join-Path $env:TEMP "cachex-exp-ttl-measure.csv") | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "measure phase failed" }

        $row = Get-Content (Join-Path $env:TEMP "cachex-exp-ttl-measure.csv") | Select-Object -Last 1
        $fields = $row.Split(",")
        $ops = [double]$fields[11]
        $hits = [int64]$fields[14]
        $misses = [int64]$fields[15]
        $errors = [int64]$fields[16]
        $hitRate = if (($hits + $misses) -gt 0) { $hits / ($hits + $misses) } else { 0.0 }

        
        
        $real = $sw.Elapsed.TotalSeconds

        "{0},{1:N1},{2},{3:0.0000},{4:0.00},{5},{6},{7}" -f `
            $TTL, $real, ($hits + $misses), $hitRate, $ops, $hits, $misses, $errors | Add-Content -Path $Out

        Write-Output ("elapsed {0,5:N1}s: hit_rate {1,6:P1}" -f $real, $hitRate)
        Start-Sleep -Seconds $IntervalSecs
    }
}
finally {
    Stop-Process -Id $node.Id -Force -ErrorAction SilentlyContinue
}
Write-Output "ttl results written to $Out"

