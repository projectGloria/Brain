@echo off
setlocal
set "GLORIA_TOOL_DIR=%~dp0"
set "GLORIA_SELF=%~f0"
powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "$lines = [System.IO.File]::ReadAllLines($env:GLORIA_SELF, [System.Text.Encoding]::UTF8); $idx = [Array]::IndexOf($lines, '# --- POWERSHELL_START ---'); $code = ($lines[($idx+1)..($lines.Length-1)] -join \"`n\"); & ([scriptblock]::Create($code)) @args" -- %*
set "EXIT_CODE=%ERRORLEVEL%"
if "%EXIT_CODE%" NEQ "0" (
    echo.
    echo [gloriaTools] Command failed with exit code %EXIT_CODE%.
    echo %CMDCMDLINE% | findstr /I /C:"%~nx0" >nul && (
        if /I "%CI%" NEQ "true" pause
    )
)
exit /b %EXIT_CODE%
# --- POWERSHELL_START ---
# Description: Show git status, version manifest sync, Brain sync, active ports, and cache sizes
$ErrorActionPreference = 'Stop'
$RepoRoot = Split-Path -Parent $env:GLORIA_TOOL_DIR.TrimEnd('\', '/')
Set-Location -LiteralPath $RepoRoot

Write-Host '=== Git ===' -ForegroundColor Cyan
$branch = (& git branch --show-current 2>$null)
$dirty = @(& git status --porcelain 2>$null)
Write-Host ("  Branch : {0}" -f $(if ($branch) { $branch } else { '<detached>' }))
Write-Host ("  Dirty  : {0} file(s)" -f $dirty.Count)

Write-Host "`n=== Version Manifests ===" -ForegroundColor Cyan
$pkg = (Get-Content -Raw -LiteralPath 'package.json' -Encoding UTF8 | ConvertFrom-Json).version
$pkgLockRaw = Get-Content -Raw -LiteralPath 'package-lock.json' -Encoding UTF8
$pkgLock = if ($pkgLockRaw -match '"version"\s*:\s*"([^"]+)"') { $Matches[1] } else { '<missing>' }
$tauriConf = (Get-Content -Raw -LiteralPath 'src-tauri/tauri.conf.json' -Encoding UTF8 | ConvertFrom-Json).version
$cargoTomlRaw = Get-Content -Raw -LiteralPath 'src-tauri/Cargo.toml' -Encoding UTF8
$cargoTomlVer = if ($cargoTomlRaw -match '(?ms)^\[package\].*?^version\s*=\s*"([^"]+)"') { $Matches[1] } else { '<missing>' }
$cargoLockRaw = Get-Content -Raw -LiteralPath 'src-tauri/Cargo.lock' -Encoding UTF8
$cargoLockVer = if ($cargoLockRaw -match '(?ms)\[\[package\]\]\s*name\s*=\s*"brain"\s*version\s*=\s*"([^"]+)"') { $Matches[1] } else { '<missing>' }
$gloriaRaw = Get-Content -Raw -LiteralPath 'gloria.toml' -Encoding UTF8
$gloriaVer = if ($gloriaRaw -match '(?m)^version\s*=\s*"([^"]+)"') { $Matches[1] } else { '<missing>' }

$versions = [ordered]@{
    'package.json'              = $pkg
    'package-lock.json'         = $pkgLock
    'src-tauri/Cargo.toml'      = $cargoTomlVer
    'src-tauri/Cargo.lock'      = $cargoLockVer
    'src-tauri/tauri.conf.json' = $tauriConf
    'gloria.toml'               = $gloriaVer
}
$uniq = @($versions.Values | Select-Object -Unique)
foreach ($k in $versions.Keys) {
    Write-Host ("  {0,-28} {1}" -f $k, $versions[$k])
}
if ($uniq.Count -eq 1) {
    Write-Host "  Sync   : OK ($($uniq[0]))" -ForegroundColor Green
} else {
    Write-Host '  Sync   : MISMATCH DETECTED' -ForegroundColor Red
}

Write-Host "`n=== Brain Sync ===" -ForegroundColor Cyan
& (Join-Path $RepoRoot 'install-brain.bat') check .

Write-Host "`n=== Active Repo Processes & Ports (1420..1470) ===" -ForegroundColor Cyan
$found = $false
Get-Process -Name 'Brain' -ErrorAction SilentlyContinue | ForEach-Object {
    $p = $null; try { $p = $_.Path } catch {}
    if ($p -and $p.StartsWith($RepoRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
        Write-Host "  Brain.exe PID $($_.Id) ($p)" -ForegroundColor Yellow
        $found = $true
    }
}
if (Get-Command Get-NetTCPConnection -ErrorAction SilentlyContinue) {
    Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | Where-Object {
        $_.LocalPort -ge 1420 -and $_.LocalPort -le 1470
    } | ForEach-Object {
        $cim = Get-CimInstance Win32_Process -Filter "ProcessId = $($_.OwningProcess)" -ErrorAction SilentlyContinue
        Write-Host ("  Port {0} -> PID {1} ({2})" -f $_.LocalPort, $_.OwningProcess, $(if ($cim) { $cim.Name } else { '?' }))
        $found = $true
    }
}
if (-not $found) { Write-Host '  None active.' -ForegroundColor DarkGray }

Write-Host "`n=== Build & Cache Sizes ===" -ForegroundColor Cyan
foreach ($rel in @('src-tauri\target', 'node_modules', 'dist', 'stage')) {
    $full = Join-Path $RepoRoot $rel
    if (Test-Path -LiteralPath $full) {
        $bytes = (Get-ChildItem -LiteralPath $full -Recurse -File -ErrorAction SilentlyContinue | Measure-Object -Property Length -Sum).Sum
        $mb = [math]::Round(($bytes / 1MB), 1)
        Write-Host ("  {0,-20} {1,8} MB" -f $rel, $mb)
    } else {
        Write-Host ("  {0,-20}        -" -f $rel) -ForegroundColor DarkGray
    }
}
