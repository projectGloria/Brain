//! Detects, installs, updates and configures the agent CLIs Brain deploys skills to:
//! Claude Code, Codex CLI and Antigravity CLI.

use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::brain_core::{normalize_path, read_utf8};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const VERSION_TIMEOUT: Duration = Duration::from_secs(10);
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(45);
const ACTION_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const LATEST_CACHE_TTL: Duration = Duration::from_secs(10 * 60);

struct ConfigDef {
    key: &'static str,
    label: &'static str,
    /// Path segments under the user's home folder.
    rel: &'static [&'static str],
    format: &'static str,
}

struct CliDef {
    id: &'static str,
    name: &'static str,
    /// Matches the `tool` ids used by skills.rs.
    skill_tool: &'static str,
    commands: &'static [&'static str],
    winget_id: &'static str,
    npm_package: Option<&'static str>,
    config_dir: &'static [&'static str],
    skills_dir: &'static [&'static str],
    configs: &'static [ConfigDef],
    docs_url: &'static str,
}

const CLIS: &[CliDef] = &[
    CliDef {
        id: "claude",
        name: "Claude Code",
        skill_tool: "claude-code",
        commands: &["claude"],
        winget_id: "Anthropic.ClaudeCode",
        npm_package: Some("@anthropic-ai/claude-code"),
        config_dir: &[".claude"],
        skills_dir: &[".claude", "skills"],
        configs: &[
            ConfigDef { key: "settings", label: "settings.json", rel: &[".claude", "settings.json"], format: "json" },
            ConfigDef { key: "memory", label: "CLAUDE.md (user memory)", rel: &[".claude", "CLAUDE.md"], format: "md" },
        ],
        docs_url: "https://docs.claude.com/en/docs/claude-code/settings",
    },
    CliDef {
        id: "codex",
        name: "Codex CLI",
        skill_tool: "codex",
        commands: &["codex"],
        winget_id: "OpenAI.Codex",
        npm_package: Some("@openai/codex"),
        config_dir: &[".codex"],
        skills_dir: &[".agents", "skills"],
        configs: &[
            ConfigDef { key: "config", label: "config.toml", rel: &[".codex", "config.toml"], format: "toml" },
            ConfigDef { key: "agents", label: "AGENTS.md (global)", rel: &[".codex", "AGENTS.md"], format: "md" },
        ],
        docs_url: "https://github.com/openai/codex",
    },
    CliDef {
        id: "antigravity",
        name: "Antigravity CLI",
        skill_tool: "antigravity",
        commands: &["agy", "antigravity-cli", "antigravity"],
        winget_id: "Google.AntigravityCLI",
        npm_package: None,
        config_dir: &[".gemini", "config"],
        skills_dir: &[".gemini", "config", "skills"],
        configs: &[
            ConfigDef { key: "config", label: "config.json", rel: &[".gemini", "config", "config.json"], format: "json" },
            ConfigDef { key: "mcp", label: "mcp_config.json", rel: &[".gemini", "config", "mcp_config.json"], format: "json" },
        ],
        docs_url: "https://antigravity.google/docs",
    },
];

#[derive(Debug, Clone, Serialize)]
pub struct CliConfigFile {
    pub key: String,
    pub label: String,
    pub path: String,
    pub format: String,
    pub exists: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CliInfo {
    pub id: String,
    pub name: String,
    pub skill_tool: String,
    pub installed: bool,
    pub version: Option<String>,
    pub exe_path: Option<String>,
    /// native / winget / npm / unknown, or "-" when not installed.
    pub source: String,
    pub winget_id: String,
    pub npm_package: Option<String>,
    pub config_dir: String,
    pub config_dir_exists: bool,
    pub skills_dir: String,
    pub skills_dir_exists: bool,
    pub config_files: Vec<CliConfigFile>,
    pub docs_url: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CliLatest {
    pub id: String,
    pub latest: Option<String>,
    pub source: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CliActionResult {
    pub id: String,
    pub ok: bool,
    pub exit_code: Option<i32>,
    pub command: String,
    pub log: String,
    pub info: CliInfo,
}

#[derive(Debug, Clone, Serialize)]
pub struct CliConfigDocument {
    pub id: String,
    pub key: String,
    pub path: String,
    pub format: String,
    pub content: String,
    pub exists: bool,
}

fn home() -> Result<PathBuf, String> {
    dirs::home_dir().ok_or_else(|| "Could not resolve user home directory".to_string())
}

fn join_home(home: &Path, rel: &[&str]) -> PathBuf {
    rel.iter().fold(home.to_path_buf(), |p, seg| p.join(seg))
}

fn find_def(id: &str) -> Result<&'static CliDef, String> {
    CLIS.iter()
        .find(|d| d.id == id)
        .ok_or_else(|| format!("Unknown CLI '{id}'."))
}

fn winget_links_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("Microsoft").join("WinGet").join("Links"))
}

