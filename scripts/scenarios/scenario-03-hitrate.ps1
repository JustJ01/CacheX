

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\_lib.ps1"

Ensure-Binaries
Start-NodeConfigs $Script:StockConfigs

Header "Scenario 3 - Cache warm-up + hit rate"
Watch "Phase 1: Hit rate stays ~0 while keys are written."
Run-Bench @{
    clients     = 8
    requests    = 200000
    "key-count" = 20000
    "value-size" = 128
    "get-ratio" = 0.0
    "key-order" = "sequential"
    seed        = 3
} "scenario-03-hitrate-warm"

Watch "Phase 2: Hit rate KPI + chart climb toward ~100% and hold."
Run-Bench @{
    clients     = 16
    requests    = 300000
    "key-count" = 20000
    "value-size" = 128
    "get-ratio" = 0.99
    "key-order" = "uniform"
    seed        = 4
} "scenario-03-hitrate-read"

Write-Host ""
Write-Host "Scenario 3 done. Hit rate should now read close to 100%." -ForegroundColor DarkGray