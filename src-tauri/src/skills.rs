//! Skills have one source each: `<brain repo>/skills/<name>/` (SKILL.md plus any
//! extra files). `gloria-init-brain` has no folder of its own; it is rendered from
//! `brain/init.md`. Install renders a copy per tool (Claude Code, Codex,
//! Antigravity), copies whole skill folders and prunes `gloria-*` skills that no
//! longer exist in the repo.

use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::brain_core::{is_safe_file_name, normalize_text, read_utf8, write_utf8_no_bom};

pub const INIT_SKILL: &str = "gloria-init-brain";
/// Only skills in this namespace are ever pruned from the tool folders, so skills
/// the user installed from elsewhere are never touched.
const MANAGED_PREFIX: &str = "gloria-";
const LEGACY_SKILLS: &[&str] = &[
    "init-brain",
    "monjita-init-brain",
    "monjita-github-release",
    "monjita-github-repo-about",
    "monjita-github-repo-readme",
];
const INIT_ANCHOR: &str = "## Non-Negotiable Rules\n";

#[derive(Debug, Clone, Serialize)]
pub struct SkillEntry {
    pub tool: String,
    pub tool_label: String,
    pub skill_name: String,
    pub is_generated: bool,
    pub repo_path: String,
    pub installed_path: String,
    pub status: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct SkillInstallReport {
    pub installed: usize,
    pub pruned: Vec<String>,
}

pub fn tool_targets() -> Result<Vec<(&'static str, &'static str, PathBuf)>, String> {
    let home = dirs::home_dir().ok_or_else(|| "Could not resolve user home directory".to_string())?;
    Ok(vec![
        ("claude-code", "Claude Code", home.join(".claude").join("skills")),
        ("codex", "Codex", home.join(".agents").join("skills")),
        (
            "antigravity",
            "Antigravity",
            home.join(".gemini").join("config").join("skills"),
        ),
    ])
}

fn tool_label(tool: &str) -> Option<&'static str> {
    match tool {
        "claude-code" => Some("Claude Code"),
        "codex" => Some("Codex"),
        "antigravity" => Some("Antigravity"),
        _ => None,
    }
}

/// Names of the source skill folders (`skills/<name>/SKILL.md`), sorted.
pub fn source_skills(repo_root: &Path) -> Vec<String> {
    let mut out: Vec<String> = fs::read_dir(repo_root.join("skills"))
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir() && p.join("SKILL.md").is_file())
                .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_string))
                .filter(|n| n != INIT_SKILL && tool_label(n).is_none())
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// The `gloria-init-brain/SKILL.md` text for `tool`, rendered from `brain/init.md`.
pub fn render_init_skill(repo_root: &Path, tool: &str) -> Result<Option<String>, String> {
    let init_path = repo_root.join("brain").join("init.md");
    if !init_path.is_file() {
        return Ok(None);
    }
    let raw = read_utf8(&init_path)?.replace("\r\n", "\n");
    let Some(pos) = raw.find(INIT_ANCHOR) else {
        return Err(format!("Could not find '{}' in {}", INIT_ANCHOR.trim(), init_path.display()));
    };
    let body = &raw[pos..];

    let std_header = concat!(
        "---\n",
        "name: gloria-init-brain\n",
        "description: \u{E800} Generate, bootstrap, refresh, or update AGENTS.md: the Brain CLI compiles personal rules from .brain/*.md and the marker; you write only the repo-specific sections. Use ONLY when the user explicitly asks to run gloria-init-brain, init-brain, bootstrap or refresh project context, learn/persist a rule, or check whether AGENTS.md is stale. Do not use for ordinary coding tasks.\n",
        "---\n",
        "# \u{E800} gloria-init-brain — Project Context & Brain Compiler\n\n",
        "User arguments: any text the user typed when invoking this skill (e.g. `check`, `learn <rule>`, or a short project description such as \"Tauri desktop app for X\").\n\n",
    );
    let claude_header = concat!(
        "---\n",
        "name: gloria-init-brain\n",
        "description: \u{E800} Generate, bootstrap, refresh, or update AGENTS.md: the Brain CLI compiles personal rules from .brain/*.md and the marker; you write only the repo-specific sections. Run only when the user explicitly asks for gloria-init-brain, init-brain, to bootstrap or refresh project context, to learn/persist a rule, or to check whether AGENTS.md is stale.\n",
        "argument-hint: \"[check] | [learn <rule>] | [short project description for bootstrap]\"\n",
        "disable-model-invocation: true\n",
        "---\n",
        "# \u{E800} /gloria-init-brain — Project Context & Brain Compiler\n\n",
        "User arguments: `$ARGUMENTS`\n\n",
    );

    Ok(Some(if tool == "claude-code" {
        let claude_body = body
            .replace("If the user's invocation starts with", "If `$ARGUMENTS` starts with")
            .replace("If the user's invocation text is exactly", "If `$ARGUMENTS` is exactly")
            .replace("If neither the invocation text nor", "If neither `$ARGUMENTS` nor");
        format!("{claude_header}{claude_body}")
    } else {
        format!("{std_header}{body}")
    }))
}

fn copy_dir(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("Failed to create '{}': {e}", dst.display()))?;
    for entry in fs::read_dir(src).map_err(|e| format!("Failed to read '{}': {e}", src.display()))?.flatten() {
        let p = entry.path();
        let target = dst.join(entry.file_name());
        if p.is_dir() {
            copy_dir(&p, &target)?;
        } else {
            fs::copy(&p, &target)
                .map_err(|e| format!("Failed to copy '{}' to '{}': {e}", p.display(), target.display()))?;
        }
    }
    Ok(())
}

