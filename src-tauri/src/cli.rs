//! Headless `Brain.exe <command>`: the deterministic parts of the daily loop
//! (check, sync, release, tools) without the window and without an AI.
//!
//! Exit codes: 0 ok, 1 error, 2 needs attention (drift, failed release, ...).

use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::brain_core::{
    check_repo_with, compute_brain_fingerprint, compute_repo_hash, error_status, install_brain,
    remove_legacy_skill, sync_project_with, RepoStatus, SyncOptions,
};
use crate::config::detect_brain_repo;
use crate::release::{self, ReleaseOutcome, ReleaseRequest, ReleaseSink};
use crate::{candidate_repos, dna, paths};

const COMMANDS: &[&str] = &[
    "check",
    "sync",
    "release",
    "tools",
    "install",
    "hash",
    "remove-legacy",
    "help",
    "--help",
    "-h",
    "--version",
];

const HELP: &str = "\
Brain CLI (no window, no AI). Exit codes: 0 ok, 1 error, 2 needs attention.

  brain check [path] [--json]            AGENTS.md / .brain status of one repo (default: current folder)
  brain check --all [root] [--json]      every repo under the scan roots (or under root)
  brain sync [path] [--accept-repo]      copy rules to .brain/, recompile Personal Rules + Playbooks,
                                         rewrite the brain-sync marker, ensure CLAUDE.md imports AGENTS.md
  brain sync --all [root]                sync every repo that has an AGENTS.md
  brain release [path]                   preview: current version, setup, suggested next
  brain release [path] <x.y.z|patch|minor|major> [--watch] [--note TEXT]
  brain release [path] --retry [--watch] retry the current version after a fix
  brain release --all [root] --patch [--plan]
                                         dispatch every repo whose HEAD is not released yet, then wait
  brain tools [path] [--only a,b] [--plan] generate gloriaTools/ (--plan: preview only)
  brain install                          mirror brain/ to %USERPROFILE%\\.brain and install all skills
  brain hash [path]                      print the brain-sync fingerprint and repo hash
  brain remove-legacy                    remove the legacy Antigravity skill

  --json prints machine-readable output instead of text.
";

/// Runs the CLI when argv names a command. `None` means: start the window.
pub fn run_from_args() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let first = args.first()?;
    if !COMMANDS.contains(&first.as_str()) {
        return None;
    }
    attach_parent_console();
    let code = match run(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    };
    let _ = std::io::stdout().flush();
    Some(code)
}

