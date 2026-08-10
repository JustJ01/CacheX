

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $root "target\debug\cachex-server.exe"

if (-not (Test-Path $exe)) {
    Write-Host "Server binary not found. Run 'cargo build -p cachex-server' first."
    exit 1
}

$pids = @()
foreach ($id in 1..3) {
    $cfg = Join-Path $root "configs\node$id.toml"
    $proc = Start-Process -FilePath $exe -ArgumentList $cfg -WorkingDirectory $root -PassThru -NoNewWindow
    $pids += $proc.Id
    Write-Host "Started node $id (pid $($proc.Id))"
    Start-Sleep -Milliseconds 500
}

$pidFile = Join-Path $root "scripts\.cluster-pids"
$pids -join "," | Out-File -FilePath $pidFile -Encoding ascii
Write-Host "Cluster started. PIDs written to scripts/.cluster-pids"