fn winget_packages_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("Microsoft").join("WinGet").join("Packages"))
}

fn npm_global_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("npm"))
}

/// Folders searched for CLI executables. The app's PATH is a snapshot taken at launch,
/// so the usual install folders are added explicitly to catch fresh installs.
fn search_dirs() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    if let Some(h) = dirs::home_dir() {
        out.push(h.join(".local").join("bin"));
    }
    out.extend(winget_links_dir());
    out.extend(npm_global_dir());
    if let Some(path) = std::env::var_os("PATH") {
        out.extend(std::env::split_paths(&path));
    }
    let mut seen = std::collections::BTreeSet::new();
    out.retain(|p| seen.insert(p.to_string_lossy().to_ascii_lowercase()));
    out
}

/// The Antigravity IDE ships its own `antigravity` launcher; it is not the CLI.
fn is_ide_launcher(path: &Path) -> bool {
    let s = path.to_string_lossy().to_ascii_lowercase();
    s.contains("\\programs\\antigravity\\")
}

fn find_executable(def: &CliDef) -> Option<PathBuf> {
    let dirs = search_dirs();
    for cmd in def.commands {
        for dir in &dirs {
            for ext in ["exe", "cmd", "bat"] {
                let candidate = dir.join(format!("{cmd}.{ext}"));
                if candidate.is_file() && !is_ide_launcher(&candidate) {
                    return Some(candidate);
                }
            }
        }
    }
    // Portable winget packages that did not register a command alias.
    let pkgs = winget_packages_dir()?;
    let prefix = format!("{}_", def.winget_id.to_ascii_lowercase());
    for entry in fs::read_dir(&pkgs).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if !name.starts_with(&prefix) {
            continue;
        }
        if let Some(exe) = pick_package_exe(&entry.path(), def.commands) {
            return Some(exe);
        }
    }
    None
}

/// Picks the CLI entry point inside a portable winget package folder. Packages such as
/// Codex ship helper executables next to the real one (`codex-code-mode-host.exe`), so a
/// declared `entrypoint` wins, then an exact name, then a `<cmd>-<target-triple>.exe` build.
fn pick_package_exe(dir: &Path, commands: &[&str]) -> Option<PathBuf> {
    let mut exes: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(OsStr::to_str).is_some_and(|e| e.eq_ignore_ascii_case("exe")))
        .collect();
    exes.sort();

    for manifest in fs::read_dir(dir).ok()?.flatten().map(|e| e.path()) {
        let is_manifest = manifest
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|n| n.to_ascii_lowercase().ends_with("package.json"));
        if !is_manifest {
            continue;
        }
        let entry = read_utf8(&manifest)
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| v.get("entrypoint").and_then(|e| e.as_str()).map(str::to_string));
        if let Some(entry) = entry {
            let p = dir.join(entry);
            if p.is_file() {
                return Some(p);
            }
        }
    }

    let stem = |p: &PathBuf| {
        p.file_stem()
            .and_then(OsStr::to_str)
            .map(str::to_ascii_lowercase)
            .unwrap_or_default()
    };
    for cmd in commands {
        if let Some(p) = exes.iter().find(|p| stem(p) == *cmd) {
            return Some(p.clone());
        }
    }
    for cmd in commands {
        let triple = format!("{cmd}-x86_64-");
        let arm = format!("{cmd}-aarch64-");
        if let Some(p) = exes.iter().find(|p| {
            let s = stem(p);
            s.starts_with(&triple) || s.starts_with(&arm)
        }) {
            return Some(p.clone());
        }
    }
    exes.into_iter().next()
}