/// Release builds use the Windows GUI subsystem, so they start without a console.
/// Attach to the caller's console only when stdout is not already redirected
/// (pipes and files keep working as they are).
#[cfg(windows)]
fn attach_parent_console() {
    use std::ffi::c_void;
    extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
        fn GetStdHandle(std_handle: u32) -> *mut c_void;
    }
    const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    const INVALID_HANDLE_VALUE: *mut c_void = -1isize as *mut c_void;
    // SAFETY: plain kernel32 calls with constant arguments.
    unsafe {
        let h = GetStdHandle(STD_OUTPUT_HANDLE);
        if h.is_null() || h == INVALID_HANDLE_VALUE {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

#[cfg(not(windows))]
fn attach_parent_console() {}

struct Args {
    positional: Vec<String>,
    flags: Vec<String>,
    values: Vec<(String, String)>,
}

impl Args {
    fn parse(raw: &[String]) -> Args {
        const WITH_VALUE: &[&str] = &["--note", "--only", "--root"];
        let mut out = Args {
            positional: Vec::new(),
            flags: Vec::new(),
            values: Vec::new(),
        };
        let mut it = raw.iter();
        while let Some(a) = it.next() {
            if let Some((k, v)) = a.split_once('=').filter(|(k, _)| k.starts_with("--")) {
                out.values.push((k.to_string(), v.to_string()));
            } else if WITH_VALUE.contains(&a.as_str()) {
                out.values.push((a.clone(), it.next().cloned().unwrap_or_default()));
            } else if a.starts_with("--") {
                out.flags.push(a.clone());
            } else {
                out.positional.push(a.clone());
            }
        }
        out
    }
    fn has(&self, flag: &str) -> bool {
        self.flags.iter().any(|f| f == flag)
    }
    fn value(&self, key: &str) -> Option<&str> {
        self.values.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
}

fn run(raw: &[String]) -> Result<i32, String> {
    let cmd = raw[0].as_str();
    let args = Args::parse(&raw[1..]);
    match cmd {
        "help" | "--help" | "-h" => {
            print!("{HELP}");
            Ok(0)
        }
        "--version" => {
            println!("brain {}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        "check" => cmd_check(&args),
        "sync" => cmd_sync(&args),
        "release" => cmd_release(&args),
        "tools" => cmd_tools(&args),
        "install" => cmd_install(&args),
        "hash" => cmd_hash(&args),
        "remove-legacy" => {
            let removed = remove_legacy_skill()?;
            println!("{}", if removed { "Removed the legacy skill." } else { "No legacy skill found." });
            Ok(0)
        }
        _ => unreachable!(),
    }
}

fn print_json<T: Serialize>(value: &T) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    println!("{text}");
    Ok(())
}

fn repo_arg(args: &Args, index: usize) -> Result<PathBuf, String> {
    let p = match args.positional.get(index) {
        Some(p) => PathBuf::from(p),
        None => std::env::current_dir().map_err(|e| e.to_string())?,
    };
    let p = std::fs::canonicalize(&p).unwrap_or(p);
    let s = p.to_string_lossy();
    Ok(PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()))
}

/// Repos for `--all`: the configured scan roots (as the app shows them), or the
/// child folders of an explicit root.
fn all_repos(root: Option<&str>) -> Result<Vec<PathBuf>, String> {
    let p = paths()?;
    match root {
        None => Ok(candidate_repos(&p)),
        Some(r) => {
            let mut out: Vec<PathBuf> = std::fs::read_dir(r)
                .map_err(|e| format!("Cannot read '{r}': {e}"))?
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| !n.starts_with('.') && !n.eq_ignore_ascii_case("node_modules"))
                })
                .collect();
            out.sort_by_key(|p| p.to_string_lossy().to_ascii_lowercase());
            Ok(out)
        }
    }
}

fn root_value(args: &Args) -> Option<String> {
    args.value("--root").map(str::to_string).or_else(|| args.positional.first().cloned())
}

fn status_line(s: &RepoStatus) -> String {
    let mut line = format!("{:<13} {:<28} {}", s.status, s.repo, s.lines_display);
    if !s.message.is_empty() && s.status != "UP TO DATE" {
        line.push_str("  ");
        line.push_str(&s.message);
    }
    line.trim_end().to_string()
}

fn cmd_check(args: &Args) -> Result<i32, String> {
    let p = paths()?;
    let ctx = p.scan_context();
    let json = args.has("--json");
    if args.has("--all") {
        let root = root_value(args);
        let mut rows: Vec<RepoStatus> = all_repos(root.as_deref())?
            .into_iter()
            .map(|r| check_repo_with(&r, &ctx).unwrap_or_else(|e| error_status(&r, e)))
            .collect();
        rows.sort_by_key(|r| (r.status == "UP TO DATE", r.repo.to_ascii_lowercase()));
        let attention = rows.iter().filter(|r| r.status != "UP TO DATE").count();
        if json {
            print_json(&rows)?;
        } else {
            for r in rows.iter().filter(|r| r.status != "UP TO DATE" || args.has("--verbose")) {
                println!("{}", status_line(r));
            }
            println!(
                "{} repo(s): {} up to date, {} need attention.",
                rows.len(),
                rows.len() - attention,
                attention
            );
        }
        return Ok(if attention > 0 { 2 } else { 0 });
    }
    let repo = repo_arg(args, 0)?;
    let s = check_repo_with(&repo, &ctx)?;
    if json {
        print_json(&s)?;
    } else {
        println!("{}", status_line(&s));
        if !s.drifted_files.is_empty() {
            println!("  changed: {}", s.drifted_files.join(", "));
        }
    }
    Ok(if s.status == "UP TO DATE" { 0 } else { 2 })
}

