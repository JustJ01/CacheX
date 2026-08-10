

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\_lib.ps1"

Ensure-Binaries

$replConfigs = @(
    (Join-Path $Script:ProjectRoot "configs\scenario\repl-node1.toml"),
    (Join-Path $Script:ProjectRoot "configs\scenario\repl-node2.toml"),
    (Join-Path $Script:ProjectRoot "configs\scenario\repl-node3.toml")
)

Header "Scenario 7 - Replication factor 2"
Start-NodeConfigs $replConfigs
Watch "Keys and Memory grow ~2x compared to the same workload with RF=1."

Run-Bench @{
    clients     = 8
    requests    = 200000
    "key-count" = 30000
    "value-size" = 128
    "get-ratio" = 0.5
    "key-order" = "sequential"
    seed        = 9
} "scenario-07-replication"

Watch "Each node stores ~30k keys total (primary + replicas) - roughly 2x RF=1."
Write-Host ""
Write-Host "Restoring the stock RF=1 cluster..." -ForegroundColor DarkGray
Start-NodeConfigs $Script:StockConfigs
Write-Host "Scenario 7 done (stock cluster restored)." -ForegroundColor Green