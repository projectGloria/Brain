use regex::Regex;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::brain_core::{normalize_text, read_utf8, write_utf8_no_bom};

#[derive(Debug, Clone, Serialize)]
pub struct DnaToolPreview {
    pub file_name: String,
    pub description: String,
    /// "NEW" | "IN_SYNC" | "OUTDATED" | "CUSTOM"
    pub status: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DnaProjectSummary {
    pub repo: String,
    pub repo_path: String,
    pub stacks: Vec<String>,
    pub default_port: Option<u16>,
    pub main_exe: Option<String>,
    pub has_gloria_toml: bool,
    pub manifests: Vec<String>,
    /// "MISSING" | "READY" | "OUTDATED" | "PARTIAL"
    pub status: String,
    pub existing_count: usize,
    pub expected_count: usize,
    pub custom_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DnaProjectInspect {
    pub summary: DnaProjectSummary,
    pub tools: Vec<DnaToolPreview>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DnaApplyResult {
    pub repo_path: String,
    pub ok: bool,
    pub written: Vec<String>,
    pub skipped_custom: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct RepoProfile {
    repo_name: String,
    stacks: Vec<String>,
    has_tauri: bool,
    has_vite: bool,
    has_next: bool,
    has_node: bool,
    has_rust: bool,
    rust_manifest: Option<String>,
    has_python: bool,
    has_gloria_toml: bool,
    has_env_example: bool,
    default_port: Option<u16>,
    main_exe: Option<String>,
    app_name: String,
    needs_webview2: bool,
    manifests: Vec<String>,
    npm_scripts: BTreeSet<String>,
    has_tsconfig: bool,
}

fn wrap_bat(description: &str, body: &str) -> String {
    let mut out = String::from(
        "@echo off\n\
         setlocal\n\
         set \"GLORIA_TOOL_DIR=%~dp0\"\n\
         set \"GLORIA_SELF=%~f0\"\n\
         powershell.exe -NoProfile -ExecutionPolicy Bypass -Command \"$lines = [System.IO.File]::ReadAllLines($env:GLORIA_SELF, [System.Text.Encoding]::UTF8); $idx = [Array]::IndexOf($lines, '# --- POWERSHELL_START ---'); $code = ($lines[($idx+1)..($lines.Length-1)] -join \\\"`n\\\"); & ([scriptblock]::Create($code)) @args\" -- %*\n\
         set \"EXIT_CODE=%ERRORLEVEL%\"\n\
         if \"%EXIT_CODE%\" NEQ \"0\" (\n\
         \techo.\n\
         \techo [gloriaTools] Command failed with exit code %EXIT_CODE%.\n\
         \techo %CMDCMDLINE% | findstr /I /C:\"%~nx0\" >nul && (\n\
         \t\tif /I \"%CI%\" NEQ \"true\" pause\n\
         \t)\n\
         )\n\
         exit /b %EXIT_CODE%\n\
         # --- POWERSHELL_START ---\n",
    );
    out.push_str(&format!("# Description: {description}\n"));
    out.push_str("$ErrorActionPreference = 'Stop'\n");
    out.push_str("$RepoRoot = Split-Path -Parent $env:GLORIA_TOOL_DIR.TrimEnd('\\', '/')\n");
    out.push_str("Set-Location -LiteralPath $RepoRoot\n\n");
    out.push_str(body.trim_start_matches('\n'));
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn extract_description(content: &str) -> String {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("# Description:") {
            return rest.trim().to_string();
        }
    }
    "Custom project script".to_string()
}

fn parse_port_from_url(url: &str) -> Option<u16> {
    let re = Regex::new(r":(\d{2,5})").ok()?;
    let cap = re.captures(url)?;
    cap.get(1)?.as_str().parse::<u16>().ok()
}

fn detect_profile(repo_root: &Path) -> RepoProfile {
    let repo_name = repo_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project")
        .to_string();

    let mut p = RepoProfile {
        repo_name: repo_name.clone(),
        app_name: repo_name,
        ..Default::default()
    };

    let manifest_candidates = [
        "package.json",
        "package-lock.json",
        "Cargo.toml",
        "src-tauri/Cargo.toml",
        "Cargo.lock",
        "src-tauri/Cargo.lock",
        "tauri.conf.json",
        "src-tauri/tauri.conf.json",
        "gloria.toml",
        "pyproject.toml",
        "manifest.json",
    ];
    for rel in manifest_candidates {
        if repo_root.join(rel).is_file() {
            p.manifests.push(rel.to_string());
        }
    }

    p.has_tsconfig = repo_root.join("tsconfig.json").is_file();
    p.has_env_example = repo_root.join(".env.example").is_file();

    // Tauri v2
    let tauri_conf_path = ["src-tauri/tauri.conf.json", "tauri.conf.json"]
        .into_iter()
        .map(|rel| repo_root.join(rel))
        .find(|path| path.is_file());
    if let Some(tc_path) = tauri_conf_path {
        p.has_tauri = true;
        p.stacks.push("Tauri v2".to_string());
        if let Ok(raw) = read_utf8(&tc_path) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(prod) = val.get("productName").and_then(|v| v.as_str()) {
                    if !prod.trim().is_empty() {
                        p.app_name = prod.trim().to_string();
                    }
                }
                if let Some(dev_url) = val
                    .get("build")
                    .and_then(|b| b.get("devUrl"))
                    .and_then(|v| v.as_str())
                {
                    p.default_port = parse_port_from_url(dev_url);
                }
            }
        }
    }

    // Node / Vite / React / Next.js
    let pkg_path = repo_root.join("package.json");
    if pkg_path.is_file() {
        p.has_node = true;
        if let Ok(raw) = read_utf8(&pkg_path) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(scripts) = val.get("scripts").and_then(|s| s.as_object()) {
                    for k in scripts.keys() {
                        p.npm_scripts.insert(k.clone());
                    }
                }
                let has_dep = |name: &str| {
                    val.get("dependencies")
                        .and_then(|d| d.get(name))
                        .is_some()
                        || val
                            .get("devDependencies")
                            .and_then(|d| d.get(name))
                            .is_some()
                };
                if has_dep("vite")
                    || repo_root.join("vite.config.ts").is_file()
                    || repo_root.join("vite.config.js").is_file()
                    || repo_root.join("vite.config.mjs").is_file()
                {
                    p.has_vite = true;
                    p.stacks.push("Vite".to_string());
                }
                if has_dep("react") {
                    p.stacks.push("React".to_string());
                }
                if has_dep("next")
                    || repo_root.join("next.config.js").is_file()
                    || repo_root.join("next.config.mjs").is_file()
                    || repo_root.join("next.config.ts").is_file()
                {
                    p.has_next = true;
                    p.stacks.push("Next.js".to_string());
                }
            }
        }
        if !p.has_vite && !p.has_next {
            p.stacks.push("Node".to_string());
        }
    }