fn cmd_sync(args: &Args) -> Result<i32, String> {
    let p = paths()?;
    let opts = SyncOptions {
        accept_repo: args.has("--accept-repo"),
    };
    let json = args.has("--json");
    let repos: Vec<PathBuf> = if args.has("--all") {
        all_repos(root_value(args).as_deref())?
            .into_iter()
            .filter(|r| r.join("AGENTS.md").is_file())
            .collect()
    } else {
        vec![repo_arg(args, 0)?]
    };

    let ctx_after = |p: &crate::Paths| p.scan_context();
    let mut results = Vec::new();
    let mut worst = 0;
    for repo in &repos {
        match sync_project_with(repo, &p.master_dir, p.fallback_brain.as_deref(), opts) {
            Ok(r) => {
                let status = check_repo_with(repo, &ctx_after(&p)).unwrap_or_else(|e| error_status(repo, e));
                if status.status != "UP TO DATE" {
                    worst = worst.max(2);
                }
                if !json {
                    let name = status.repo.clone();
                    let agents = if r.agents_updated { "AGENTS.md updated" } else { "AGENTS.md unchanged" };
                    let (inline, books) = r
                        .compile
                        .as_ref()
                        .map(|c| (c.inline_rules, c.playbooks))
                        .unwrap_or((0, 0));
                    println!(
                        "{name}: {} -> {agents}, {inline} rule(s), {books} playbook(s), CLAUDE.md {}",
                        r.marker, r.claude_md
                    );
                    if let Some(c) = &r.compile {
                        for w in &c.warnings {
                            println!("  warning: {w}");
                        }
                    }
                    if status.status != "UP TO DATE" {
                        println!("  {}", status_line(&status));
                    }
                }
                results.push(serde_json::json!({ "sync": r, "status": status }));
            }
            Err(e) => {
                worst = 1;
                if !json {
                    eprintln!("{}: error: {e}", repo.display());
                }
                results.push(serde_json::json!({ "repo_path": repo.to_string_lossy(), "error": e }));
            }
        }
    }
    if json {
        if results.len() == 1 && !args.has("--all") {
            print_json(&results[0])?;
        } else {
            print_json(&results)?;
        }
    } else if args.has("--all") {
        println!("Synced {} repo(s).", repos.len());
    }
    Ok(worst)
}

fn cmd_hash(args: &Args) -> Result<i32, String> {
    let p = paths()?;
    let ctx = p.scan_context();
    let repo = repo_arg(args, 0)?;
    let fp = compute_brain_fingerprint(&ctx.src_dir, Some(&repo.join(".brain")))?;
    let rh = compute_repo_hash(&repo);
    if args.has("--json") {
        print_json(&serde_json::json!({ "fingerprint": fp, "repo_hash": rh }))?;
    } else {
        println!("<!-- brain-sync: {fp} repo: {rh} -->");
    }
    Ok(0)
}

fn cmd_install(args: &Args) -> Result<i32, String> {
    let p = paths()?;
    let repo = p
        .brain_repo
        .clone()
        .filter(|r| crate::config::is_valid_brain_repo(r))
        .or_else(detect_brain_repo);
    let r = install_brain(repo.as_deref(), &p.master_dir)?;
    if args.has("--json") {
        print_json(&r)?;
    } else {
        println!(
            "Installed {} rule file(s) to {}, {} skill copies; fingerprint {}.",
            r.master_files_count,
            p.master_dir.display(),
            r.installed_skills_count,
            r.fingerprint
        );
        for s in &r.pruned_skills {
            println!("  pruned skill {s}");
        }
        for s in &r.pruned_master {
            println!("  pruned rule {s}");
        }
        if let Some(cmd) = &r.brain_cmd {
            println!("  shim: {cmd}");
        }
        for s in &r.stub_files {
            println!("  warning: unfilled stub in {s}");
        }
    }
    Ok(0)
}

