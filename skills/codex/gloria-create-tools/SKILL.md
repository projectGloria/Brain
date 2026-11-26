---
name: gloria-create-tools
description:  Inspects the current repository and scaffolds tailored Windows-first dev scripts in gloriaTools/ (smart free-port dev runner, pre-flight check, clean & unlock, local release pack, setup/doctor, process stopper, status, release preview, dependency audit, and interactive menu). Trigger when the user invokes gloria-create-tools or asks to create/generate project dev tools or scripts.
---

#  gloria-create-tools — Project Dev Tools Scaffolder

Inspect the current repository and scaffold tailored, Windows-first hybrid `.bat` + PowerShell developer scripts inside `gloriaTools/`.

## Non-Negotiable Rules
1. **Source truth only:** Inspect manifests (`package.json`, `Cargo.toml`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `gloria.toml`, `pyproject.toml`, `requirements.txt`, `go.mod`, `*.csproj`, `.env.example`, `AGENTS.md`) before designing any script. Never invent commands, ports, or binary names.
2. **Output folder & format:** Write all generated tools into `gloriaTools/*.bat` in the repository root using the hybrid `.bat` + PowerShell pattern (`# --- POWERSHELL_START ---`), UTF-8 without BOM, resolving the repo root from `Split-Path -Parent $env:GLORIA_TOOL_DIR.TrimEnd('\','/')` so every script works from the repo root, inside `gloriaTools/`, or when double-clicked in File Explorer.
3. **Double-click friendly, CLI-safe:** Pause on failure **only** when launched via Explorer double-click (`$env:CMDCMDLINE` containing `/c` and not running non-interactively), never hanging CLI, CI, or agent invocations.
4. **Header metadata for `menu.bat`:** Every `gloriaTools\*.bat` script must include `# Description: <concise 1-line summary>` immediately after `# --- POWERSHELL_START ---` so `menu.bat` can discover and list tools dynamically.
5. **Preserve custom tools:** Never silently delete or overwrite custom user-created scripts in `gloriaTools/`. Mark existing files as `Update` or `Keep` in the preview.
6. **Safe process targeting:** `stop.bat` and `clean.bat` must only terminate processes that match the project's binary name or whose command line / working directory belongs to the current repo—never kill unrelated processes on the machine.
7. **Path privacy:** Use `C:\Users\you` in any examples or comments; never hardcode a local username.
8. **No git mutations:** Never run `git commit`, `git push`, `git tag`, or bump versions. `gloriaTools/` is tracked in Git by default unless the user explicitly asks to exclude it.
9. **Preview before write:** Phases 1–3 are strictly read-only. Do not create or modify any file until the user replies `apply`.

---

## Hybrid `.bat` Wrapper Template

Use this exact wrapper for every generated `gloriaTools\<tool>.bat`:

```bat
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
# Description: <concise 1-line summary>
$ErrorActionPreference = 'Stop'
$RepoRoot = Split-Path -Parent $env:GLORIA_TOOL_DIR.TrimEnd('\', '/')
Set-Location -LiteralPath $RepoRoot
```

---

## Workflow

Execute in strict order from the repository root. **Phases 1–3 are strictly read-only.**

### Phase 1: Inspect Repository (read-only)
1. **Stack & Manifests:** Read `AGENTS.md`, `package.json`, `Cargo.toml`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `gloria.toml`, `pyproject.toml`, `requirements.txt`, `go.mod`, `*.csproj`, `.env.example`, and `.github/workflows/*.yml` (whichever exist).
2. **Ports & Binaries:** Extract exact default dev port(s) (e.g., `build.devUrl` in `tauri.conf.json`, Vite/Next/FastAPI server ports), `main_exe` / `productName`, build/test/lint commands, and version manifest paths.
3. **Existing Tools:** List `gloriaTools/` if it already exists and identify any custom user scripts to preserve.

### Phase 2: Select & Design `gloriaTools/*.bat` (read-only)
Select only the tools that apply to the detected stack from the catalog below, filling every command, port, and path from Phase 1 source truth:

- **`dev.bat` — Smart Dev Runner:**
  - Probes the repo's default port (e.g. `1420` for Tauri, `5173` for Vite, `3000` for Next/React, `8000` for FastAPI/Python) via `[System.Net.Sockets.TcpListener]` on `127.0.0.1` and active TCP listeners.
  - If the port is busy, scans upward (`+1` up to `+50`) to select the first free port and prints the chosen port clearly.
  - For **Tauri v2 + Vite**, overrides **both** Vite (`--port <PORT> --strictPort`) and Tauri (`--config "{\"build\":{\"devUrl\":\"http://localhost:<PORT>\",\"beforeDevCommand\":\"npx vite --port <PORT> --strictPort\"}}"`) so they never desync.
  - Auto-checks and installs missing `node_modules` or `.venv` if absent.
  - Supports `--port <N>` to override the starting port.
