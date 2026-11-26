//! Native, zero-AI-token release runner for projectGloria repositories.
//! Executes preflight checks, bumps version manifests & lockfiles, validates
//! `gloria.toml` locally, commits & pushes to `main`, dispatches `release.yml`,
//! streams `gh run watch`, and reports the live GitHub Release URL & assets.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

use crate::brain_core::{read_utf8, write_utf8_no_bom};
use crate::clis;

pub const EVENT_STEP: &str = "release-step";
pub const EVENT_LOG: &str = "release-log";
pub const EVENT_DONE: &str = "release-done";

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const SHORT_TIMEOUT: Duration = Duration::from_secs(30);
const LOCKFILE_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Debug, Clone, Serialize)]
pub struct ReleaseVersionFile {
    pub path: String,
    pub version: String,
    pub matches_primary: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReleasePreflight {
    pub repo_name: String,
    pub repo_path: String,
    pub current_version: Option<String>,
    pub suggested_version: String,
    pub version_files: Vec<ReleaseVersionFile>,
    pub git_branch: Option<String>,
    pub dirty_files: Vec<String>,
    pub last_release: Option<String>,
    pub existing_tags: Vec<String>,
    /// "ready" | "custom" | "missing"
    pub workflow_status: String,
    pub has_gloria_toml: bool,
    pub is_gloria_setup: bool,
    pub gh_available: bool,
    pub dev_self_release: bool,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseRequest {
    pub run_id: String,
    pub repo_path: String,
    pub version: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReleaseStarted {
    pub run_id: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize)]
struct StepEvent<'a> {
    run_id: &'a str,
    step_id: &'a str,
    status: &'a str,
    detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct LogEvent<'a> {
    run_id: &'a str,
    /// "out" | "err" | "info"
    stream: &'a str,
    text: String,
}

#[derive(Debug, Clone, Serialize)]
struct DoneEvent<'a> {
    run_id: &'a str,
    ok: bool,
    cancelled: bool,
    old_version: Option<String>,
    new_version: String,
    run_url: Option<String>,
    release_url: Option<String>,
    assets: Vec<String>,
    error: Option<String>,
}

struct ReleaseHandle {
    current_pid: Arc<Mutex<Option<u32>>>,
    cancelled: Arc<AtomicBool>,
}

fn running() -> &'static Mutex<HashMap<String, ReleaseHandle>> {
    static RUNS: OnceLock<Mutex<HashMap<String, ReleaseHandle>>> = OnceLock::new();
    RUNS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn emit_step(app: &AppHandle, run_id: &str, step_id: &str, status: &str, detail: Option<String>) {
    let _ = app.emit(
        EVENT_STEP,
        StepEvent {
            run_id,
            step_id,
            status,
            detail,
        },
    );
}

fn emit_log(app: &AppHandle, run_id: &str, stream: &'static str, text: String) {
    let _ = app.emit(
        EVENT_LOG,
        LogEvent {
            run_id,
            stream,
            text,
        },
    );
}

fn build_cmd(program: &str, args: &[&str], cwd: &Path) -> Command {
    #[cfg(windows)]
    let mut cmd = {
        let is_cmd_shim = program.eq_ignore_ascii_case("npm") || program.eq_ignore_ascii_case("npx");
        if is_cmd_shim {
            let mut c = Command::new("cmd.exe");
            c.arg("/C").arg(program);
            c.args(args);
            c
        } else {
            let mut c = Command::new(program);
            c.args(args);
            c
        }
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = Command::new(program);
        c.args(args);
        c
    };

    cmd.current_dir(cwd)
        .env("NO_COLOR", "1")
        .env("FORCE_COLOR", "0")
        .stdin(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    cmd
}

fn run_capture_in_dir(
    cwd: &Path,
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<(Option<i32>, String, String, bool), String> {
    let mut cmd = build_cmd(program, args, cwd);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to start '{program}': {e}"))?;

    let out_pipe = child.stdout.take();
    let err_pipe = child.stderr.take();

    let out_t = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut p) = out_pipe {
            let _ = p.read_to_end(&mut buf);
        }
        clis::clean_output(&buf)
    });
    let err_t = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut p) = err_pipe {
            let _ = p.read_to_end(&mut buf);
        }
        clis::clean_output(&buf)
    });

    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) => {
                if start.elapsed() >= timeout {
                    timed_out = true;
                    let _ = kill_tree(child.id());
                    break child.wait().ok();
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("Failed waiting for '{program}': {e}")),
        }
    };

    let stdout = out_t.join().unwrap_or_default();
    let stderr = err_t.join().unwrap_or_default();
    Ok((status.and_then(|s| s.code()), stdout, stderr, timed_out))
}

fn pump_stream<R: Read + Send + 'static>(
    app: AppHandle,
    run_id: String,
    pipe: R,
    stream: &'static str,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(pipe);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            let line = clis::clean_output(&buf);
            if line.trim().is_empty() {
                continue;
            }
            for l in line.split('\n') {
                if !l.trim().is_empty() {
                    emit_log(&app, &run_id, stream, l.to_string());
                }
            }
        }
    })
}

