@echo off
setlocal
set "GLORIA_TOOL_DIR=%~dp0"
set "GLORIA_SELF=%~f0"
powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "$lines = [System.IO.File]::ReadAllLines($env:GLORIA_SELF, [System.Text.Encoding]::UTF8); $idx = [Array]::IndexOf($lines, '# --- POWERSHELL_START ---'); $code = ($lines[($idx+1)..($lines.Length-1)] -join \"`n\"); & ([scriptblock]::Create($code)) @args" -- %*
set "EXIT_CODE=%ERRORLEVEL%"
if "%EXIT_CODE%" NEQ "0" (
    echo.
    echo [gloriaTools] Command failed with exit code %EXIT_CODE%.
    echo %CMDCMDLINE% | findstr /I /C:"/c" >nul && (
        if /I "%CI%" NEQ "true" pause
    )
)
exit /b %EXIT_CODE%
# --- POWERSHELL_START ---
# Description: Smart Tauri + Vite dev runner with automatic free-port selection
$ErrorActionPreference = 'Stop'
$RepoRoot = Split-Path -Parent $env:GLORIA_TOOL_DIR.TrimEnd('\', '/')
Set-Location -LiteralPath $RepoRoot

$startPort = 1420
$extraArgs = @()
for ($i = 0; $i -lt $args.Count; $i++) {
    $a = [string]$args[$i]
    if ($a -eq '--port' -and ($i + 1) -lt $args.Count) {
        $parsed = 0
        if (-not [int]::TryParse([string]$args[$i + 1], [ref]$parsed) -or $parsed -lt 1 -or $parsed -gt 65535) {
            throw "Invalid port number: $($args[$i + 1])"
        }
        $startPort = $parsed
        $i++
    } elseif ($a -like '--port=*') {
        $val = $a.Substring(7)
        $parsed = 0
        if (-not [int]::TryParse($val, [ref]$parsed) -or $parsed -lt 1 -or $parsed -gt 65535) {
            throw "Invalid port number: $val"
        }
        $startPort = $parsed
    } else {
        $extraArgs += $a
    }
}

if (-not (Test-Path -LiteralPath (Join-Path $RepoRoot 'node_modules'))) {
    Write-Host '[dev] node_modules missing; running npm ci...' -ForegroundColor Yellow
    & npm ci
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

function Test-PortFree([int]$Port) {
    $active = [System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners()
    foreach ($ep in $active) {
        if ($ep.Port -eq $Port) { return $false }
    }
    $listener = $null
    try {
        $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, $Port)
        $listener.Start()
        return $true
    } catch {
        return $false
    } finally {
        if ($listener) {
            try { $listener.Stop() } catch {}
        }
    }
}

$chosenPort = $null
for ($p = $startPort; $p -le ($startPort + 50) -and $p -le 65535; $p++) {
    if (Test-PortFree $p) {
        $chosenPort = $p
        break
    }
}
if (-not $chosenPort) {
    throw "No free port found in range $startPort..$($startPort + 50)."
}

if ($chosenPort -ne $startPort) {
    Write-Host "[dev] Port $startPort is busy -> selected free port $chosenPort" -ForegroundColor Cyan
} else {
    Write-Host "[dev] Using port $chosenPort" -ForegroundColor Green
}

$tmpConfig = Join-Path $env:TEMP ("gloria-tauri-dev-$chosenPort-$([guid]::NewGuid().ToString('N').Substring(0,8)).json")
$cfgObj = @{
    build = @{
        devUrl = "http://localhost:$chosenPort"
        beforeDevCommand = "npx vite --port $chosenPort --strictPort"
    }
}
$cfgJson = $cfgObj | ConvertTo-Json -Depth 5
[System.IO.File]::WriteAllText($tmpConfig, $cfgJson, (New-Object System.Text.UTF8Encoding($false)))

try {
    & npx tauri dev --config $tmpConfig @extraArgs
    exit $LASTEXITCODE
} finally {
    if (Test-Path -LiteralPath $tmpConfig) {
        Remove-Item -LiteralPath $tmpConfig -Force -ErrorAction SilentlyContinue
    }
}
