use regex::Regex;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::{legacy_skill_dir, legacy_skill_dirs};

#[derive(Debug, Clone)]
pub struct BrainFileEntry {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepoStatus {
    pub repo: String,
    pub repo_path: String,
    pub status: String,
    pub lines_count: Option<usize>,
    pub lines_display: String,
    pub sync_marker: String,
    pub expected: String,
    pub message: String,
    pub drifted_files: Vec<String>,
    pub has_project_brain: bool,
    pub has_claude_md: bool,
    pub local_playbooks: Vec<String>,
    pub is_git_repo: bool,
    pub is_brain_repo: bool,
    /// True when `<repo>/.brain/` holds an identical copy of every master rule and no stale ones.
    pub project_brain_in_sync: bool,
    /// MISSING / MATCHES / DIFFERS / NO TEMPLATE against the shared .gitignore draft.
    pub gitignore_status: String,
}

/// Master brain state computed once per scan and shared by every `check_repo` call.
pub struct ScanContext {
    pub src_dir: PathBuf,
    /// (lowercase name, normalized content) for each master rule, init excluded.
    pub master_files: Vec<(String, String)>,
    pub brain_repo: Option<PathBuf>,
    pub gitignore_template: Option<String>,
}

impl ScanContext {
    pub fn new(
        master_dir: &Path,
        fallback_brain_dir: Option<&Path>,
        brain_repo: Option<&Path>,
        gitignore_template: Option<String>,
    ) -> Self {
        let src_dir = resolve_source_dir(master_dir, fallback_brain_dir);
        let master_files = get_brain_files(&src_dir, None)
            .into_iter()
            .map(|f| {
                let content = read_utf8(&f.path).map(|c| normalize_text(&c)).unwrap_or_default();
                (f.name.to_ascii_lowercase(), content)
            })
            .collect();
        Self {
            src_dir,
            master_files,
            brain_repo: brain_repo.map(Path::to_path_buf),
            gitignore_template,
        }
    }
}

fn resolve_source_dir(master_dir: &Path, fallback_brain_dir: Option<&Path>) -> PathBuf {
    if master_dir.is_dir() {
        master_dir.to_path_buf()
    } else if let Some(fb) = fallback_brain_dir.filter(|p| p.is_dir()) {
        fb.to_path_buf()
    } else {
        master_dir.to_path_buf()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncResult {
    pub repo_path: String,
    pub copied: Vec<String>,
    pub pruned: Vec<String>,
    pub git_excluded: bool,
    pub fingerprint: String,
    pub marker: String,
    /// True when AGENTS.md was rewritten (sections and/or marker changed).
    pub agents_updated: bool,
    pub repo_hash: String,
    /// "created", "ok", "custom" (exists without `@AGENTS.md`) or "skipped" (no AGENTS.md).
    pub claude_md: String,
    pub compile: Option<crate::compile::CompileReport>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SyncOptions {
    /// Re-baseline the repo hash so REPO DRIFT clears (after the AI refreshed
    /// Commands/Architecture). Without it an existing repo hash is kept.
    pub accept_repo: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallResult {
    pub compiled_skills: usize,
    pub master_files_count: usize,
    pub pruned_master: Vec<String>,
    pub installed_skills_count: usize,
    pub fingerprint: String,
    pub marker: String,
    pub stub_files: Vec<String>,
    pub legacy_skill_present: bool,
    pub pruned_skills: Vec<String>,
    /// Path of the `brain.cmd` shim the skills call, when it could be written.
    pub brain_cmd: Option<String>,
}

/// Decodes bytes the way PowerShell's `ReadAllText(..., UTF8)` does: honours UTF-8 and
/// UTF-16 BOMs and replaces invalid UTF-8 sequences with U+FFFD instead of failing.
pub fn decode_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    if bytes.len() >= 2 && (bytes[..2] == [0xFF, 0xFE] || bytes[..2] == [0xFE, 0xFF]) {
        let le = bytes[0] == 0xFF;
        let (pairs, _) = bytes[2..].as_chunks::<2>();
        let units: Vec<u16> = pairs
            .iter()
            .map(|&c| if le { u16::from_le_bytes(c) } else { u16::from_be_bytes(c) })
            .collect();
        return String::from_utf16_lossy(&units);
    }
    String::from_utf8_lossy(bytes).into_owned()
}

pub fn read_utf8(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("Failed to read '{}': {e}", path.display()))?;
    Ok(decode_text(&bytes))
}

/// Strips a BOM, converts CRLF/CR to LF and trims trailing newlines — the same
/// normalization `Get-BrainFingerprint` applies before hashing.
pub fn normalize_text(raw: &str) -> String {
    let stripped = raw.strip_prefix('\u{FEFF}').unwrap_or(raw);
    stripped
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim_end_matches('\n')
        .to_string()
}

pub fn write_utf8_no_bom(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory '{}': {e}", parent.display()))?;
        }
    }
    fs::write(path, content.as_bytes())
        .map_err(|e| format!("Failed to write '{}': {e}", path.display()))
}

/// A single path segment made only of `[A-Za-z0-9._-]`, not starting with `.` and
/// without `..` — rejects separators, drive prefixes (`C:`) and reserved characters.
pub fn is_safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains("..")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

pub fn is_init_filename(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "init.md" || lower == "init-brain.md"
}

pub fn is_local_filename(name: &str) -> bool {
    name.to_ascii_lowercase().starts_with("local-")
}

pub fn get_brain_files(brain_dir: &Path, project_brain_dir: Option<&Path>) -> Vec<BrainFileEntry> {
    let mut map: BTreeMap<String, BrainFileEntry> = BTreeMap::new();

    if brain_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(brain_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if !name.to_ascii_lowercase().ends_with(".md") || is_init_filename(name) {
                    continue;
                }
                map.insert(
                    name.to_ascii_lowercase(),
                    BrainFileEntry {
                        name: name.to_string(),
                        path,
                    },
                );
            }
        }
    }

    if let Some(proj_dir) = project_brain_dir {
        if proj_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(proj_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_file() {
                        continue;
                    }
                    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                        continue;
                    };
                    if !name.to_ascii_lowercase().ends_with(".md") || is_init_filename(name) {
                        continue;
                    }
                    let key = name.to_ascii_lowercase();
                    if (!map.contains_key(&key) && is_local_filename(name)) || !brain_dir.is_dir() {
                        map.insert(
                            key,
                            BrainFileEntry {
                                name: name.to_string(),
                                path,
                            },
                        );
                    }
                }
            }
        }
    }

    map.into_values().collect()
}

