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
    pub has_check_bat: bool,
    pub needs_bootstrap: bool,
    pub bootstrap_plan: Vec<String>,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseRequest {
    pub run_id: String,
    pub repo_path: String,
    pub version: String,
    pub note: Option<String>,
    #[serde(default)]
    pub run_local_check: bool,
    #[serde(default)]
    pub retry_same_version: bool,
    #[serde(default)]
    pub pre_fix: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReleaseFailureDiagnosis {
    pub category: String,
    pub summary: String,
    pub location: Option<String>,
    pub fix_action: Option<String>,
    pub fix_label: Option<String>,
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
    diagnosis: Option<ReleaseFailureDiagnosis>,
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
        .env("CI", "true")
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

pub fn clean_log_line(raw: &str) -> String {
    static CARET_ANSI: OnceLock<Regex> = OnceLock::new();
    let re = CARET_ANSI.get_or_init(|| {
        Regex::new(r"\^\[\[[0-9;?]*[A-Za-z]").expect("valid caret ansi regex")
    });
    re.replace_all(raw, "").into_owned()
}

fn push_recent(recent: &Arc<Mutex<Vec<String>>>, line: &str) {
    if let Ok(mut buf) = recent.lock() {
        buf.push(line.to_string());
        if buf.len() > 200 {
            let drain = buf.len() - 200;
            buf.drain(0..drain);
        }
    }
}

fn pump_stream<R: Read + Send + 'static>(
    app: AppHandle,
    run_id: String,
    pipe: R,
    stream: &'static str,
    recent: Arc<Mutex<Vec<String>>>,
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
            let line = clean_log_line(&clis::clean_output(&buf));
            if line.trim().is_empty() {
                continue;
            }
            for l in line.split('\n') {
                if !l.trim().is_empty() {
                    push_recent(&recent, l);
                    emit_log(&app, &run_id, stream, l.to_string());
                }
            }
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn run_streaming(
    app: &AppHandle,
    run_id: &str,
    current_pid: &Arc<Mutex<Option<u32>>>,
    cancelled: &Arc<AtomicBool>,
    recent: &Arc<Mutex<Vec<String>>>,
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
        .map(|p| pump_stream(app.clone(), run_id.to_string(), p, "out", recent.clone()));
    let err_t = child
        .stderr
        .take()
        .map(|p| pump_stream(app.clone(), run_id.to_string(), p, "err", recent.clone()));

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

    // Workflow status (missing/custom release.yml is auto-bootstrapped via GloriaSetup)
    let wf_path = repo_path.join(".github/workflows/release.yml");
    let workflow_status = if !wf_path.is_file() {
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

    let has_check_bat = repo_path.join("gloriaTools/check.bat").is_file();

    let mut bootstrap_plan = Vec::new();
    if !is_gloria_setup {
        let is_desktop = is_desktop_app(repo_path);
        if !has_gloria_toml && is_desktop {
            bootstrap_plan.push("Create gloria.toml from repo manifests".to_string());
        }
        if workflow_status == "missing" {
            bootstrap_plan.push("Create .github/workflows/release.yml (GloriaSetup template)".to_string());
        } else if workflow_status == "custom" {
            bootstrap_plan.push("Migrate .github/workflows/release.yml to GloriaSetup template".to_string());
        }
    }
    let needs_bootstrap = !bootstrap_plan.is_empty();

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
        has_check_bat,
        needs_bootstrap,
        bootstrap_plan,
        blockers,
    })
}

const GLORIA_SETUP_TEMPLATE: &str = r#"# projectGloria app release. Copy to .github/workflows/release.yml and edit only the `build` job.
#
# Runs only when started on purpose: /gloria-github-release, or `gh workflow run release.yml`.
# Pushing to main never releases.
#
#   check   (shared)  version files agree, version not released yet, gloria.toml valid, picks newest gloria-setup
#   build   (this repo) builds the app and uploads its files as the artifact `app`
#   release (shared)  packs with that gloria-setup + dry run (or, without gloria.toml, publishes the files as-is),
#                     then publishes GitHub Release v<version> with SHA256SUMS.txt
#
# The shared jobs live in projectGloria/GloriaSetup and are used from @main, so every release picks up
# the newest checks and the newest gloria-setup without editing this file.
name: Release

on:
  workflow_dispatch:
    inputs:
      dry_run:
        description: Build and verify everything, but do not publish
        type: boolean
        default: false

permissions:
  contents: read

concurrency:
  group: release
  cancel-in-progress: false

jobs:
  check:
    uses: projectGloria/GloriaSetup/.github/workflows/app-check.yml@main

  build:
    needs: check
    runs-on: windows-latest
    env:
      VERSION: ${{ needs.check.outputs.version }}
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7

      # ---- App-specific: build, test, and copy the files to install into stage/ ----
      # Installer apps: stage/ is the app folder (it must contain gloria.toml's main_exe).
      # Extensions / Android: stage/ holds the files to publish (e.g. MyExt-<version>.zip, MyApp-<version>.apk).
      - name: Build
        shell: pwsh
        run: |
          throw 'Replace this step with the app build, then copy the result into stage/'

      - uses: actions/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd9 # v7
        with:
          name: app
          path: stage/
          if-no-files-found: error

  release:
    needs: [check, build]
    uses: projectGloria/GloriaSetup/.github/workflows/app-release.yml@main
    permissions:
      contents: write
    with:
      version: ${{ needs.check.outputs.version }}
      installer: ${{ needs.check.outputs.installer == 'true' }}
      gloria_setup: ${{ needs.check.outputs.gloria_setup }}
      dry_run: ${{ inputs.dry_run }}
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppStack {
    Tauri,
    Electron,
    Rust,
    Extension,
    Android,
    Python,
    Unknown,
}

fn detect_app_stack(repo_root: &Path) -> AppStack {
    if repo_root.join("src-tauri/tauri.conf.json").is_file()
        || repo_root.join("tauri.conf.json").is_file()
    {
        return AppStack::Tauri;
    }
    let ext_manifest = repo_root.join("manifest.json");
    if ext_manifest.is_file()
        && read_utf8(&ext_manifest)
            .map(|r| r.contains("manifest_version"))
            .unwrap_or(false)
    {
        return AppStack::Extension;
    }
    if repo_root.join("android/app/build.gradle").is_file()
        || repo_root.join("android/app/build.gradle.kts").is_file()
        || repo_root.join("app/build.gradle.kts").is_file()
    {
        return AppStack::Android;
    }
    let pkg_path = repo_root.join("package.json");
    if pkg_path.is_file() {
        if let Ok(raw) = read_utf8(&pkg_path) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
                let has_electron = ["dependencies", "devDependencies"].iter().any(|k| {
                    val.get(k)
                        .and_then(|d| d.as_object())
                        .is_some_and(|o| o.contains_key("electron") || o.contains_key("electron-builder"))
                });
                if has_electron {
                    return AppStack::Electron;
                }
            }
        }
    }
    if repo_root.join("Cargo.toml").is_file() || repo_root.join("src-tauri/Cargo.toml").is_file() {
        return AppStack::Rust;
    }
    if repo_root.join("pyproject.toml").is_file() || repo_root.join("requirements.txt").is_file() {
        return AppStack::Python;
    }
    AppStack::Unknown
}

