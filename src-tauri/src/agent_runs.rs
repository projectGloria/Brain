//! Runs a coding-agent CLI (Claude Code, Codex, Antigravity) non-interactively inside a
//! repository, streams its output to the UI as events, and lets the UI stop it or send a
//! follow-up turn into the same conversation.

use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

use crate::brain_core::read_utf8;
use crate::clis;

pub const EVENT_OUTPUT: &str = "agent-run-output";
pub const EVENT_SESSION: &str = "agent-run-session";
pub const EVENT_EXIT: &str = "agent-run-exit";

const MODELS_TIMEOUT: Duration = Duration::from_secs(40);
const MODELS_CACHE_TTL: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone, Deserialize)]
pub struct AgentRunRequest {
    /// Chosen by the UI so output events can be matched before this call returns.
    pub run_id: String,
    pub cli_id: String,
    pub repo_path: String,
    pub prompt: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    /// "plan" (read-only), "edits" (may edit files) or "full" (no permission checks).
    pub permission: String,
    /// Conversation to continue. "last" means "the most recent one in this folder".
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentRunStarted {
    pub run_id: String,
    pub command: String,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct OutputEvent<'a> {
    run_id: &'a str,
    /// "out", "err" or "info".
    stream: &'a str,
    text: String,
}

#[derive(Debug, Clone, Serialize)]
struct SessionEvent<'a> {
    run_id: &'a str,
    session_id: String,
}