pub fn compute_fingerprint_from_pairs(pairs: &[(String, String)]) -> String {
    if pairs.is_empty() {
        return "000000000000".to_string();
    }

    // Ordinal (byte) ordering. PowerShell 5.1's `Sort-Object` is culture-aware, so file
    // names that differ only by `-`/`_` (e.g. `github.md` vs `github-actions.md`) could
    // sort differently there; keep rule names distinct enough to avoid that.
    let mut sorted: BTreeMap<String, String> = BTreeMap::new();
    for (name, content) in pairs {
        if is_init_filename(name) || !name.to_ascii_lowercase().ends_with(".md") {
            continue;
        }
        sorted.insert(name.to_ascii_lowercase(), content.clone());
    }

    if sorted.is_empty() {
        return "000000000000".to_string();
    }

    let mut buf = String::new();
    for (lower_name, raw) in sorted {
        let norm = normalize_text(&raw);
        buf.push_str(&lower_name);
        buf.push('\n');
        buf.push_str(&norm);
        buf.push('\n');
    }

    let mut hasher = Sha256::new();
    hasher.update(buf.as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    hex[..12].to_string()
}

pub fn compute_brain_fingerprint(
    brain_dir: &Path,
    project_brain_dir: Option<&Path>,
) -> Result<String, String> {
    let files = get_brain_files(brain_dir, project_brain_dir);
    if files.is_empty() {
        return Ok("000000000000".to_string());
    }

    let mut pairs = Vec::with_capacity(files.len());
    for f in files {
        let raw = read_utf8(&f.path)?;
        pairs.push((f.name, raw));
    }

    Ok(compute_fingerprint_from_pairs(&pairs))
}

pub fn detect_stubs(content: &str) -> Vec<String> {
    let norm = content.replace("\r\n", "\n").replace('\r', "\n");
    let re = Regex::new(r"(?m)^[ \t]*-[ \t]+(?:\*\*[^*\n]+\*\*:?[ \t]*)?<[^`>\n]+>[ \t]*$")
        .expect("valid stub regex");
    re.find_iter(&norm)
        .map(|m| m.as_str().trim().to_string())
        .collect()
}

/// Reads the `.md` files of `<repo>/.brain/` as (lowercase name, normalized content).
fn read_project_brain(proj_brain: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(proj_brain) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_file() {
                continue;
            }
            let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.to_ascii_lowercase().ends_with(".md") {
                continue;
            }
            let content = read_utf8(&p).map(|c| normalize_text(&c)).unwrap_or_default();
            out.push((name.to_ascii_lowercase(), content));
        }
    }
    out
}

