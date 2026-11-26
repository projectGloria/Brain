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
# Description: Audit outdated/vulnerable npm and Cargo dependencies (--apply to update)
$ErrorActionPreference = 'Stop'
$RepoRoot = Split-Path -Parent $env:GLORIA_TOOL_DIR.TrimEnd('\', '/')
Set-Location -LiteralPath $RepoRoot

$apply = $false
foreach ($a in $args) {
    if ($a -eq '--apply') { $apply = $true }
}

if ($apply) {
    Write-Host '[update-deps] Updating npm dependencies...' -ForegroundColor Cyan
    & npm update
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Write-Host '[update-deps] Updating Cargo.lock...' -ForegroundColor Cyan
    & cargo update --manifest-path src-tauri/Cargo.toml
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Write-Host '[update-deps] Done.' -ForegroundColor Green
    exit 0
}

Write-Host '=== npm outdated ===' -ForegroundColor Cyan
& npm outdated
$global:LASTEXITCODE = 0

Write-Host "`n=== npm audit ===" -ForegroundColor Cyan
& npm audit
$global:LASTEXITCODE = 0

if (Get-Command cargo-outdated -ErrorAction SilentlyContinue) {
    Write-Host "`n=== cargo outdated ===" -ForegroundColor Cyan
    & cargo outdated --manifest-path src-tauri/Cargo.toml
    $global:LASTEXITCODE = 0
} else {
    Write-Host "`n[info] cargo-outdated not installed (optional: cargo install cargo-outdated)." -ForegroundColor DarkGray
}

Write-Host "`nRun with --apply to update package-lock.json and Cargo.lock." -ForegroundColor DarkGray
