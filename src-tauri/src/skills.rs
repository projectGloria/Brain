use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

use crate::brain_core::{is_safe_file_name, normalize_text, read_utf8, write_utf8_no_bom};

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

pub fn list_skills(repo_root: Option<&Path>) -> Result<Vec<SkillEntry>, String> {
    let Some(root) = repo_root else {
        return Ok(Vec::new());
    };
    let skills_root = root.join("skills");
    if !skills_root.is_dir() {
        return Ok(Vec::new());
    }

    let targets = tool_targets()?;
    let mut entries = Vec::new();

    for (tool_id, tool_label, install_root) in targets {
        let tool_dir = skills_root.join(tool_id);
        if !tool_dir.is_dir() {
            continue;
        }
        let Ok(dirs) = fs::read_dir(&tool_dir) else {
            continue;
        };

        let mut skill_dirs: Vec<PathBuf> = dirs
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir() && p.join("SKILL.md").is_file())
            .collect();
        skill_dirs.sort();

        for sd in skill_dirs {
            let Some(skill_name) = sd.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let repo_skill = sd.join("SKILL.md");
            let installed_skill = install_root.join(skill_name).join("SKILL.md");
            // One unreadable skill must not hide the rest of the list.
            let Ok(content) = read_utf8(&repo_skill) else {
                continue;
            };
            let norm_repo = normalize_text(&content);

            let status = if !installed_skill.is_file() {
                "NOT_INSTALLED".to_string()
            } else {
                let installed_raw = read_utf8(&installed_skill).unwrap_or_default();
                if norm_repo == normalize_text(&installed_raw) {
                    "SYNCED".to_string()
                } else {
                    "OUT_OF_SYNC".to_string()
                }
            };

            entries.push(SkillEntry {
                tool: tool_id.to_string(),
                tool_label: tool_label.to_string(),
                skill_name: skill_name.to_string(),
                is_generated: skill_name == "gloria-init-brain",
                repo_path: repo_skill.to_string_lossy().to_string(),
                installed_path: installed_skill.to_string_lossy().to_string(),
                status,
                content,
            });
        }
    }

    Ok(entries)
}

pub fn save_skill(
    repo_root: &Path,
    tool: &str,
    skill_name: &str,
    content: &str,
) -> Result<SkillEntry, String> {
    let valid_tool = match tool {
        "claude-code" => "Claude Code",
        "codex" => "Codex",
        "antigravity" => "Antigravity",
        _ => return Err(format!("Unsupported tool '{tool}'.")),
    };

    if skill_name == "gloria-init-brain" {
        return Err(
            "gloria-init-brain/SKILL.md is generated from brain/init.md. Edit brain/init.md instead."
                .to_string(),
        );
    }

    if !is_safe_file_name(skill_name) {
        return Err(format!("Invalid skill name '{skill_name}'."));
    }

    let skill_path = repo_root
        .join("skills")
        .join(tool)
        .join(skill_name)
        .join("SKILL.md");
    if !skill_path.is_file() {
        return Err(format!("Skill file '{}' does not exist.", skill_path.display()));
    }

    let normalized = content.replace("\r\n", "\n");
    write_utf8_no_bom(&skill_path, &normalized)?;

    let targets = tool_targets()?;
    let install_root = targets
        .into_iter()
        .find(|(id, _, _)| *id == tool)
        .map(|(_, _, p)| p)
        .ok_or_else(|| "Target tool path not found".to_string())?;
    let installed_skill = install_root.join(skill_name).join("SKILL.md");

    let status = if !installed_skill.is_file() {
        "NOT_INSTALLED".to_string()
    } else {
        let installed_raw = read_utf8(&installed_skill).unwrap_or_default();
        if normalize_text(&normalized) == normalize_text(&installed_raw) {
            "SYNCED".to_string()
        } else {
            "OUT_OF_SYNC".to_string()
        }
    };

    Ok(SkillEntry {
        tool: tool.to_string(),
        tool_label: valid_tool.to_string(),
        skill_name: skill_name.to_string(),
        is_generated: false,
        repo_path: skill_path.to_string_lossy().to_string(),
        installed_path: installed_skill.to_string_lossy().to_string(),
        status,
        content: normalized,
    })
}