fn is_desktop_app(repo_root: &Path) -> bool {
    matches!(
        detect_app_stack(repo_root),
        AppStack::Tauri | AppStack::Electron | AppStack::Rust | AppStack::Python
    )
}

fn find_ico_recursive(dir: &Path, root: &Path, depth: usize, out: &mut Vec<String>) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if matches!(
                name.as_str(),
                "node_modules" | "target" | ".git" | "dist" | "stage" | ".venv"
            ) {
                continue;
            }
            find_ico_recursive(&path, root, depth + 1, out);
        } else if path.is_file()
            && path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("ico"))
        {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
}

fn find_best_ico(repo_root: &Path) -> Option<String> {
    let mut found = Vec::new();
    find_ico_recursive(repo_root, repo_root, 0, &mut found);
    if found.is_empty() {
        return None;
    }
    for preferred in ["Gloria.ico", "src-tauri/icons/icon.ico", "icons/icon.ico"] {
        if let Some(hit) = found.iter().find(|p| p.eq_ignore_ascii_case(preferred)) {
            return Some(hit.clone());
        }
    }
    found.sort();
    found.into_iter().next()
}

fn detect_main_exe(repo_root: &Path, repo_name: &str, stack: AppStack) -> String {
    if let Ok(raw) = read_utf8(&repo_root.join("gloria.toml")) {
        if let Ok(val) = raw.parse::<toml::Value>() {
            if let Some(exe) = val.get("main_exe").and_then(|v| v.as_str()) {
                if !exe.trim().is_empty() {
                    return exe.trim().to_string();
                }
            }
        }
    }
    for rel in ["src-tauri/Cargo.toml", "Cargo.toml"] {
        let p = repo_root.join(rel);
        if p.is_file() {
            if let Ok(raw) = read_utf8(&p) {
                if let Ok(val) = raw.parse::<toml::Value>() {
                    if let Some(bins) = val.get("bin").and_then(|b| b.as_array()) {
                        if let Some(name) = bins.first().and_then(|b| b.get("name")).and_then(|v| v.as_str()) {
                            return format!("{name}.exe");
                        }
                    }
                    if let Some(name) = val
                        .get("package")
                        .and_then(|pkg| pkg.get("name"))
                        .and_then(|v| v.as_str())
                    {
                        return format!("{name}.exe");
                    }
                }
            }
        }
    }
    if stack == AppStack::Electron {
        if let Ok(raw) = read_utf8(&repo_root.join("package.json")) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(prod) = val
                    .get("build")
                    .and_then(|b| b.get("productName"))
                    .and_then(|v| v.as_str())
                    .or_else(|| val.get("name").and_then(|v| v.as_str()))
                {
                    if !prod.trim().is_empty() {
                        return format!("{}.exe", prod.trim());
                    }
                }
            }
        }
    }
    format!("{repo_name}.exe")
}

