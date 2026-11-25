use regex::Regex;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

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

    let marker_re =
        Regex::new(r"<!--\s*brain-sync:\s*([0-9a-fA-F]{12})\b").expect("valid marker regex");
    let marker_fp = last_non_empty.and_then(|line| {
        marker_re
            .captures(line)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_ascii_lowercase())
    });

    let agents_mtime: Option<SystemTime> = fs::metadata(&agents_path)
        .ok()
        .and_then(|m| m.modified().ok());

    let watch_files = [
        "package.json",
        "pyproject.toml",
        "Cargo.toml",
        "src-tauri\\Cargo.toml",
        "src-tauri\\tauri.conf.json",
        "tauri.conf.json",
        "gloria.toml",
        "go.mod",
    ];

    let mut drifted_files = Vec::new();
    if let Some(a_time) = agents_mtime {
        for wf in watch_files {
            let p = repo_path.join(wf.replace('\\', std::path::MAIN_SEPARATOR_STR));
            if p.is_file() {
                if let Ok(meta) = fs::metadata(&p) {
                    if let Ok(mtime) = meta.modified() {
                        if mtime > a_time {
                            drifted_files.push(wf.replace('\\', "/"));
                        }
                    }
                }
            }
        }

        let wf_dir = repo_path.join(".github").join("workflows");
        if wf_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&wf_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if !p.is_file() {
                        continue;
                    }
                    let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
                        continue;
                    };
                    if !name.to_ascii_lowercase().ends_with(".yml") {
                        continue;
                    }
                    if let Ok(meta) = fs::metadata(&p) {
                        if let Ok(mtime) = meta.modified() {
                            if mtime > a_time {
                                drifted_files.push(format!(".github/workflows/{name}"));
                            }
                        }
                    }
                }
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
            " Refresh suggested: repo files changed since last sync ({}).",
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
    Ok(SyncResult {
        repo_path: repo_path.to_string_lossy().to_string(),
        copied,
        pruned,
        git_excluded,
        marker: format!("<!-- brain-sync: {fp} -->"),
        fingerprint: fp,
    })
}

pub fn sync_skill_files_from_init(repo_root: &Path) -> Result<bool, String> {
    let init_path = repo_root.join("brain").join("init.md");
    if !init_path.is_file() {
        return Ok(false);
    }

    let raw = read_utf8(&init_path)?.replace("\r\n", "\n");
    let marker = "## Non-Negotiable Rules\n";
    let Some(pos) = raw.find(marker) else {
        return Err(format!("Could not find '{marker}' in {}", init_path.display()));
    };
    let body = &raw[pos..];

    let std_header = concat!(
        "---\n",
        "name: gloria-init-brain\n",
        "description: \u{E800} Generate, bootstrap, refresh, or update AGENTS.md by combining repo ground truth with personal rules from .brain/*.md (synced from the master ~/.brain). Use ONLY when the user explicitly asks to run gloria-init-brain, init-brain, bootstrap or refresh project context, learn/persist a rule, or check whether AGENTS.md is stale. Do not use for ordinary coding tasks.\n",
        "---\n",
        "# \u{E800} gloria-init-brain — Project Context & Brain Compiler\n\n",
        "Generate, bootstrap, refresh, or update `AGENTS.md` by combining **repository ground truth** with matching personal rules from `.brain/*.md`, enforcing a strict 120-line budget and aggressive pruning on every run.\n\n",
        "User arguments: any text the user typed when invoking this skill (e.g. `check`, `learn <rule>`, or a short project description such as \"Tauri desktop app for X\").\n",
    );

    let claude_header = concat!(
        "---\n",
        "name: gloria-init-brain\n",
        "description: \u{E800} Generate, bootstrap, refresh, or update AGENTS.md by combining repo ground truth with personal rules from .brain/*.md (synced from the master ~/.brain). Run only when the user explicitly asks for gloria-init-brain, init-brain, to bootstrap or refresh project context, to learn/persist a rule, or to check whether AGENTS.md is stale.\n",
        "argument-hint: \"[check] | [learn <rule>] | [short project description for bootstrap]\"\n",
        "disable-model-invocation: true\n",
        "---\n",
        "# \u{E800} /gloria-init-brain — Project Context & Brain Compiler\n\n",
        "Generate, bootstrap, refresh, or update `AGENTS.md` by combining **repository ground truth** with matching personal rules from `.brain/*.md`, enforcing a strict 120-line budget and aggressive pruning on every run.\n\n",
        "User arguments: `$ARGUMENTS`\n",
    );

    let claude_body = body
        .replace("If the user's invocation starts with", "If `$ARGUMENTS` starts with")
        .replace("If the user's invocation text is exactly", "If `$ARGUMENTS` is exactly")
        .replace("If neither the invocation text nor", "If neither `$ARGUMENTS` nor");

    write_utf8_no_bom(
        &repo_root
            .join("skills")
            .join("antigravity")
            .join("gloria-init-brain")
            .join("SKILL.md"),
        &format!("{std_header}{body}"),
    )?;
    write_utf8_no_bom(
        &repo_root
            .join("skills")
            .join("codex")
            .join("gloria-init-brain")
            .join("SKILL.md"),
        &format!("{std_header}{body}"),
    )?;
    write_utf8_no_bom(
        &repo_root
            .join("skills")
            .join("claude-code")
            .join("gloria-init-brain")
            .join("SKILL.md"),
        &format!("{claude_header}{claude_body}"),
    )?;

    Ok(true)
}