fn cmd_tools(args: &Args) -> Result<i32, String> {
    let repo = repo_arg(args, 0)?;
    let only: Option<Vec<String>> = args.value("--only").map(|v| {
        v.split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| if s.to_ascii_lowercase().ends_with(".bat") { s.to_string() } else { format!("{s}.bat") })
            .collect()
    });
    if args.has("--plan") {
        let inspect = dna::inspect_repo(&repo);
        if args.has("--json") {
            print_json(&inspect)?;
        } else {
            let s = &inspect.summary;
            println!("{}: stacks {}", s.repo, s.stacks.join(", "));
            for t in &inspect.tools {
                println!("  {:<9} {:<18} {}", t.status, t.file_name, t.description);
            }
        }
        return Ok(0);
    }
    let r = dna::create_tools(&repo, only.as_deref());
    if args.has("--json") {
        print_json(&r)?;
    } else if r.ok {
        println!("gloriaTools/: wrote {}", if r.written.is_empty() { "nothing".to_string() } else { r.written.join(", ") });
        if !r.skipped_custom.is_empty() {
            println!("  kept custom: {}", r.skipped_custom.join(", "));
        }
    } else {
        eprintln!("error: {}", r.error.clone().unwrap_or_default());
    }
    Ok(if r.ok { 0 } else { 1 })
}

// ---------------------------------------------------------------- release

struct CliSink {
    json: bool,
    verbose: bool,
    prefix: String,
    recent: Mutex<Vec<String>>,
}

impl ReleaseSink for CliSink {
    fn step(&self, _run_id: &str, step_id: &str, status: &str, detail: Option<String>) {
        if self.json || status == "running" {
            return;
        }
        let d = detail.map(|d| format!(" ({d})")).unwrap_or_default();
        println!("{}{step_id}: {status}{d}", self.prefix);
    }
    fn log(&self, _run_id: &str, stream: &str, text: String) {
        if let Ok(mut r) = self.recent.lock() {
            r.push(text.clone());
            let n = r.len();
            if n > 40 {
                r.drain(0..n - 40);
            }
        }
        if self.json {
            return;
        }
        if stream == "info" || self.verbose {
            println!("{}  {text}", self.prefix);
        }
    }
    fn done(&self, _outcome: &ReleaseOutcome) {}
}

const AUTO_FIXES: &[&str] = &["cargo_fmt", "refresh_lockfiles", "resync_versions"];

fn bump(current: &str, part: &str) -> Result<String, String> {
    let base = release::numeric_version_prefix(current);
    let nums: Vec<u64> = base.split('.').map(|n| n.parse::<u64>()).collect::<Result<_, _>>().map_err(|_| {
        format!("Current version '{current}' is not x.y.z.")
    })?;
    let [ma, mi, pa] = nums[..] else {
        return Err(format!("Current version '{current}' is not x.y.z."));
    };
    Ok(match part {
        "major" => format!("{}.0.0", ma + 1),
        "minor" => format!("{ma}.{}.0", mi + 1),
        _ if current.contains('-') => release::suggest_next_version(Some(current)),
        _ => format!("{ma}.{mi}.{}", pa + 1),
    })
}

fn run_id(repo: &Path) -> String {
    format!(
        "cli-{}-{}",
        repo.file_name().and_then(|n| n.to_str()).unwrap_or("repo"),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    )
}