    if p.default_port.is_none() {
        for vc in ["vite.config.ts", "vite.config.js", "vite.config.mjs"] {
            let vp = repo_root.join(vc);
            if vp.is_file() {
                if let Ok(raw) = read_utf8(&vp) {
                    if let Ok(re) = Regex::new(r"port\s*:\s*(\d{2,5})") {
                        if let Some(cap) = re.captures(&raw) {
                            p.default_port =
                                cap.get(1).and_then(|m| m.as_str().parse::<u16>().ok());
                        }
                    }
                }
                break;
            }
        }
    }

    // Rust
    let cargo_rel = if repo_root.join("src-tauri/Cargo.toml").is_file() {
        Some("src-tauri/Cargo.toml")
    } else if repo_root.join("Cargo.toml").is_file() {
        Some("Cargo.toml")
    } else {
        None
    };
    if let Some(rel) = cargo_rel {
        p.has_rust = true;
        p.rust_manifest = Some(rel.to_string());
        p.stacks.push("Rust".to_string());
        if let Ok(raw) = read_utf8(&repo_root.join(rel)) {
            if let Ok(val) = raw.parse::<toml::Value>() {
                if let Some(bins) = val.get("bin").and_then(|b| b.as_array()) {
                    if let Some(name) = bins
                        .first()
                        .and_then(|b| b.get("name"))
                        .and_then(|v| v.as_str())
                    {
                        p.main_exe = Some(format!("{name}.exe"));
                    }
                } else if !p.has_tauri {
                    if let Some(name) = val
                        .get("package")
                        .and_then(|pkg| pkg.get("name"))
                        .and_then(|v| v.as_str())
                    {
                        p.main_exe = Some(format!("{name}.exe"));
                    }
                }
            }
        }
    }

    // Python
    if repo_root.join("pyproject.toml").is_file() || repo_root.join("requirements.txt").is_file() {
        p.has_python = true;
        p.stacks.push("Python".to_string());
    }

    // Extension
    let ext_manifest = repo_root.join("manifest.json");
    if ext_manifest.is_file() {
        if let Ok(raw) = read_utf8(&ext_manifest) {
            if raw.contains("manifest_version") {
                p.stacks.push("Extension".to_string());
            }
        }
    }

    // Android
    if repo_root.join("android/app/build.gradle").is_file()
        || repo_root.join("android/app/build.gradle.kts").is_file()
        || repo_root.join("app/build.gradle.kts").is_file()
    {
        p.stacks.push("Android".to_string());
    }

    // gloria.toml
    let gloria_path = repo_root.join("gloria.toml");
    if gloria_path.is_file() {
        p.has_gloria_toml = true;
        if let Ok(raw) = read_utf8(&gloria_path) {
            if let Ok(val) = raw.parse::<toml::Value>() {
                if let Some(name) = val.get("name").and_then(|v| v.as_str()) {
                    p.app_name = name.to_string();
                }
                if let Some(exe) = val.get("main_exe").and_then(|v| v.as_str()) {
                    p.main_exe = Some(exe.to_string());
                }
                if let Some(wv2) = val.get("needs_webview2").and_then(|v| v.as_bool()) {
                    p.needs_webview2 = wv2;
                }
            }
        }
    }

    if p.main_exe.is_none() && p.has_tauri {
        p.main_exe = Some(format!("{}.exe", p.app_name));
    }

    if p.default_port.is_none() {
        p.default_port = if p.has_tauri {
            Some(1420)
        } else if p.has_vite {
            Some(5173)
        } else if p.has_next || (p.has_node && p.npm_scripts.contains("dev")) {
            Some(3000)
        } else if p.has_python {
            Some(8000)
        } else {
            None
        };
    }

    if p.stacks.is_empty() {
        p.stacks.push("Generic".to_string());
    }

    p
}

fn render_dev_bat(p: &RepoProfile) -> Option<(String, String)> {
    let port = p.default_port;
    let desc = if p.has_tauri {
        "Smart Tauri + Vite dev runner with automatic free-port selection".to_string()
    } else if p.has_vite {
        "Smart Vite dev runner with automatic free-port selection".to_string()
    } else if p.has_next || (p.has_node && p.npm_scripts.contains("dev")) {
        "Smart Node dev runner with automatic free-port selection".to_string()
    } else if p.has_rust {
        "Run Rust binary in dev mode".to_string()
    } else if p.has_python {
        "Smart Python dev runner with automatic free-port selection".to_string()
    } else {
        return None;
    };

    let mut body = String::new();
    if let Some(default_port) = port {
        body.push_str(&format!(
            "$startPort = {default_port}\n\
             $extraArgs = @()\n\
             for ($i = 0; $i -lt $args.Count; $i++) {{\n\
             \t$a = [string]$args[$i]\n\
             \tif ($a -eq '--port' -and ($i + 1) -lt $args.Count) {{\n\
             \t\t$parsed = 0\n\
             \t\tif (-not [int]::TryParse([string]$args[$i + 1], [ref]$parsed) -or $parsed -lt 1 -or $parsed -gt 65535) {{ throw \"Invalid port: $($args[$i + 1])\" }}\n\
             \t\t$startPort = $parsed\n\
             \t\t$i++\n\
             \t}} elseif ($a -like '--port=*') {{\n\
             \t\t$parsed = 0\n\
             \t\tif (-not [int]::TryParse($a.Substring(7), [ref]$parsed) -or $parsed -lt 1 -or $parsed -gt 65535) {{ throw \"Invalid port: $a\" }}\n\
             \t\t$startPort = $parsed\n\
             \t}} else {{\n\
             \t\t$extraArgs += $a\n\
             \t}}\n\
             }}\n\n"
        ));
    }

    if p.has_node {
        body.push_str(
            "if (-not (Test-Path -LiteralPath (Join-Path $RepoRoot 'node_modules'))) {\n\
             \tWrite-Host '[dev] node_modules missing; running npm ci...' -ForegroundColor Yellow\n\
             \t& npm ci\n\
             \tif ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }\n\
             }\n\n",
        );
    }

    if port.is_some() {
        body.push_str(
            "function Test-PortFree([int]$Port) {\n\
             \t$active = [System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners()\n\
             \tforeach ($ep in $active) { if ($ep.Port -eq $Port) { return $false } }\n\
             \t$listener = $null\n\
             \ttry {\n\
             \t\t$listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, $Port)\n\
             \t\t$listener.Start()\n\
             \t\treturn $true\n\
             \t} catch { return $false } finally {\n\
             \t\tif ($listener) { try { $listener.Stop() } catch {} }\n\
             \t}\n\
             }\n\n\
             $chosenPort = $null\n\
             for ($p = $startPort; $p -le ($startPort + 50) -and $p -le 65535; $p++) {\n\
             \tif (Test-PortFree $p) { $chosenPort = $p; break }\n\
             }\n\
             if (-not $chosenPort) { throw \"No free port found in range $startPort..$($startPort + 50).\" }\n\
             if ($chosenPort -ne $startPort) {\n\
             \tWrite-Host \"[dev] Port $startPort is busy -> selected free port $chosenPort\" -ForegroundColor Cyan\n\
             } else {\n\
             \tWrite-Host \"[dev] Using port $chosenPort\" -ForegroundColor Green\n\
             }\n\n",
        );
    }

    if p.has_tauri {
        body.push_str(
            "$tmpConfig = Join-Path $env:TEMP (\"gloria-tauri-dev-$chosenPort-$([guid]::NewGuid().ToString('N').Substring(0,8)).json\")\n\
             $cfgObj = @{ build = @{ devUrl = \"http://localhost:$chosenPort\"; beforeDevCommand = \"npx vite --port $chosenPort --strictPort\" } }\n\
             [System.IO.File]::WriteAllText($tmpConfig, ($cfgObj | ConvertTo-Json -Depth 5), (New-Object System.Text.UTF8Encoding($false)))\n\
             try {\n\
             \t& npx tauri dev --config $tmpConfig @extraArgs\n\
             \texit $LASTEXITCODE\n\
             } finally {\n\
             \tif (Test-Path -LiteralPath $tmpConfig) { Remove-Item -LiteralPath $tmpConfig -Force -ErrorAction SilentlyContinue }\n\
             }\n",
        );
    } else if p.has_vite {
        body.push_str(
            "& npx vite --port $chosenPort --strictPort @extraArgs\n\
             exit $LASTEXITCODE\n",
        );
    } else if p.has_next {
        body.push_str(
            "& npx next dev -p $chosenPort @extraArgs\n\
             exit $LASTEXITCODE\n",
        );
    } else if p.has_node {
        body.push_str(
            "$env:PORT = \"$chosenPort\"\n\
             & npm run dev @extraArgs\n\
             exit $LASTEXITCODE\n",
        );
    } else if p.has_rust {
        let manifest = p.rust_manifest.as_deref().unwrap_or("Cargo.toml");
        let bin_flag = rust_bin_flag(p);
        body.push_str(&format!(
            "& cargo run --manifest-path {manifest} {bin_flag}-- @args\n\
             exit $LASTEXITCODE\n"
        ));
    } else if p.has_python {
        body.push_str(
            "$py = if (Test-Path -LiteralPath '.venv\\Scripts\\python.exe') { '.venv\\Scripts\\python.exe' } else { 'python' }\n\
             $env:PORT = \"$chosenPort\"\n\
             & $py -m uvicorn main:app --reload --port $chosenPort @extraArgs\n\
             exit $LASTEXITCODE\n",
        );
    }

    let bat = wrap_bat(&desc, &body);
    Some((desc, bat))
}

