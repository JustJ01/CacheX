

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\_lib.ps1"

Ensure-Binaries
Start-NodeConfigs $Script:StockConfigs

Header "Scenario 5 - TTL expiry (TTL = 10s)"
Watch "Phase 1: Keys + Memory climb; hit rate ~0."
Run-Bench @{
    clients     = 8
    requests    = 100000
    "key-count" = 50000
    "value-size" = 128
    "get-ratio" = 0.2
    "key-order" = "sequential"
    seed        = 6
    ttl         = 10
} "scenario-05-ttl-write"

Write-Host ""
Write-Host "TTL expiry window active. Keys should expire over the next ~12s." -ForegroundColor DarkGray
Watch "Keys column and Memory % drop to ~0 as the 10s TTL expires."
Start-Sleep -Seconds 14

Watch "Phase 2: reading the expired keyspace again - all misses, hit rate ~0."
Run-Bench @{
    clients     = 8
    requests    = 100000
    "key-count" = 50000
    "value-size" = 128
    "get-ratio" = 0.99
    "key-order" = "uniform"
    seed        = 7
} "scenario-05-ttl-read"

Write-Host ""
Write-Host "Scenario 5 done. Keys stayed expired, so reads still miss." -ForegroundColor DarkGray