use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

use crate::brain_core::{
    compute_brain_fingerprint, compute_fingerprint_from_pairs, detect_stubs, get_brain_files,
    is_init_filename, read_utf8, write_utf8_no_bom,
};

#[derive(Debug, Clone, Serialize)]
pub struct RuleSummary {
    pub name: String,
    pub path: String,
    pub is_init: bool,
    pub apply_when: Vec<String>,
    pub mode: String,
    pub trigger: Option<String>,
    pub line_count: usize,
    pub stubs: Vec<String>,
    pub has_non_negotiable_marker: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleDocument {
    pub name: String,
    pub path: String,
    pub is_init: bool,
    pub apply_when: Vec<String>,
    pub mode: String,
    pub trigger: Option<String>,
    pub line_count: usize,
    pub stubs: Vec<String>,
    pub has_non_negotiable_marker: bool,
    pub raw_content: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FingerprintPreview {
    pub saved_fingerprint: String,
    pub draft_fingerprint: String,
    pub changed: bool,
    pub stubs: Vec<String>,
    pub has_non_negotiable_marker: bool,
}

pub fn resolve_brain_dir(brain_repo_path: Option<&str>, master_dir: &Path) -> Result<PathBuf, String> {
    if let Some(repo) = brain_repo_path {
        let candidate = Path::new(repo).join("brain");
        if candidate.is_dir() {
            return Ok(candidate);
        }
    }
    if master_dir.is_dir() {
        return Ok(master_dir.to_path_buf());
    }
    Err("Neither the Brain repository brain/ folder nor ~/.brain exists.".to_string())
}

fn parse_frontmatter(raw: &str) -> (Vec<String>, String, Option<String>, String) {
    let norm = raw.replace("\r\n", "\n").replace('\r', "\n");
    if !norm.starts_with("---\n") {
        return (Vec::new(), "inline".to_string(), None, norm);
    }

    let rest = &norm[4..];
    let Some(end_idx) = rest.find("\n---") else {
        return (Vec::new(), "inline".to_string(), None, norm);
    };

    let fm_block = &rest[..end_idx];
    let after_marker = &rest[end_idx + 4..];
    let body = after_marker
        .strip_prefix('\n')
        .unwrap_or(after_marker)
        .to_string();

    let mut apply_when = Vec::new();
    let mut mode = "inline".to_string();
    let mut trigger = None;

    for line in fm_block.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            let key = k.trim().to_ascii_lowercase();
            let val = v.trim();
            match key.as_str() {
                "apply_when" => {
                    apply_when = val
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                "mode" => {
                    if !val.is_empty() {
                        mode = val.to_string();
                    }
                }
                "trigger" => {
                    if !val.is_empty() {
                        trigger = Some(val.to_string());
                    }
                }
                _ => {}
            }
        }
    }

    (apply_when, mode, trigger, body)
}

fn build_rule_document(name: &str, path: &Path, raw: String) -> RuleDocument {
    let is_init = is_init_filename(name);
    let line_count = raw.lines().count();
    let stubs = if is_init {
        Vec::new()
    } else {
        detect_stubs(&raw)
    };
    let norm = raw.replace("\r\n", "\n");
    let has_non_negotiable_marker = norm.contains("## Non-Negotiable Rules\n");
    let (apply_when, mode, trigger, body) = if is_init {
        (vec!["compiler".to_string()], "prompt".to_string(), None, norm)
    } else {
        parse_frontmatter(&raw)
    };

    RuleDocument {
        name: name.to_string(),
        path: path.to_string_lossy().to_string(),
        is_init,
        apply_when,
        mode,
        trigger,
        line_count,
        stubs,
        has_non_negotiable_marker,
        raw_content: raw,
        body,
    }
}

pub fn list_rules(brain_dir: &Path) -> Result<Vec<RuleSummary>, String> {
    if !brain_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut docs = Vec::new();
    let entries = fs::read_dir(brain_dir)
        .map_err(|e| format!("Failed to read '{}': {e}", brain_dir.display()))?;

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
        let raw = read_utf8(&p)?;
        let doc = build_rule_document(name, &p, raw);
        docs.push(RuleSummary {
            name: doc.name,
            path: doc.path,
            is_init: doc.is_init,
            apply_when: doc.apply_when,
            mode: doc.mode,
            trigger: doc.trigger,
            line_count: doc.line_count,
            stubs: doc.stubs,
            has_non_negotiable_marker: doc.has_non_negotiable_marker,
        });
    }

    docs.sort_by(|a, b| {
        a.is_init
            .cmp(&b.is_init)
            .then_with(|| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()))
    });

    Ok(docs)
}

