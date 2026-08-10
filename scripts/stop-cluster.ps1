

$root = Split-Path -Parent $PSScriptRoot
$pidFile = Join-Path $root "scripts\.cluster-pids"

if (Test-Path $pidFile) {
    $pids = (Get-Content $pidFile).Split(",") | Where-Object { $_ }
    foreach ($processId in $pids) {
        Stop-Process -Id ([int]$processId) -Force -ErrorAction SilentlyContinue
        Write-Host "Stopped pid $processId"
    }
    Remove-Item $pidFile -ErrorAction SilentlyContinue
} else {
    Write-Host "No pid file found; killing anything on 7001-7003..."
    foreach ($port in 7001..7003) {
        $conn = Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue
        if ($conn) { $conn | ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue; Write-Host "Killed pid $($_.OwningProcess) on $port" } }
    }
}
Write-Host "Cluster stopped."