fn render_check_bat(p: &RepoProfile) -> (String, String) {
    let desc = "Run pre-flight build, tests, lint, version manifest sync, and repo checks";
    let mut body = String::from(
        "$results = [System.Collections.Generic.List[object]]::new()\n\
         function Invoke-Step([string]$Name, [scriptblock]$Action) {\n\
         \tWrite-Host \"`n=== [$Name] ===\" -ForegroundColor Cyan\n\
         \t$ok = $false\n\
         \ttry {\n\
         \t\t$global:LASTEXITCODE = 0\n\
         \t\t& $Action\n\
         \t\tif ($LASTEXITCODE -eq 0) { $ok = $true }\n\
         \t} catch {\n\
         \t\tWrite-Host $_ -ForegroundColor Red\n\
         \t\t$ok = $false\n\
         \t}\n\
         \t[void]$results.Add([PSCustomObject]@{ Name = $Name; Passed = $ok })\n\
         }\n\n",
    );

    if p.has_node && p.npm_scripts.contains("build") {
        body.push_str("Invoke-Step 'Frontend build (npm run build)' { & npm run build }\n\n");
    } else if p.has_tsconfig {
        body.push_str("Invoke-Step 'TypeScript check (tsc --noEmit)' { & npx tsc --noEmit }\n\n");
    }
    if p.has_node && p.npm_scripts.contains("lint") {
        body.push_str("Invoke-Step 'Frontend lint (npm run lint)' { & npm run lint }\n\n");
    }
    if p.has_node && p.npm_scripts.contains("test") {
        body.push_str("Invoke-Step 'Frontend tests (npm test)' { & npm test }\n\n");
    }
    if let Some(ref manifest) = p.rust_manifest {
        body.push_str(&format!(
            "Invoke-Step 'Rust tests (cargo test)' {{ & cargo test --manifest-path {manifest} }}\n\n\
             Invoke-Step 'Rust lint (cargo clippy)' {{ & cargo clippy --manifest-path {manifest} --all-targets -- -D warnings }}\n\n"
        ));
    }
    if p.has_python {
        body.push_str(
            "if (Get-Command pytest -ErrorAction SilentlyContinue) { Invoke-Step 'Python tests (pytest)' { & pytest } }\n\n",
        );
    }

    if p.manifests.len() > 1 {
        body.push_str(
            "Invoke-Step 'Version manifest sync' {\n\
             \t$versions = [ordered]@{}\n\
             \tif (Test-Path -LiteralPath 'package.json') { $versions['package.json'] = (Get-Content -Raw -LiteralPath 'package.json' -Encoding UTF8 | ConvertFrom-Json).version }\n\
             \tif (Test-Path -LiteralPath 'package-lock.json') {\n\
             \t\t$raw = Get-Content -Raw -LiteralPath 'package-lock.json' -Encoding UTF8\n\
             \t\t$versions['package-lock.json'] = if ($raw -match '\"version\"\\s*:\\s*\"([^\"]+)\"') { $Matches[1] } else { '<missing>' }\n\
             \t}\n\
             \tforeach ($tc in @('src-tauri/tauri.conf.json', 'tauri.conf.json')) {\n\
             \t\tif (Test-Path -LiteralPath $tc) { $versions[$tc] = (Get-Content -Raw -LiteralPath $tc -Encoding UTF8 | ConvertFrom-Json).version }\n\
             \t}\n\
             \tforeach ($ct in @('src-tauri/Cargo.toml', 'Cargo.toml')) {\n\
             \t\tif (Test-Path -LiteralPath $ct) {\n\
             \t\t\t$raw = Get-Content -Raw -LiteralPath $ct -Encoding UTF8\n\
             \t\t\t$versions[$ct] = if ($raw -match '(?ms)^\\[(?:workspace\\.)?package\\].*?^version\\s*=\\s*\"([^\"]+)\"') { $Matches[1] } else { '<missing>' }\n\
             \t\t}\n\
             \t}\n\
             \tif (Test-Path -LiteralPath 'gloria.toml') {\n\
             \t\t$raw = Get-Content -Raw -LiteralPath 'gloria.toml' -Encoding UTF8\n\
             \t\t$versions['gloria.toml'] = if ($raw -match '(?m)^version\\s*=\\s*\"([^\"]+)\"') { $Matches[1] } else { '<missing>' }\n\
             \t}\n\
             \tforeach ($k in $versions.Keys) { Write-Host (\"  {0,-28} {1}\" -f $k, $versions[$k]) }\n\
             \t$uniq = @($versions.Values | Select-Object -Unique)\n\
             \tif ($uniq.Count -ne 1 -or $uniq[0] -eq '<missing>') { throw 'Version mismatch across manifests!' }\n\
             }\n\n",
        );
    }

    if p.has_gloria_toml {
        body.push_str(
            "Invoke-Step 'GloriaSetup manifest check' { & npx --yes gloria-setup@latest --check --manifest gloria.toml }\n\n",
        );
    }

    body.push_str(
        "$brainBat = if (Test-Path -LiteralPath (Join-Path $RepoRoot 'install-brain.bat')) { Join-Path $RepoRoot 'install-brain.bat' } elseif (Test-Path -LiteralPath (Join-Path $env:USERPROFILE '.brain\\install-brain.bat')) { Join-Path $env:USERPROFILE '.brain\\install-brain.bat' } else { $null }\n\
         if ($brainBat) { Invoke-Step 'Brain sync check' { & $brainBat check . } }\n\n\
         Write-Host \"`n=== Summary ===\" -ForegroundColor Cyan\n\
         $failed = 0\n\
         foreach ($r in $results) {\n\
         \tif ($r.Passed) { Write-Host (\"  [PASS] {0}\" -f $r.Name) -ForegroundColor Green }\n\
         \telse { Write-Host (\"  [FAIL] {0}\" -f $r.Name) -ForegroundColor Red; $failed++ }\n\
         }\n\
         if ($failed -gt 0) { exit 1 }\n\
         exit 0\n",
    );

    (desc.to_string(), wrap_bat(desc, &body))
}