/// The executable Brain runs for this CLI, if it is installed.
pub fn executable_for(id: &str) -> Result<PathBuf, String> {
    let def = find_def(id)?;
    find_executable(def).ok_or_else(|| format!("{} is not installed.", def.name))
}

/// Same as `base_command` but for long-running agent processes: stdin is left to the caller.
pub(crate) fn process_command(program: &Path, args: &[String]) -> Command {
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    base_command(program, &refs)
}

pub(crate) fn run_capture_pub(
    program: &Path,
    args: &[&str],
    timeout: Duration,
) -> Result<(Option<i32>, String, String, bool), String> {
    run_capture(program, args, timeout).map(|o| (o.exit_code, o.stdout, o.stderr, o.timed_out))
}

fn classify_source(exe: &Path) -> String {
    let p = normalize_path(exe).to_ascii_lowercase();
    let under = |dir: Option<PathBuf>| {
        dir.map(|d| p.starts_with(&normalize_path(&d).to_ascii_lowercase()))
            .unwrap_or(false)
    };
    if under(dirs::home_dir().map(|h| h.join(".local").join("bin"))) {
        "native".to_string()
    } else if under(npm_global_dir()) {
        "npm".to_string()
    } else if under(dirs::data_local_dir().map(|d| d.join("Microsoft").join("WinGet"))) {
        "winget".to_string()
    } else {
        "unknown".to_string()
    }
}

pub fn parse_version(text: &str) -> Option<String> {
    let re = Regex::new(r"(\d+\.\d+\.\d+(?:-[0-9A-Za-z.\-]+)?)").expect("valid version regex");
    re.captures(text).and_then(|c| c.get(1)).map(|m| m.as_str().to_string())
}

pub fn parse_winget_version(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.trim().strip_prefix("Version:").map(|v| v.trim().to_string()))
        .filter(|v| !v.is_empty())
}

struct RunOutput {
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    timed_out: bool,
}

fn base_command(program: &Path, args: &[&str]) -> Command {
    let is_script = program
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"));
    let mut cmd = if is_script {
        let mut c = Command::new("cmd.exe");
        c.arg("/D").arg("/C").arg(program).args(args);
        c
    } else {
        let mut c = Command::new(program);
        c.args(args);
        c
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    scrub_host_env(&mut cmd);
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd
}

/// Drops the Claude Code host variables Brain may have inherited (see `inherited_host_vars`).
pub(crate) fn scrub_host_env(cmd: &mut Command) {
    for key in inherited_host_vars(std::env::vars_os().map(|(k, _)| k)) {
        cmd.env_remove(key);
    }
}

/// When Brain itself was started from inside a Claude Code session (for example
/// `npm run tauri:dev` in a Claude Code terminal), it inherits that session's
/// host variables. A child `claude` then thinks a host app owns its login and
/// fails with "Not logged in", so those variables are dropped. The user's own
/// settings (`ANTHROPIC_API_KEY`, `CLAUDE_CONFIG_DIR`, ...) are kept, and
/// `ANTHROPIC_BASE_URL` is only dropped when it came from such a host session.
fn inherited_host_vars(keys: impl Iterator<Item = std::ffi::OsString>) -> Vec<std::ffi::OsString> {
    let keys: Vec<_> = keys.collect();
    let nested = keys.iter().any(|k| k == "CLAUDECODE");
    if !nested {
        return Vec::new();
    }
    keys.into_iter()
        .filter(|k| {
            let k = k.to_string_lossy();
            k == "CLAUDECODE"
                || k == "CLAUDE_PID"
                || k == "CLAUDE_EFFORT"
                || k == "ANTHROPIC_BASE_URL"
                || k.starts_with("CLAUDE_CODE_")
                || k.starts_with("CLAUDE_AGENT_SDK_")
                || k.starts_with("CLAUDE_PREVIEW_")
        })
        .collect()
}

fn run_capture(program: &Path, args: &[&str], timeout: Duration) -> Result<RunOutput, String> {
    let mut child = base_command(program, args)
        .spawn()
        .map_err(|e| format!("Failed to start '{}': {e}", program.display()))?;

    let mut out_pipe = child.stdout.take();
    let mut err_pipe = child.stderr.take();
    let out_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(p) = out_pipe.as_mut() {
            let _ = p.read_to_end(&mut buf);
        }
        buf
    });
    let err_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(p) = err_pipe.as_mut() {
            let _ = p.read_to_end(&mut buf);
        }
        buf
    });

    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                timed_out = true;
                break child.wait().ok();
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(format!("Failed waiting for '{}': {e}", program.display())),
        }
    };

    let stdout = clean_output(&out_thread.join().unwrap_or_default());
    let stderr = clean_output(&err_thread.join().unwrap_or_default());
    Ok(RunOutput {
        exit_code: status.and_then(|s| s.code()),
        stdout,
        stderr,
        timed_out,
    })
}