fn detect_description(repo_root: &Path, repo_name: &str) -> String {
    if let Ok(raw) = read_utf8(&repo_root.join("package.json")) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(desc) = val.get("description").and_then(|v| v.as_str()) {
                if !desc.trim().is_empty() {
                    return desc.trim().to_string();
                }
            }
        }
    }
    for rel in ["src-tauri/Cargo.toml", "Cargo.toml"] {
        if let Ok(raw) = read_utf8(&repo_root.join(rel)) {
            if let Ok(val) = raw.parse::<toml::Value>() {
                if let Some(desc) = val
                    .get("package")
                    .and_then(|p| p.get("description"))
                    .and_then(|v| v.as_str())
                {
                    if !desc.trim().is_empty() {
                        return desc.trim().to_string();
                    }
                }
            }
        }
    }
    format!("{repo_name} desktop application")
}

fn detect_tauri_identifier(repo_root: &Path) -> Option<String> {
    for rel in ["src-tauri/tauri.conf.json", "tauri.conf.json"] {
        let p = repo_root.join(rel);
        if p.is_file() {
            if let Ok(raw) = read_utf8(&p) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
                    if let Some(id) = val.get("identifier").and_then(|v| v.as_str()) {
                        if !id.trim().is_empty() {
                            return Some(id.trim().to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

fn build_steps_for_stack(repo_root: &Path, repo_name: &str, main_exe: &str, stack: AppStack) -> Result<String, String> {
    let has_pkg_lock = repo_root.join("package-lock.json").is_file();
    let has_cargo_lock = repo_root.join("src-tauri/Cargo.lock").is_file() || repo_root.join("Cargo.lock").is_file();
    let has_tsconfig = repo_root.join("tsconfig.json").is_file();
    let npm_cache_line = if has_pkg_lock {
        "\n          cache: \"npm\""
    } else {
        ""
    };

    match stack {
        AppStack::Tauri => {
            let install_cmd = if has_pkg_lock { "npm ci" } else { "npm install" };
            let test_cmd = if has_cargo_lock {
                "cargo test --manifest-path src-tauri/Cargo.toml --locked"
            } else {
                "cargo test --manifest-path src-tauri/Cargo.toml"
            };
            let ts_line = if has_tsconfig { "          npx tsc --noEmit\n" } else { "" };
            Ok(format!(
                r#"      - name: Setup Node.js
        uses: actions/setup-node@949feb2413d6458794dcd2491c4babbbce0c15c1 # v7
        with:
          node-version: "22"{npm_cache_line}

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@39008827e6b4319a349e9a42dfc90579dd25f1ca # stable

      - name: Cache Rust build
        uses: swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0 # v2
        with:
          workspaces: "src-tauri -> target"
          cache-on-failure: true

      - name: Install dependencies
        shell: pwsh
        run: {install_cmd}

      - name: Run frontend and Rust checks
        shell: pwsh
        run: |
{ts_line}          {test_cmd}

      - name: Build Tauri release binary
        shell: pwsh
        run: npx tauri build --no-bundle

      - name: Stage application payload
        shell: pwsh
        run: |
          $ErrorActionPreference = "Stop"
          if (Test-Path stage) {{ Remove-Item -Recurse -Force stage }}
          New-Item -ItemType Directory -Path stage | Out-Null
          $bin = "src-tauri\target\release\{main_exe}"
          if (-not (Test-Path $bin)) {{ throw "Built binary not found: $bin" }}
          Copy-Item $bin "stage\{main_exe}""#
            ))
        }
        AppStack::Electron => {
            let install_cmd = if has_pkg_lock { "npm ci" } else { "npm install" };
            Ok(format!(
                r#"      - name: Setup Node.js
        uses: actions/setup-node@949feb2413d6458794dcd2491c4babbbce0c15c1 # v7
        with:
          node-version: "22"{npm_cache_line}

      - name: Install dependencies
        shell: pwsh
        run: {install_cmd}

      - name: Build Electron app
        shell: pwsh
        run: npx electron-builder --win --x64 --dir --publish never

      - name: Stage application payload
        shell: pwsh
        run: |
          $ErrorActionPreference = "Stop"
          if (Test-Path stage) {{ Remove-Item -Recurse -Force stage }}
          New-Item -ItemType Directory -Path stage | Out-Null
          Copy-Item "dist\win-unpacked\*" "stage\" -Recurse -Force"#
            ))
        }
        AppStack::Rust => {
            let locked = if has_cargo_lock { " --locked" } else { "" };
            Ok(format!(
                r#"      - name: Setup Rust
        uses: dtolnay/rust-toolchain@39008827e6b4319a349e9a42dfc90579dd25f1ca # stable

      - name: Cache Rust build
        uses: swatinem/rust-cache@d1527c8f67ef9af357290475d4af4b0fab8f12e0 # v2
        with:
          cache-on-failure: true

      - name: Run Rust tests
        shell: pwsh
        run: cargo test{locked}

      - name: Build release binary
        shell: pwsh
        run: cargo build --release{locked}

      - name: Stage application payload
        shell: pwsh
        run: |
          $ErrorActionPreference = "Stop"
          if (Test-Path stage) {{ Remove-Item -Recurse -Force stage }}
          New-Item -ItemType Directory -Path stage | Out-Null
          $bin = "target\release\{main_exe}"
          if (-not (Test-Path $bin)) {{ throw "Built binary not found: $bin" }}
          Copy-Item $bin "stage\{main_exe}""#
            ))
        }
        AppStack::Extension => {
            let has_pkg = repo_root.join("package.json").is_file();
            let build_step = if has_pkg {
                let install_cmd = if has_pkg_lock { "npm ci" } else { "npm install" };
                format!(
                    r#"      - name: Setup Node.js
        uses: actions/setup-node@949feb2413d6458794dcd2491c4babbbce0c15c1 # v7
        with:
          node-version: "22"{npm_cache_line}

      - name: Build extension
        shell: pwsh
        run: |
          {install_cmd}
          npm run build
"#
                )
            } else {
                String::new()
            };
            let src_dir = if repo_root.join("dist").is_dir() || has_pkg { "dist\\*" } else { "*" };
            Ok(format!(
                r#"{build_step}      - name: Package extension
        shell: pwsh
        run: |
          $ErrorActionPreference = "Stop"
          if (Test-Path stage) {{ Remove-Item -Recurse -Force stage }}
          New-Item -ItemType Directory -Path stage | Out-Null
          Compress-Archive -Path "{src_dir}" -DestinationPath "stage\{repo_name}-$env:VERSION.zip" -Force"#
            ))
        }
        other => Err(format!(
            "Cannot auto-generate .github/workflows/release.yml for stack '{other:?}' yet. Add .github/workflows/release.yml manually."
        )),
    }
}

pub fn bootstrap_gloria_setup(
    repo_root: &Path,
    version_override: Option<&str>,
) -> Result<Vec<String>, String> {
    if !repo_root.is_dir() {
        return Err(format!("Repository '{}' does not exist.", repo_root.display()));
    }
    if is_gloria_setup_repo(repo_root) {
        return Ok(Vec::new());
    }

    let repo_name = repo_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("App")
        .to_string();
    let stack = detect_app_stack(repo_root);
    let main_exe = detect_main_exe(repo_root, &repo_name, stack);
    let mut created = Vec::new();

    // 1. gloria.toml (desktop apps only)
    let gloria_path = repo_root.join("gloria.toml");
    if !gloria_path.is_file() && is_desktop_app(repo_root) {
        let version = version_override
            .map(normalize_version)
            .filter(|v| !v.is_empty())
            .or_else(|| detect_version_files(repo_root, false).into_iter().next().map(|(_, v)| v))
            .unwrap_or_else(|| "0.1.0".to_string());
        let desc = detect_description(repo_root, &repo_name).replace('"', "\\\"");
        let ico = find_best_ico(repo_root).ok_or_else(|| {
            "No .ico icon found in repository. Add an .ico file (e.g. Gloria.ico or src-tauri/icons/icon.ico) so GloriaSetup can pack the installer.".to_string()
        })?;
        let mut lines = vec![
            format!("id = \"{repo_name}\""),
            format!("name = \"{repo_name}\""),
            format!("version = \"{version}\""),
            format!("main_exe = \"{main_exe}\""),
            format!("icon = \"{ico}\""),
        ];
        lines.push("accent = \"#FF6B00\"".to_string());
        lines.push(format!("description = \"{desc}\""));
        if stack == AppStack::Tauri {
            lines.push("needs_webview2 = true".to_string());
        }
        let mut data_folders = vec![format!("'%APPDATA%\\{repo_name}'")];
        if let Some(ident) = detect_tauri_identifier(repo_root) {
            data_folders.push(format!("'%LOCALAPPDATA%\\{ident}'"));
        }
        lines.push(format!("data_folders = [{}]", data_folders.join(", ")));
        let content = format!("{}\n", lines.join("\n"));
        write_utf8_no_bom(&gloria_path, &content)?;
        created.push("gloria.toml".to_string());
    }

    // 2. .github/workflows/release.yml
    let wf_path = repo_root.join(".github/workflows/release.yml");
    let needs_wf = !wf_path.is_file()
        || !read_utf8(&wf_path)
            .map(|r| r.contains("GloriaSetup/.github/workflows/app-check.yml@main"))
            .unwrap_or(false);
    if needs_wf {
        let template = repo_root
            .parent()
            .map(|p| p.join("GloriaSetup/templates/release.yml"))
            .filter(|p| p.is_file())
            .and_then(|p| read_utf8(&p).ok())
            .unwrap_or_else(|| GLORIA_SETUP_TEMPLATE.to_string());

        let build_steps = build_steps_for_stack(repo_root, &repo_name, &main_exe, stack)?;
        let placeholder_re = Regex::new(
            r"(?ms)^      - name: Build\r?\n        shell: pwsh\r?\n        run: \|\r?\n          throw 'Replace this step with the app build, then copy the result into stage/'",
        )
        .map_err(|e| e.to_string())?;
        if !placeholder_re.is_match(&template) {
            return Err("Could not locate placeholder Build step in GloriaSetup template.".to_string());
        }
        let rendered = placeholder_re
            .replace(&template, build_steps.as_str())
            .into_owned();
        if let Some(parent) = wf_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed creating .github/workflows/: {e}"))?;
        }
        write_utf8_no_bom(&wf_path, &rendered)?;
        created.push(".github/workflows/release.yml".to_string());
    }

    Ok(created)
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

pub fn diagnose_failure(error: &str, log_lines: &[String], version: &str) -> ReleaseFailureDiagnosis {
    let joined = format!("{}\n{}", log_lines.join("\n"), error);
    let lower = joined.to_ascii_lowercase();

    static LOC_RE: OnceLock<Regex> = OnceLock::new();
    let loc_re = LOC_RE.get_or_init(|| {
        Regex::new(
            r"(?i)([A-Za-z0-9_./\\-]+\.(?:ps1|rs|ts|tsx|js|mjs|json|toml|yml|yaml|py|cs|gradle|kts)):(\d+)",
        )
        .expect("valid location regex")
    });
    let location = loc_re.captures(&joined).map(|c| {
        let path = c[1].replace('\\', "/");
        let short = if let Some(idx) = path.rfind("/scripts/") {
            &path[idx + 1..]
        } else if let Some(idx) = path.rfind("/src") {
            &path[idx + 1..]
        } else {
            &path
        };
        format!("{short}:{}", &c[2])
    });

    if lower.contains("already exists on github") || lower.contains("already released") {
        return ReleaseFailureDiagnosis {
            category: "already_released".to_string(),
            summary: format!("Release v{version} already exists on GitHub. Pick a newer version."),
            location,
            fix_action: None,
            fix_label: None,
        };
    }

    if lower.contains("cargo fmt failed") || lower.contains("diff in ") {
        return ReleaseFailureDiagnosis {
            category: "cargo_fmt".to_string(),
            summary: "Rust formatting check (cargo fmt) failed.".to_string(),
            location,
            fix_action: Some("cargo_fmt".to_string()),
            fix_label: Some("Run cargo fmt & Retry".to_string()),
        };
    }

    if lower.contains("cannot update the lock file")
        || lower.contains("cargo.lock needs to be updated")
        || (lower.contains("--locked") && lower.contains("cargo"))
        || lower.contains("npm ci` can only install packages when your package.json and package-lock.json")
    {
        return ReleaseFailureDiagnosis {
            category: "lockfile".to_string(),
            summary: "Lockfile is out of sync with the manifest (--locked / npm ci failed).".to_string(),
            location,
            fix_action: Some("refresh_lockfiles".to_string()),
            fix_label: Some("Refresh lockfiles & Retry".to_string()),
        };
    }

    if lower.contains("version mismatch") || lower.contains("version files disagree") {
        return ReleaseFailureDiagnosis {
            category: "version_mismatch".to_string(),
            summary: "Version manifests do not agree.".to_string(),
            location,
            fix_action: Some("resync_versions".to_string()),
            fix_label: Some("Re-sync versions & Retry".to_string()),
        };
    }

    if lower.contains("gloria-setup --check failed") {
        return ReleaseFailureDiagnosis {
            category: "gloria_toml".to_string(),
            summary: "gloria.toml validation failed. Fix gloria.toml and retry.".to_string(),
            location,
            fix_action: None,
            fix_label: None,
        };
    }

    let best_line = log_lines
        .iter()
        .rev()
        .find(|l| {
            let t = l.trim();
            t.contains("Exception:")
                || t.contains("throw '")
                || t.contains("error[")
                || t.contains("FAILED")
                || t.contains("##[error]")
        })
        .map(|l| {
            let t = l.trim();
            if let Some((_, rest)) = t.split_once("Exception:") {
                return rest.trim().trim_matches('"').to_string();
            }
            if let Some((_, rest)) = t.split_once("##[error]") {
                return rest.trim().to_string();
            }
            t.to_string()
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| error.to_string());

    ReleaseFailureDiagnosis {
        category: "ci_error".to_string(),
        summary: best_line,
        location,
        fix_action: Some("retry".to_string()),
        fix_label: Some(format!("Retry v{version}")),
    }
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
    let run_local_check = req.run_local_check;
    let retry_same_version = req.retry_same_version;
    let pre_fix = req.pre_fix.clone();
    let version_ret = new_version.clone();
    let run_id_ret = run_id.clone();

    std::thread::spawn(move || {
        let mut old_version: Option<String> = None;
        let mut run_url: Option<String> = None;
        let mut release_url: Option<String> = None;
        let mut assets: Vec<String> = Vec::new();
        let mut current_step = "preflight";
        let recent = Arc::new(Mutex::new(Vec::<String>::new()));

        let result: Result<(), String> = (|| {
            // 1. Preflight
            emit_step(&app, &run_id, "preflight", "running", None);
            emit_log(&app, &run_id, "info", "Running preflight checks...".to_string());
            let mut pre = preflight(&repo)?;
            old_version = pre.current_version.clone();
            if !pre.blockers.is_empty() {
                return Err(pre.blockers.join(" "));
            }
            if pre.needs_bootstrap && !pre.is_gloria_setup {
                emit_log(
                    &app,
                    &run_id,
                    "info",
                    "Configuring GloriaSetup (gloria.toml / release.yml)...".to_string(),
                );
                let created = bootstrap_gloria_setup(&repo, Some(&new_version))?;
                for f in &created {
                    emit_log(&app, &run_id, "info", format!("Generated {f}"));
                }
                pre.has_gloria_toml = repo.join("gloria.toml").is_file();
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

            // Optional deterministic pre-fix before bumping/committing
            if let Some(ref fix) = pre_fix {
                match fix.as_str() {
                    "cargo_fmt" => {
                        emit_log(&app, &run_id, "info", "Applying deterministic fix: cargo fmt --all...".to_string());
                        let fmt_dir = if repo.join("src-tauri/Cargo.toml").is_file() {
                            repo.join("src-tauri")
                        } else {
                            repo.clone()
                        };
                        let code = run_streaming(
                            &app,
                            &run_id,
                            &current_pid,
                            &cancelled,
                            &recent,
                            &fmt_dir,
                            "cargo",
                            &["fmt", "--all"],
                        )?;
                        if code != 0 {
                            return Err(format!("cargo fmt --all failed (exit {code})."));
                        }
                    }
                    "refresh_lockfiles" => {
                        emit_log(&app, &run_id, "info", "Applying deterministic fix: refreshing lockfiles...".to_string());
                        for dir in [&repo, &repo.join("src-tauri")] {
                            if dir.join("Cargo.lock").is_file() {
                                let code = run_streaming(
                                    &app,
                                    &run_id,
                                    &current_pid,
                                    &cancelled,
                                    &recent,
                                    dir,
                                    "cargo",
                                    &["update", "--workspace", "--offline"],
                                )?;
                                if code != 0 {
                                    return Err(format!("cargo update --workspace --offline failed (exit {code})."));
                                }
                            }
                        }
                        if repo.join("package-lock.json").is_file() {
                            let code = run_streaming(
                                &app,
                                &run_id,
                                &current_pid,
                                &cancelled,
                                &recent,
                                &repo,
                                "npm",
                                &["install", "--package-lock-only"],
                            )?;
                            if code != 0 {
                                return Err(format!("npm install --package-lock-only failed (exit {code})."));
                            }
                        }
                    }
                    _ => {}
                }
            }

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

            // 3. Validate gloria.toml (+ optional local check.bat)
            current_step = "validate";
            let check_bat = repo.join("gloriaTools/check.bat");
            let has_gloria_check = pre.has_gloria_toml && !pre.is_gloria_setup;
            let has_local_check = run_local_check && check_bat.is_file();
            if has_gloria_check || has_local_check {
                emit_step(&app, &run_id, "validate", "running", None);
                if has_gloria_check {
                    let code = run_streaming(
                        &app,
                        &run_id,
                        &current_pid,
                        &cancelled,
                        &recent,
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
                }
                if has_local_check {
                    emit_log(&app, &run_id, "info", "Running local gloriaTools/check.bat...".to_string());
                    let code = run_streaming(
                        &app,
                        &run_id,
                        &current_pid,
                        &cancelled,
                        &recent,
                        &repo,
                        "cmd.exe",
                        &["/C", "gloriaTools\\check.bat"],
                    )?;
                    if code != 0 {
                        return Err(format!("gloriaTools/check.bat failed (exit {code})."));
                    }
                }
                emit_step(&app, &run_id, "validate", "done", Some("Validated".to_string()));
            } else {
                emit_step(&app, &run_id, "validate", "skipped", Some("No gloria.toml".to_string()));
            }

            // 4. Commit & push
            current_step = "push";
            emit_step(&app, &run_id, "push", "running", None);
            let add_code = run_streaming(&app, &run_id, &current_pid, &cancelled, &recent, &repo, "git", &["add", "-A"])?;
            if add_code != 0 {
                return Err(format!("git add -A failed (exit {add_code})."));
            }

            let diff_quiet = run_capture_in_dir(&repo, "git", &["diff", "--cached", "--quiet"], SHORT_TIMEOUT)?;
            if diff_quiet.0 != Some(0) {
                let commit_msg = if retry_same_version {
                    match pre_fix.as_deref() {
                        Some("cargo_fmt") => format!("Fix formatting so the {new_version} release builds."),
                        Some("refresh_lockfiles") => format!("Fix lockfiles so the {new_version} release builds."),
                        Some("resync_versions") => format!("Sync version manifests so the {new_version} release builds."),
                        _ => format!("Fix release build for {new_version}."),
                    }
                } else {
                    match &note {
                        Some(n) => format!("Release {new_version}: {}.", n.trim_end_matches('.')),
                        None => format!("Release {new_version}."),
                    }
                };
                let c_code = run_streaming(
                    &app,
                    &run_id,
                    &current_pid,
                    &cancelled,
                    &recent,
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
                &recent,
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
                &recent,
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
                &recent,
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
                        let cleaned = clean_log_line(l);
                        push_recent(&recent, &cleaned);
                        emit_log(&app, &run_id, "err", cleaned);
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
                        diagnosis: None,
                    },
                );
            }
            Err(err) => {
                let diag = if is_cancelled {
                    None
                } else {
                    let snapshot = recent.lock().map(|b| b.clone()).unwrap_or_default();
                    Some(diagnose_failure(&err, &snapshot, &new_version))
                };
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
                        diagnosis: diag,
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

    #[test]
    fn strips_caret_escaped_ansi_codes() {
        let raw = "^[[31;1m^[[36;1m 115 | ^[[0m  throw 'Defender scan must precede'^[[0m";
        assert_eq!(
            clean_log_line(raw),
            " 115 |   throw 'Defender scan must precede'"
        );
    }

    #[test]
    fn diagnoses_known_and_custom_ci_failures() {
        let fmt_diag = diagnose_failure(
            "GitHub Actions run #1 failed",
            &["Diff in src/main.rs at line 12:".to_string(), "cargo fmt failed".to_string()],
            "0.2.0",
        );
        assert_eq!(fmt_diag.category, "cargo_fmt");
        assert_eq!(fmt_diag.fix_action.as_deref(), Some("cargo_fmt"));

        let navalia_diag = diagnose_failure(
            "GitHub Actions run #45886567484 failed (exit 1).",
            &[
                "Exception: D:\\a\\Navalia\\Navalia\\scripts\\Test-ReleaseAntivirus.ps1:115".to_string(),
                "Defender scan must precede uploading the staged app artifact.".to_string(),
            ],
            "0.2.0",
        );
        assert_eq!(navalia_diag.category, "ci_error");
        assert_eq!(navalia_diag.location.as_deref(), Some("scripts/Test-ReleaseAntivirus.ps1:115"));
        assert_eq!(navalia_diag.fix_action.as_deref(), Some("retry"));
    }

    #[test]
    fn bootstraps_tauri_repo_with_gloria_toml_and_release_yml() {
        let dir = make_temp_dir("bootstrap");
        fs::create_dir_all(dir.join("src-tauri/icons")).unwrap();
        fs::write(dir.join("src-tauri/icons/icon.ico"), "ico").unwrap();
        fs::write(
            dir.join("package.json"),
            r#"{"name":"festina-lente","version":"0.1.0","description":"Focus timer"}"#,
        )
        .unwrap();
        fs::write(dir.join("package-lock.json"), "{}").unwrap();
        fs::write(
            dir.join("src-tauri/tauri.conf.json"),
            r#"{"productName":"FestinaLente","version":"0.1.0","identifier":"com.projectgloria.festinalente"}"#,
        )
        .unwrap();
        fs::write(
            dir.join("src-tauri/Cargo.toml"),
            "[package]\nname = \"festina-lente\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        let created = bootstrap_gloria_setup(&dir, Some("0.1.1")).unwrap();
        assert_eq!(
            created,
            vec![
                "gloria.toml".to_string(),
                ".github/workflows/release.yml".to_string()
            ]
        );

        let toml_raw = read_utf8(&dir.join("gloria.toml")).unwrap();
        assert!(toml_raw.contains("version = \"0.1.1\""));
        assert!(toml_raw.contains("main_exe = \"festina-lente.exe\""));
        assert!(toml_raw.contains("icon = \"src-tauri/icons/icon.ico\""));
        assert!(toml_raw.contains("needs_webview2 = true"));
        assert!(toml_raw.contains("'%APPDATA%\\"));
        assert!(toml_raw.contains("'%LOCALAPPDATA%\\com.projectgloria.festinalente'"));

        let wf_raw = read_utf8(&dir.join(".github/workflows/release.yml")).unwrap();
        assert!(wf_raw.contains("GloriaSetup/.github/workflows/app-check.yml@main"));
        assert!(wf_raw.contains("GloriaSetup/.github/workflows/app-release.yml@main"));
        assert!(wf_raw.contains("festina-lente.exe"));

        let _ = fs::remove_dir_all(&dir);
    }
}