fn run_streaming(
    app: &AppHandle,
    run_id: &str,
    current_pid: &Arc<Mutex<Option<u32>>>,
    cancelled: &Arc<AtomicBool>,
    cwd: &Path,
    program: &str,
    args: &[&str],
) -> Result<i32, String> {
    if cancelled.load(Ordering::Relaxed) {
        return Err("Cancelled.".to_string());
    }
    emit_log(app, run_id, "info", format!("$ {program} {}", args.join(" ")));

    let mut cmd = build_cmd(program, args, cwd);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to start '{program}': {e}"))?;

    if let Ok(mut slot) = current_pid.lock() {
        *slot = Some(child.id());
    }

    let out_t = child
        .stdout
        .take()
        .map(|p| pump_stream(app.clone(), run_id.to_string(), p, "out"));
    let err_t = child
        .stderr
        .take()
        .map(|p| pump_stream(app.clone(), run_id.to_string(), p, "err"));

    let status = child
        .wait()
        .map_err(|e| format!("Failed waiting for '{program}': {e}"));

    if let Ok(mut slot) = current_pid.lock() {
        *slot = None;
    }

    for t in [out_t, err_t].into_iter().flatten() {
        let _ = t.join();
    }

    let status = status?;
    if cancelled.load(Ordering::Relaxed) {
        return Err("Cancelled.".to_string());
    }
    Ok(status.code().unwrap_or(-1))
}

fn kill_tree(pid: u32) -> Result<(), String> {
    #[cfg(windows)]
    {
        let pid_s = pid.to_string();
        clis::run_capture_pub(
            Path::new("taskkill.exe"),
            &["/PID", &pid_s, "/T", "/F"],
            Duration::from_secs(15),
        )
        .map(|_| ())
    }
    #[cfg(not(windows))]
    {
        Command::new("kill")
            .arg("-9")
            .arg(pid.to_string())
            .status()
            .map(|_| ())
            .map_err(|e| format!("Failed to stop process {pid}: {e}"))
    }
}

pub fn cancel(run_id: &str) -> Result<bool, String> {
    let pid = {
        let map = running()
            .lock()
            .map_err(|_| "Release table is poisoned.".to_string())?;
        let Some(h) = map.get(run_id) else {
            return Ok(false);
        };
        h.cancelled.store(true, Ordering::Relaxed);
        let pid = *h.current_pid.lock().unwrap_or_else(|e| e.into_inner());
        pid
    };
    if let Some(p) = pid {
        let _ = kill_tree(p);
    }
    Ok(true)
}

pub fn cancel_all() {
    let pids: Vec<u32> = running()
        .lock()
        .map(|map| {
            map.values()
                .filter_map(|h| {
                    h.cancelled.store(true, Ordering::Relaxed);
                    *h.current_pid.lock().unwrap_or_else(|e| e.into_inner())
                })
                .collect()
        })
        .unwrap_or_default();
    for pid in pids {
        let _ = kill_tree(pid);
    }
}

pub fn normalize_version(raw: &str) -> String {
    raw.trim()
        .trim_start_matches(['v', 'V'])
        .trim()
        .to_string()
}

pub fn validate_version(v: &str) -> Result<(), String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$").expect("valid semver regex")
    });
    if v.is_empty() {
        return Err("Version cannot be empty.".to_string());
    }
    if !re.is_match(v) {
        return Err(format!(
            "Invalid version '{v}'. Expected semver like 1.2.0 or 1.2.0-alpha.3."
        ));
    }
    Ok(())
}

pub fn suggest_next_version(current: Option<&str>) -> String {
    let Some(cur) = current.map(normalize_version).filter(|s| !s.is_empty()) else {
        return "0.1.0".to_string();
    };
    if let Some((base, pre)) = cur.split_once('-') {
        if let Some((prefix, tail)) = pre.rsplit_once('.') {
            if let Ok(num) = tail.parse::<u64>() {
                return format!("{base}-{prefix}.{}", num + 1);
            }
        }
        return format!("{base}-{pre}.1");
    }
    let parts: Vec<&str> = cur.split('.').collect();
    if parts.len() == 3 {
        if let Ok(patch) = parts[2].parse::<u64>() {
            return format!("{}.{}.{}", parts[0], parts[1], patch + 1);
        }
    }
    "0.1.0".to_string()
}

pub fn numeric_version_prefix(v: &str) -> String {
    v.split('-').next().unwrap_or(v).to_string()
}

fn is_gloria_setup_repo(repo_root: &Path) -> bool {
    if !repo_root.join("crates/gloria-pack").is_dir() {
        return false;
    }
    let npm_pkg = repo_root.join("npm/package.json");
    if let Ok(raw) = read_utf8(&npm_pkg) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
            return val.get("name").and_then(|v| v.as_str()) == Some("gloria-setup");
        }
    }
    false
}

fn read_toml_section_version(raw: &str, sections: &[&str]) -> Option<String> {
    let mut current_section = String::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') && !trimmed.starts_with("[[") {
            current_section = trimmed[1..trimmed.len() - 1].trim().to_string();
            continue;
        }
        if sections.contains(&current_section.as_str()) {
            if let Some(rest) = trimmed.strip_prefix("version") {
                let rest = rest.trim_start();
                if let Some(rest) = rest.strip_prefix('=') {
                    let val = rest.trim();
                    if let Some(inner) = val.strip_prefix('"').and_then(|s| s.split('"').next()) {
                        return Some(inner.to_string());
                    }
                }
            }
        }
    }
    None
}

fn replace_toml_section_version(raw: &str, sections: &[&str], new_version: &str) -> Option<String> {
    let line_ending = if raw.contains("\r\n") { "\r\n" } else { "\n" };
    let re = Regex::new(r#"^(\s*version\s*=\s*")([^"]+)(".*)$"#).ok()?;
    let mut current_section = String::new();
    let mut changed = false;
    let mut out_lines: Vec<String> = Vec::new();

    for line in raw.split('\n') {
        let line_clean = line.strip_suffix('\r').unwrap_or(line);
        let trimmed = line_clean.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') && !trimmed.starts_with("[[") {
            current_section = trimmed[1..trimmed.len() - 1].trim().to_string();
            out_lines.push(line_clean.to_string());
            continue;
        }
        if !changed && sections.contains(&current_section.as_str()) {
            if let Some(cap) = re.captures(line_clean) {
                out_lines.push(format!("{}{new_version}{}", &cap[1], &cap[3]));
                changed = true;
                continue;
            }
        }
        out_lines.push(line_clean.to_string());
    }

    if changed {
        Some(out_lines.join(line_ending))
    } else {
        None
    }
}