/// Replaces `<install_root>/<name>` with the source folder (or the rendered init skill).
fn install_one(repo_root: &Path, tool: &str, install_root: &Path, name: &str) -> Result<(), String> {
    let dst = install_root.join(name);
    if dst.exists() {
        fs::remove_dir_all(&dst).map_err(|e| format!("Failed to replace '{}': {e}", dst.display()))?;
    }
    if name == INIT_SKILL {
        if let Some(text) = render_init_skill(repo_root, tool)? {
            write_utf8_no_bom(&dst.join("SKILL.md"), &text)?;
        }
        Ok(())
    } else {
        copy_dir(&repo_root.join("skills").join(name), &dst)
    }
}

/// Installs every skill into every tool folder and prunes managed skills that are gone.
pub fn install_all(repo_root: &Path) -> Result<SkillInstallReport, String> {
    let mut names = source_skills(repo_root);
    if repo_root.join("brain").join("init.md").is_file() {
        names.push(INIT_SKILL.to_string());
    }
    let wanted: BTreeSet<&str> = names.iter().map(String::as_str).collect();
    let mut report = SkillInstallReport::default();

    for (tool, _, root) in tool_targets()? {
        fs::create_dir_all(&root).map_err(|e| format!("Failed to create '{}': {e}", root.display()))?;
        for legacy in LEGACY_SKILLS {
            let old = root.join(legacy);
            if old.exists() {
                let _ = fs::remove_dir_all(&old);
                report.pruned.push(format!("{tool}/{legacy}"));
            }
        }
        if let Ok(entries) = fs::read_dir(&root) {
            for e in entries.flatten() {
                let Some(n) = e.file_name().to_str().map(str::to_string) else { continue };
                if e.path().is_dir() && n.starts_with(MANAGED_PREFIX) && !wanted.contains(n.as_str()) {
                    fs::remove_dir_all(e.path())
                        .map_err(|err| format!("Failed to prune '{}': {err}", e.path().display()))?;
                    report.pruned.push(format!("{tool}/{n}"));
                }
            }
        }
        for name in &names {
            install_one(repo_root, tool, &root, name)?;
            report.installed += 1;
        }
    }
    Ok(report)
}

fn status_of(expected: &str, installed: &Path) -> String {
    if !installed.is_file() {
        "NOT_INSTALLED".to_string()
    } else if normalize_text(expected) == normalize_text(&read_utf8(installed).unwrap_or_default()) {
        "SYNCED".to_string()
    } else {
        "OUT_OF_SYNC".to_string()
    }
}

pub fn list_skills(repo_root: Option<&Path>) -> Result<Vec<SkillEntry>, String> {
    let Some(root) = repo_root else {
        return Ok(Vec::new());
    };
    let names = source_skills(root);
    let mut entries = Vec::new();

    for (tool_id, label, install_root) in tool_targets()? {
        if let Ok(Some(content)) = render_init_skill(root, tool_id) {
            let installed = install_root.join(INIT_SKILL).join("SKILL.md");
            entries.push(SkillEntry {
                tool: tool_id.to_string(),
                tool_label: label.to_string(),
                skill_name: INIT_SKILL.to_string(),
                is_generated: true,
                repo_path: root.join("brain").join("init.md").to_string_lossy().to_string(),
                installed_path: installed.to_string_lossy().to_string(),
                status: status_of(&content, &installed),
                content,
            });
        }
        for name in &names {
            let repo_skill = root.join("skills").join(name).join("SKILL.md");
            // One unreadable skill must not hide the rest of the list.
            let Ok(content) = read_utf8(&repo_skill) else { continue };
            let installed = install_root.join(name).join("SKILL.md");
            entries.push(SkillEntry {
                tool: tool_id.to_string(),
                tool_label: label.to_string(),
                skill_name: name.clone(),
                is_generated: false,
                repo_path: repo_skill.to_string_lossy().to_string(),
                installed_path: installed.to_string_lossy().to_string(),
                status: status_of(&content, &installed),
                content,
            });
        }
    }
    entries.sort_by(|a, b| a.tool.cmp(&b.tool).then_with(|| a.skill_name.cmp(&b.skill_name)));
    Ok(entries)
}

/// Saves the single source of a skill and reinstalls it for every tool.
pub fn save_skill(repo_root: &Path, tool: &str, skill_name: &str, content: &str) -> Result<SkillEntry, String> {
    let label = tool_label(tool).ok_or_else(|| format!("Unsupported tool '{tool}'."))?;
    if skill_name == INIT_SKILL {
        return Err("gloria-init-brain is generated from brain/init.md. Edit brain/init.md instead.".to_string());
    }
    if !is_safe_file_name(skill_name) {
        return Err(format!("Invalid skill name '{skill_name}'."));
    }
    let skill_path = repo_root.join("skills").join(skill_name).join("SKILL.md");
    if !skill_path.is_file() {
        return Err(format!("Skill file '{}' does not exist.", skill_path.display()));
    }
    let normalized = content.replace("\r\n", "\n");
    write_utf8_no_bom(&skill_path, &normalized)?;

    let mut installed_path = PathBuf::new();
    for (t, _, root) in tool_targets()? {
        if root.is_dir() || root.parent().is_some_and(Path::is_dir) {
            install_one(repo_root, t, &root, skill_name)?;
        }
        if t == tool {
            installed_path = root.join(skill_name).join("SKILL.md");
        }
    }

    Ok(SkillEntry {
        tool: tool.to_string(),
        tool_label: label.to_string(),
        skill_name: skill_name.to_string(),
        is_generated: false,
        repo_path: skill_path.to_string_lossy().to_string(),
        status: status_of(&normalized, &installed_path),
        installed_path: installed_path.to_string_lossy().to_string(),
        content: normalized,
    })
}
