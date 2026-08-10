

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\_lib.ps1"

Ensure-Binaries
Start-NodeConfigs $Script:StockConfigs

Header "Scenario 8 - Client scalability ramp (4 -> 16 -> 64 clients)"

Watch "Bump 1 of 3: 4 clients."
Run-Bench @{
    clients     = 4
    requests    = 150000
    "key-count" = 30000
    "value-size" = 128
    "get-ratio" = 0.8
    "key-order" = "uniform"
    seed        = 10
} "scenario-08-scalability-4"

Watch "Bump 2 of 3: 16 clients - throughput should be noticeably higher."
Run-Bench @{
    clients     = 16
    requests    = 150000
    "key-count" = 30000
    "value-size" = 128
    "get-ratio" = 0.8
    "key-order" = "uniform"
    seed        = 11
} "scenario-08-scalability-16"

Watch "Bump 3 of 3: 64 clients - throughput flattens (saturated); latency rises."
Run-Bench @{
    clients     = 64
    requests    = 150000
    "key-count" = 30000
    "value-size" = 128
    "get-ratio" = 0.8
    "key-order" = "uniform"
    seed        = 12
} "scenario-08-scalability-64"

Write-Host ""
Write-Host "Scenario 8 done. Check the three bump heights and the rising latency" -ForegroundColor DarkGray
Write-Host "bands on the charts." -ForegroundColor DarkGray