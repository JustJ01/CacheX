

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\_lib.ps1"

Ensure-Binaries
Start-NodeConfigs $Script:StockConfigs

Header "Scenario 1 - Read-heavy load (throughput, hit rate, latency)"
Watch "Throughput chart climbs and stays elevated while the benchmark runs; then decays."
Watch "Hit rate KPI climbs; GET latency p50/p95/p99 form a low steady band."

Run-Bench @{
    clients     = 16
    requests    = 400000
    "key-count" = 40000
    "value-size" = 128
    "get-ratio" = 0.8
    "key-order" = "sequential"
    seed        = 1
} "scenario-01-load"

Write-Host ""
Write-Host "Scenario 1 done. The dashboard keeps the last 120 samples, so the" -ForegroundColor DarkGray
Write-Host "throughput bump stays visible on the chart for a couple of minutes." -ForegroundColor DarkGray