#[derive(Debug, Clone, Serialize)]
struct ExitEvent<'a> {
    run_id: &'a str,
    exit_code: Option<i32>,
    cancelled: bool,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelOption {
    pub id: String,
    pub label: String,
    /// Effort levels this model accepts; empty means "use the catalog-wide list".
    pub efforts: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelCatalog {
    pub cli_id: String,
    pub models: Vec<ModelOption>,
    pub efforts: Vec<String>,
    pub source: String,
    pub error: Option<String>,
}

struct RunHandle {
    pid: u32,
    cancelled: Arc<AtomicBool>,
}

fn running() -> &'static Mutex<HashMap<String, RunHandle>> {
    static RUNS: OnceLock<Mutex<HashMap<String, RunHandle>>> = OnceLock::new();
    RUNS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn models_cache() -> &'static Mutex<HashMap<String, (Instant, ModelCatalog)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (Instant, ModelCatalog)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// A random-enough v4-shaped UUID for Claude's `--session-id` (no extra crates needed).
pub fn new_uuid() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let mut h = Sha256::new();
    h.update(nanos.to_le_bytes());
    h.update(std::process::id().to_le_bytes());
    h.update(COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    let mut b: [u8; 16] = h.finalize()[..16].try_into().expect("16 bytes");
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

fn clean_opt(v: &Option<String>) -> Option<String> {
    v.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("default"))
        .map(str::to_string)
}

/// Rejects values that would be read as flags or break the TOML override.
fn safe_token(label: &str, v: &str) -> Result<(), String> {
    let ok = !v.starts_with('-')
        && v.chars().all(|c| c.is_ascii_alphanumeric() || "._-:/[]@".contains(c));
    if ok {
        Ok(())
    } else {
        Err(format!("Invalid {label} '{v}'."))
    }
}

/// The program arguments for one turn, plus the Claude session id when one was assigned.
#[derive(Debug, PartialEq)]
pub struct BuiltArgs {
    pub args: Vec<String>,
    pub session_id: Option<String>,
    /// The prompt is sent on stdin instead of argv (needed for `.cmd` shims).
    pub prompt_on_stdin: bool,
}

pub fn build_args(req: &AgentRunRequest, via_script: bool, new_session: &str) -> Result<BuiltArgs, String> {
    let model = clean_opt(&req.model);
    let effort = clean_opt(&req.effort);
    if let Some(ref m) = model {
        safe_token("model", m)?;
    }
    if let Some(ref e) = effort {
        safe_token("effort", e)?;
    }
    let resume = clean_opt(&req.session_id);
    if let Some(ref s) = resume {
        safe_token("session id", s)?;
    }
    let prompt = req.prompt.trim().to_string();
    if prompt.is_empty() {
        return Err("The prompt is empty.".to_string());
    }
    // A prompt that starts with '-' would be parsed as a flag.
    let prompt_arg = if prompt.starts_with('-') { format!(" {prompt}") } else { prompt };
    if !matches!(req.permission.as_str(), "plan" | "edits" | "full") {
        return Err(format!("Unknown permission mode '{}'.", req.permission));
    }

    let mut a: Vec<String> = Vec::new();
    let mut session_id = None;
    let mut prompt_on_stdin = false;
    match req.cli_id.as_str() {
        "claude" => {
            a.extend(strings(&["-p", "--output-format", "stream-json", "--verbose"]));
            if let Some(m) = model {
                a.extend(["--model".into(), m]);
            }
            if let Some(e) = effort {
                a.extend(["--effort".into(), e]);
            }
            match req.permission.as_str() {
                "plan" => a.extend(strings(&["--permission-mode", "plan"])),
                "edits" => a.extend(strings(&["--permission-mode", "acceptEdits"])),
                _ => a.push("--dangerously-skip-permissions".into()),
            }
            match resume.as_deref() {
                Some("last") => a.push("--continue".into()),
                Some(id) => {
                    a.extend(["--resume".into(), id.to_string()]);
                    session_id = Some(id.to_string());
                }
                None => {
                    a.extend(["--session-id".into(), new_session.to_string()]);
                    session_id = Some(new_session.to_string());
                }
            }
            if via_script {
                prompt_on_stdin = true;
            } else {
                a.push(prompt_arg);
            }
        }
        "codex" => {
            a.push("exec".into());
            if resume.is_some() {
                a.push("resume".into());
            } else {
                a.extend(strings(&["--color", "never"]));
            }
            a.push("--skip-git-repo-check".into());
            if let Some(m) = model {
                a.extend(["-m".into(), m]);
            }
            if let Some(e) = effort {
                a.extend(["-c".into(), format!("model_reasoning_effort=\"{e}\"")]);
            }
            let sandbox = match req.permission.as_str() {
                "plan" => Some("read-only"),
                "edits" => Some("workspace-write"),
                _ => None,
            };
            match (sandbox, resume.is_some()) {
                (Some(s), false) => a.extend(["-s".into(), s.to_string()]),
                (Some(s), true) => a.extend(["-c".into(), format!("sandbox_mode=\"{s}\"")]),
                (None, _) => a.push("--dangerously-bypass-approvals-and-sandbox".into()),
            }
            match resume.as_deref() {
                Some("last") => a.push("--last".into()),
                Some(id) => {
                    a.push(id.to_string());
                    session_id = Some(id.to_string());
                }
                None => {}
            }
            if via_script {
                a.push("-".into());
                prompt_on_stdin = true;
            } else {
                a.push(prompt_arg);
            }
        }
        "antigravity" => {
            if let Some(m) = model {
                a.extend(["--model".into(), m]);
            }
            if let Some(e) = effort {
                a.extend(["--effort".into(), e]);
            }
            match req.permission.as_str() {
                "plan" => a.extend(strings(&["--mode", "plan"])),
                "edits" => a.extend(strings(&["--mode", "accept-edits"])),
                _ => a.push("--dangerously-skip-permissions".into()),
            }
            match resume.as_deref() {
                Some("last") => a.push("--continue".into()),
                Some(id) => {
                    a.extend(["--conversation".into(), id.to_string()]);
                    session_id = Some(id.to_string());
                }
                None => {}
            }
            // Flags first: the prompt must be the last argument.
            a.extend(["-p".into(), prompt_arg]);
            if via_script {
                return Err("Antigravity CLI must be the agy.exe build to run prompts.".to_string());
            }
        }
        other => return Err(format!("Unknown CLI '{other}'.")),
    }
    Ok(BuiltArgs { args: a, session_id, prompt_on_stdin })
}

/// Shell-style rendering of the command, for display only.
pub fn display_command(program: &Path, args: &[String]) -> String {
    let name = program.file_stem().and_then(OsStr::to_str).unwrap_or("?");
    let name = if name.starts_with("codex-") { "codex" } else { name };
    let mut out = name.to_string();
    for a in args {
        out.push(' ');
        if a.is_empty() || a.chars().any(|c| c.is_whitespace() || "\"'&|<>^".contains(c)) {
            out.push('"');
            out.push_str(&a.replace('"', "\\\""));
            out.push('"');
        } else {
            out.push_str(a);
        }
    }
    out
}

fn is_script(program: &Path) -> bool {
    program
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"))
}

fn short(text: &str, max: usize) -> String {
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= max {
        one_line
    } else {
        let cut: String = one_line.chars().take(max).collect();
        format!("{cut}…")
    }
}

/// Turns one line of Claude's `stream-json` output into readable log lines.
pub fn format_claude_event(line: &str) -> Vec<(String, &'static str)> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return vec![(line.to_string(), "out")];
    };
    let mut out = Vec::new();
    match v.get("type").and_then(|t| t.as_str()) {
        Some("system") if v.get("subtype").and_then(|s| s.as_str()) == Some("init") => {
            let model = v.get("model").and_then(|m| m.as_str()).unwrap_or("?");
            out.push((format!("● session started · model {model}"), "info"));
        }
        Some("assistant") => {
            let items = v
                .pointer("/message/content")
                .and_then(|c| c.as_array())
                .cloned()
                .unwrap_or_default();
            for item in items {
                match item.get("type").and_then(|t| t.as_str()) {
                    Some("text") => {
                        let text = item.get("text").and_then(|t| t.as_str()).unwrap_or("");
                        if !text.trim().is_empty() {
                            out.push((text.trim_end().to_string(), "out"));
                        }
                    }
                    Some("tool_use") => {
                        let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("tool");
                        let input = item.get("input").cloned().unwrap_or_default();
                        let detail = ["command", "file_path", "pattern", "path", "url", "description", "skill", "prompt"]
                            .iter()
                            .find_map(|k| input.get(*k).and_then(|x| x.as_str()))
                            .map(|s| short(s, 160))
                            .unwrap_or_default();
                        out.push((format!("→ {name} {detail}").trim_end().to_string(), "info"));
                    }
                    _ => {}
                }
            }
        }
        Some("user") => {
            let items = v
                .pointer("/message/content")
                .and_then(|c| c.as_array())
                .cloned()
                .unwrap_or_default();
            for item in items {
                if item.get("is_error").and_then(|e| e.as_bool()) == Some(true) {
                    let text = match item.get("content") {
                        Some(serde_json::Value::String(s)) => s.clone(),
                        Some(other) => other.to_string(),
                        None => String::new(),
                    };
                    out.push((format!("  ✗ {}", short(&text, 220)), "err"));
                }
            }
        }
        Some("result") => {
            let is_error = v.get("is_error").and_then(|e| e.as_bool()).unwrap_or(false);
            let secs = v.get("duration_ms").and_then(|d| d.as_f64()).unwrap_or(0.0) / 1000.0;
            let cost = v
                .get("total_cost_usd")
                .and_then(|c| c.as_f64())
                .map(|c| format!(" · ${c:.4}"))
                .unwrap_or_default();
            if is_error {
                if let Some(r) = v.get("result").and_then(|r| r.as_str()) {
                    out.push((r.to_string(), "err"));
                }
            }
            let sub = v.get("subtype").and_then(|s| s.as_str()).unwrap_or("done");
            out.push((format!("● {sub} in {secs:.1}s{cost}"), "info"));
        }
        _ => {}
    }
    out
}