fn read_json_version(raw: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|v| v.get("version").and_then(|s| s.as_str()).map(str::to_string))
}

fn replace_first_json_version(raw: &str, new_version: &str) -> Option<String> {
    let re = Regex::new(r#"("version"\s*:\s*")([^"]+)(")"#).ok()?;
    if !re.is_match(raw) {
        return None;
    }
    Some(
        re.replace(raw, |caps: &regex::Captures| {
            format!("{}{new_version}{}", &caps[1], &caps[3])
        })
        .into_owned(),
    )
}

fn detect_version_files(repo_root: &Path, is_gloria_setup: bool) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = Vec::new();

    if is_gloria_setup {
        if let Ok(raw) = read_utf8(&repo_root.join("Cargo.toml")) {
            if let Some(v) = read_toml_section_version(&raw, &["workspace.package"]) {
                found.push(("Cargo.toml".to_string(), v));
            }
        }
        if let Ok(raw) = read_utf8(&repo_root.join("npm/package.json")) {
            if let Some(v) = read_json_version(&raw) {
                found.push(("npm/package.json".to_string(), v));
            }
        }
        if let Ok(raw) = read_utf8(&repo_root.join("README.md")) {
            if let Ok(re) = Regex::new(r"projectGloria/GloriaSetup@v([0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?)") {
                if let Some(cap) = re.captures(&raw) {
                    found.push(("README.md".to_string(), cap[1].to_string()));
                }
            }
        }
        return found;
    }

    // Priority order determines primary version: gloria.toml > package.json > tauri.conf > Cargo.toml > others
    let gloria_path = repo_root.join("gloria.toml");
    if gloria_path.is_file() {
        if let Ok(raw) = read_utf8(&gloria_path) {
            if let Ok(re) = Regex::new(r#"(?m)^version\s*=\s*"([^"]+)""#) {
                if let Some(cap) = re.captures(&raw) {
                    found.push(("gloria.toml".to_string(), cap[1].to_string()));
                }
            }
        }
    }

    for rel in ["package.json", "package-lock.json", "src-tauri/tauri.conf.json", "tauri.conf.json"] {
        let p = repo_root.join(rel);
        if p.is_file() {
            if let Ok(raw) = read_utf8(&p) {
                if let Some(v) = read_json_version(&raw) {
                    found.push((rel.to_string(), v));
                }
            }
        }
    }

    for rel in ["src-tauri/Cargo.toml", "Cargo.toml"] {
        let p = repo_root.join(rel);
        if p.is_file() {
            if let Ok(raw) = read_utf8(&p) {
                if let Some(v) = read_toml_section_version(&raw, &["package", "workspace.package"]) {
                    found.push((rel.to_string(), v));
                }
            }
        }
    }

    let ext_manifest = repo_root.join("manifest.json");
    if ext_manifest.is_file() {
        if let Ok(raw) = read_utf8(&ext_manifest) {
            if raw.contains("manifest_version") {
                if let Some(v) = read_json_version(&raw) {
                    found.push(("manifest.json".to_string(), v));
                }
            }
        }
    }

    if let Ok(gradle_re) = Regex::new(r#"versionName\s*=?\s*"([^"]+)""#) {
        for rel in [
            "android/app/build.gradle",
            "android/app/build.gradle.kts",
            "app/build.gradle.kts",
        ] {
            let p = repo_root.join(rel);
            if p.is_file() {
                if let Ok(raw) = read_utf8(&p) {
                    if let Some(cap) = gradle_re.captures(&raw) {
                        found.push((rel.to_string(), cap[1].to_string()));
                    }
                }
            }
        }
    }

    let py_path = repo_root.join("pyproject.toml");
    if py_path.is_file() {
        if let Ok(raw) = read_utf8(&py_path) {
            if let Some(v) = read_toml_section_version(&raw, &["project"]) {
                found.push(("pyproject.toml".to_string(), v));
            }
        }
    }

    let pub_path = repo_root.join("pubspec.yaml");
    if pub_path.is_file() {
        if let Ok(raw) = read_utf8(&pub_path) {
            if let Ok(re) = Regex::new(r"(?m)^version:\s*([^\s#]+)") {
                if let Some(cap) = re.captures(&raw) {
                    found.push(("pubspec.yaml".to_string(), cap[1].to_string()));
                }
            }
        }
    }

    found
}