fn render_clean_bat(p: &RepoProfile) -> (String, String) {
    let desc = "Stop locked project processes and clean build/cache directories (--deep, --dry-run)";
    let mut dirs = vec!["dist", "stage"];
    if p.has_tauri {
        dirs.push("src-tauri\\target");
    } else if p.has_rust {
        dirs.push("target");
    }
    if p.has_vite {
        dirs.push("node_modules\\.vite");
    }
    if p.has_next {
        dirs.push(".next");
    }
    if p.has_python {
        dirs.push("__pycache__");
        dirs.push(".pytest_cache");
    }

    let dirs_list = dirs
        .iter()
        .map(|d| format!("'{d}'"))
        .collect::<Vec<_>>()
        .join(", ");

    let deep_add = if p.has_node && p.has_python {
        "$targets += 'node_modules'; $targets += '.venv'"
    } else if p.has_node {
        "$targets += 'node_modules'"
    } else if p.has_python {
        "$targets += '.venv'"
    } else {
        ""
    };

    let body = format!(
        "$deep = $false\n\
         $dryRun = $false\n\
         foreach ($a in $args) {{\n\
         \tif ($a -eq '--deep') {{ $deep = $true }}\n\
         \telseif ($a -eq '--dry-run') {{ $dryRun = $true }}\n\
         }}\n\
         if (-not $dryRun) {{\n\
         \t$stopBat = Join-Path $env:GLORIA_TOOL_DIR 'stop.bat'\n\
         \tif (Test-Path -LiteralPath $stopBat) {{ & cmd.exe /c \"`\"$stopBat`\"\" }}\n\
         }}\n\
         $targets = @({dirs_list})\n\
         if ($deep) {{ {deep_add} }}\n\
         foreach ($rel in $targets) {{\n\
         \t$full = Join-Path $RepoRoot $rel\n\
         \tif (Test-Path -LiteralPath $full) {{\n\
         \t\tif ($dryRun) {{ Write-Host \"[clean --dry-run] Would remove: $rel\" -ForegroundColor Yellow }}\n\
         \t\telse {{ Write-Host \"[clean] Removing $rel...\" -ForegroundColor Cyan; Remove-Item -LiteralPath $full -Recurse -Force }}\n\
         \t}}\n\
         }}\n\
         Write-Host '[clean] Done.' -ForegroundColor Green\n"
    );

    (desc.to_string(), wrap_bat(desc, &body))
}

fn rust_bin_flag(p: &RepoProfile) -> String {
    let Some(ref exe) = p.main_exe else {
        return String::new();
    };
    let stem = exe
        .strip_suffix(".exe")
        .or_else(|| exe.strip_suffix(".EXE"))
        .unwrap_or(exe)
        .trim();
    if stem.is_empty() {
        String::new()
    } else {
        format!("--bin {stem} ")
    }
}

fn render_pack_bat(p: &RepoProfile) -> Option<(String, String)> {
    if !p.has_gloria_toml {
        return None;
    }
    let desc = "Build release binary, stage, pack GloriaSetup installer, and verify /S --dry-run";
    let bin_flag = rust_bin_flag(p);
    let build_cmd = if p.has_tauri {
        "& npx tauri build --no-bundle".to_string()
    } else if p.has_rust {
        format!("& cargo build --release {bin_flag}").trim_end().to_string()
    } else {
        "& npm run build".to_string()
    };
    let bin_rel = if p.has_tauri {
        "src-tauri\\target\\release\\$mainExe"
    } else {
        "target\\release\\$mainExe"
    };

    let body = format!(
        "$gloriaRaw = Get-Content -Raw -LiteralPath 'gloria.toml' -Encoding UTF8\n\
         if ($gloriaRaw -notmatch '(?m)^version\\s*=\\s*\"([^\"]+)\"') {{ throw 'Could not parse version from gloria.toml' }}\n\
         $version = $Matches[1]\n\
         $appName = if ($gloriaRaw -match '(?m)^name\\s*=\\s*\"([^\"]+)\"') {{ $Matches[1] }} else {{ '{app}' }}\n\
         $mainExe = if ($gloriaRaw -match '(?m)^main_exe\\s*=\\s*\"([^\"]+)\"') {{ $Matches[1] }} else {{ '{exe}' }}\n\n\
         Write-Host \"[pack] Validating gloria.toml (v$version)...\" -ForegroundColor Cyan\n\
         & npx --yes gloria-setup@latest --check --manifest gloria.toml --version $version\n\
         if ($LASTEXITCODE -ne 0) {{ exit $LASTEXITCODE }}\n\n\
         Write-Host '[pack] Building release binary...' -ForegroundColor Cyan\n\
         {build_cmd}\n\
         if ($LASTEXITCODE -ne 0) {{ exit $LASTEXITCODE }}\n\n\
         $binPath = Join-Path $RepoRoot \"{bin_rel}\"\n\
         if (-not (Test-Path -LiteralPath $binPath)) {{ throw \"Built binary not found: $binPath\" }}\n\n\
         $stageDir = Join-Path $RepoRoot 'stage'\n\
         if (Test-Path -LiteralPath $stageDir) {{ Remove-Item -LiteralPath $stageDir -Recurse -Force }}\n\
         New-Item -ItemType Directory -Path $stageDir | Out-Null\n\
         Copy-Item -LiteralPath $binPath -Destination (Join-Path $stageDir $mainExe) -Force\n\n\
         $distDir = Join-Path $RepoRoot 'dist'\n\
         New-Item -ItemType Directory -Force -Path $distDir | Out-Null\n\n\
         Write-Host '[pack] Packing installer via gloria-setup@latest...' -ForegroundColor Cyan\n\
         & npx --yes gloria-setup@latest --app $stageDir --manifest gloria.toml --version $version --out $distDir\n\
         if ($LASTEXITCODE -ne 0) {{ exit $LASTEXITCODE }}\n\n\
         $setupExe = Join-Path $distDir \"${{appName}}_${{version}}_x64-setup.exe\"\n\
         if (-not (Test-Path -LiteralPath $setupExe)) {{ throw \"Expected installer not found: $setupExe\" }}\n\n\
         Write-Host \"[pack] Verifying $setupExe with /S --dry-run...\" -ForegroundColor Cyan\n\
         $proc = Start-Process -FilePath $setupExe -ArgumentList '/S --dry-run' -Wait -PassThru\n\
         if ($proc.ExitCode -ne 0) {{ throw \"Installer dry-run failed with exit code $($proc.ExitCode)\" }}\n\
         Write-Host \"[pack] Success: $setupExe (dry-run ExitCode=0)\" -ForegroundColor Green\n",
        app = p.app_name,
        exe = p.main_exe.as_deref().unwrap_or("App.exe"),
    );

    Some((desc.to_string(), wrap_bat(desc, &body)))
}

