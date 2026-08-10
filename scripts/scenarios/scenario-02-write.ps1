

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\_lib.ps1"

Ensure-Binaries
Start-NodeConfigs $Script:StockConfigs

Header "Scenario 2 - Write-heavy load (keys + memory growth)"
Watch "Node table: Keys column climbs toward ~30k per node; Memory % climbs."
Watch "Hit rate KPI stays near 0 (all writes, no reads)."

Run-Bench @{
    clients     = 16
    requests    = 200000
    "key-count" = 90000
    "value-size" = 128
    "get-ratio" = 0.1
    "key-order" = "sequential"
    seed        = 2
} "scenario-02-write"

Write-Host ""
Write-Host "Scenario 2 done. Keys are persisted per node and stay visible on the" -ForegroundColor DarkGray
Write-Host "node table until the cluster restarts." -ForegroundColor DarkGray