pub fn preflight(repo_path: &Path) -> Result<ReleasePreflight, String> {
    if !repo_path.is_dir() {
        return Err(format!("Repository '{}' does not exist.", repo_path.display()));
    }
    let repo_name = repo_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project")
        .to_string();

    let is_gloria_setup = is_gloria_setup_repo(repo_path);
    let has_gloria_toml = repo_path.join("gloria.toml").is_file();
    let raw_files = detect_version_files(repo_path, is_gloria_setup);
    let current_version = raw_files.first().map(|(_, v)| v.clone());
    let primary_numeric = current_version.as_deref().map(numeric_version_prefix);

    let version_files = raw_files
        .into_iter()
        .map(|(path, version)| {
            let matches_primary = match (&current_version, &primary_numeric) {
                (Some(primary), Some(num)) => {
                    if path == "manifest.json" {
                        &version == num
                    } else {
                        &version == primary
                    }
                }
                _ => true,
            };
            ReleaseVersionFile {
                path,
                version,
                matches_primary,
            }
        })
        .collect();

    let suggested_version = suggest_next_version(current_version.as_deref());

    let mut blockers = Vec::new();

    // Git branch & dirty files
    let is_git_repo = repo_path.join(".git").exists();
    let mut git_branch = None;
    let mut dirty_files = Vec::new();
    if is_git_repo {
        if let Ok((Some(0), out, _, false)) =
            run_capture_in_dir(repo_path, "git", &["branch", "--show-current"], SHORT_TIMEOUT)
        {
            let b = out.trim().to_string();
            if !b.is_empty() {
                if b != "main" {
                    blockers.push(format!("Current branch is '{b}' (releases must run on 'main')."));
                }
                git_branch = Some(b);
            } else {
                blockers.push("Git HEAD is detached; switch to 'main' before releasing.".to_string());
            }
        }
        if let Ok((Some(0), out, _, false)) =
            run_capture_in_dir(repo_path, "git", &["status", "--short"], SHORT_TIMEOUT)
        {
            dirty_files = out
                .lines()
                .map(|l| l.trim_end().to_string())
                .filter(|l| !l.trim().is_empty())
                .collect();
        }
    } else {
        blockers.push("Not a git repository.".to_string());
    }

    // Workflow status
    let wf_path = repo_path.join(".github/workflows/release.yml");
    let workflow_status = if !wf_path.is_file() {
        blockers.push("Missing .github/workflows/release.yml.".to_string());
        "missing".to_string()
    } else if is_gloria_setup {
        "ready".to_string()
    } else if let Ok(raw) = read_utf8(&wf_path) {
        if raw.contains("GloriaSetup/.github/workflows/app-check.yml@main") {
            "ready".to_string()
        } else {
            "custom".to_string()
        }
    } else {
        "custom".to_string()
    };

    // gh CLI + recent releases
    let gh_available = run_capture_in_dir(repo_path, "gh", &["--version"], SHORT_TIMEOUT)
        .map(|(c, _, _, t)| !t && c == Some(0))
        .unwrap_or(false);
    if !gh_available {
        blockers.push("GitHub CLI (gh) is not installed or not on PATH.".to_string());
    }

    let mut existing_tags = Vec::new();
    let mut last_release = None;
    if gh_available {
        if let Ok((Some(0), out, _, false)) = run_capture_in_dir(
            repo_path,
            "gh",
            &["release", "list", "--limit", "15", "--json", "tagName"],
            SHORT_TIMEOUT,
        ) {
            if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&out) {
                for item in arr {
                    if let Some(tag) = item.get("tagName").and_then(|t| t.as_str()) {
                        existing_tags.push(tag.to_string());
                    }
                }
                last_release = existing_tags.first().cloned();
            }
        }
    }

    let dev_self_release = if cfg!(debug_assertions) {
        let target_dir = repo_path.join("src-tauri").join("target");
        let norm = |p: &Path| {
            std::fs::canonicalize(p)
                .unwrap_or_else(|_| p.to_path_buf())
                .to_string_lossy()
                .trim_start_matches(r"\\?\")
                .to_ascii_lowercase()
        };
        std::env::current_exe()
            .ok()
            .is_some_and(|exe| norm(&exe).starts_with(&norm(&target_dir)))
    } else {
        false
    };

    Ok(ReleasePreflight {
        repo_name,
        repo_path: repo_path.to_string_lossy().to_string(),
        current_version,
        suggested_version,
        version_files,
        git_branch,
        dirty_files,
        last_release,
        existing_tags,
        workflow_status,
        has_gloria_toml,
        is_gloria_setup,
        gh_available,
        dev_self_release,
        blockers,
    })
}