fn render_setup_bat(p: &RepoProfile) -> (String, String) {
    let desc = "Verify required toolchains and install missing project dependencies";
    let mut tool_rows = Vec::new();
    if p.has_node {
        tool_rows.push("@{ Name = 'node'; Args = @('--version'); Required = $true }");
        tool_rows.push("@{ Name = 'npm'; Args = @('--version'); Required = $true }");
    }
    if p.has_rust {
        tool_rows.push("@{ Name = 'cargo'; Args = @('--version'); Required = $true }");
        tool_rows.push("@{ Name = 'rustc'; Args = @('--version'); Required = $true }");
    }
    if p.has_python {
        tool_rows.push("@{ Name = 'python'; Args = @('--version'); Required = $true }");
    }
    tool_rows.push("@{ Name = 'gh'; Args = @('--version'); Required = $false }");

    let mut body = format!(
        "$missing = @()\n\
         $tools = @({})\n\
         Write-Host '=== Toolchain Check ===' -ForegroundColor Cyan\n\
         foreach ($t in $tools) {{\n\
         \t$cmd = Get-Command $t.Name -ErrorAction SilentlyContinue\n\
         \tif (-not $cmd) {{\n\
         \t\t$tag = if ($t.Required) {{ 'MISSING (required)' }} else {{ 'missing (optional)' }}\n\
         \t\tWrite-Host (\"  {{0,-8}} : {{1}}\" -f $t.Name, $tag) -ForegroundColor $(if ($t.Required) {{ 'Red' }} else {{ 'Yellow' }})\n\
         \t\tif ($t.Required) {{ $missing += $t.Name }}\n\
         \t}} else {{\n\
         \t\t$ver = (& $t.Name @($t.Args) 2>$null | Select-Object -First 1)\n\
         \t\tWrite-Host (\"  {{0,-8}} : {{1}}\" -f $t.Name, $ver) -ForegroundColor Green\n\
         \t}}\n\
         }}\n",
        tool_rows.join(", ")
    );

    if p.needs_webview2 {
        body.push_str(
            "$wv2Keys = @('HKLM:\\SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}','HKLM:\\SOFTWARE\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}','HKCU:\\Software\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}')\n\
             $wv2Ver = $null\n\
             foreach ($k in $wv2Keys) { if (Test-Path $k) { $pv = (Get-ItemProperty -Path $k -ErrorAction SilentlyContinue).pv; if ($pv -and $pv -ne '0.0.0.0') { $wv2Ver = $pv; break } } }\n\
             if ($wv2Ver) { Write-Host (\"  {0,-8} : {1}\" -f 'WebView2', $wv2Ver) -ForegroundColor Green } else { Write-Host '  WebView2 : MISSING' -ForegroundColor Red; $missing += 'WebView2' }\n",
        );
    }

    body.push_str("if ($missing.Count -gt 0) { throw \"Missing required prerequisites: $($missing -join ', ')\" }\n");

    if p.has_node {
        body.push_str(
            "if (-not (Test-Path -LiteralPath (Join-Path $RepoRoot 'node_modules'))) {\n\
             \tWrite-Host '[setup] Installing npm dependencies (npm ci)...' -ForegroundColor Cyan\n\
             \t& npm ci\n\
             \tif ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }\n\
             }\n",
        );
    }
    if p.has_env_example {
        body.push_str(
            "if ((Test-Path -LiteralPath '.env.example') -and (Test-Path -LiteralPath '.env')) {\n\
             \t$ex = Get-Content '.env.example' | Where-Object { $_ -match '^([A-Za-z0-9_]+)=' } | ForEach-Object { $Matches[1] }\n\
             \t$cur = Get-Content '.env' | Where-Object { $_ -match '^([A-Za-z0-9_]+)=' } | ForEach-Object { $Matches[1] }\n\
             \t$diff = $ex | Where-Object { $cur -notcontains $_ }\n\
             \tif ($diff) { Write-Host \"[setup] Missing .env keys: $($diff -join ', ')\" -ForegroundColor Yellow }\n\
             } elseif (Test-Path -LiteralPath '.env.example') {\n\
             \tWrite-Host '[setup] .env missing (copy from .env.example).' -ForegroundColor Yellow\n\
             }\n",
        );
    }
    body.push_str("Write-Host '[setup] Environment is ready.' -ForegroundColor Green\n");

    (desc.to_string(), wrap_bat(desc, &body))
}