pub fn read_rule(brain_dir: &Path, name: &str) -> Result<RuleDocument, String> {
    let safe_name = sanitize_rule_filename(name)?;
    let path = brain_dir.join(&safe_name);
    if !path.is_file() {
        return Err(format!("Rule file '{}' does not exist.", safe_name));
    }
    let raw = read_utf8(&path)?;
    Ok(build_rule_document(&safe_name, &path, raw))
}

pub fn save_rule(brain_dir: &Path, name: &str, raw_content: &str) -> Result<RuleDocument, String> {
    let safe_name = sanitize_rule_filename(name)?;
    let path = brain_dir.join(&safe_name);
    if is_init_filename(&safe_name) {
        let norm = raw_content.replace("\r\n", "\n");
        if !norm.contains("## Non-Negotiable Rules\n") {
            return Err(
                "init.md must preserve the exact '## Non-Negotiable Rules' heading required for skill compilation."
                    .to_string(),
            );
        }
    }
    let normalized = raw_content.replace("\r\n", "\n");
    write_utf8_no_bom(&path, &normalized)?;
    Ok(build_rule_document(&safe_name, &path, normalized))
}

pub fn create_rule(brain_dir: &Path, raw_name: &str) -> Result<RuleDocument, String> {
    let trimmed = raw_name.trim();
    if trimmed.is_empty() {
        return Err("Rule filename cannot be empty.".to_string());
    }
    let with_ext = if trimmed.to_ascii_lowercase().ends_with(".md") {
        trimmed.to_string()
    } else {
        format!("{trimmed}.md")
    };
    let safe_name = sanitize_rule_filename(&with_ext)?;
    if is_init_filename(&safe_name) {
        return Err("Cannot create a rule named init.md.".to_string());
    }
    let path = brain_dir.join(&safe_name);
    if path.exists() {
        return Err(format!("Rule '{}' already exists.", safe_name));
    }

    let stem = safe_name.trim_end_matches(".md");
    let template = format!(
        "---\napply_when: always\nmode: inline\n---\n- **{stem}:** Describe a concrete rule here.\n"
    );
    write_utf8_no_bom(&path, &template)?;
    Ok(build_rule_document(&safe_name, &path, template))
}

pub fn delete_rule(brain_dir: &Path, name: &str) -> Result<(), String> {
    let safe_name = sanitize_rule_filename(name)?;
    if is_init_filename(&safe_name) {
        return Err("init.md cannot be deleted.".to_string());
    }
    let path = brain_dir.join(&safe_name);
    if !path.is_file() {
        return Err(format!("Rule '{}' does not exist.", safe_name));
    }
    fs::remove_file(&path)
        .map_err(|e| format!("Failed to delete '{}': {e}", path.display()))
}

pub fn preview_fingerprint(
    brain_dir: &Path,
    file_name: Option<&str>,
    draft_content: Option<&str>,
) -> Result<FingerprintPreview, String> {
    let saved_fingerprint = compute_brain_fingerprint(brain_dir, None)?;
    let files = get_brain_files(brain_dir, None);

    let mut pairs: Vec<(String, String)> = Vec::with_capacity(files.len());
    let target_lower = file_name.map(|s| s.to_ascii_lowercase());
    let mut replaced = false;

    for f in files {
        if let (Some(target), Some(draft)) = (&target_lower, draft_content) {
            if f.name.to_ascii_lowercase() == *target {
                pairs.push((f.name, draft.to_string()));
                replaced = true;
                continue;
            }
        }
        let raw = read_utf8(&f.path)?;
        pairs.push((f.name, raw));
    }

    if !replaced {
        if let (Some(name), Some(draft)) = (file_name, draft_content) {
            if !is_init_filename(name) && name.to_ascii_lowercase().ends_with(".md") {
                pairs.push((name.to_string(), draft.to_string()));
            }
        }
    }

    let draft_fingerprint = compute_fingerprint_from_pairs(&pairs);

    let (stubs, has_non_negotiable_marker) = match (file_name, draft_content) {
        (Some(name), Some(draft)) => {
            let is_init = is_init_filename(name);
            let s = if is_init {
                Vec::new()
            } else {
                detect_stubs(draft)
            };
            let has_marker = draft
                .replace("\r\n", "\n")
                .contains("## Non-Negotiable Rules\n");
            (s, has_marker)
        }
        _ => (Vec::new(), true),
    };

    Ok(FingerprintPreview {
        changed: saved_fingerprint != draft_fingerprint,
        saved_fingerprint,
        draft_fingerprint,
        stubs,
        has_non_negotiable_marker,
    })
}

fn sanitize_rule_filename(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains("..")
    {
        return Err(format!("Invalid rule filename '{}'.", name));
    }
    if !trimmed.to_ascii_lowercase().ends_with(".md") {
        return Err("Rule files must end with .md".to_string());
    }
    Ok(trimmed.to_string())
}