pub fn bump_version_files(
    repo_root: &Path,
    new_version: &str,
    run_lockfile_cmds: bool,
) -> Result<Vec<String>, String> {
    let is_gloria_setup = is_gloria_setup_repo(repo_root);
    let mut updated = Vec::new();

    if is_gloria_setup {
        let cargo_path = repo_root.join("Cargo.toml");
        if cargo_path.is_file() {
            let raw = read_utf8(&cargo_path)?;
            if let Some(next) = replace_toml_section_version(&raw, &["workspace.package"], new_version) {
                write_utf8_no_bom(&cargo_path, &next)?;
                updated.push("Cargo.toml".to_string());
            }
        }
        let npm_pkg = repo_root.join("npm/package.json");
        if npm_pkg.is_file() {
            let raw = read_utf8(&npm_pkg)?;
            if let Some(next) = replace_first_json_version(&raw, new_version) {
                write_utf8_no_bom(&npm_pkg, &next)?;
                updated.push("npm/package.json".to_string());
            }
        }
        let readme = repo_root.join("README.md");
        if readme.is_file() {
            let raw = read_utf8(&readme)?;
            let re = Regex::new(
                r"projectGloria/GloriaSetup@v[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?",
            )
            .map_err(|e| e.to_string())?;
            if re.is_match(&raw) {
                let next = re
                    .replace_all(&raw, format!("projectGloria/GloriaSetup@v{new_version}"))
                    .into_owned();
                write_utf8_no_bom(&readme, &next)?;
                updated.push("README.md".to_string());
            }
        }
        if run_lockfile_cmds && repo_root.join("Cargo.lock").is_file() {
            let (code, _, err, timed_out) = run_capture_in_dir(
                repo_root,
                "cargo",
                &["update", "--workspace", "--offline"],
                LOCKFILE_TIMEOUT,
            )?;
            if timed_out || code != Some(0) {
                return Err(format!("cargo update --workspace --offline failed: {err}"));
            }
            updated.push("Cargo.lock".to_string());
        }
        return Ok(updated);
    }

    // gloria.toml
    let gloria_path = repo_root.join("gloria.toml");
    if gloria_path.is_file() {
        let raw = read_utf8(&gloria_path)?;
        let re = Regex::new(r#"(?m)^(version\s*=\s*")([^"]+)(")"#).map_err(|e| e.to_string())?;
        if re.is_match(&raw) {
            let next = re
                .replace(&raw, |c: &regex::Captures| format!("{}{new_version}{}", &c[1], &c[3]))
                .into_owned();
            write_utf8_no_bom(&gloria_path, &next)?;
            updated.push("gloria.toml".to_string());
        }
    }

    // package.json + package-lock.json
    let pkg_path = repo_root.join("package.json");
    if pkg_path.is_file() {
        let raw = read_utf8(&pkg_path)?;
        if let Some(next) = replace_first_json_version(&raw, new_version) {
            write_utf8_no_bom(&pkg_path, &next)?;
            updated.push("package.json".to_string());
        }
        if run_lockfile_cmds && repo_root.join("package-lock.json").is_file() {
            let (code, _, err, timed_out) = run_capture_in_dir(
                repo_root,
                "npm",
                &[
                    "version",
                    new_version,
                    "--no-git-tag-version",
                    "--allow-same-version",
                ],
                LOCKFILE_TIMEOUT,
            )?;
            if timed_out || code != Some(0) {
                let (c2, _, err2, t2) = run_capture_in_dir(
                    repo_root,
                    "npm",
                    &["install", "--package-lock-only"],
                    LOCKFILE_TIMEOUT,
                )?;
                if t2 || c2 != Some(0) {
                    return Err(format!(
                        "Failed updating package-lock.json: {err} / {err2}"
                    ));
                }
            }
            updated.push("package-lock.json".to_string());
        }
    }

    // tauri.conf.json
    for rel in ["src-tauri/tauri.conf.json", "tauri.conf.json"] {
        let p = repo_root.join(rel);
        if p.is_file() {
            let raw = read_utf8(&p)?;
            if let Some(next) = replace_first_json_version(&raw, new_version) {
                write_utf8_no_bom(&p, &next)?;
                updated.push(rel.to_string());
            }
        }
    }

    // Cargo.toml + Cargo.lock
    for rel in ["src-tauri/Cargo.toml", "Cargo.toml"] {
        let p = repo_root.join(rel);
        if p.is_file() {
            let raw = read_utf8(&p)?;
            if let Some(next) =
                replace_toml_section_version(&raw, &["package", "workspace.package"], new_version)
            {
                write_utf8_no_bom(&p, &next)?;
                updated.push(rel.to_string());
            }
            if run_lockfile_cmds {
                if let Some(cargo_dir) = p.parent() {
                    if cargo_dir.join("Cargo.lock").is_file() {
                        let (code, _, err, timed_out) = run_capture_in_dir(
                            cargo_dir,
                            "cargo",
                            &["update", "--workspace", "--offline"],
                            LOCKFILE_TIMEOUT,
                        )?;
                        if timed_out || code != Some(0) {
                            return Err(format!(
                                "cargo update --workspace --offline failed in {}: {err}",
                                cargo_dir.display()
                            ));
                        }
                        let lock_rel = if rel.starts_with("src-tauri/") {
                            "src-tauri/Cargo.lock"
                        } else {
                            "Cargo.lock"
                        };
                        updated.push(lock_rel.to_string());
                    }
                }
            }
        }
    }

    // Chrome extension manifest.json (numbers only)
    let ext_manifest = repo_root.join("manifest.json");
    if ext_manifest.is_file() {
        let raw = read_utf8(&ext_manifest)?;
        if raw.contains("manifest_version") {
            let numeric = numeric_version_prefix(new_version);
            if let Some(next) = replace_first_json_version(&raw, &numeric) {
                write_utf8_no_bom(&ext_manifest, &next)?;
                updated.push("manifest.json".to_string());
            }
        }
    }

    // Android build.gradle / build.gradle.kts
    let gradle_re = Regex::new(r#"(versionName\s*=?\s*")([^"]+)(")"#).map_err(|e| e.to_string())?;
    for rel in [
        "android/app/build.gradle",
        "android/app/build.gradle.kts",
        "app/build.gradle.kts",
    ] {
        let p = repo_root.join(rel);
        if p.is_file() {
            let raw = read_utf8(&p)?;
            if gradle_re.is_match(&raw) {
                let next = gradle_re
                    .replace(&raw, |c: &regex::Captures| format!("{}{new_version}{}", &c[1], &c[3]))
                    .into_owned();
                write_utf8_no_bom(&p, &next)?;
                updated.push(rel.to_string());
            }
        }
    }

    // pyproject.toml
    let py_path = repo_root.join("pyproject.toml");
    if py_path.is_file() {
        let raw = read_utf8(&py_path)?;
        if let Some(next) = replace_toml_section_version(&raw, &["project"], new_version) {
            write_utf8_no_bom(&py_path, &next)?;
            updated.push("pyproject.toml".to_string());
        }
    }

    // pubspec.yaml
    let pub_path = repo_root.join("pubspec.yaml");
    if pub_path.is_file() {
        let raw = read_utf8(&pub_path)?;
        let re = Regex::new(r"(?m)^(version:\s*)([^\s#]+)").map_err(|e| e.to_string())?;
        if re.is_match(&raw) {
            let next = re
                .replace(&raw, |c: &regex::Captures| format!("{}{new_version}", &c[1]))
                .into_owned();
            write_utf8_no_bom(&pub_path, &next)?;
            updated.push("pubspec.yaml".to_string());
        }
    }

    Ok(updated)
}

fn find_workflow_run(repo: &Path, head_sha: &str) -> Option<(u64, Option<String>)> {
    let (code, out, _, timed_out) = run_capture_in_dir(
        repo,
        "gh",
        &[
            "run",
            "list",
            "--workflow",
            "release.yml",
            "--limit",
            "5",
            "--json",
            "databaseId,headSha,url",
        ],
        SHORT_TIMEOUT,
    )
    .ok()?;
    if timed_out || code != Some(0) {
        return None;
    }
    let arr: Vec<serde_json::Value> = serde_json::from_str(&out).ok()?;
    for item in arr {
        if item.get("headSha").and_then(|s| s.as_str()) == Some(head_sha) {
            let id = item.get("databaseId").and_then(|v| v.as_u64())?;
            let url = item.get("url").and_then(|v| v.as_str()).map(str::to_string);
            return Some((id, url));
        }
    }
    None
}

fn fetch_release_details(repo: &Path, tag: &str) -> (Option<String>, Vec<String>) {
    let Ok((Some(0), out, _, false)) = run_capture_in_dir(
        repo,
        "gh",
        &["release", "view", tag, "--json", "url,assets"],
        SHORT_TIMEOUT,
    ) else {
        return (None, Vec::new());
    };
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&out) else {
        return (None, Vec::new());
    };
    let url = val.get("url").and_then(|u| u.as_str()).map(str::to_string);
    let assets = val
        .get("assets")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.get("name").and_then(|n| n.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    (url, assets)
}

pub fn start(app: AppHandle, req: ReleaseRequest) -> Result<ReleaseStarted, String> {
    let run_id = req.run_id.trim().to_string();
    if run_id.is_empty() {
        return Err("Missing run id.".to_string());
    }
    let repo = PathBuf::from(req.repo_path.trim());
    if !repo.is_dir() {
        return Err(format!("Repository folder '{}' does not exist.", repo.display()));
    }
    let new_version = normalize_version(&req.version);
    validate_version(&new_version)?;

    let current_pid = Arc::new(Mutex::new(None));
    let cancelled = Arc::new(AtomicBool::new(false));

    if let Ok(mut map) = running().lock() {
        map.insert(
            run_id.clone(),
            ReleaseHandle {
                current_pid: current_pid.clone(),
                cancelled: cancelled.clone(),
            },
        );
    }

    let note = req.note.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let version_ret = new_version.clone();
    let run_id_ret = run_id.clone();

    std::thread::spawn(move || {
        let mut old_version: Option<String> = None;
        let mut run_url: Option<String> = None;
        let mut release_url: Option<String> = None;
        let mut assets: Vec<String> = Vec::new();
        let mut current_step = "preflight";

        let result: Result<(), String> = (|| {
            // 1. Preflight
            emit_step(&app, &run_id, "preflight", "running", None);
            emit_log(&app, &run_id, "info", "Running preflight checks...".to_string());
            let pre = preflight(&repo)?;
            old_version = pre.current_version.clone();
            if !pre.blockers.is_empty() {
                return Err(pre.blockers.join(" "));
            }
            let tag = format!("v{new_version}");
            if let Ok((Some(0), _, _, false)) =
                run_capture_in_dir(&repo, "gh", &["release", "view", &tag], SHORT_TIMEOUT)
            {
                return Err(format!("Release {tag} already exists on GitHub."));
            }
            emit_step(
                &app,
                &run_id,
                "preflight",
                "done",
                Some(format!(
                    "{} -> {}",
                    old_version.as_deref().unwrap_or("none"),
                    new_version
                )),
            );

            // 2. Bump
            current_step = "bump";
            emit_step(&app, &run_id, "bump", "running", None);
            emit_log(
                &app,
                &run_id,
                "info",
                "Bumping version manifests and lockfiles...".to_string(),
            );
            let bumped = bump_version_files(&repo, &new_version, true)?;
            if bumped.is_empty() {
                emit_log(&app, &run_id, "info", "No version files found to bump.".to_string());
            } else {
                for f in &bumped {
                    emit_log(&app, &run_id, "info", format!("Bumped {f} -> {new_version}"));
                }
            }
            emit_step(
                &app,
                &run_id,
                "bump",
                "done",
                Some(format!("{} file(s)", bumped.len())),
            );

            // 3. Validate gloria.toml
            current_step = "validate";
            if pre.has_gloria_toml && !pre.is_gloria_setup {
                emit_step(&app, &run_id, "validate", "running", None);
                let code = run_streaming(
                    &app,
                    &run_id,
                    &current_pid,
                    &cancelled,
                    &repo,
                    "npx",
                    &[
                        "--yes",
                        "gloria-setup@latest",
                        "--check",
                        "--manifest",
                        "gloria.toml",
                        "--version",
                        &new_version,
                    ],
                )?;
                if code != 0 {
                    return Err(format!("gloria-setup --check failed (exit {code})."));
                }
                emit_step(&app, &run_id, "validate", "done", Some("gloria.toml valid".to_string()));
            } else {
                emit_step(&app, &run_id, "validate", "skipped", Some("No gloria.toml".to_string()));
            }

            // 4. Commit & push
            current_step = "push";
            emit_step(&app, &run_id, "push", "running", None);
            let add_code = run_streaming(&app, &run_id, &current_pid, &cancelled, &repo, "git", &["add", "-A"])?;
            if add_code != 0 {
                return Err(format!("git add -A failed (exit {add_code})."));
            }

            let diff_quiet = run_capture_in_dir(&repo, "git", &["diff", "--cached", "--quiet"], SHORT_TIMEOUT)?;
            if diff_quiet.0 != Some(0) {
                let commit_msg = match &note {
                    Some(n) => format!("Release {new_version}: {}.", n.trim_end_matches('.')),
                    None => format!("Release {new_version}."),
                };
                let c_code = run_streaming(
                    &app,
                    &run_id,
                    &current_pid,
                    &cancelled,
                    &repo,
                    "git",
                    &["commit", "-m", &commit_msg],
                )?;
                if c_code != 0 {
                    return Err(format!("git commit failed (exit {c_code})."));
                }
            } else {
                emit_log(&app, &run_id, "info", "Working tree clean; pushing existing HEAD.".to_string());
            }

            let push_code = run_streaming(
                &app,
                &run_id,
                &current_pid,
                &cancelled,
                &repo,
                "git",
                &["push", "origin", "main"],
            )?;
            if push_code != 0 {
                return Err(format!("git push origin main failed (exit {push_code})."));
            }
            emit_step(&app, &run_id, "push", "done", Some("origin/main".to_string()));

            // 5. Dispatch workflow & match headSha
            current_step = "dispatch";
            emit_step(&app, &run_id, "dispatch", "running", None);
            let (_, head_out, _, _) =
                run_capture_in_dir(&repo, "git", &["rev-parse", "HEAD"], SHORT_TIMEOUT)?;
            let head_sha = head_out.trim().to_string();
            if head_sha.is_empty() {
                return Err("Could not read git rev-parse HEAD.".to_string());
            }

            let wf_code = run_streaming(
                &app,
                &run_id,
                &current_pid,
                &cancelled,
                &repo,
                "gh",
                &["workflow", "run", "release.yml", "--ref", "main"],
            )?;
            if wf_code != 0 {
                return Err(format!("gh workflow run failed (exit {wf_code})."));
            }

            emit_log(&app, &run_id, "info", format!("Waiting for GitHub Actions run for {}...", &head_sha[..7.min(head_sha.len())]));
            let mut found_run_id: Option<u64> = None;
            for _ in 0..15 {
                if cancelled.load(Ordering::Relaxed) {
                    return Err("Cancelled.".to_string());
                }
                std::thread::sleep(Duration::from_secs(3));
                if let Some((id, url)) = find_workflow_run(&repo, &head_sha) {
                    found_run_id = Some(id);
                    run_url = url;
                    break;
                }
            }
            let wf_run_id = found_run_id.ok_or_else(|| {
                "Timed out waiting for GitHub Actions to create the workflow run.".to_string()
            })?;
            if let Some(ref url) = run_url {
                emit_log(&app, &run_id, "info", format!("Workflow run #{wf_run_id}: {url}"));
            }
            emit_step(&app, &run_id, "dispatch", "done", Some(format!("#{wf_run_id}")));

            // 6. Watch workflow
            current_step = "watch";
            emit_step(&app, &run_id, "watch", "running", Some("Building on GitHub Actions...".to_string()));
            let wf_id_str = wf_run_id.to_string();
            let watch_code = run_streaming(
                &app,
                &run_id,
                &current_pid,
                &cancelled,
                &repo,
                "gh",
                &["run", "watch", &wf_id_str, "--exit-status", "--interval", "15"],
            )?;
            if watch_code != 0 {
                if let Ok((_, log_out, _, _)) = run_capture_in_dir(
                    &repo,
                    "gh",
                    &["run", "view", &wf_id_str, "--log-failed"],
                    SHORT_TIMEOUT,
                ) {
                    let lines: Vec<&str> = log_out.lines().collect();
                    let tail_start = lines.len().saturating_sub(60);
                    for l in &lines[tail_start..] {
                        emit_log(&app, &run_id, "err", (*l).to_string());
                    }
                }
                return Err(format!("GitHub Actions run #{wf_run_id} failed (exit {watch_code})."));
            }
            emit_step(&app, &run_id, "watch", "done", Some("CI passed".to_string()));

            // 7. Report live release
            current_step = "report";
            emit_step(&app, &run_id, "report", "running", None);
            let (rel_url, rel_assets) = fetch_release_details(&repo, &tag);
            release_url = rel_url;
            assets = rel_assets;

            if let Some(ref url) = release_url {
                emit_log(&app, &run_id, "info", format!("Release live: {url}"));
            }
            if !assets.is_empty() {
                emit_log(&app, &run_id, "info", format!("Assets: {}", assets.join(", ")));
            }
            if pre.is_gloria_setup {
                if let Ok((Some(0), npm_v, _, false)) =
                    run_capture_in_dir(&repo, "npm", &["view", "gloria-setup", "version"], SHORT_TIMEOUT)
                {
                    emit_log(&app, &run_id, "info", format!("npm gloria-setup: {}", npm_v.trim()));
                }
            }
            emit_step(&app, &run_id, "report", "done", Some(tag));

            Ok(())
        })();

        if let Ok(mut map) = running().lock() {
            map.remove(&run_id);
        }

        let is_cancelled = cancelled.load(Ordering::Relaxed);
        match result {
            Ok(()) => {
                let _ = app.emit(
                    EVENT_DONE,
                    DoneEvent {
                        run_id: &run_id,
                        ok: true,
                        cancelled: false,
                        old_version,
                        new_version,
                        run_url,
                        release_url,
                        assets,
                        error: None,
                    },
                );
            }
            Err(err) => {
                emit_step(&app, &run_id, current_step, "failed", Some(err.clone()));
                emit_log(&app, &run_id, "err", err.clone());
                let _ = app.emit(
                    EVENT_DONE,
                    DoneEvent {
                        run_id: &run_id,
                        ok: false,
                        cancelled: is_cancelled,
                        old_version,
                        new_version,
                        run_url,
                        release_url,
                        assets,
                        error: Some(err),
                    },
                );
            }
        }
    });

    Ok(ReleaseStarted {
        run_id: run_id_ret,
        version: version_ret,
    })
}

#[allow(dead_code)]
fn _keep_btreemap_import_used(_: BTreeMap<String, String>) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("brain-release-test-{label}-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn suggests_next_patch_and_prerelease_versions() {
        assert_eq!(suggest_next_version(None), "0.1.0");
        assert_eq!(suggest_next_version(Some("0.4.0")), "0.4.1");
        assert_eq!(suggest_next_version(Some("v1.2.9")), "1.2.10");
        assert_eq!(suggest_next_version(Some("1.2.0-alpha.3")), "1.2.0-alpha.4");
        assert_eq!(suggest_next_version(Some("0.5.0-rc.0")), "0.5.0-rc.1");
    }

    #[test]
    fn normalizes_and_validates_versions() {
        assert_eq!(normalize_version("  v1.2.3 "), "1.2.3");
        assert!(validate_version("1.2.3").is_ok());
        assert!(validate_version("1.2.3-alpha.1").is_ok());
        assert!(validate_version("").is_err());
        assert!(validate_version("1.2").is_err());
        assert!(validate_version("1.2.3 && calc").is_err());
    }

    #[test]
    fn bumps_tauri_and_gloria_toml_without_touching_deps() {
        let dir = make_temp_dir("tauri");
        fs::create_dir_all(dir.join("src-tauri")).unwrap();
        fs::write(
            dir.join("gloria.toml"),
            "id = \"Demo\"\nname = \"Demo\"\nversion = \"0.4.0\"\nmain_exe = \"Demo.exe\"\n",
        )
        .unwrap();
        fs::write(
            dir.join("package.json"),
            "{\n  \"name\": \"demo\",\n  \"version\": \"0.4.0\",\n  \"dependencies\": {\n    \"react\": \"^18.0.0\"\n  }\n}\n",
        )
        .unwrap();
        fs::write(
            dir.join("src-tauri/tauri.conf.json"),
            "{\n  \"productName\": \"Demo\",\n  \"version\": \"0.4.0\"\n}\n",
        )
        .unwrap();
        fs::write(
            dir.join("src-tauri/Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.4.0\"\n\n[dependencies]\nserde = { version = \"1.0\" }\n",
        )
        .unwrap();

        let updated = bump_version_files(&dir, "0.4.1", false).unwrap();
        assert_eq!(
            updated,
            vec![
                "gloria.toml",
                "package.json",
                "src-tauri/tauri.conf.json",
                "src-tauri/Cargo.toml"
            ]
        );

        let cargo = read_utf8(&dir.join("src-tauri/Cargo.toml")).unwrap();
        assert!(cargo.contains("version = \"0.4.1\""));
        assert!(cargo.contains("serde = { version = \"1.0\" }"));
        assert!(read_utf8(&dir.join("gloria.toml")).unwrap().contains("version = \"0.4.1\""));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn bumps_extension_manifest_with_numeric_prefix() {
        let dir = make_temp_dir("ext");
        fs::write(dir.join("package.json"), "{\"name\":\"ext\",\"version\":\"1.0.0\"}").unwrap();
        fs::write(
            dir.join("manifest.json"),
            "{\"manifest_version\": 3, \"name\": \"Ext\", \"version\": \"1.0.0\"}",
        )
        .unwrap();

        let updated = bump_version_files(&dir, "1.2.0-beta.2", false).unwrap();
        assert_eq!(updated, vec!["package.json", "manifest.json"]);
        assert!(read_utf8(&dir.join("package.json")).unwrap().contains("1.2.0-beta.2"));
        assert!(read_utf8(&dir.join("manifest.json")).unwrap().contains("\"1.2.0\""));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn bumps_gloria_setup_manifests_and_readme() {
        let dir = make_temp_dir("gloriasetup");
        fs::create_dir_all(dir.join("crates/gloria-pack")).unwrap();
        fs::create_dir_all(dir.join("npm")).unwrap();
        fs::write(
            dir.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/gloria-pack\"]\n\n[workspace.package]\nversion = \"0.9.0\"\n",
        )
        .unwrap();
        fs::write(
            dir.join("npm/package.json"),
            "{\n  \"name\": \"gloria-setup\",\n  \"version\": \"0.9.0\"\n}\n",
        )
        .unwrap();
        fs::write(
            dir.join("README.md"),
            "uses: projectGloria/GloriaSetup@v0.9.0\n",
        )
        .unwrap();

        let updated = bump_version_files(&dir, "0.9.1", false).unwrap();
        assert_eq!(updated, vec!["Cargo.toml", "npm/package.json", "README.md"]);
        assert!(read_utf8(&dir.join("Cargo.toml")).unwrap().contains("version = \"0.9.1\""));
        assert!(read_utf8(&dir.join("npm/package.json")).unwrap().contains("\"0.9.1\""));
        assert!(read_utf8(&dir.join("README.md")).unwrap().contains("GloriaSetup@v0.9.1"));

        let _ = fs::remove_dir_all(&dir);
    }
}