fn render_stop_bat(p: &RepoProfile) -> (String, String) {
    let desc = "Stop lingering repo binaries and dev server processes (--port N)";
    let start_port = p.default_port.unwrap_or(3000);
    let end_port = start_port.saturating_add(50);
    let proc_stem = p
        .main_exe
        .as_deref()
        .map(|e| e.trim_end_matches(".exe"))
        .unwrap_or("");

    let exe_block = if !proc_stem.is_empty() {
        format!(
            "Get-Process -Name '{proc_stem}' -ErrorAction SilentlyContinue | ForEach-Object {{\n\
             \t$procPath = $null; try {{ $procPath = $_.Path }} catch {{}}\n\
             \tif ($procPath -and $procPath.StartsWith($RepoRoot, [System.StringComparison]::OrdinalIgnoreCase)) {{\n\
             \t\tWrite-Host \"[stop] Stopping {proc_stem}.exe (PID $($_.Id))...\" -ForegroundColor Yellow\n\
             \t\tStop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue\n\
             \t\t$stopped++\n\
             \t}}\n\
             }}\n"
        )
    } else {
        String::new()
    };

    let body = format!(
        "$ports = {start_port}..{end_port}\n\
         for ($i = 0; $i -lt $args.Count; $i++) {{\n\
         \t$a = [string]$args[$i]\n\
         \tif ($a -eq '--port' -and ($i + 1) -lt $args.Count) {{ $parsed = 0; if ([int]::TryParse([string]$args[$i + 1], [ref]$parsed)) {{ $ports = @($parsed) }}; $i++ }}\n\
         \telseif ($a -like '--port=*') {{ $parsed = 0; if ([int]::TryParse($a.Substring(7), [ref]$parsed)) {{ $ports = @($parsed) }} }}\n\
         }}\n\
         $stopped = 0\n\
         {exe_block}\
         $pidsToCheck = New-Object System.Collections.Generic.HashSet[int]\n\
         if (Get-Command Get-NetTCPConnection -ErrorAction SilentlyContinue) {{\n\
         \tGet-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | Where-Object {{ $ports -contains $_.LocalPort }} | ForEach-Object {{ [void]$pidsToCheck.Add([int]$_.OwningProcess) }}\n\
         }}\n\
         foreach ($procId in $pidsToCheck) {{\n\
         \tif ($procId -le 4) {{ continue }}\n\
         \t$cim = Get-CimInstance Win32_Process -Filter \"ProcessId = $procId\" -ErrorAction SilentlyContinue\n\
         \tif (-not $cim) {{ continue }}\n\
         \t$cmd = [string]$cim.CommandLine\n\
         \tif ($cmd -and ($cmd.IndexOf($RepoRoot, [System.StringComparison]::OrdinalIgnoreCase) -ge 0 -or $cmd -match '(?i)gloria-tauri-dev-')) {{\n\
         \t\tWrite-Host \"[stop] Stopping $($cim.Name) (PID $procId)...\" -ForegroundColor Yellow\n\
         \t\tStop-Process -Id $procId -Force -ErrorAction SilentlyContinue\n\
         \t\t$stopped++\n\
         \t}}\n\
         }}\n\
         if ($stopped -eq 0) {{ Write-Host '[stop] No active repo processes found.' -ForegroundColor DarkGray }} else {{ Write-Host \"[stop] Stopped $stopped process(es).\" -ForegroundColor Green }}\n"
    );

    (desc.to_string(), wrap_bat(desc, &body))
}

fn render_status_bat(p: &RepoProfile) -> (String, String) {
    let desc = "Show git status, version manifest sync, Brain sync, active ports, and cache sizes";
    let start_port = p.default_port.unwrap_or(3000);
    let end_port = start_port.saturating_add(50);

    let body = format!(
        "Write-Host '=== Git ===' -ForegroundColor Cyan\n\
         $branch = (& git branch --show-current 2>$null)\n\
         $dirty = @(& git status --porcelain 2>$null)\n\
         Write-Host (\"  Branch : {{0}}\" -f $(if ($branch) {{ $branch }} else {{ '<detached>' }}))\n\
         Write-Host (\"  Dirty  : {{0}} file(s)\" -f $dirty.Count)\n\n\
         Write-Host \"`n=== Brain Sync ===\" -ForegroundColor Cyan\n\
         $brainBat = if (Test-Path -LiteralPath (Join-Path $RepoRoot 'install-brain.bat')) {{ Join-Path $RepoRoot 'install-brain.bat' }} elseif (Test-Path -LiteralPath (Join-Path $env:USERPROFILE '.brain\\install-brain.bat')) {{ Join-Path $env:USERPROFILE '.brain\\install-brain.bat' }} else {{ $null }}\n\
         if ($brainBat) {{ & $brainBat check . }} else {{ Write-Host '  install-brain.bat not found.' -ForegroundColor DarkGray }}\n\n\
         Write-Host \"`n=== Active Ports ({start_port}..{end_port}) ===\" -ForegroundColor Cyan\n\
         $found = $false\n\
         if (Get-Command Get-NetTCPConnection -ErrorAction SilentlyContinue) {{\n\
         \tGet-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | Where-Object {{ $_.LocalPort -ge {start_port} -and $_.LocalPort -le {end_port} }} | ForEach-Object {{\n\
         \t\t$cim = Get-CimInstance Win32_Process -Filter \"ProcessId = $($_.OwningProcess)\" -ErrorAction SilentlyContinue\n\
         \t\tWrite-Host (\"  Port {{0}} -> PID {{1}} ({{2}})\" -f $_.LocalPort, $_.OwningProcess, $(if ($cim) {{ $cim.Name }} else {{ '?' }}))\n\
         \t\t$found = $true\n\
         \t}}\n\
         }}\n\
         if (-not $found) {{ Write-Host '  None active.' -ForegroundColor DarkGray }}\n"
    );

    (desc.to_string(), wrap_bat(desc, &body))
}

fn render_run_bat(p: &RepoProfile) -> Option<(String, String)> {
    if let Some(ref exe) = p.main_exe {
        let desc = format!("Launch the compiled release {exe} (--build to rebuild first)");
        let rel_bin = if p.has_tauri {
            format!("src-tauri\\target\\release\\{exe}")
        } else {
            format!("target\\release\\{exe}")
        };
        let bin_flag = rust_bin_flag(p);
        let build_cmd = if p.has_tauri {
            "& npx tauri build --no-bundle".to_string()
        } else {
            format!("& cargo build --release {bin_flag}").trim_end().to_string()
        };
        let body = format!(
            "$rebuild = $false\n\
             foreach ($a in $args) {{ if ($a -eq '--build') {{ $rebuild = $true }} }}\n\
             $binPath = Join-Path $RepoRoot '{rel_bin}'\n\
             if ($rebuild -or -not (Test-Path -LiteralPath $binPath)) {{\n\
             \tWrite-Host '[run] Building release binary...' -ForegroundColor Cyan\n\
             \t{build_cmd}\n\
             \tif ($LASTEXITCODE -ne 0) {{ exit $LASTEXITCODE }}\n\
             }}\n\
             if (-not (Test-Path -LiteralPath $binPath)) {{ throw \"Release binary not found: $binPath\" }}\n\
             Write-Host \"[run] Launching $binPath...\" -ForegroundColor Green\n\
             Start-Process -FilePath $binPath -WorkingDirectory $RepoRoot\n"
        );
        return Some((desc.clone(), wrap_bat(&desc, &body)));
    }
    if p.has_vite {
        let desc = "Preview production Vite build (--build to rebuild first)";
        let body = "$rebuild = $false\n\
            foreach ($a in $args) { if ($a -eq '--build') { $rebuild = $true } }\n\
            if ($rebuild -or -not (Test-Path -LiteralPath 'dist')) { & npm run build; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE } }\n\
            & npx vite preview\n";
        return Some((desc.to_string(), wrap_bat(desc, body)));
    }
    None
}

