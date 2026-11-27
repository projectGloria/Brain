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
# Description: Run pre-flight build, Rust tests, clippy, version sync, gloria.toml, and Brain checks
$ErrorActionPreference = 'Stop'
$RepoRoot = Split-Path -Parent $env:GLORIA_TOOL_DIR.TrimEnd('\', '/')
Set-Location -LiteralPath $RepoRoot

$results = [System.Collections.Generic.List[object]]::new()

function Invoke-Step([string]$Name, [scriptblock]$Action) {
    Write-Host "`n=== [$Name] ===" -ForegroundColor Cyan
    $ok = $false
    try {
        $global:LASTEXITCODE = 0
        & $Action
        if ($LASTEXITCODE -eq 0) { $ok = $true }
    } catch {
        Write-Host $_ -ForegroundColor Red
        $ok = $false
    }
    [void]$results.Add([PSCustomObject]@{ Name = $Name; Passed = $ok })
}

Invoke-Step 'Frontend build (tsc && vite build)' {
    & npm run build
}

Invoke-Step 'Rust tests (cargo test)' {
    & cargo test --manifest-path src-tauri/Cargo.toml
}

Invoke-Step 'Rust lint (cargo clippy)' {
    & cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
}

Invoke-Step 'Version manifest sync' {
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

    $map = [ordered]@{
        'package.json'          = $pkg
        'package-lock.json'     = $pkgLock
        'src-tauri/Cargo.toml'  = $cargoTomlVer
        'src-tauri/Cargo.lock'  = $cargoLockVer
        'src-tauri/tauri.conf.json' = $tauriConf
        'gloria.toml'           = $gloriaVer
    }
    foreach ($k in $map.Keys) {
        Write-Host ("  {0,-28} {1}" -f $k, $map[$k])
    }
    $unique = @($map.Values | Select-Object -Unique)
    if ($unique.Count -ne 1 -or $unique[0] -eq '<missing>') {
        throw "Version mismatch across manifests!"
    }
}

Invoke-Step 'GloriaSetup manifest check' {
    & npx --yes gloria-setup@latest --check --manifest gloria.toml
}

Invoke-Step 'Brain sync check' {
    & (Join-Path $RepoRoot 'install-brain.bat') check .
}

Write-Host "`n=== Summary ===" -ForegroundColor Cyan
$failed = 0
foreach ($r in $results) {
    if ($r.Passed) {
        Write-Host ("  [PASS] {0}" -f $r.Name) -ForegroundColor Green
    } else {
        Write-Host ("  [FAIL] {0}" -f $r.Name) -ForegroundColor Red
        $failed++
    }
}

if ($failed -gt 0) {
    exit 1
}
exit 0