/// Decodes console output and drops winget's spinner / progress-bar noise.
pub(crate) fn clean_output(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes).replace("\r\n", "\n");
    let ansi = Regex::new(r"\x1b\[[0-9;?]*[A-Za-z]").expect("valid ansi regex");
    let text = ansi.replace_all(&text, "");
    let mut lines: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        // Keep only what remains visible after carriage-return redraws.
        let line = raw.rsplit('\r').next().unwrap_or("").trim_end();
        let t = line.trim();
        if t.is_empty() && lines.last().is_some_and(|l: &String| l.is_empty()) {
            continue;
        }
        if matches!(t, "-" | "\\" | "|" | "/") || t.chars().all(|c| "█▒░ ".contains(c)) && !t.is_empty() {
            continue;
        }
        lines.push(line.to_string());
    }
    lines.join("\n").trim().to_string()
}

fn command_exists(name: &str) -> bool {
    search_dirs().iter().any(|d| {
        ["exe", "cmd"].iter().any(|ext| d.join(format!("{name}.{ext}")).is_file())
    }) || dirs::data_local_dir()
        .map(|d| d.join("Microsoft").join("WindowsApps").join(format!("{name}.exe")).exists())
        .unwrap_or(false)
}

fn resolve_tool(name: &str) -> PathBuf {
    for d in search_dirs() {
        for ext in ["exe", "cmd"] {
            let p = d.join(format!("{name}.{ext}"));
            if p.is_file() {
                return p;
            }
        }
    }
    if let Some(p) = dirs::data_local_dir()
        .map(|d| d.join("Microsoft").join("WindowsApps").join(format!("{name}.exe")))
        .filter(|p| p.exists())
    {
        return p;
    }
    PathBuf::from(format!("{name}.exe"))
}

pub fn detect(id: &str) -> Result<CliInfo, String> {
    let def = find_def(id)?;
    let home = home()?;
    let config_dir = join_home(&home, def.config_dir);
    let skills_dir = join_home(&home, def.skills_dir);
    let config_files = def
        .configs
        .iter()
        .map(|c| {
            let p = join_home(&home, c.rel);
            CliConfigFile {
                key: c.key.to_string(),
                label: c.label.to_string(),
                exists: p.is_file(),
                path: p.to_string_lossy().to_string(),
                format: c.format.to_string(),
            }
        })
        .collect();

    let exe = find_executable(def);
    let mut version = None;
    let mut error = None;
    if let Some(ref exe_path) = exe {
        match run_capture(exe_path, &["--version"], VERSION_TIMEOUT) {
            Ok(out) if out.timed_out => error = Some("`--version` timed out.".to_string()),
            Ok(out) => {
                version = parse_version(&out.stdout).or_else(|| parse_version(&out.stderr));
                if version.is_none() {
                    error = Some(format!(
                        "Could not read a version from `--version`: {}",
                        first_line(&out.stdout, &out.stderr)
                    ));
                }
            }
            Err(e) => error = Some(e),
        }
    }

    Ok(CliInfo {
        id: def.id.to_string(),
        name: def.name.to_string(),
        skill_tool: def.skill_tool.to_string(),
        installed: exe.is_some(),
        version,
        source: exe.as_deref().map(classify_source).unwrap_or_else(|| "-".to_string()),
        exe_path: exe.map(|p| p.to_string_lossy().to_string()),
        winget_id: def.winget_id.to_string(),
        npm_package: def.npm_package.map(str::to_string),
        config_dir_exists: config_dir.is_dir(),
        config_dir: config_dir.to_string_lossy().to_string(),
        skills_dir_exists: skills_dir.is_dir(),
        skills_dir: skills_dir.to_string_lossy().to_string(),
        config_files,
        docs_url: def.docs_url.to_string(),
        error,
    })
}