fn render_update_deps_bat(p: &RepoProfile) -> Option<(String, String)> {
    if !p.has_node && !p.has_rust && !p.has_python {
        return None;
    }
    let desc = "Audit outdated/vulnerable dependencies (--apply to update lockfiles)";
    let mut apply_block = String::new();
    let mut audit_block = String::new();

    if p.has_node {
        apply_block.push_str("& npm update; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }\n");
        audit_block.push_str(
            "Write-Host '=== npm outdated ===' -ForegroundColor Cyan; & npm outdated; $global:LASTEXITCODE = 0\n\
             Write-Host \"`n=== npm audit ===\" -ForegroundColor Cyan; & npm audit; $global:LASTEXITCODE = 0\n",
        );
    }
    if let Some(ref manifest) = p.rust_manifest {
        apply_block.push_str(&format!(
            "& cargo update --manifest-path {manifest}; if ($LASTEXITCODE -ne 0) {{ exit $LASTEXITCODE }}\n"
        ));
        audit_block.push_str(&format!(
            "if (Get-Command cargo-outdated -ErrorAction SilentlyContinue) {{ Write-Host \"`n=== cargo outdated ===\" -ForegroundColor Cyan; & cargo outdated --manifest-path {manifest}; $global:LASTEXITCODE = 0 }}\n"
        ));
    }
    if p.has_python {
        audit_block.push_str(
            "Write-Host \"`n=== pip list --outdated ===\" -ForegroundColor Cyan; & python -m pip list --outdated; $global:LASTEXITCODE = 0\n",
        );
    }

    let body = format!(
        "$apply = $false\n\
         foreach ($a in $args) {{ if ($a -eq '--apply') {{ $apply = $true }} }}\n\
         if ($apply) {{\n\
         {apply_block}\
         \tWrite-Host '[update-deps] Done.' -ForegroundColor Green\n\
         \texit 0\n\
         }}\n\
         {audit_block}\
         Write-Host \"`nRun with --apply to update lockfiles.\" -ForegroundColor DarkGray\n"
    );

    Some((desc.to_string(), wrap_bat(desc, &body)))
}

fn render_menu_bat() -> (String, String) {
    let desc = "Interactive launcher for all scripts in gloriaTools/";
    let body = "$ToolDir = $env:GLORIA_TOOL_DIR.TrimEnd('\\', '/')\n\
        while ($true) {\n\
        \t$scripts = Get-ChildItem -LiteralPath $ToolDir -Filter '*.bat' -File | Where-Object { $_.Name -ine 'menu.bat' } | Sort-Object Name\n\
        \tWrite-Host \"`n=== gloriaTools ($RepoRoot) ===\" -ForegroundColor Cyan\n\
        \tfor ($i = 0; $i -lt $scripts.Count; $i++) {\n\
        \t\t$raw = Get-Content -LiteralPath $scripts[$i].FullName -Encoding UTF8 -ErrorAction SilentlyContinue\n\
        \t\t$descLine = $raw | Where-Object { $_ -match '^#\\s*Description:\\s*(.+)$' } | Select-Object -First 1\n\
        \t\t$d = if ($descLine -and $descLine -match '^#\\s*Description:\\s*(.+)$') { $Matches[1].Trim() } else { '' }\n\
        \t\tWrite-Host (\"  [{0,2}] {1,-18} {2}\" -f ($i + 1), $scripts[$i].BaseName, $d)\n\
        \t}\n\
        \tWrite-Host '  [ q] Quit'\n\
        \t$choice = Read-Host \"`nSelect tool\"\n\
        \tif (-not $choice -or $choice.Trim() -ieq 'q') { break }\n\
        \t$idx = 0\n\
        \tif ([int]::TryParse($choice.Trim(), [ref]$idx) -and $idx -ge 1 -and $idx -le $scripts.Count) {\n\
        \t\t$target = $scripts[$idx - 1].FullName\n\
        \t\tWrite-Host \"`n--- Running $($scripts[$idx - 1].Name) ---\" -ForegroundColor Green\n\
        \t\t& cmd.exe /c \"`\"$target`\"\"\n\
        \t\tWrite-Host \"--- Finished (exit $LASTEXITCODE) ---\" -ForegroundColor DarkGray\n\
        \t} else { Write-Host 'Invalid selection.' -ForegroundColor Yellow }\n\
        }\n";
    (desc.to_string(), wrap_bat(desc, body))
}

fn generate_expected_tools(p: &RepoProfile) -> BTreeMap<String, (String, String)> {
    let mut map = BTreeMap::new();
    if let Some(t) = render_dev_bat(p) {
        map.insert("dev.bat".to_string(), t);
    }
    map.insert("check.bat".to_string(), render_check_bat(p));
    map.insert("clean.bat".to_string(), render_clean_bat(p));
    if let Some(t) = render_pack_bat(p) {
        map.insert("pack.bat".to_string(), t);
    }
    map.insert("setup.bat".to_string(), render_setup_bat(p));
    map.insert("stop.bat".to_string(), render_stop_bat(p));
    map.insert("status.bat".to_string(), render_status_bat(p));
    if let Some(t) = render_run_bat(p) {
        map.insert("run.bat".to_string(), t);
    }
    if let Some(t) = render_update_deps_bat(p) {
        map.insert("update-deps.bat".to_string(), t);
    }
    map.insert("menu.bat".to_string(), render_menu_bat());
    map
}

pub fn inspect_repo(repo_path: &Path) -> DnaProjectInspect {
    let profile = detect_profile(repo_path);
    let expected = generate_expected_tools(&profile);
    let tools_dir = repo_path.join("gloriaTools");

    let mut disk_files: BTreeMap<String, PathBuf> = BTreeMap::new();
    if tools_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&tools_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if name.to_ascii_lowercase().ends_with(".bat") {
                            disk_files.insert(name.to_ascii_lowercase(), path);
                        }
                    }
                }
            }
        }
    }

    let mut tools = Vec::new();
    let mut existing_count = 0usize;
    let mut in_sync_count = 0usize;
    let mut outdated_count = 0usize;

    for (file_name, (description, content)) in &expected {
        let key = file_name.to_ascii_lowercase();
        let status = if let Some(disk_path) = disk_files.remove(&key) {
            existing_count += 1;
            let disk_raw = read_utf8(&disk_path).unwrap_or_default();
            if normalize_text(&disk_raw) == normalize_text(content) {
                in_sync_count += 1;
                "IN_SYNC"
            } else {
                outdated_count += 1;
                "OUTDATED"
            }
        } else {
            "NEW"
        };
        tools.push(DnaToolPreview {
            file_name: file_name.clone(),
            description: description.clone(),
            status: status.to_string(),
            content: content.clone(),
        });
    }

    let custom_count = disk_files.len();
    for (_key, custom_path) in disk_files {
        let file_name = custom_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("custom.bat")
            .to_string();
        let raw = read_utf8(&custom_path).unwrap_or_default();
        tools.push(DnaToolPreview {
            file_name,
            description: extract_description(&raw),
            status: "CUSTOM".to_string(),
            content: raw,
        });
    }

    let expected_count = expected.len();
    let status = if existing_count == 0 {
        "MISSING"
    } else if in_sync_count == expected_count {
        "READY"
    } else if outdated_count > 0 {
        "OUTDATED"
    } else {
        "PARTIAL"
    };

    DnaProjectInspect {
        summary: DnaProjectSummary {
            repo: profile.repo_name,
            repo_path: repo_path.to_string_lossy().to_string(),
            stacks: profile.stacks,
            default_port: profile.default_port,
            main_exe: profile.main_exe,
            has_gloria_toml: profile.has_gloria_toml,
            manifests: profile.manifests,
            status: status.to_string(),
            existing_count,
            expected_count,
            custom_count,
        },
        tools,
    }
}

