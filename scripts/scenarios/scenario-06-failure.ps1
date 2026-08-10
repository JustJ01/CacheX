

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\_lib.ps1"

Ensure-Binaries
Start-NodeConfigs $Script:StockConfigs

Run-Bench @{
    clients     = 8
    requests    = 60000
    "key-count" = 20000
    "value-size" = 128
    "get-ratio" = 0.5
    "key-order" = "uniform"
    seed        = 8
} "scenario-06-failure-warm"

$nodePid = Get-NodePid 2
if (-not $nodePid) {
    Write-Host "Could not find node 2 process. Is the cluster running?" -ForegroundColor Red
    exit 1
}

Header "Scenario 6 - Node failure and recovery"
Write-Host "Killing node 2 (pid $nodePid)..." -ForegroundColor Magenta
Stop-Process -Id $nodePid -Force -ErrorAction SilentlyContinue
Write-Host "Node 2 killed. The heartbeat will mark it Failed within ~2s." -ForegroundColor Magenta

Watch "Nodes KPI drops to 2/3; node 2 row shows a red 'Down' pill; Req/sec falls."
Start-Sleep -Seconds 6

Write-Host "Restarting node 2..." -ForegroundColor Magenta
Restart-Node 2 (Join-Path $Script:ProjectRoot "configs\node2.toml")
Write-Host "Node 2 restarted. It rejoins the cluster on the next heartbeat." -ForegroundColor Magenta

Watch "Nodes KPI returns to 3/3; node 2 row is green 'Healthy' again."
Start-Sleep -Seconds 5

Write-Host ""
Write-Host "Scenario 6 done. Failure was detected by the heartbeat, not the client." -ForegroundColor DarkGray