/// True when `.brain/` holds an identical copy of every master rule and no stale
/// non-local rule files — i.e. "Sync .brain/" would change nothing.
pub fn project_brain_matches(master_files: &[(String, String)], project_files: &[(String, String)]) -> bool {
    let proj: BTreeMap<&str, &str> = project_files
        .iter()
        .map(|(n, c)| (n.as_str(), c.as_str()))
        .collect();
    let master_names: BTreeSet<&str> = master_files.iter().map(|(n, _)| n.as_str()).collect();
    let all_copied = master_files
        .iter()
        .all(|(n, c)| proj.get(n.as_str()).is_some_and(|pc| pc == c));
    let no_stale = proj
        .keys()
        .all(|n| master_names.contains(n) || is_local_filename(n));
    all_copied && no_stale
}

pub fn check_repo_with(repo_path: &Path, ctx: &ScanContext) -> Result<RepoStatus, String> {
    if !repo_path.is_dir() {
        return Err(format!("'{}' is not an existing folder.", repo_path.display()));
    }
    let repo_name = repo_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("repo")
        .to_string();
    let repo_path_str = repo_path.to_string_lossy().to_string();

    let proj_brain = repo_path.join(".brain");
    let has_project_brain = proj_brain.is_dir();
    let has_claude_md = repo_path.join("CLAUDE.md").is_file();
    let is_git_repo = repo_path.join(".git").exists();
    let is_brain_repo = ctx
        .brain_repo
        .as_deref()
        .map(|br| same_path_ci(br, repo_path))
        .unwrap_or(false);

    let project_files = if has_project_brain {
        read_project_brain(&proj_brain)
    } else {
        Vec::new()
    };

    let mut local_playbooks: Vec<String> = Vec::new();
    if has_project_brain {
        if let Ok(entries) = fs::read_dir(&proj_brain) {
            for e in entries.flatten() {
                if let Some(name) = e.file_name().to_str() {
                    if name.to_ascii_lowercase().ends_with(".md") && is_local_filename(name) {
                        local_playbooks.push(name.to_string());
                    }
                }
            }
        }
        local_playbooks.sort();
    }

    let expected_fp = if ctx.src_dir.is_dir() {
        let master_names: BTreeSet<&str> = ctx.master_files.iter().map(|(n, _)| n.as_str()).collect();
        let mut pairs = ctx.master_files.clone();
        for (name, content) in &project_files {
            if is_local_filename(name) && !is_init_filename(name) && !master_names.contains(name.as_str()) {
                pairs.push((name.clone(), content.clone()));
            }
        }
        compute_fingerprint_from_pairs(&pairs)
    } else {
        compute_brain_fingerprint(&ctx.src_dir, Some(&proj_brain))?
    };

    let project_brain_in_sync = if ctx.master_files.is_empty() {
        true
    } else {
        has_project_brain && project_brain_matches(&ctx.master_files, &project_files)
    };
    let gitignore_status =
        crate::gitignore::quick_status(repo_path, ctx.gitignore_template.as_deref());

    let agents_path = repo_path.join("AGENTS.md");

    if !agents_path.is_file() {
        return Ok(RepoStatus {
            repo: repo_name,
            repo_path: repo_path_str,
            status: "NO AGENTS.MD".to_string(),
            lines_count: None,
            lines_display: "-".to_string(),
            sync_marker: "-".to_string(),
            expected: expected_fp,
            message: "AGENTS.md does not exist.".to_string(),
            drifted_files: Vec::new(),
            has_project_brain,
            has_claude_md,
            local_playbooks,
            is_git_repo,
            is_brain_repo,
            project_brain_in_sync,
            gitignore_status,
        });
    }

    let agents_content = read_utf8(&agents_path)?;
    let lines: Vec<&str> = agents_content.lines().collect();
    let line_count = lines.len();
    let last_non_empty = lines.iter().rev().find(|l| !l.trim().is_empty()).copied();

    let (marker_fp, marker_repo) = match last_non_empty.and_then(parse_marker) {
        Some((fp, repo)) => (Some(fp), repo),
        None => (None, None),
    };

    // REPO DRIFT is content based: a checkout, clone or `touch` never trips it, and
    // version bumps / dependency updates are ignored (see `canonical_repo_file`).
    // Old markers without a repo hash never report REPO DRIFT.
    let mut drifted_files = Vec::new();
    if let Some(ref stored) = marker_repo {
        let entries = repo_hash_entries(repo_path);
        if &combine_repo_hash(&entries) != stored {
            let previous = read_repo_state(&proj_brain);
            if previous.is_empty() {
                drifted_files = entries.iter().map(|(n, _)| n.clone()).collect();
            } else {
                let now: BTreeMap<&str, &str> =
                    entries.iter().map(|(n, h)| (n.as_str(), h.as_str())).collect();
                for (n, h) in &now {
                    if previous.get(*n).map(String::as_str) != Some(*h) {
                        drifted_files.push(n.to_string());
                    }
                }
                for n in previous.keys() {
                    if !now.contains_key(n.as_str()) {
                        drifted_files.push(format!("{n} (removed)"));
                    }
                }
            }
            if drifted_files.is_empty() {
                drifted_files.push("watched files".to_string());
            }
        }
    }

    let mut status;
    let mut msg;
    match &marker_fp {
        None => {
            status = "NO MARKER".to_string();
            msg = "Refresh due: no sync marker.".to_string();
        }
        Some(fp) if fp != &expected_fp => {
            status = "BRAIN DRIFT".to_string();
            msg = format!("Refresh due: brain rules changed ({fp} != {expected_fp}).");
        }
        Some(_) => {
            status = "UP TO DATE".to_string();
            msg = "AGENTS.md is up to date.".to_string();
        }
    }

    if !drifted_files.is_empty() {
        if status == "UP TO DATE" {
            status = "REPO DRIFT".to_string();
        }
        msg.push_str(&format!(
            " Refresh suggested: manifests or workflows changed since the last accepted sync ({}); re-run /gloria-init-brain (it ends with `brain sync --accept-repo`).",
            drifted_files.join(", ")
        ));
    }

    if line_count > 120 {
        status = "OVER BUDGET".to_string();
        msg.push_str(&format!(" Line budget exceeded ({line_count} / 120)."));
    }

    Ok(RepoStatus {
        repo: repo_name,
        repo_path: repo_path_str,
        status,
        lines_count: Some(line_count),
        lines_display: format!("{line_count}/120"),
        sync_marker: marker_fp.unwrap_or_else(|| "none".to_string()),
        expected: expected_fp,
        message: msg,
        drifted_files,
        has_project_brain,
        has_claude_md,
        local_playbooks,
        is_git_repo,
        is_brain_repo,
        project_brain_in_sync,
        gitignore_status,
    })
}