fn first_line(a: &str, b: &str) -> String {
    let line = a.lines().chain(b.lines()).find(|l| !l.trim().is_empty()).unwrap_or("(no output)");
    line.chars().take(160).collect()
}

pub fn list() -> Vec<CliInfo> {
    let handles: Vec<_> = CLIS
        .iter()
        .map(|d| std::thread::spawn(move || detect(d.id)))
        .collect();
    handles
        .into_iter()
        .zip(CLIS)
        .map(|(h, d)| match h.join() {
            Ok(Ok(info)) => info,
            Ok(Err(e)) => placeholder(d, e),
            Err(_) => placeholder(d, "Detection crashed.".to_string()),
        })
        .collect()
}

fn placeholder(def: &CliDef, error: String) -> CliInfo {
    CliInfo {
        id: def.id.to_string(),
        name: def.name.to_string(),
        skill_tool: def.skill_tool.to_string(),
        installed: false,
        version: None,
        exe_path: None,
        source: "-".to_string(),
        winget_id: def.winget_id.to_string(),
        npm_package: def.npm_package.map(str::to_string),
        config_dir: String::new(),
        config_dir_exists: false,
        skills_dir: String::new(),
        skills_dir_exists: false,
        config_files: Vec::new(),
        docs_url: def.docs_url.to_string(),
        error: Some(error),
    }
}

fn latest_cache() -> &'static Mutex<HashMap<String, (Instant, CliLatest)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (Instant, CliLatest)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lookup_latest(def: &CliDef, source: &str) -> CliLatest {
    let make = |latest: Option<String>, src: &str, error: Option<String>| CliLatest {
        id: def.id.to_string(),
        latest,
        source: src.to_string(),
        error,
    };

    if source == "npm" {
        if let Some(pkg) = def.npm_package {
            let npm = resolve_tool("npm");
            return match run_capture(&npm, &["view", pkg, "version"], LOOKUP_TIMEOUT) {
                Ok(out) => match parse_version(&out.stdout) {
                    Some(v) => make(Some(v), "npm", None),
                    None => make(None, "npm", Some(first_line(&out.stdout, &out.stderr))),
                },
                Err(e) => make(None, "npm", Some(e)),
            };
        }
    }

    if !command_exists("winget") {
        return make(None, "winget", Some("winget is not available on this machine.".to_string()));
    }
    let winget = resolve_tool("winget");
    let args = [
        "show", "--id", def.winget_id, "--exact", "--source", "winget",
        "--disable-interactivity", "--accept-source-agreements",
    ];
    match run_capture(&winget, &args, LOOKUP_TIMEOUT) {
        Ok(out) if out.timed_out => make(None, "winget", Some("winget timed out.".to_string())),
        Ok(out) => match parse_winget_version(&out.stdout) {
            Some(v) => make(Some(v), "winget", None),
            None => make(None, "winget", Some(first_line(&out.stdout, &out.stderr))),
        },
        Err(e) => make(None, "winget", Some(e)),
    }
}