fn codex_session_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)session id:\s*([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})")
            .expect("valid session regex")
    })
}

fn emit_line(app: &AppHandle, run_id: &str, stream: &'static str, text: String) {
    let _ = app.emit(EVENT_OUTPUT, OutputEvent { run_id, stream, text });
}

fn pump<R: Read + Send + 'static>(
    app: AppHandle,
    run_id: String,
    cli_id: String,
    pipe: R,
    stream: &'static str,
    session_known: Arc<AtomicBool>,
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
            if cli_id == "claude" && stream == "out" {
                for (text, s) in format_claude_event(&line) {
                    emit_line(&app, &run_id, s, text);
                }
                continue;
            }
            if cli_id == "codex" && !session_known.load(Ordering::Relaxed) {
                if let Some(c) = codex_session_re().captures(&line) {
                    session_known.store(true, Ordering::Relaxed);
                    let _ = app.emit(
                        EVENT_SESSION,
                        SessionEvent { run_id: &run_id, session_id: c[1].to_string() },
                    );
                }
            }
            for l in line.split('\n') {
                emit_line(&app, &run_id, stream, l.to_string());
            }
        }
    })
}

pub fn start(app: AppHandle, req: AgentRunRequest) -> Result<AgentRunStarted, String> {
    if req.run_id.trim().is_empty() {
        return Err("Missing run id.".to_string());
    }
    let repo = PathBuf::from(req.repo_path.trim());
    if !repo.is_dir() {
        return Err(format!("Repository folder '{}' does not exist.", repo.display()));
    }
    let program = clis::executable_for(&req.cli_id)?;
    let via_script = is_script(&program);
    let built = build_args(&req, via_script, &new_uuid())?;
    let command = display_command(&program, &built.args);

    let mut cmd: Command = clis::process_command(&program, &built.args);
    cmd.current_dir(&repo)
        .env("NO_COLOR", "1")
        .env("FORCE_COLOR", "0")
        .stdin(if built.prompt_on_stdin { Stdio::piped() } else { Stdio::null() });
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to start '{}': {e}", program.display()))?;

    if built.prompt_on_stdin {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(req.prompt.trim().as_bytes());
        }
    }

    let cancelled = Arc::new(AtomicBool::new(false));
    if let Ok(mut map) = running().lock() {
        map.insert(
            req.run_id.clone(),
            RunHandle { pid: child.id(), cancelled: cancelled.clone() },
        );
    }

    emit_line(&app, &req.run_id, "info", format!("$ {command}"));
    emit_line(&app, &req.run_id, "info", format!("  in {}", repo.display()));

    let session_known = Arc::new(AtomicBool::new(built.session_id.is_some()));
    let out_t = child.stdout.take().map(|p| {
        pump(app.clone(), req.run_id.clone(), req.cli_id.clone(), p, "out", session_known.clone())
    });
    let err_t = child.stderr.take().map(|p| {
        pump(app.clone(), req.run_id.clone(), req.cli_id.clone(), p, "err", session_known.clone())
    });

    let run_id = req.run_id.clone();
    let app_wait = app.clone();
    std::thread::spawn(move || {
        let status = child.wait();
        for t in [out_t, err_t].into_iter().flatten() {
            let _ = t.join();
        }
        if let Ok(mut map) = running().lock() {
            map.remove(&run_id);
        }
        let (exit_code, error) = match status {
            Ok(s) => (s.code(), None),
            Err(e) => (None, Some(format!("Failed waiting for the process: {e}"))),
        };
        let _ = app_wait.emit(
            EVENT_EXIT,
            ExitEvent {
                run_id: &run_id,
                exit_code,
                cancelled: cancelled.load(Ordering::Relaxed),
                error,
            },
        );
    });

    Ok(AgentRunStarted { run_id: req.run_id, command, session_id: built.session_id })
}

