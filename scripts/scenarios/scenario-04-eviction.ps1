

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\_lib.ps1"

Ensure-Binaries

$evictConfigs = @(
    (Join-Path $Script:ProjectRoot "configs\scenario\evict-node1.toml"),
    (Join-Path $Script:ProjectRoot "configs\scenario\evict-node2.toml"),
    (Join-Path $Script:ProjectRoot "configs\scenario\evict-node3.toml")
)

Header "Scenario 4 - LRU eviction (1 MiB cache per node)"
Start-NodeConfigs $evictConfigs
Watch "Node table: Memory pins near 100%; Keys settle around the ~5-6k capacity."

Run-Bench @{
    clients     = 8
    requests    = 200000
    "key-count" = 30000
    "value-size" = 128
    "get-ratio" = 0.5
    "key-order" = "sequential"
    seed        = 5
} "scenario-04-eviction"

Watch "Evictions column has grown to tens of thousands; Hit rate is low."
Write-Host ""
Write-Host "Restoring the stock 100 MiB cluster so later scenarios run normally..." -ForegroundColor DarkGray
Start-NodeConfigs $Script:StockConfigs
Write-Host "Scenario 4 done (stock cluster restored)." -ForegroundColor Green