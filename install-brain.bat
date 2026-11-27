@echo off
setlocal
rem Thin launcher: every command is implemented by Brain.exe (src-tauri/src/cli.rs),
rem so the fingerprint, sync and skills logic exists only once.
rem The PowerShell payload below is written to a temp .ps1 and run with -File.
set "BRAIN_SCRIPT_DIR=%~dp0"
set "BRAIN_PS1=%TEMP%\brain-launcher-%RANDOM%%RANDOM%.ps1"
powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "$l=[IO.File]::ReadAllLines('%~f0');$i=[Array]::IndexOf($l,'# --- POWERSHELL_START ---');[IO.File]::WriteAllLines($env:BRAIN_PS1,$l[($i+1)..($l.Length-1)])"
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%BRAIN_PS1%" %*
set "BRAIN_RC=%ERRORLEVEL%"
del "%BRAIN_PS1%" >nul 2>&1
exit /b %BRAIN_RC%
# --- POWERSHELL_START ---
$ErrorActionPreference = 'Stop'
$ScriptDir = $env:BRAIN_SCRIPT_DIR.TrimEnd('\', '/')

# Old command names map onto the Brain.exe CLI.
$argv = @($args)
if ($argv.Count -eq 0) { $argv = @('install') }
elseif ($argv[0] -eq 'check-all') { $argv = @('check', '--all') + $argv[1..($argv.Count)] | Where-Object { $_ -ne $null } }

$candidates = @(
    $env:BRAIN_EXE,
    (Join-Path $ScriptDir 'src-tauri\target\release\Brain.exe'),
    (Join-Path $ScriptDir 'src-tauri\target\debug\Brain.exe'),
    (Join-Path $env:LOCALAPPDATA 'Programs\Brain\Brain.exe')
)
$onPath = Get-Command 'Brain.exe' -ErrorAction SilentlyContinue
if ($onPath) { $candidates += $onPath.Source }
$exe = $candidates | Where-Object { $_ -and (Test-Path -LiteralPath $_ -PathType Leaf) } | Select-Object -First 1

if ($exe) {
    # Pipe through so the GUI-subsystem exe writes to this console.
    & $exe @argv | Out-Host
    exit $LASTEXITCODE
}

$cargo = Get-Command 'cargo' -ErrorAction SilentlyContinue
if ($cargo) {
    Write-Host 'Brain.exe not found; building it with cargo (first run only)...'
    & $cargo.Source build --release --manifest-path (Join-Path $ScriptDir 'src-tauri\Cargo.toml')
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    & (Join-Path $ScriptDir 'src-tauri\target\release\Brain.exe') @argv | Out-Host
    exit $LASTEXITCODE
}

Write-Error 'Brain.exe not found. Install Brain, set BRAIN_EXE, or install Rust and re-run to build it.'
exit 1