/// Latest published version for each CLI, cached for ten minutes unless `force`.
pub fn latest_versions(force: bool) -> Vec<CliLatest> {
    let infos = list();
    let handles: Vec<_> = CLIS
        .iter()
        .zip(infos)
        .map(|(def, info)| {
            std::thread::spawn(move || {
                let key = format!("{}|{}", def.id, info.source);
                if !force {
                    if let Ok(cache) = latest_cache().lock() {
                        if let Some((at, hit)) = cache.get(&key) {
                            if at.elapsed() < LATEST_CACHE_TTL {
                                return hit.clone();
                            }
                        }
                    }
                }
                let fresh = lookup_latest(def, &info.source);
                if fresh.error.is_none() {
                    if let Ok(mut cache) = latest_cache().lock() {
                        cache.insert(key, (Instant::now(), fresh.clone()));
                    }
                }
                fresh
            })
        })
        .collect();
    handles
        .into_iter()
        .zip(CLIS)
        .map(|(h, def)| {
            h.join().unwrap_or_else(|_| CliLatest {
                id: def.id.to_string(),
                latest: None,
                source: "-".to_string(),
                error: Some("Lookup crashed.".to_string()),
            })
        })
        .collect()
}

fn winget_action(verb: &str, def: &CliDef) -> (PathBuf, Vec<String>) {
    let args = [
        verb, "--id", def.winget_id, "--exact", "--source", "winget", "--silent",
        "--accept-package-agreements", "--accept-source-agreements", "--disable-interactivity",
    ];
    (resolve_tool("winget"), args.iter().map(|s| s.to_string()).collect())
}

fn run_action(def: &CliDef, program: PathBuf, args: Vec<String>) -> Result<CliActionResult, String> {
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let command = format!(
        "{} {}",
        program.file_name().and_then(OsStr::to_str).unwrap_or("?"),
        args.join(" ")
    );
    let out = run_capture(&program, &arg_refs, ACTION_TIMEOUT)?;
    if let Ok(mut cache) = latest_cache().lock() {
        cache.retain(|k, _| !k.starts_with(&format!("{}|", def.id)));
    }
    let mut log = String::new();
    log.push_str(&format!("> {command}\n"));
    if !out.stdout.is_empty() {
        log.push_str(&out.stdout);
        log.push('\n');
    }
    if !out.stderr.is_empty() {
        log.push_str(&out.stderr);
        log.push('\n');
    }
    if out.timed_out {
        log.push_str("Timed out and was stopped.\n");
    }
    log.push_str(&format!(
        "Exit code: {}",
        out.exit_code.map(|c| c.to_string()).unwrap_or_else(|| "-".to_string())
    ));
    let info = detect(def.id)?;
    Ok(CliActionResult {
        id: def.id.to_string(),
        ok: !out.timed_out && out.exit_code == Some(0),
        exit_code: out.exit_code,
        command,
        log,
        info,
    })
}

pub fn install(id: &str) -> Result<CliActionResult, String> {
    let def = find_def(id)?;
    if find_executable(def).is_some() {
        return Err(format!("{} is already installed. Use Update instead.", def.name));
    }
    if !command_exists("winget") {
        return Err("winget (App Installer) is required to install CLIs. Install it from the Microsoft Store.".to_string());
    }
    let (program, args) = winget_action("install", def);
    run_action(def, program, args)
}

pub fn update(id: &str) -> Result<CliActionResult, String> {
    let def = find_def(id)?;
    let exe = find_executable(def)
        .ok_or_else(|| format!("{} is not installed. Use Install instead.", def.name))?;
    let source = classify_source(&exe);
    let (program, args) = match (def.id, source.as_str()) {
        ("claude", "native") => (exe, vec!["update".to_string()]),
        (_, "npm") if def.npm_package.is_some() => (
            resolve_tool("npm"),
            vec!["install".into(), "-g".into(), format!("{}@latest", def.npm_package.unwrap_or_default())],
        ),
        _ => {
            if !command_exists("winget") {
                return Err("winget (App Installer) is required to update this CLI.".to_string());
            }
            winget_action("upgrade", def)
        }
    };
    run_action(def, program, args)
}

fn config_def(def: &CliDef, key: &str) -> Result<(&'static ConfigDef, PathBuf), String> {
    let cfg = def
        .configs
        .iter()
        .find(|c| c.key == key)
        .ok_or_else(|| format!("Unknown config file '{key}' for {}.", def.name))?;
    Ok((cfg, join_home(&home()?, cfg.rel)))
}