/// One release with the deterministic auto-fixes (fmt, lockfiles, version resync)
/// retried up to twice. Code failures are returned to the caller (the AI).
fn release_one(repo: &Path, version: &str, retry: bool, watch: bool, note: Option<String>, sink: Arc<CliSink>) -> ReleaseOutcome {
    let mut pre_fix: Option<String> = None;
    let mut retry_same = retry;
    let mut attempt = 0;
    loop {
        attempt += 1;
        let req = ReleaseRequest {
            run_id: run_id(repo),
            repo_path: repo.to_string_lossy().to_string(),
            version: version.to_string(),
            note: note.clone(),
            run_local_check: false,
            retry_same_version: retry_same,
            pre_fix: pre_fix.clone(),
            watch,
        };
        let outcome = release::run_blocking(
            sink.clone(),
            req,
            Arc::new(AtomicBool::new(false)),
            Arc::new(Mutex::new(None)),
        );
        let fix = outcome
            .diagnosis
            .as_ref()
            .and_then(|d| d.fix_action.clone())
            .filter(|f| AUTO_FIXES.contains(&f.as_str()));
        match fix {
            Some(f) if !outcome.ok && attempt < 3 && pre_fix.as_deref() != Some(f.as_str()) => {
                if !sink.json {
                    println!("{}auto-fix: {f}, retrying v{version}", sink.prefix);
                }
                pre_fix = Some(f);
                retry_same = true;
            }
            _ => return outcome,
        }
    }
}

fn print_outcome(o: &ReleaseOutcome, recent: &[String]) {
    if o.ok {
        match (&o.release_url, &o.run_url) {
            (Some(url), _) => println!("Released v{}: {url}", o.new_version),
            (None, Some(run)) => println!("Dispatched v{}: {run}", o.new_version),
            _ => println!("Released v{}.", o.new_version),
        }
        if !o.assets.is_empty() {
            println!("Assets: {}", o.assets.join(", "));
        }
        return;
    }
    println!("FAILED v{}: {}", o.new_version, o.error.clone().unwrap_or_default());
    if let Some(d) = &o.diagnosis {
        println!("diagnosis: [{}] {}", d.category, d.summary);
        if let Some(loc) = &d.location {
            println!("location: {loc}");
        }
    }
    if let Some(url) = &o.run_url {
        println!("run: {url}");
    }
    if !recent.is_empty() {
        println!("last output:");
        for l in recent.iter().rev().take(25).rev() {
            println!("  {l}");
        }
    }
}