/// A RepoStatus row for a repo whose check failed, so one bad repo never aborts a scan.
pub fn error_status(repo_path: &Path, error: String) -> RepoStatus {
    RepoStatus {
        repo: repo_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("repo")
            .to_string(),
        repo_path: repo_path.to_string_lossy().to_string(),
        status: "ERROR".to_string(),
        lines_count: None,
        lines_display: "-".to_string(),
        sync_marker: "-".to_string(),
        expected: "-".to_string(),
        message: error,
        drifted_files: Vec::new(),
        has_project_brain: false,
        has_claude_md: false,
        local_playbooks: Vec::new(),
        is_git_repo: false,
        is_brain_repo: false,
        project_brain_in_sync: true,
        gitignore_status: "-".to_string(),
    }
}

pub fn sync_project(
    repo_path: &Path,
    master_dir: &Path,
    fallback_brain_dir: Option<&Path>,
) -> Result<SyncResult, String> {
    sync_project_with(repo_path, master_dir, fallback_brain_dir, SyncOptions::default())
}

/// Copies the master rules into `<repo>/.brain/`, git-excludes it and, when
/// `AGENTS.md` exists, recompiles the Brain sections, rewrites the marker and
/// makes sure `CLAUDE.md` imports `@AGENTS.md`. No AI involved.
pub fn sync_project_with(
    repo_path: &Path,
    master_dir: &Path,
    fallback_brain_dir: Option<&Path>,
    opts: SyncOptions,
) -> Result<SyncResult, String> {
    if !repo_path.is_dir() {
        return Err(format!("'{}' is not an existing folder.", repo_path.display()));
    }
    let src_dir = if master_dir.is_dir() {
        master_dir.to_path_buf()
    } else if let Some(fb) = fallback_brain_dir {
        if fb.is_dir() {
            fb.to_path_buf()
        } else {
            return Err(format!("No master brain found at {}", master_dir.display()));
        }
    } else {
        return Err(format!("No master brain found at {}", master_dir.display()));
    };

    let proj_brain = repo_path.join(".brain");
    fs::create_dir_all(&proj_brain)
        .map_err(|e| format!("Failed to create '{}': {e}", proj_brain.display()))?;

    let master_files = get_brain_files(&src_dir, None);
    let mut master_names = BTreeSet::new();
    let mut copied = Vec::new();

    for f in &master_files {
        master_names.insert(f.name.to_ascii_lowercase());
        let dest = proj_brain.join(&f.name);
        fs::copy(&f.path, &dest)
            .map_err(|e| format!("Failed to copy '{}' to '{}': {e}", f.path.display(), dest.display()))?;
        copied.push(f.name.clone());
    }

    let mut pruned = Vec::new();
    if let Ok(entries) = fs::read_dir(&proj_brain) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_file() {
                continue;
            }
            let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.to_ascii_lowercase().ends_with(".md") {
                continue;
            }
            let lower = name.to_ascii_lowercase();
            if !master_names.contains(&lower) && !is_local_filename(name) {
                fs::remove_file(&p)
                    .map_err(|e| format!("Failed to prune '{}': {e}", p.display()))?;
                pruned.push(name.to_string());
            }
        }
    }

    let git_dir = repo_path.join(".git");
    let mut git_excluded = false;
    if git_dir.is_dir() {
        let exclude_path = git_dir.join("info").join("exclude");
        let existing = if exclude_path.is_file() {
            read_utf8(&exclude_path)?
        } else {
            String::new()
        };
        let exclude_re = Regex::new(r"(?m)^\s*\.brain/?\s*$").expect("valid exclude regex");
        if !exclude_re.is_match(&existing) {
            let mut updated = existing;
            if !updated.is_empty() && !updated.ends_with('\n') {
                updated.push('\n');
            }
            updated.push_str(".brain/\n");
            write_utf8_no_bom(&exclude_path, &updated)?;
        }
        git_excluded = true;
    }

    let fp = compute_brain_fingerprint(&src_dir, Some(&proj_brain))?;

    let agents_path = repo_path.join("AGENTS.md");
    let entries = repo_hash_entries(repo_path);
    let mut repo_hash = combine_repo_hash(&entries);
    let mut agents_updated = false;
    let mut compile_report = None;
    let mut claude_md = "skipped".to_string();
    let mut marker = format!("<!-- brain-sync: {fp} repo: {repo_hash} -->");

    if agents_path.is_file() {
        let original = read_utf8(&agents_path)?;
        let stored_repo = original
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .and_then(parse_marker)
            .and_then(|(_, r)| r);
        let accept = opts.accept_repo || stored_repo.is_none();
        if let (false, Some(stored)) = (accept, stored_repo) {
            repo_hash = stored;
        }
        marker = format!("<!-- brain-sync: {fp} repo: {repo_hash} -->");

        let rules: Vec<crate::compile::RuleFile> = get_brain_files(&src_dir, Some(&proj_brain))
            .into_iter()
            .filter_map(|f| {
                read_utf8(&f.path)
                    .ok()
                    .map(|raw| crate::compile::parse_rule(&f.name, &raw))
            })
            .collect();
        let traits = crate::compile::detect_traits(repo_path);
        let mut report = crate::compile::CompileReport::default();
        let compiled =
            crate::compile::compile_agents_text(&original, &rules, &traits, &marker, &mut report);
        let compiled = if original.contains("\r\n") {
            compiled.replace('\n', "\r\n")
        } else {
            compiled
        };
        if compiled != original {
            write_utf8_no_bom(&agents_path, &compiled)?;
            agents_updated = true;
        }
        let lines = compiled.lines().count();
        if lines > 120 {
            report
                .warnings
                .push(format!("AGENTS.md is {lines} lines (hard cap 120, target 60)."));
        }
        if accept {
            write_repo_state(&proj_brain, &entries)?;
        }

        let claude_path = repo_path.join("CLAUDE.md");
        claude_md = if !claude_path.is_file() {
            write_utf8_no_bom(&claude_path, "@AGENTS.md\n")?;
            "created".to_string()
        } else if read_utf8(&claude_path)?.contains("@AGENTS.md") {
            "ok".to_string()
        } else {
            report
                .warnings
                .push("CLAUDE.md exists but does not import @AGENTS.md.".to_string());
            "custom".to_string()
        };
        compile_report = Some(report);
    }

    Ok(SyncResult {
        repo_path: repo_path.to_string_lossy().to_string(),
        copied,
        pruned,
        git_excluded,
        marker,
        fingerprint: fp,
        agents_updated,
        repo_hash,
        claude_md,
        compile: compile_report,
    })
}