pub fn read_config(id: &str, key: &str) -> Result<CliConfigDocument, String> {
    let def = find_def(id)?;
    let (cfg, path) = config_def(def, key)?;
    let exists = path.is_file();
    let content = if exists { read_utf8(&path)? } else { String::new() };
    Ok(CliConfigDocument {
        id: def.id.to_string(),
        key: cfg.key.to_string(),
        path: path.to_string_lossy().to_string(),
        format: cfg.format.to_string(),
        content,
        exists,
    })
}

pub fn validate_config(format: &str, content: &str) -> Result<(), String> {
    match format {
        "json" => serde_json::from_str::<serde_json::Value>(content)
            .map(|_| ())
            .map_err(|e| format!("Invalid JSON: {e}")),
        "toml" => toml::from_str::<toml::Value>(content)
            .map(|_| ())
            .map_err(|e| format!("Invalid TOML: {}", e.to_string().trim())),
        _ => Ok(()),
    }
}

/// Validates, backs up the current file to `<file>.bak`, then writes atomically.
pub fn save_config(id: &str, key: &str, content: &str) -> Result<CliConfigDocument, String> {
    let def = find_def(id)?;
    let (cfg, path) = config_def(def, key)?;
    validate_config(cfg.format, content)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create '{}': {e}", parent.display()))?;
    }
    if path.is_file() {
        let bak = PathBuf::from(format!("{}.bak", path.to_string_lossy()));
        fs::copy(&path, &bak).map_err(|e| format!("Failed to back up to '{}': {e}", bak.display()))?;
    }
    let tmp = PathBuf::from(format!("{}.brain-tmp", path.to_string_lossy()));
    fs::write(&tmp, content.as_bytes())
        .map_err(|e| format!("Failed to write '{}': {e}", tmp.display()))?;
    fs::rename(&tmp, &path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("Failed to replace '{}': {e}", path.display())
    })?;
    read_config(id, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_vars_are_dropped_only_inside_claude_code() {
        let keys = |v: &[&str]| v.iter().map(std::ffi::OsString::from).collect::<Vec<_>>();
        let outside = keys(&["PATH", "ANTHROPIC_BASE_URL", "CLAUDE_CODE_ENTRYPOINT"]);
        assert!(inherited_host_vars(outside.into_iter()).is_empty());

        let inside = keys(&[
            "PATH",
            "CLAUDECODE",
            "CLAUDE_CODE_ENTRYPOINT",
            "ANTHROPIC_BASE_URL",
            "ANTHROPIC_API_KEY",
            "CLAUDE_CONFIG_DIR",
        ]);
        let dropped = inherited_host_vars(inside.into_iter());
        assert_eq!(
            dropped,
            keys(&["CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT", "ANTHROPIC_BASE_URL"])
        );
    }

    #[test]
    fn parses_cli_and_winget_versions() {
        assert_eq!(parse_version("2.1.329 (Claude Code)").as_deref(), Some("2.1.329"));
        assert_eq!(parse_version("codex-cli 0.172.0").as_deref(), Some("0.172.0"));
        assert_eq!(parse_version("v1.5.17-beta.2\n").as_deref(), Some("1.5.17-beta.2"));
        assert_eq!(parse_version("no version here"), None);
        let show = "Found Antigravity CLI [Google.AntigravityCLI]\nVersion: 1.5.17\nPublisher: Google\n";
        assert_eq!(parse_winget_version(show).as_deref(), Some("1.5.17"));
    }

    #[test]
    fn validates_json_and_toml() {
        assert!(validate_config("json", "{\"a\": 1}").is_ok());
        assert!(validate_config("json", "{a: 1}").is_err());
        assert!(validate_config("toml", "model = \"o3\"\n[x]\ny = 1").is_ok());
        assert!(validate_config("toml", "model = ").is_err());
        assert!(validate_config("md", "anything <goes>").is_ok());
    }

    #[test]
    fn clean_output_drops_spinner_noise() {
        let raw = "   - \r   \\ \r\nFound X\r\n  ██████▒▒▒  1 MB / 2 MB\rSuccessfully installed\r\n";
        let cleaned = clean_output(raw.as_bytes());
        assert!(cleaned.contains("Found X"));
        assert!(cleaned.contains("Successfully installed"));
        assert!(!cleaned.contains('█'));
    }
}