fn cmd_release(args: &Args) -> Result<i32, String> {
    if args.has("--all") {
        return cmd_release_all(args);
    }
    let json = args.has("--json");
    // Positional: [path] <version>; a lone positional that looks like a version is the version.
    let is_version = |s: &str| {
        matches!(s, "patch" | "minor" | "major") || s.trim_start_matches('v').starts_with(|c: char| c.is_ascii_digit())
    };
    let (path_arg, version_arg) = match args.positional.as_slice() {
        [] => (None, None),
        [one] if is_version(one) => (None, Some(one.clone())),
        [one] => (Some(one.clone()), None),
        [p, v, ..] => (Some(p.clone()), Some(v.clone())),
    };
    let repo = match path_arg {
        Some(p) => repo_arg(&Args { positional: vec![p], flags: vec![], values: vec![] }, 0)?,
        None => repo_arg(args, usize::MAX)?,
    };
    let pre = release::preflight(&repo)?;
    let retry = args.has("--retry");
    if version_arg.is_none() && !retry {
        // Preview: what the skill shows before asking for the version.
        if json {
            print_json(&pre)?;
        } else {
            println!("Repo:            {}", pre.repo_name);
            println!("Current version: {}", pre.current_version.as_deref().unwrap_or("none"));
            for f in pre.version_files.iter().filter(|f| !f.matches_primary) {
                println!("  disagrees:     {} = {}", f.path, f.version);
            }
            println!("Last release:    {}", pre.last_release.as_deref().unwrap_or("none"));
            let setup = match pre.workflow_status.as_str() {
                "ready" => "ready".to_string(),
                _ if pre.needs_bootstrap => format!("will be set up: {}", pre.bootstrap_plan.join("; ")),
                other => other.to_string(),
            };
            println!("Release setup:   {setup}");
            if !pre.dirty_files.is_empty() {
                println!("Uncommitted:     {} file(s) (only release files are committed)", pre.dirty_files.len());
            }
            for b in &pre.blockers {
                println!("blocked: {b}");
            }
            println!("Suggested:       {}", pre.suggested_version);
        }
        return Ok(if pre.blockers.is_empty() { 0 } else { 1 });
    }
    if !pre.blockers.is_empty() {
        if json {
            print_json(&pre)?;
        } else {
            for b in &pre.blockers {
                eprintln!("blocked: {b}");
            }
        }
        return Ok(1);
    }
    let version = if retry {
        pre.current_version.clone().ok_or("No current version to retry.")?
    } else {
        match version_arg.as_deref() {
            None => unreachable!("handled by the preview above"),
            Some(part @ ("patch" | "minor" | "major")) => bump(pre.current_version.as_deref().unwrap_or("0.0.0"), part)?,
            Some(v) => release::normalize_version(v),
        }
    };
    if !json {
        println!(
            "{}: {} -> {version}{}",
            pre.repo_name,
            pre.current_version.as_deref().unwrap_or("none"),
            if pre.needs_bootstrap { format!(" (bootstrap: {})", pre.bootstrap_plan.join("; ")) } else { String::new() }
        );
    }
    let sink = Arc::new(CliSink {
        json,
        verbose: args.has("--verbose"),
        prefix: String::new(),
        recent: Mutex::new(Vec::new()),
    });
    let outcome = release_one(
        &repo,
        &version,
        retry,
        args.has("--watch"),
        args.value("--note").map(str::to_string),
        sink.clone(),
    );
    if json {
        print_json(&outcome)?;
    } else {
        let recent = sink.recent.lock().map(|r| r.clone()).unwrap_or_default();
        print_outcome(&outcome, if outcome.ok { &[] } else { &recent });
    }
    Ok(if outcome.ok { 0 } else { 2 })
}