/// `(fingerprint, repo hash)` from a `<!-- brain-sync: <fp> [repo: <hash>] -->` line.
pub fn parse_marker(line: &str) -> Option<(String, Option<String>)> {
    let re = Regex::new(
        r"<!--\s*brain-sync:\s*([0-9a-fA-F]{12})\b(?:\s+repo:\s*([0-9a-fA-F]{12})\b)?",
    )
    .expect("valid marker regex");
    let c = re.captures(line)?;
    Some((
        c.get(1)?.as_str().to_ascii_lowercase(),
        c.get(2).map(|m| m.as_str().to_ascii_lowercase()),
    ))
}

const REPO_WATCH_FILES: &[&str] = &[
    "package.json",
    "pyproject.toml",
    "Cargo.toml",
    "src-tauri/Cargo.toml",
    "src-tauri/tauri.conf.json",
    "tauri.conf.json",
    "gloria.toml",
    "go.mod",
];

const JSON_IGNORED_KEYS: &[&str] = &[
    "version",
    "dependencies",
    "devDependencies",
    "peerDependencies",
    "optionalDependencies",
];

const TOML_IGNORED_KEYS: &[&str] = &["dependencies", "dev-dependencies", "build-dependencies", "target"];

fn sha12(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect::<String>()[..12].to_string()
}