pub fn scan_repos(repo_paths: &[String]) -> Vec<DnaProjectSummary> {
    repo_paths
        .iter()
        .map(|p| inspect_repo(Path::new(p)).summary)
        .collect()
}

pub fn create_tools(repo_path: &Path, tool_names: Option<&[String]>) -> DnaApplyResult {
    let inspect = inspect_repo(repo_path);
    let tools_dir = repo_path.join("gloriaTools");
    if let Err(e) = fs::create_dir_all(&tools_dir) {
        return DnaApplyResult {
            repo_path: repo_path.to_string_lossy().to_string(),
            ok: false,
            written: Vec::new(),
            skipped_custom: Vec::new(),
            error: Some(format!("Failed to create gloriaTools/: {e}")),
        };
    }

    let selected: Option<BTreeSet<String>> = tool_names.map(|list| {
        list.iter()
            .map(|s| s.trim().to_ascii_lowercase())
            .collect()
    });

    let mut written = Vec::new();
    let mut skipped_custom = Vec::new();

    for tool in &inspect.tools {
        if tool.status == "CUSTOM" {
            skipped_custom.push(tool.file_name.clone());
            continue;
        }
        if let Some(ref set) = selected {
            if !set.contains(&tool.file_name.to_ascii_lowercase()) {
                continue;
            }
        }
        let target = tools_dir.join(&tool.file_name);
        if let Err(e) = write_utf8_no_bom(&target, &tool.content) {
            return DnaApplyResult {
                repo_path: repo_path.to_string_lossy().to_string(),
                ok: false,
                written,
                skipped_custom,
                error: Some(format!("Failed writing {}: {e}", tool.file_name)),
            };
        }
        written.push(tool.file_name.clone());
    }

    DnaApplyResult {
        repo_path: repo_path.to_string_lossy().to_string(),
        ok: true,
        written,
        skipped_custom,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("brain-dna-test-{label}-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn tauri_vite_repo_detects_stack_and_transitions_to_ready() {
        let dir = make_temp_dir("tauri");
        fs::create_dir_all(dir.join("src-tauri")).unwrap();
        fs::write(
            dir.join("package.json"),
            r#"{"name":"demo","version":"0.2.0","scripts":{"dev":"vite","build":"tsc && vite build"},"dependencies":{"react":"^18.0.0"},"devDependencies":{"vite":"^5.0.0"}}"#,
        )
        .unwrap();
        fs::write(
            dir.join("src-tauri/tauri.conf.json"),
            r#"{"productName":"DemoApp","version":"0.2.0","build":{"devUrl":"http://localhost:1420"}}"#,
        )
        .unwrap();
        fs::write(
            dir.join("src-tauri/Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.2.0\"\n",
        )
        .unwrap();
        fs::write(
            dir.join("gloria.toml"),
            "id = \"DemoApp\"\nname = \"DemoApp\"\nversion = \"0.2.0\"\nmain_exe = \"DemoApp.exe\"\nneeds_webview2 = true\n",
        )
        .unwrap();

        let before = inspect_repo(&dir);
        assert_eq!(before.summary.status, "MISSING");
        assert_eq!(before.summary.default_port, Some(1420));
        assert_eq!(before.summary.main_exe.as_deref(), Some("DemoApp.exe"));
        assert!(before.summary.stacks.contains(&"Tauri v2".to_string()));
        assert!(before.tools.iter().any(|t| t.file_name == "pack.bat"));
        assert!(before.tools.iter().any(|t| t.file_name == "dev.bat"));

        let res = create_tools(&dir, None);
        assert!(res.ok);
        assert!(!res.written.is_empty());

        let after = inspect_repo(&dir);
        assert_eq!(after.summary.status, "READY");
        assert_eq!(after.summary.existing_count, after.summary.expected_count);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn python_repo_preserves_custom_script_and_skips_pack() {
        let dir = make_temp_dir("python");
        fs::write(dir.join("pyproject.toml"), "[project]\nname = \"pytool\"\n").unwrap();
        fs::create_dir_all(dir.join("gloriaTools")).unwrap();
        fs::write(
            dir.join("gloriaTools/custom.bat"),
            "@echo off\n# Description: My custom helper\n",
        )
        .unwrap();

        let inspect = inspect_repo(&dir);
        assert!(inspect.summary.stacks.contains(&"Python".to_string()));
        assert_eq!(inspect.summary.default_port, Some(8000));
        assert!(!inspect.tools.iter().any(|t| t.file_name == "pack.bat"));
        assert_eq!(inspect.summary.custom_count, 1);

        let res = create_tools(&dir, None);
        assert!(res.ok);
        assert_eq!(res.skipped_custom, vec!["custom.bat".to_string()]);
        assert!(dir.join("gloriaTools/custom.bat").is_file());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cargo_workspace_uses_bin_flag_from_gloria_toml() {
        let dir = make_temp_dir("workspace");
        fs::write(
            dir.join("Cargo.toml"),
            "[workspace]\nmembers = [\"apps/dock\"]\n\n[workspace.package]\nversion = \"0.1.25\"\n",
        )
        .unwrap();
        fs::write(
            dir.join("gloria.toml"),
            "id = \"Navalia\"\nname = \"Navalia\"\nversion = \"0.1.25\"\nmain_exe = \"navalia.exe\"\n",
        )
        .unwrap();

        let inspect = inspect_repo(&dir);
        let dev = inspect.tools.iter().find(|t| t.file_name == "dev.bat").unwrap();
        let pack = inspect.tools.iter().find(|t| t.file_name == "pack.bat").unwrap();
        let check = inspect.tools.iter().find(|t| t.file_name == "check.bat").unwrap();

        assert!(dev.content.contains("cargo run --manifest-path Cargo.toml --bin navalia -- @args"));
        assert!(pack.content.contains("cargo build --release --bin navalia"));
        assert!(check.content.contains("(?:workspace\\.)?package"));

        let _ = fs::remove_dir_all(&dir);
    }
}