/// Stops a run and every process it started (agents spawn shells and tools).
pub fn cancel(run_id: &str) -> Result<bool, String> {
    let pid = {
        let map = running().lock().map_err(|_| "Run table is poisoned.".to_string())?;
        match map.get(run_id) {
            Some(h) => {
                h.cancelled.store(true, Ordering::Relaxed);
                h.pid
            }
            None => return Ok(false),
        }
    };
    kill_tree(pid)?;
    Ok(true)
}

/// Stops every run; called when the app closes so no agent keeps editing in the background.
pub fn cancel_all() {
    let pids: Vec<u32> = running()
        .lock()
        .map(|m| m.values().map(|h| h.pid).collect())
        .unwrap_or_default();
    for pid in pids {
        let _ = kill_tree(pid);
    }
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

const CLAUDE_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

fn claude_catalog() -> ModelCatalog {
    let models = [
        ("fable", "Fable (latest alias)"),
        ("opus", "Opus (latest alias)"),
        ("sonnet", "Sonnet (latest alias)"),
        ("haiku", "Haiku (latest alias)"),
        ("claude-fable-5-2", "Claude Fable 5.2"),
        ("claude-opus-5-5", "Claude Opus 5.5"),
        ("claude-sonnet-5-5", "Claude Sonnet 5.5"),
        ("claude-haiku-5-5", "Claude Haiku 5.5"),
    ];
    ModelCatalog {
        cli_id: "claude".into(),
        models: models
            .iter()
            .map(|(id, label)| ModelOption { id: id.to_string(), label: label.to_string(), efforts: vec![] })
            .collect(),
        efforts: strings(CLAUDE_EFFORTS),
        source: "built-in list".into(),
        error: None,
    }
}

pub fn parse_codex_models(json: &str) -> Result<Vec<ModelOption>, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("Invalid models cache: {e}"))?;
    let list = v
        .get("models")
        .and_then(|m| m.as_array())
        .ok_or_else(|| "Models cache has no 'models' list.".to_string())?;
    Ok(list
        .iter()
        .filter(|m| m.get("visibility").and_then(|x| x.as_str()).unwrap_or("list") == "list")
        .filter_map(|m| {
            let id = m.get("slug").and_then(|s| s.as_str())?.to_string();
            let label = m
                .get("display_name")
                .and_then(|s| s.as_str())
                .unwrap_or(&id)
                .to_string();
            let efforts = m
                .get("supported_reasoning_levels")
                .and_then(|l| l.as_array())
                .map(|levels| {
                    levels
                        .iter()
                        .filter_map(|l| {
                            l.get("effort").and_then(|e| e.as_str()).or_else(|| l.as_str()).map(str::to_string)
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(ModelOption { id, label, efforts })
        })
        .collect())
}

fn codex_catalog() -> ModelCatalog {
    let fallback_efforts = strings(&["minimal", "low", "medium", "high", "xhigh"]);
    let path = dirs::home_dir().map(|h| h.join(".codex").join("models_cache.json"));
    let parsed = path
        .as_deref()
        .filter(|p| p.is_file())
        .ok_or_else(|| "No ~/.codex/models_cache.json yet; run Codex once to fill it.".to_string())
        .and_then(read_utf8)
        .and_then(|t| parse_codex_models(&t));
    match parsed {
        Ok(models) => {
            let mut efforts: Vec<String> = Vec::new();
            for e in models.iter().flat_map(|m| m.efforts.iter()) {
                if !efforts.contains(e) {
                    efforts.push(e.clone());
                }
            }
            ModelCatalog {
                cli_id: "codex".into(),
                models,
                efforts: if efforts.is_empty() { fallback_efforts } else { efforts },
                source: "~/.codex/models_cache.json".into(),
                error: None,
            }
        }
        Err(e) => ModelCatalog {
            cli_id: "codex".into(),
            models: vec![],
            efforts: fallback_efforts,
            source: "-".into(),
            error: Some(e),
        },
    }
}

pub fn parse_agy_models(text: &str) -> Vec<ModelOption> {
    text.lines()
        .filter_map(|l| {
            let (id, label) = l.split_once('\t')?;
            let id = id.trim();
            if id.is_empty() || id.contains(' ') {
                return None;
            }
            Some(ModelOption { id: id.to_string(), label: label.trim().to_string(), efforts: vec![] })
        })
        .collect()
}

fn antigravity_catalog() -> ModelCatalog {
    let efforts = strings(CLAUDE_EFFORTS);
    let result = clis::executable_for("antigravity").and_then(|exe| {
        clis::run_capture_pub(&exe, &["models"], MODELS_TIMEOUT).and_then(|(code, out, err, timed_out)| {
            if timed_out {
                return Err("`agy models` timed out.".to_string());
            }
            let models = parse_agy_models(&out);
            if models.is_empty() {
                let msg = err.lines().chain(out.lines()).find(|l| !l.trim().is_empty()).unwrap_or("no output");
                return Err(format!("`agy models` returned no models (exit {}): {msg}", code.unwrap_or(-1)));
            }
            Ok(models)
        })
    });
    match result {
        Ok(models) => ModelCatalog {
            cli_id: "antigravity".into(),
            models,
            efforts,
            source: "agy models".into(),
            error: None,
        },
        Err(e) => ModelCatalog {
            cli_id: "antigravity".into(),
            models: vec![],
            efforts,
            source: "-".into(),
            error: Some(e),
        },
    }
}

/// Models and effort levels the UI offers for a CLI (cached for 30 minutes unless `force`).
pub fn model_catalog(cli_id: &str, force: bool) -> Result<ModelCatalog, String> {
    if !force {
        if let Ok(cache) = models_cache().lock() {
            if let Some((at, hit)) = cache.get(cli_id) {
                if at.elapsed() < MODELS_CACHE_TTL {
                    return Ok(hit.clone());
                }
            }
        }
    }
    let catalog = match cli_id {
        "claude" => claude_catalog(),
        "codex" => codex_catalog(),
        "antigravity" => antigravity_catalog(),
        other => return Err(format!("Unknown CLI '{other}'.")),
    };
    if catalog.error.is_none() {
        if let Ok(mut cache) = models_cache().lock() {
            cache.insert(cli_id.to_string(), (Instant::now(), catalog.clone()));
        }
    }
    Ok(catalog)
}

const AUTH_TIMEOUT: Duration = Duration::from_secs(20);

/// Whether a CLI has its own login. `state` is "signed_in", "signed_out" or
/// "unknown" (the CLI has no status command, or it could not be read).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AuthStatus {
    pub cli_id: String,
    pub state: String,
    pub detail: Option<String>,
}

fn auth(cli_id: &str, state: &str, detail: Option<String>) -> AuthStatus {
    AuthStatus {
        cli_id: cli_id.into(),
        state: state.into(),
        detail,
    }
}

/// Reads `claude auth status` (JSON with `loggedIn` and `authMethod`).
fn parse_claude_auth(stdout: &str) -> Option<(bool, Option<String>)> {
    let start = stdout.find('{')?;
    let end = stdout.rfind('}')?;
    let v: serde_json::Value = serde_json::from_str(stdout.get(start..=end)?).ok()?;
    let logged_in = v.get("loggedIn")?.as_bool()?;
    let method = v
        .get("authMethod")
        .and_then(|m| m.as_str())
        .filter(|m| !m.is_empty() && *m != "none")
        .map(|m| format!("via {m}"));
    Some((logged_in, method))
}

pub fn auth_status(cli_id: &str) -> Result<AuthStatus, String> {
    let program = clis::executable_for(cli_id)?;
    match cli_id {
        "claude" => {
            let (_, stdout, stderr, timed_out) =
                clis::run_capture_pub(&program, &["auth", "status"], AUTH_TIMEOUT)?;
            if timed_out {
                return Ok(auth(cli_id, "unknown", Some("claude auth status timed out.".into())));
            }
            Ok(match parse_claude_auth(&stdout) {
                Some((true, method)) => auth(cli_id, "signed_in", method),
                Some((false, _)) => auth(cli_id, "signed_out", None),
                None => auth(
                    cli_id,
                    "unknown",
                    Some(stderr.lines().next().unwrap_or("").trim().to_string())
                        .filter(|s| !s.is_empty()),
                ),
            })
        }
        "codex" => {
            let (code, stdout, stderr, timed_out) =
                clis::run_capture_pub(&program, &["login", "status"], AUTH_TIMEOUT)?;
            if timed_out {
                return Ok(auth(cli_id, "unknown", Some("codex login status timed out.".into())));
            }
            let text = format!("{stdout}\n{stderr}");
            let first = text.lines().map(str::trim).find(|l| !l.is_empty()).map(str::to_string);
            Ok(if code == Some(0) {
                auth(cli_id, "signed_in", first)
            } else {
                auth(cli_id, "signed_out", None)
            })
        }
        // Antigravity CLI has no status command; it signs in on its first interactive start.
        "antigravity" => Ok(auth(cli_id, "unknown", None)),
        other => Err(format!("Unknown CLI '{other}'.")),
    }
}

/// The interactive command that signs a CLI in.
fn login_args(cli_id: &str) -> Result<(&'static str, &'static str), String> {
    match cli_id {
        "claude" => Ok(("Sign in to Claude Code", "auth login")),
        "codex" => Ok(("Sign in to Codex", "login")),
        "antigravity" => Ok(("Sign in to Antigravity CLI", "")),
        other => Err(format!("Unknown CLI '{other}'.")),
    }
}

/// Opens a visible terminal window running the CLI's own sign-in, since it needs
/// a browser round-trip and sometimes a pasted code. The window stays open so the
/// user can read the result.
#[cfg(windows)]
pub fn open_login(cli_id: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let (title, args) = login_args(cli_id)?;
    let program = clis::executable_for(cli_id)?;
    let exe = program.to_string_lossy().to_string();
    if exe.chars().any(|c| "\"&|<>^%".contains(c)) {
        return Err(format!("Cannot open a terminal for this path: {exe}"));
    }
    let mut cmd = Command::new("cmd.exe");
    cmd.raw_arg(format!("/D /C start \"{title}\" cmd.exe /K \"\"{exe}\" {args}\""))
        .creation_flags(0x0800_0000)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    clis::scrub_host_env(&mut cmd);
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not open a terminal: {e}"))
}

#[cfg(not(windows))]
pub fn open_login(cli_id: &str) -> Result<(), String> {
    let (_, args) = login_args(cli_id)?;
    Err(format!("Run the CLI with '{args}' in a terminal to sign in."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(cli: &str, permission: &str, session: Option<&str>) -> AgentRunRequest {
        AgentRunRequest {
            run_id: "r1".into(),
            cli_id: cli.into(),
            repo_path: ".".into(),
            prompt: "/gloria-init-brain".into(),
            model: Some("opus".into()),
            effort: Some("high".into()),
            permission: permission.into(),
            session_id: session.map(str::to_string),
        }
    }

    #[test]
    fn claude_args_assign_and_resume_sessions() {
        let b = build_args(&req("claude", "edits", None), false, "abc").unwrap();
        assert_eq!(
            b.args,
            strings(&[
                "-p", "--output-format", "stream-json", "--verbose", "--model", "opus", "--effort",
                "high", "--permission-mode", "acceptEdits", "--session-id", "abc", "/gloria-init-brain",
            ])
        );
        assert_eq!(b.session_id.as_deref(), Some("abc"));

        let b = build_args(&req("claude", "full", Some("s-1")), true, "abc").unwrap();
        assert!(b.args.contains(&"--dangerously-skip-permissions".to_string()));
        assert!(b.args.windows(2).any(|w| w[0] == "--resume" && w[1] == "s-1"));
        assert!(b.prompt_on_stdin);
        assert!(!b.args.contains(&"/gloria-init-brain".to_string()));
    }

    #[test]
    fn codex_args_use_sandbox_and_resume() {
        let mut r = req("codex", "plan", None);
        r.prompt = "$gloria-init-brain".into();
        let b = build_args(&r, false, "x").unwrap();
        assert_eq!(b.args[0], "exec");
        assert!(b.args.windows(2).any(|w| w[0] == "-s" && w[1] == "read-only"));
        assert!(b.args.contains(&"model_reasoning_effort=\"high\"".to_string()));
        assert_eq!(b.args.last().unwrap(), "$gloria-init-brain");
        assert_eq!(b.session_id, None);

        r.session_id = Some("last".into());
        r.permission = "edits".into();
        let b = build_args(&r, false, "x").unwrap();
        assert_eq!(&b.args[..2], &strings(&["exec", "resume"])[..]);
        assert!(b.args.contains(&"sandbox_mode=\"workspace-write\"".to_string()));
        assert!(b.args.contains(&"--last".to_string()));
    }

    #[test]
    fn antigravity_prompt_is_last_and_defaults_are_dropped() {
        let mut r = req("antigravity", "edits", None);
        r.model = Some("default".into());
        r.effort = None;
        let b = build_args(&r, false, "x").unwrap();
        assert_eq!(b.args, strings(&["--mode", "accept-edits", "-p", "/gloria-init-brain"]));
    }

    #[test]
    fn rejects_unsafe_values() {
        let mut r = req("claude", "edits", None);
        r.model = Some("--dangerously-skip-permissions".into());
        assert!(build_args(&r, false, "x").is_err());
        let mut r = req("codex", "edits", None);
        r.effort = Some("high\" x=\"1".into());
        assert!(build_args(&r, false, "x").is_err());
        let mut r = req("claude", "yolo", None);
        r.model = None;
        assert!(build_args(&r, false, "x").is_err());
        let mut r = req("claude", "edits", None);
        r.prompt = "   ".into();
        assert!(build_args(&r, false, "x").is_err());
    }

    #[test]
    fn uuid_has_v4_shape_and_is_unique() {
        let a = new_uuid();
        let b = new_uuid();
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
        assert_eq!(&a[14..15], "4");
    }

    #[test]
    fn formats_claude_stream_events() {
        let tool = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Reading."},{"type":"tool_use","name":"Bash","input":{"command":"git log -n 5"}}]}}"#;
        let lines = format_claude_event(tool);
        assert_eq!(lines[0], ("Reading.".to_string(), "out"));
        assert_eq!(lines[1], ("→ Bash git log -n 5".to_string(), "info"));
        let done = r#"{"type":"result","subtype":"success","is_error":false,"duration_ms":2500,"total_cost_usd":0.01}"#;
        assert_eq!(format_claude_event(done)[0].0, "● success in 2.5s · $0.0100");
        assert!(format_claude_event(r#"{"type":"user","message":{"content":[]}}"#).is_empty());
        assert_eq!(format_claude_event("plain text")[0].0, "plain text");
    }

    #[test]
    fn parses_model_lists() {
        let codex = r#"{"models":[{"slug":"gpt-a","display_name":"GPT A","visibility":"list","supported_reasoning_levels":[{"effort":"low"},{"effort":"high"}]},{"slug":"hidden","visibility":"hide"}]}"#;
        let m = parse_codex_models(codex).unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].efforts, strings(&["low", "high"]));
        let agy = "Fetching available models...\ngemini-x-high\tGemini X (High)\nclaude-opus-5-5-low\tClaude Opus 5.5 (Low)\n";
        let m = parse_agy_models(agy);
        assert_eq!(m.len(), 2);
        assert_eq!(m[1].label, "Claude Opus 5.5 (Low)");
    }

    #[test]
    fn finds_codex_session_id() {
        let line = "session id: 0199a1b2-c3d4-4e5f-8a9b-0c1d2e3f4a5b";
        assert_eq!(
            &codex_session_re().captures(line).unwrap()[1],
            "0199a1b2-c3d4-4e5f-8a9b-0c1d2e3f4a5b"
        );
    }

    #[test]
    fn claude_auth_status_is_parsed() {
        let out = r#"{ "loggedIn": false, "authMethod": "none", "apiProvider": "firstParty" }"#;
        assert_eq!(parse_claude_auth(out), Some((false, None)));
        let out = "warning
{\"loggedIn\": true, \"authMethod\": \"claude.ai\"}
";
        assert_eq!(parse_claude_auth(out), Some((true, Some("via claude.ai".into()))));
        assert_eq!(parse_claude_auth("Not logged in"), None);
    }

    #[test]
    fn login_commands_are_known() {
        assert_eq!(login_args("claude").unwrap().1, "auth login");
        assert_eq!(login_args("codex").unwrap().1, "login");
        assert!(login_args("nope").is_err());
    }
}