fn strip_toml_noise(table: &mut toml::Table) {
    table.remove("version");
    for k in TOML_IGNORED_KEYS {
        table.remove(*k);
    }
}

/// Canonical text of a watched file: versions and dependency lists are dropped so
/// releases and dependency bumps don't flag REPO DRIFT; everything else counts.
fn canonical_repo_file(rel: &str, raw: &str) -> String {
    let norm = normalize_text(raw);
    let lower = rel.to_ascii_lowercase();
    if lower.ends_with(".json") {
        if let Ok(serde_json::Value::Object(mut map)) = serde_json::from_str::<serde_json::Value>(&norm) {
            for k in JSON_IGNORED_KEYS {
                map.remove(*k);
            }
            let sorted: BTreeMap<String, serde_json::Value> = map.into_iter().collect();
            return serde_json::to_string(&sorted).unwrap_or(norm);
        }
    } else if lower.ends_with(".toml") {
        if let Ok(mut table) = norm.parse::<toml::Table>() {
            strip_toml_noise(&mut table);
            for (_, v) in table.iter_mut() {
                if let toml::Value::Table(sub) = v {
                    strip_toml_noise(sub);
                    if let Some(toml::Value::Table(pkg)) = sub.get_mut("package") {
                        pkg.remove("version");
                    }
                }
            }
            let sorted: BTreeMap<String, toml::Value> = table.into_iter().collect();
            return toml::to_string(&sorted).unwrap_or(norm);
        }
    }
    norm
}

/// `(relative path, 12-hex content hash)` for every watched file that exists, sorted.
pub fn repo_hash_entries(repo: &Path) -> Vec<(String, String)> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for rel in REPO_WATCH_FILES {
        let p = repo.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        if !p.is_file() {
            continue;
        }
        if let Ok(raw) = read_utf8(&p) {
            out.insert(rel.to_string(), sha12(&canonical_repo_file(rel, &raw)));
        }
    }
    if let Ok(entries) = fs::read_dir(repo.join(".github").join("workflows")) {
        for e in entries.flatten() {
            let p = e.path();
            let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let lower = name.to_ascii_lowercase();
            if p.is_file() && (lower.ends_with(".yml") || lower.ends_with(".yaml")) {
                if let Ok(raw) = read_utf8(&p) {
                    out.insert(format!(".github/workflows/{name}"), sha12(&normalize_text(&raw)));
                }
            }
        }
    }
    out.into_iter().collect()
}

pub fn combine_repo_hash(entries: &[(String, String)]) -> String {
    let mut buf = String::new();
    for (n, h) in entries {
        buf.push_str(n);
        buf.push('\n');
        buf.push_str(h);
        buf.push('\n');
    }
    sha12(&buf)
}

pub fn compute_repo_hash(repo: &Path) -> String {
    combine_repo_hash(&repo_hash_entries(repo))
}

/// Per-file hashes at the last accepted sync, so REPO DRIFT can name the changed files.
const REPO_STATE_FILE: &str = "repo-state.json";

