

$ErrorActionPreference = "Stop"

$Script:ScenarioRoot = $PSScriptRoot
$Script:ProjectRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$Script:ServerExe = Join-Path $Script:ProjectRoot "target\debug\cachex-server.exe"
$Script:BenchExe = Join-Path $Script:ProjectRoot "target\debug\cachex-bench.exe"
$Script:PidFile = Join-Path $Script:ScenarioRoot ".scenario-pids"
$Script:ResultDir = Join-Path $Script:ScenarioRoot "results"
$Script:NodesArg = "127.0.0.1:7001,127.0.0.1:7002,127.0.0.1:7003"
$Script:StockConfigs = 1..3 | ForEach-Object { Join-Path $Script:ProjectRoot "configs\node$_.toml" }

function Header([string]$text) {
    Write-Host ""
    Write-Host "=== $text ===" -ForegroundColor Cyan
}

function Watch([string]$text) {
    Write-Host ""
    Write-Host ">> WATCH THE DASHBOARD: $text" -ForegroundColor Yellow
    Write-Host ""
}

function Info([string]$text) {
    Write-Host "$text" -ForegroundColor DarkGray
}

function Ensure-Binaries {
    if (-not (Test-Path $Script:ServerExe)) {
        throw "Server binary missing ($($Script:ServerExe)). Run 'cargo build -p cachex-server' first."
    }
    if (-not (Test-Path $Script:BenchExe)) {
        throw "Bench binary missing ($($Script:BenchExe)). Run 'cargo build -p cachex-bench' first."
    }
    New-Item -ItemType Directory -Path $Script:ResultDir -Force | Out-Null
}

function Stop-AnyCluster {
    if (Test-Path $Script:PidFile) {
        $lines = Get-Content $Script:PidFile | Where-Object { $_ -match '^\d+=\d+$' }
        foreach ($line in $lines) {
            $procId = [int](($line -split '=')[1])
            Stop-Process -Id $procId -Force -ErrorAction SilentlyContinue
            Write-Host "Stopped pid $procId" -ForegroundColor DarkGray
        }
        Remove-Item $Script:PidFile -ErrorAction SilentlyContinue
    }
    foreach ($port in @(7001..7003) + @(8001..8003) + @(9001..9003)) {
        $conn = Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue
        if ($conn) {
            $conn | ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }
        }
    }
    Start-Sleep -Milliseconds 800
}

function Start-NodeConfigs([string[]]$configPaths) {
    Stop-AnyCluster
    $procIds = @()
    foreach ($cfg in $configPaths) {
        $name = [IO.Path]::GetFileNameWithoutExtension($cfg)
        $out = Join-Path $Script:ScenarioRoot "$name.out.log"
        $err = Join-Path $Script:ScenarioRoot "$name.err.log"
        $proc = Start-Process -FilePath $Script:ServerExe -ArgumentList $cfg `
            -WorkingDirectory $Script:ProjectRoot -PassThru `
            -RedirectStandardOutput $out -RedirectStandardError $err
        $procIds += $proc.Id
        Write-Host "Started $name (pid $($proc.Id))" -ForegroundColor DarkGray
        Start-Sleep -Milliseconds 700
    }
    $deadline = (Get-Date).AddSeconds(20)
    foreach ($port in 9001..9003) {
        while ((Get-Date) -lt $deadline) {
            try {
                Invoke-WebRequest -Uri "http://127.0.0.1:$port/metrics" -UseBasicParsing -TimeoutSec 2 | Out-Null
                break
            } catch {
                Start-Sleep -Milliseconds 500
            }
        }
    }
    $lines = @()
    for ($i = 0; $i -lt $procIds.Count; $i++) {
        $lines += "700$($i + 1)=$($procIds[$i])"
    }
    $lines | Out-File $Script:PidFile -Encoding ascii
    Write-Host "Cluster up (metrics on 9001-9003)." -ForegroundColor Green
}

function Get-NodePid([int]$nodeId) {
    if (Test-Path $Script:PidFile) {
        $line = Get-Content $Script:PidFile | Where-Object { $_ -match "^700$nodeId=" } | Select-Object -First 1
        if ($line) { return [int](($line -split '=')[1]) }
    }
    $conn = Get-NetTCPConnection -LocalPort (7000 + $nodeId) -State Listen -ErrorAction SilentlyContinue
    if ($conn) { return $conn[0].OwningProcess }
    return $null
}

function Restart-Node([int]$nodeId, [string]$configPath) {
    $existing = Get-NodePid $nodeId
    if ($existing) {
        Stop-Process -Id $existing -Force -ErrorAction SilentlyContinue
        Write-Host "Killed node $nodeId (pid $existing)" -ForegroundColor DarkGray
        Start-Sleep -Seconds 2
    }
    $name = "node$nodeId"
    $out = Join-Path $Script:ScenarioRoot "$name.out.log"
    $err = Join-Path $Script:ScenarioRoot "$name.err.log"
    $proc = Start-Process -FilePath $Script:ServerExe -ArgumentList $configPath `
        -WorkingDirectory $Script:ProjectRoot -PassThru `
        -RedirectStandardOutput $out -RedirectStandardError $err
    Write-Host "Restarted node $nodeId (pid $($proc.Id))" -ForegroundColor DarkGray

    $lines = @()
    if (Test-Path $Script:PidFile) { $lines = Get-Content $Script:PidFile }
    $updated = $false
    $lines = $lines | ForEach-Object {
        if ($_ -match "^700$nodeId=") {
            $updated = $true
            "700$nodeId=$($proc.Id)"
        } else {
            $_
        }
    }
    if (-not $updated) { $lines += "700$nodeId=$($proc.Id)" }
    $lines | Out-File $Script:PidFile -Encoding ascii

    $deadline = (Get-Date).AddSeconds(15)
    while ((Get-Date) -lt $deadline) {
        try {
            Invoke-WebRequest -Uri "http://127.0.0.1:$((9000 + $nodeId))/metrics" -UseBasicParsing -TimeoutSec 2 | Out-Null
            break
        } catch {
            Start-Sleep -Milliseconds 500
        }
    }
}

function Run-Bench([hashtable]$params, [string]$name) {
    $csv = Join-Path $Script:ResultDir "$name.csv"
    $argv = @("--nodes", $Script:NodesArg, "--output", $csv)
    foreach ($rawKey in $params.Keys) {
        
        
        
        $flag = if ($rawKey -eq "key-count") { "--keys" } else { "--$rawKey" }
        $argv += $flag
        $argv += "$($params[$rawKey])"
    }
    Write-Host ""
    Write-Host "Running bench: $($argv -join ' ')" -ForegroundColor Green
    & $Script:BenchExe @argv
    if ($LASTEXITCODE -ne 0) { throw "bench failed with exit code $LASTEXITCODE" }
    Write-Host "Results saved to $csv" -ForegroundColor DarkGray
}