fn git_out(repo: &Path, args: &[&str]) -> Option<String> {
    let mut cmd = std::process::Command::new("git");
    cmd.args(args).current_dir(repo).stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[derive(Serialize)]
struct BatchRow {
    repo: String,
    action: String,
    version: Option<String>,
    run_url: Option<String>,
    result: String,
    error: Option<String>,
}

/// The released commit of `v<version>` on origin (peeled for annotated tags).
fn remote_tag_commit(repo: &Path, version: &str) -> Option<String> {
    let tag = format!("refs/tags/v{version}");
    let out = git_out(repo, &["ls-remote", "--tags", "origin", &tag, &format!("{tag}^{{}}")])?;
    let mut sha = None;
    for line in out.lines() {
        let mut parts = line.split_whitespace();
        let (Some(s), Some(r)) = (parts.next(), parts.next()) else { continue };
        if r.ends_with("^{}") || sha.is_none() {
            sha = Some(s.to_string());
        }
    }
    sha
}

fn cmd_release_all(args: &Args) -> Result<i32, String> {
    let json = args.has("--json");
    let plan_only = args.has("--plan");
    if !args.has("--patch") && !plan_only {
        return Err("brain release --all only bumps patch versions: pass --patch (or --plan to preview).".to_string());
    }
    let repos = all_repos(root_value(args).as_deref())?;
    let mut rows: Vec<BatchRow> = Vec::new();
    let mut dispatched: Vec<(usize, PathBuf, u64)> = Vec::new();

    for repo in repos {
        let name = repo.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string();
        let mut row = BatchRow { repo: name.clone(), action: String::new(), version: None, run_url: None, result: String::new(), error: None };
        let eligible = repo.join(".git").exists()
            && (repo.join("gloria.toml").is_file() || repo.join(".github/workflows/release.yml").is_file());
        if !eligible {
            continue;
        }
        let Ok(pre) = release::preflight(&repo) else { continue };
        if pre.is_gloria_setup {
            row.action = "skip".into();
            row.result = "GloriaSetup releases itself".into();
            rows.push(row);
            continue;
        }
        if !pre.blockers.is_empty() {
            row.action = "skip".into();
            row.result = "blocked".into();
            row.error = Some(pre.blockers.join(" "));
            rows.push(row);
            continue;
        }
        let Some(current) = pre.current_version.clone() else {
            row.action = "skip".into();
            row.result = "no version file".into();
            rows.push(row);
            continue;
        };
        if bump(&current, "patch").is_err() {
            row.action = "skip".into();
            row.result = "invalid version".into();
            row.error = Some(format!("'{current}' is not x.y.z; fix the version file first"));
            rows.push(row);
            continue;
        }
        let head = git_out(&repo, &["rev-parse", "HEAD"]).unwrap_or_default();
        let (version, retry) = match remote_tag_commit(&repo, &current) {
            Some(sha) if sha == head => {
                row.action = "skip".into();
                row.result = format!("v{current} is HEAD");
                rows.push(row);
                continue;
            }
            Some(_) => (bump(&current, "patch")?, false),
            None => (current.clone(), true),
        };
        row.version = Some(version.clone());
        row.action = if retry { format!("release v{version} (untagged)") } else { format!("release {current} -> {version}") };
        if plan_only {
            row.result = "planned".into();
            if !json {
                println!("{name}: {}", row.action);
            }
            rows.push(row);
            continue;
        }
        if !json {
            println!("{name}: {}", row.action);
        }
        let sink = Arc::new(CliSink { json, verbose: false, prefix: format!("  [{name}] "), recent: Mutex::new(Vec::new()) });
        let o = release_one(&repo, &version, retry, false, None, sink);
        row.run_url = o.run_url.clone();
        if o.ok {
            row.result = "dispatched".into();
            if let Some(id) = o.workflow_run_id {
                dispatched.push((rows.len(), repo.clone(), id));
            }
        } else {
            row.result = "failed".into();
            row.error = o.error.clone();
        }
        rows.push(row);
    }

    // Wait for every dispatched run; GitHub builds them in parallel.
    let deadline = Instant::now() + Duration::from_secs(90 * 60);
    let mut pending = dispatched;
    if !pending.is_empty() && !json {
        println!("Waiting for {} workflow run(s)...", pending.len());
    }
    while !pending.is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_secs(20));
        pending.retain(|(idx, repo, id)| {
            let out = std::process::Command::new("gh")
                .args(["run", "view", &id.to_string(), "--json", "status,conclusion"])
                .current_dir(repo)
                .stdin(std::process::Stdio::null())
                .output();
            let Ok(out) = out else { return true };
            let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
            if v.get("status").and_then(|s| s.as_str()) != Some("completed") {
                return true;
            }
            let concl = v.get("conclusion").and_then(|s| s.as_str()).unwrap_or("unknown").to_string();
            let row = &mut rows[*idx];
            row.result = if concl == "success" { "released".into() } else { format!("CI {concl}") };
            if !json {
                println!("{}: {}", row.repo, row.result);
            }
            false
        });
    }
    for (idx, _, _) in &pending {
        rows[*idx].result = "still running".into();
    }

    let failed = rows.iter().filter(|r| r.result == "failed" || r.result.starts_with("CI ") || r.result == "still running").count();
    if json {
        print_json(&rows)?;
    } else {
        println!("\nSummary:");
        for r in &rows {
            let extra = r.error.as_deref().map(|e| format!(" - {e}")).unwrap_or_default();
            println!("  {:<28} {:<14} {}{extra}", r.repo, r.result, r.version.as_deref().map(|v| format!("v{v}")).unwrap_or_default());
        }
    }
    Ok(if failed > 0 { 2 } else { 0 })
}