pub fn install_brain(repo_root: Option<&Path>, master_dir: &Path) -> Result<InstallResult, String> {
    let home = dirs::home_dir().ok_or_else(|| "Could not resolve user home directory".to_string())?;
    let mut compiled_skills = 0usize;

    let is_repo_root = repo_root
        .map(|r| r.join("brain").join("init.md").is_file())
        .unwrap_or(false);

    if is_repo_root {
        let root = repo_root.unwrap();
        if sync_skill_files_from_init(root)? {
            compiled_skills = 3;
        }
    }

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
    if is_repo_root {
        let root = repo_root.unwrap();
        let tool_targets = [
            ("skills\\claude-code", home.join(".claude").join("skills")),
            ("skills\\codex", home.join(".agents").join("skills")),
            (
                "skills\\antigravity",
                home.join(".gemini").join("config").join("skills"),
            ),
        ];

        for (src_rel, dst_root) in tool_targets {
            let src_root = root.join(src_rel.replace('\\', std::path::MAIN_SEPARATOR_STR));
            if !src_root.is_dir() {
                continue;
            }

            for legacy in [
                "init-brain",
                "monjita-init-brain",
                "monjita-github-release",
                "monjita-github-repo-about",
                "monjita-github-repo-readme",
            ] {
                let old_dir = dst_root.join(legacy);
                if old_dir.exists() {
                    let _ = fs::remove_dir_all(&old_dir);
                }
            }

            if let Ok(dirs) = fs::read_dir(&src_root) {
                for entry in dirs.flatten() {
                    let skill_dir = entry.path();
                    if !skill_dir.is_dir() {
                        continue;
                    }
                    let src_file = skill_dir.join("SKILL.md");
                    if !src_file.is_file() {
                        continue;
                    }
                    let Some(skill_name) = skill_dir.file_name().and_then(|n| n.to_str()) else {
                        continue;
                    };
                    let dst_dir = dst_root.join(skill_name);
                    fs::create_dir_all(&dst_dir).map_err(|e| {
                        format!("Failed to create skill dir '{}': {e}", dst_dir.display())
                    })?;
                    let dst_file = dst_dir.join("SKILL.md");
                    fs::copy(&src_file, &dst_file).map_err(|e| {
                        format!("Failed to install skill '{}': {e}", dst_file.display())
                    })?;
                    installed_skills_count += 1;
                }
            }
        }
    }

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
    })
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