- **`check.bat` — Pre-Flight Check:**
  - Runs every verified quality gate in sequence (e.g., `tsc --noEmit`, `cargo clippy --all-targets` / `eslint` / `ruff`, `cargo test` / `vitest` / `pytest`).
  - Verifies that all detected version manifests carry the exact same version.
  - Runs `npx --yes gloria-setup@latest --check --manifest gloria.toml` if `gloria.toml` exists.
  - Runs `"%USERPROFILE%\.brain\install-brain.bat" check .` if `%USERPROFILE%\.brain\install-brain.bat` exists.
  - Prints a pass/fail summary and exits non-zero if any check fails.
- **`clean.bat` — Clean & Unlock:**
  - Stops any running project processes holding Windows file locks first (via `stop.bat` or matching project process logic).
  - Removes build/cache directories (`dist/`, `src-tauri/target/`, `.vite/`, `__pycache__/`, etc., matching the repo stack).
  - Supports `--deep` (also removes `node_modules/` / `.venv/`) and `--dry-run` (previews deletions without removing anything).
- **`pack.bat` — Local Release & Pack** *(only when `gloria.toml` or a local release bundle applies)*:
  - Builds release binaries, stages files into a clean `stage/` folder, and packs `dist/<Name>_<version>_x64-setup.exe` using `npx --yes gloria-setup@latest` (never pinned).
  - Verifies the built installer with `Start-Process -FilePath ... -ArgumentList '/S --dry-run' -Wait -PassThru` and confirms `ExitCode -eq 0`.
  - Never commits, pushes, or bumps versions.
- **`setup.bat` — Environment Setup / Doctor:**
  - Checks required toolchains (`node`, `npm`, `cargo`, `rustc`, `python`, `gh`, and WebView2 runtime when `needs_webview2 = true`) and prints installed versions.
  - Installs missing project dependencies.
  - Compares `.env` against `.env.example` if present and reports missing keys without printing secret values.
- **`stop.bat` — Port & Process Killer:**
  - Finds and stops lingering dev servers listening on the project's port range (verified by repo path or command line so unrelated apps are never touched) and any locked project binary (`<main_exe>`).
  - Supports `--port <N>` to target a specific port.
- **`status.bat` — Project Status:**
  - Prints git branch and dirty file count, versions across every manifest (flagging any mismatch), Brain sync status, active project processes/ports, and build/cache folder sizes.
- **`run.bat` — Production / Release Preview:**
  - Launches the compiled release binary (e.g., `src-tauri\target\release\<App>.exe`) or runs `vite preview` / production server on a smart free port.
  - Supports `--build` to rebuild before launching.
- **`update-deps.bat` — Dependency Audit:**
  - Read-only by default: reports outdated packages and security audits (`npm outdated`, `npm audit`, `cargo outdated` if installed, `pip list --outdated`).
  - Only modifies lockfiles when invoked with `--apply`.
- **`menu.bat` — Interactive Launcher:**
  - Dynamically scans `gloriaTools\*.bat` (excluding itself), reads each file's `# Description:` header, prints a numbered menu, and runs the user's selection.

### Phase 3: Preview & Wait (read-only)
Print, in this order, then stop and wait:
1. **Detected Stack:** Runtime, frameworks, package manager, manifests, default port(s), and `main_exe` (if any).
2. **Proposed `gloriaTools/` Scripts:** Table listing each `gloriaTools/<name>.bat`, action (`New` / `Update` / `Keep`), and 1-line purpose.
3. **Key Settings:** Default port + fallback range, binary name, and exact commands wired into `check.bat` / `pack.bat`.
4. **Prompt:** End with: `Reply "apply" to write these tools, or tell me what to change.`

### Phase 4: Apply & Verify (only after user replies `apply`)
1. Write all approved `gloriaTools/*.bat` files using UTF-8 without BOM.
2. Validate every generated `.bat` file's PowerShell payload (everything after `# --- POWERSHELL_START ---`) using `[System.Management.Automation.Language.Parser]::ParseInput(...)` and confirm 0 syntax errors. Fix any syntax error before reporting.
3. Print a concise summary of created/updated files and how to run them (e.g., `.\gloriaTools\dev.bat` or double-click `gloriaTools\menu.bat`). Do not commit, push, or bump versions.
