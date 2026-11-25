use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::brain_core::{normalize_text, read_utf8, write_utf8_no_bom};

pub const DEFAULT_TEMPLATE: &str = "\
# --- OS junk ---
.DS_Store
Thumbs.db
desktop.ini
ehthumbs.db
$RECYCLE.BIN/

# --- Editors / IDEs ---
.vscode/*
!.vscode/extensions.json
!.vscode/settings.json
.idea/
*.swp
*.swo
*~

# --- Dependencies ---
node_modules/
.pnpm-store/
.venv/
venv/
__pycache__/
*.pyc

# --- Build output ---
dist/
build/
out/
target/
*.tsbuildinfo

# --- Logs ---
*.log
npm-debug.log*
yarn-debug.log*
yarn-error.log*
pnpm-debug.log*

# --- Env & secrets ---
.env
.env.*
!.env.example
*.pem
*.key

# --- Local agent state ---
.claude/settings.local.json
";

#[derive(Debug, Clone, Serialize)]
pub struct GitignoreTemplate {
    pub path: String,
    pub content: String,
    pub exists: bool,
    pub line_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitignoreRepoStatus {
    pub repo_path: String,
    pub status: String,
    pub repo_line_count: usize,
    pub template_line_count: usize,
    pub missing_lines: Vec<String>,
    pub extra_lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitignoreApplyResult {
    pub repo_path: String,
    pub ok: bool,
    pub error: Option<String>,
    pub previous_existed: bool,
}

/// `<brain_repo>/templates/gitignore` when the Brain repo is configured, otherwise
/// `%APPDATA%\Brain\gitignore` so the draft still works without a repo.
pub fn template_path(brain_repo: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(repo) = brain_repo {
        if repo.is_dir() {
            return Ok(repo.join("templates").join("gitignore"));
        }
    }
    let cfg = dirs::config_dir().ok_or_else(|| "Could not resolve %APPDATA% directory".to_string())?;
    Ok(cfg.join("Brain").join("gitignore"))
}

fn content_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
}

fn ensure_trailing_newline(text: &str) -> String {
    let norm = normalize_text(text);
    if norm.is_empty() {
        String::new()
    } else {
        format!("{norm}\n")
    }
}

pub fn read_template(brain_repo: Option<&Path>) -> Result<GitignoreTemplate, String> {
    let path = template_path(brain_repo)?;
    let exists = path.is_file();
    let content = if exists {
        read_utf8(&path)?
    } else {
        DEFAULT_TEMPLATE.to_string()
    };
    let content = ensure_trailing_newline(&content);
    Ok(GitignoreTemplate {
        path: path.to_string_lossy().to_string(),
        line_count: content.lines().count(),
        content,
        exists,
    })
}

/// Normalized template text, or `None` when no draft has been saved yet.
pub fn load_template_text(brain_repo: Option<&Path>) -> Option<String> {
    let path = template_path(brain_repo).ok()?;
    if !path.is_file() {
        return None;
    }
    read_utf8(&path).ok().map(|t| normalize_text(&t))
}

pub fn save_template(brain_repo: Option<&Path>, content: &str) -> Result<GitignoreTemplate, String> {
    let path = template_path(brain_repo)?;
    write_utf8_no_bom(&path, &ensure_trailing_newline(content))?;
    read_template(brain_repo)
}

/// MISSING / MATCHES / DIFFERS / NO TEMPLATE for a repo, comparing normalized text.
pub fn quick_status(repo_path: &Path, template: Option<&str>) -> String {
    let Some(tpl) = template else {
        return "NO TEMPLATE".to_string();
    };
    let gi = repo_path.join(".gitignore");
    if !gi.is_file() {
        return "MISSING".to_string();
    }
    match read_utf8(&gi) {
        Ok(text) if normalize_text(&text) == tpl => "MATCHES".to_string(),
        Ok(_) => "DIFFERS".to_string(),
        Err(_) => "DIFFERS".to_string(),
    }
}

pub fn repo_status(repo_path: &Path, brain_repo: Option<&Path>) -> Result<GitignoreRepoStatus, String> {
    let template = load_template_text(brain_repo);
    let status = quick_status(repo_path, template.as_deref());
    let tpl_text = template.unwrap_or_default();
    let gi = repo_path.join(".gitignore");
    let repo_text = if gi.is_file() {
        normalize_text(&read_utf8(&gi)?)
    } else {
        String::new()
    };

    let tpl_lines = content_lines(&tpl_text);
    let repo_lines = content_lines(&repo_text);
    let repo_set: BTreeSet<&String> = repo_lines.iter().collect();
    let tpl_set: BTreeSet<&String> = tpl_lines.iter().collect();

    let missing_lines = tpl_lines
        .iter()
        .filter(|l| !repo_set.contains(l))
        .cloned()
        .collect();
    let extra_lines = repo_lines
        .iter()
        .filter(|l| !tpl_set.contains(l))
        .cloned()
        .collect();

    Ok(GitignoreRepoStatus {
        repo_path: repo_path.to_string_lossy().to_string(),
        status,
        repo_line_count: if repo_text.is_empty() { 0 } else { repo_text.lines().count() },
        template_line_count: if tpl_text.is_empty() { 0 } else { tpl_text.lines().count() },
        missing_lines,
        extra_lines,
    })
}

pub fn apply(repo_path: &Path, brain_repo: Option<&Path>) -> Result<bool, String> {
    if !repo_path.is_dir() {
        return Err(format!("'{}' is not an existing folder.", repo_path.display()));
    }
    let tpl_path = template_path(brain_repo)?;
    if !tpl_path.is_file() {
        return Err("Save the .gitignore draft in the .gitignore tab before applying it.".to_string());
    }
    let content = ensure_trailing_newline(&read_utf8(&tpl_path)?);
    let target = repo_path.join(".gitignore");
    let previous_existed = target.is_file();
    fs::write(&target, content.as_bytes())
        .map_err(|e| format!("Failed to write '{}': {e}", target.display()))?;
    Ok(previous_existed)
}

pub fn apply_many(repo_paths: &[String], brain_repo: Option<&Path>) -> Vec<GitignoreApplyResult> {
    repo_paths
        .iter()
        .map(|rp| match apply(Path::new(rp), brain_repo) {
            Ok(previous_existed) => GitignoreApplyResult {
                repo_path: rp.clone(),
                ok: true,
                error: None,
                previous_existed,
            },
            Err(e) => GitignoreApplyResult {
                repo_path: rp.clone(),
                ok: false,
                error: Some(e),
                previous_existed: false,
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("brain_gi_{label}_{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn status_missing_matches_differs_with_crlf() {
        let root = temp_dir("status");
        let brain = root.join("brain_repo");
        let repo = root.join("repo");
        fs::create_dir_all(&brain).unwrap();
        fs::create_dir_all(&repo).unwrap();

        save_template(Some(&brain), "node_modules/\r\ndist/\r\n").unwrap();
        let tpl = load_template_text(Some(&brain));
        assert_eq!(quick_status(&repo, tpl.as_deref()), "MISSING");

        fs::write(repo.join(".gitignore"), "node_modules/\r\ndist/\r\n\r\n").unwrap();
        assert_eq!(quick_status(&repo, tpl.as_deref()), "MATCHES");

        fs::write(repo.join(".gitignore"), "node_modules/\n").unwrap();
        assert_eq!(quick_status(&repo, tpl.as_deref()), "DIFFERS");
        let st = repo_status(&repo, Some(&brain)).unwrap();
        assert_eq!(st.missing_lines, vec!["dist/".to_string()]);

        assert!(apply(&repo, Some(&brain)).unwrap());
        assert_eq!(quick_status(&repo, tpl.as_deref()), "MATCHES");

        assert_eq!(quick_status(&repo, None), "NO TEMPLATE");
        let _ = fs::remove_dir_all(&root);
    }
}