fn read_repo_state(proj_brain: &Path) -> BTreeMap<String, String> {
    read_utf8(&proj_brain.join(REPO_STATE_FILE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn write_repo_state(proj_brain: &Path, entries: &[(String, String)]) -> Result<(), String> {
    let map: BTreeMap<&str, &str> = entries.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    let json = serde_json::to_string_pretty(&map).map_err(|e| e.to_string())?;
    write_utf8_no_bom(&proj_brain.join(REPO_STATE_FILE), &format!("{json}\n"))
}

pub fn install_brain(repo_root: Option<&Path>, master_dir: &Path) -> Result<InstallResult, String> {
    let is_repo_root = repo_root
        .map(|r| r.join("brain").join("init.md").is_file())
        .unwrap_or(false);
    let compiled_skills = if is_repo_root { 3 } else { 0 };

    let source_brain_dir = if is_repo_root {
        repo_root.unwrap().join("brain")
    } else if master_dir.is_dir() {
        master_dir.to_path_buf()
    } else {
        return Err("Brain repository path is not configured and ~/.brain does not exist.".to_string());
    };

    fs::create_dir_all(master_dir)
        .map_err(|e| format!("Failed to create '{}': {e}", master_dir.display()))?;

    let mut source_names = BTreeSet::new();
    let mut master_files_count = 0usize;
    if let Ok(entries) = fs::read_dir(&source_brain_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_file() {
                continue;
            }
            let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.to_ascii_lowercase().ends_with(".md") {
                continue;
            }
            source_names.insert(name.to_ascii_lowercase());
            let dest = master_dir.join(name);
            if !same_path_ci(&p, &dest) {
                fs::copy(&p, &dest)
                    .map_err(|e| format!("Failed to mirror '{}': {e}", p.display()))?;
            }
            master_files_count += 1;
        }
    }

    let mut pruned_master = Vec::new();
    if let Ok(entries) = fs::read_dir(master_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_file() {
                continue;
            }
            let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.to_ascii_lowercase().ends_with(".md") {
                continue;
            }
            let lower = name.to_ascii_lowercase();
            if !source_names.contains(&lower) && !is_local_filename(name) {
                fs::remove_file(&p)
                    .map_err(|e| format!("Failed to prune master '{}': {e}", p.display()))?;
                pruned_master.push(name.to_string());
            }
        }
    }

    if let Some(root) = repo_root {
        let bat_src = root.join("install-brain.bat");
        let bat_dst = master_dir.join("install-brain.bat");
        if bat_src.is_file() && !same_path_ci(&bat_src, &bat_dst) {
            fs::copy(&bat_src, &bat_dst)
                .map_err(|e| format!("Failed to copy install-brain.bat to master: {e}"))?;
        }
    }

    let mut installed_skills_count = 0usize;
    let mut pruned_skills = Vec::new();
    if is_repo_root {
        let report = crate::skills::install_all(repo_root.unwrap())?;
        installed_skills_count = report.installed;
        pruned_skills = report.pruned;
    }
    let brain_cmd = write_brain_cmd_shim(master_dir);

    let fp = compute_brain_fingerprint(master_dir, None)?;
    let mut stub_files = Vec::new();
    for f in get_brain_files(master_dir, None) {
        if let Ok(content) = read_utf8(&f.path) {
            if !detect_stubs(&content).is_empty() {
                stub_files.push(f.name);
            }
        }
    }

    let legacy_skill_present = legacy_skill_dir().map(|p| p.exists()).unwrap_or(false);

    Ok(InstallResult {
        compiled_skills,
        master_files_count,
        pruned_master,
        installed_skills_count,
        marker: format!("<!-- brain-sync: {fp} -->"),
        fingerprint: fp,
        stub_files,
        legacy_skill_present,
        pruned_skills,
        brain_cmd,
    })
}

/// Writes `<master>/brain.cmd` (`@"<Brain.exe>" %*`). Brain.exe is a GUI-subsystem
/// binary, so PowerShell would not wait for it; a batch shim makes every shell wait
/// and keeps the exit code. Returns the shim path, or None when not run from Brain.exe.
pub fn write_brain_cmd_shim(master_dir: &Path) -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let name = exe.file_stem()?.to_str()?.to_ascii_lowercase();
    if name != "brain" {
        return None;
    }
    let shim = master_dir.join("brain.cmd");
    let body = format!("@echo off
\"{}\" %*
", exe.display());
    if read_utf8(&shim).ok().as_deref() != Some(body.as_str()) {
        write_utf8_no_bom(&shim, &body).ok()?;
    }
    Some(shim.to_string_lossy().to_string())
}

pub fn remove_legacy_skill() -> Result<bool, String> {
    let mut removed = false;
    for dir in legacy_skill_dirs()? {
        if dir.exists() {
            fs::remove_dir_all(&dir)
                .map_err(|e| format!("Failed to remove legacy skill '{}': {e}", dir.display()))?;
            removed = true;
        }
    }
    Ok(removed)
}

/// Comparable form of a path: `/` becomes `\`, existing paths are canonicalized
/// (without the `\\?\` prefix) and trailing separators are trimmed.
pub fn normalize_path(p: &Path) -> String {
    let base = fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let s = base.to_string_lossy().replace('/', "\\");
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    let trimmed = s.trim_end_matches('\\');
    if trimmed.ends_with(':') {
        format!("{trimmed}\\")
    } else {
        trimmed.to_string()
    }
}

pub fn same_path_ci(a: &Path, b: &Path) -> bool {
    normalize_path(a).to_lowercase() == normalize_path(b).to_lowercase()
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
        let dir = std::env::temp_dir().join(format!("brain_test_{label}_{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn fingerprint_normalizes_crlf_and_trailing_newlines() {
        let pair_lf = vec![("workflow.md".to_string(), "line1\nline2\n".to_string())];
        let pair_crlf = vec![("WORKFLOW.MD".to_string(), "line1\r\nline2\r\n\r\n".to_string())];
        assert_eq!(
            compute_fingerprint_from_pairs(&pair_lf),
            compute_fingerprint_from_pairs(&pair_crlf)
        );
    }

    #[test]
    fn fingerprint_ignores_init_and_includes_local_md() {
        let root = make_temp_dir("local");
        let master = root.join("master");
        let proj = root.join("proj_brain");
        fs::create_dir_all(&master).unwrap();
        fs::create_dir_all(&proj).unwrap();

        write_utf8_no_bom(&master.join("init.md"), "# ignored").unwrap();
        write_utf8_no_bom(&master.join("workflow.md"), "- stay on main").unwrap();
        write_utf8_no_bom(&proj.join("local-notes.md"), "- local playbook").unwrap();
        write_utf8_no_bom(&proj.join("ghost.md"), "- should be ignored").unwrap();

        let files = get_brain_files(&master, Some(&proj));
        let names: Vec<String> = files.iter().map(|f| f.name.clone()).collect();
        assert_eq!(names, vec!["local-notes.md", "workflow.md"]);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn stub_detection_ignores_backticked_placeholders_and_catches_raw_stubs() {
        let sample = "\
- **Output:** `dist/<Name>_<version>_x64-setup.exe`
- **Empty:** <fill this in>
- <another stub>
";
        let stubs = detect_stubs(sample);
        assert_eq!(stubs.len(), 2);
        assert!(stubs[0].contains("<fill this in>"));
        assert!(stubs[1].contains("<another stub>"));
    }

    #[test]
    fn project_brain_in_sync_after_sync_and_detects_stale_or_changed() {
        let root = make_temp_dir("insync");
        let master = root.join("master");
        let repo = root.join("repo");
        fs::create_dir_all(&master).unwrap();
        fs::create_dir_all(repo.join(".git")).unwrap();
        write_utf8_no_bom(&master.join("workflow.md"), "- stay on main\r\n").unwrap();
        write_utf8_no_bom(&master.join("init.md"), "# ignored").unwrap();

        let ctx = ScanContext::new(&master, None, None, None);
        assert!(!check_repo_with(&repo, &ctx).unwrap().project_brain_in_sync);

        sync_project(&repo, &master, None).unwrap();
        write_utf8_no_bom(&repo.join(".brain").join("local-notes.md"), "- mine").unwrap();
        let st = check_repo_with(&repo, &ctx).unwrap();
        assert!(st.project_brain_in_sync);
        assert_eq!(st.status, "NO AGENTS.MD");

        write_utf8_no_bom(&repo.join(".brain").join("old-rule.md"), "- stale").unwrap();
        assert!(!check_repo_with(&repo, &ctx).unwrap().project_brain_in_sync);
        fs::remove_file(repo.join(".brain").join("old-rule.md")).unwrap();

        write_utf8_no_bom(&repo.join(".brain").join("workflow.md"), "- edited").unwrap();
        assert!(!check_repo_with(&repo, &ctx).unwrap().project_brain_in_sync);

        assert!(sync_project(&root.join("missing"), &master, None).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn decode_text_handles_boms_and_invalid_utf8() {
        assert_eq!(decode_text(&[0xEF, 0xBB, 0xBF, b'h', b'i']), "hi");
        assert_eq!(decode_text(&[0xFF, 0xFE, b'h', 0, b'i', 0]), "hi");
        assert_eq!(decode_text(&[b'a', 0xFF, b'b']), "a\u{FFFD}b");
    }

    #[test]
    fn repo_brain_fingerprint_is_valid_hex() {
        let repo_brain = Path::new("../brain");
        if repo_brain.is_dir() {
            let fp = compute_brain_fingerprint(repo_brain, None).unwrap();
            println!("rust_repo_brain_fingerprint={fp}");
            assert_eq!(fp.len(), 12);
            assert!(fp.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        }
    }
}
