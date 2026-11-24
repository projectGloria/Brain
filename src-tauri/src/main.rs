#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod brain_core;
mod config;
mod rules;
mod skills;

use brain_core::{
    check_repo, compute_brain_fingerprint, detect_stubs, get_brain_files, install_brain,
    read_utf8, remove_legacy_skill, sync_project, InstallResult, RepoStatus, SyncResult,
};
use config::{
    is_valid_brain_repo, legacy_skill_dir, load_or_init_config, master_brain_dir, save_config,
    AppConfig,
};
use rules::{
    create_rule, delete_rule, list_rules, preview_fingerprint, read_rule, resolve_brain_dir,
    save_rule, FingerprintPreview, RuleDocument, RuleSummary,
};
use serde::Serialize;
use skills::{list_skills, save_skill, SkillEntry};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
struct ToolPathInfo {
    tool: String,
    label: String,
    path: String,
    exists: bool,
}

#[derive(Debug, Clone, Serialize)]
struct AppStateSnapshot {
    config: AppConfig,
    master_dir: String,
    master_exists: bool,
    brain_dir: Option<String>,
    master_fingerprint: String,
    repo_fingerprint: String,
    master_in_sync_with_repo: bool,
    stub_files: Vec<String>,
    legacy_skill_present: bool,
    legacy_skill_path: String,
    tool_paths: Vec<ToolPathInfo>,
}

fn build_app_state() -> Result<AppStateSnapshot, String> {
    let cfg = load_or_init_config()?;
    let master_dir = master_brain_dir()?;
    let master_exists = master_dir.is_dir();

    let repo_brain_dir = cfg
        .brain_repo_path
        .as_deref()
        .map(|p| Path::new(p).join("brain"))
        .filter(|p| p.is_dir());

    let effective_dir = if master_exists {
        master_dir.clone()
    } else if let Some(ref rbd) = repo_brain_dir {
        rbd.clone()
    } else {
        master_dir.clone()
    };

    let master_fingerprint = compute_brain_fingerprint(&effective_dir, None)?;
    let repo_fingerprint = if let Some(ref rbd) = repo_brain_dir {
        compute_brain_fingerprint(rbd, None)?
    } else {
        master_fingerprint.clone()
    };

    let master_in_sync_with_repo = match &repo_brain_dir {
        Some(_) => master_exists && master_fingerprint == repo_fingerprint,
        None => master_exists,
    };

    let stub_check_dir = repo_brain_dir.as_ref().unwrap_or(&effective_dir);
    let mut stub_files = Vec::new();
    for f in get_brain_files(stub_check_dir, None) {
        if let Ok(c) = read_utf8(&f.path) {
            if !detect_stubs(&c).is_empty() {
                stub_files.push(f.name);
            }
        }
    }

    let legacy_dir = legacy_skill_dir()?;
    let legacy_skill_present = legacy_dir.exists();
    let home = dirs::home_dir().ok_or_else(|| "Could not resolve home directory".to_string())?;

    let tool_paths = vec![
        ToolPathInfo {
            tool: "claude-code".to_string(),
            label: "Claude Code".to_string(),
            path: home.join(".claude").join("skills").to_string_lossy().to_string(),
            exists: home.join(".claude").join("skills").is_dir(),
        },
        ToolPathInfo {
            tool: "codex".to_string(),
            label: "Codex".to_string(),
            path: home.join(".agents").join("skills").to_string_lossy().to_string(),
            exists: home.join(".agents").join("skills").is_dir(),
        },
        ToolPathInfo {
            tool: "antigravity".to_string(),
            label: "Antigravity".to_string(),
            path: home
                .join(".gemini")
                .join("config")
                .join("skills")
                .to_string_lossy()
                .to_string(),
            exists: home.join(".gemini").join("config").join("skills").is_dir(),
        },
    ];

    Ok(AppStateSnapshot {
        config: cfg,
        master_dir: master_dir.to_string_lossy().to_string(),
        master_exists,
        brain_dir: repo_brain_dir.map(|p| p.to_string_lossy().to_string()),
        master_fingerprint,
        repo_fingerprint,
        master_in_sync_with_repo,
        stub_files,
        legacy_skill_present,
        legacy_skill_path: legacy_dir.to_string_lossy().to_string(),
        tool_paths,
    })
}

#[tauri::command]
fn get_app_state() -> Result<AppStateSnapshot, String> {
    build_app_state()
}

#[tauri::command]
fn update_config(config: AppConfig) -> Result<AppStateSnapshot, String> {
    if let Some(ref repo) = config.brain_repo_path {
        let p = Path::new(repo);
        if !repo.trim().is_empty() && !is_valid_brain_repo(p) {
            return Err(format!(
                "'{}' does not look like the Brain repository (missing brain/init.md or install-brain.bat).",
                repo
            ));
        }
    }
    save_config(&config)?;
    build_app_state()
}

#[tauri::command]
fn scan_repos() -> Result<Vec<RepoStatus>, String> {
    let cfg = load_or_init_config()?;
    let master_dir = master_brain_dir()?;
    let fallback_brain = cfg
        .brain_repo_path
        .as_deref()
        .map(|p| Path::new(p).join("brain"));
    let brain_repo = cfg.brain_repo_path.as_deref().map(Path::new);

    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut candidate_paths: Vec<PathBuf> = Vec::new();

    let mut push_if_new = |p: PathBuf| {
        if !p.is_dir() {
            return;
        }
        let key = p.to_string_lossy().replace('/', "\\").to_ascii_lowercase();
        if seen.insert(key) {
            candidate_paths.push(p);
        }
    };

    if let Some(br) = brain_repo {
        push_if_new(br.to_path_buf());
    }

    for root_str in &cfg.scan_roots {
        let root = Path::new(root_str);
        if !root.is_dir() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with('.') || name.eq_ignore_ascii_case("node_modules") {
                        continue;
                    }
                }
                push_if_new(path);
            }
        }
    }

    for custom_str in &cfg.custom_repos {
        push_if_new(PathBuf::from(custom_str));
    }

    let mut results = Vec::with_capacity(candidate_paths.len());
    for repo_path in candidate_paths {
        results.push(check_repo(
            &repo_path,
            &master_dir,
            fallback_brain.as_deref(),
            brain_repo,
        )?);
    }

    results.sort_by(|a, b| {
        b.is_brain_repo
            .cmp(&a.is_brain_repo)
            .then_with(|| a.repo.to_ascii_lowercase().cmp(&b.repo.to_ascii_lowercase()))
    });

    Ok(results)
}

#[tauri::command]
fn check_repo_cmd(repo_path: String) -> Result<RepoStatus, String> {
    let cfg = load_or_init_config()?;
    let master_dir = master_brain_dir()?;
    let fallback_brain = cfg
        .brain_repo_path
        .as_deref()
        .map(|p| Path::new(p).join("brain"));
    let brain_repo = cfg.brain_repo_path.as_deref().map(Path::new);
    check_repo(
        Path::new(&repo_path),
        &master_dir,
        fallback_brain.as_deref(),
        brain_repo,
    )
}

#[tauri::command]
fn sync_repo_cmd(repo_path: String) -> Result<SyncResult, String> {
    let cfg = load_or_init_config()?;
    let master_dir = master_brain_dir()?;
    let fallback_brain = cfg
        .brain_repo_path
        .as_deref()
        .map(|p| Path::new(p).join("brain"));
    sync_project(Path::new(&repo_path), &master_dir, fallback_brain.as_deref())
}

#[tauri::command]
fn sync_all_repos_cmd(repo_paths: Vec<String>) -> Result<Vec<SyncResult>, String> {
    let cfg = load_or_init_config()?;
    let master_dir = master_brain_dir()?;
    let fallback_brain = cfg
        .brain_repo_path
        .as_deref()
        .map(|p| Path::new(p).join("brain"));

    let mut results = Vec::with_capacity(repo_paths.len());
    for rp in repo_paths {
        results.push(sync_project(
            Path::new(&rp),
            &master_dir,
            fallback_brain.as_deref(),
        )?);
    }
    Ok(results)
}

#[tauri::command]
fn install_brain_cmd() -> Result<InstallResult, String> {
    let cfg = load_or_init_config()?;
    let master_dir = master_brain_dir()?;
    let repo_root = cfg.brain_repo_path.as_deref().map(Path::new);
    install_brain(repo_root, &master_dir)
}

#[tauri::command]
fn remove_legacy_skill_cmd() -> Result<bool, String> {
    remove_legacy_skill()
}

#[tauri::command]
fn list_rules_cmd() -> Result<Vec<RuleSummary>, String> {
    let cfg = load_or_init_config()?;
    let master_dir = master_brain_dir()?;
    let brain_dir = resolve_brain_dir(cfg.brain_repo_path.as_deref(), &master_dir)?;
    list_rules(&brain_dir)
}

#[tauri::command]
fn read_rule_cmd(name: String) -> Result<RuleDocument, String> {
    let cfg = load_or_init_config()?;
    let master_dir = master_brain_dir()?;
    let brain_dir = resolve_brain_dir(cfg.brain_repo_path.as_deref(), &master_dir)?;
    read_rule(&brain_dir, &name)
}

#[tauri::command]
fn save_rule_cmd(name: String, raw_content: String) -> Result<RuleDocument, String> {
    let cfg = load_or_init_config()?;
    let master_dir = master_brain_dir()?;
    let brain_dir = resolve_brain_dir(cfg.brain_repo_path.as_deref(), &master_dir)?;
    save_rule(&brain_dir, &name, &raw_content)
}

#[tauri::command]
fn create_rule_cmd(name: String) -> Result<RuleDocument, String> {
    let cfg = load_or_init_config()?;
    let master_dir = master_brain_dir()?;
    let brain_dir = resolve_brain_dir(cfg.brain_repo_path.as_deref(), &master_dir)?;
    create_rule(&brain_dir, &name)
}

#[tauri::command]
fn delete_rule_cmd(name: String) -> Result<(), String> {
    let cfg = load_or_init_config()?;
    let master_dir = master_brain_dir()?;
    let brain_dir = resolve_brain_dir(cfg.brain_repo_path.as_deref(), &master_dir)?;
    delete_rule(&brain_dir, &name)
}

#[tauri::command]
fn preview_fingerprint_cmd(
    file_name: Option<String>,
    draft_content: Option<String>,
) -> Result<FingerprintPreview, String> {
    let cfg = load_or_init_config()?;
    let master_dir = master_brain_dir()?;
    let brain_dir = resolve_brain_dir(cfg.brain_repo_path.as_deref(), &master_dir)?;
    preview_fingerprint(&brain_dir, file_name.as_deref(), draft_content.as_deref())
}

#[tauri::command]
fn list_skills_cmd() -> Result<Vec<SkillEntry>, String> {
    let cfg = load_or_init_config()?;
    let repo_root = cfg.brain_repo_path.as_deref().map(Path::new);
    list_skills(repo_root)
}

#[tauri::command]
fn save_skill_cmd(tool: String, skill_name: String, content: String) -> Result<SkillEntry, String> {
    let cfg = load_or_init_config()?;
    let repo_root = cfg
        .brain_repo_path
        .as_deref()
        .map(Path::new)
        .ok_or_else(|| "Brain repository path is not configured.".to_string())?;
    save_skill(repo_root, &tool, &skill_name, &content)
}

#[tauri::command]
fn open_in_explorer_cmd(path: String) -> Result<(), String> {
    let target = Path::new(&path);
    if !target.exists() {
        return Err(format!("Path '{}' does not exist.", path));
    }
    Command::new("explorer.exe")
        .arg(target)
        .spawn()
        .map_err(|e| format!("Failed to open Explorer: {e}"))?;
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_app_state,
            update_config,
            scan_repos,
            check_repo_cmd,
            sync_repo_cmd,
            sync_all_repos_cmd,
            install_brain_cmd,
            remove_legacy_skill_cmd,
            list_rules_cmd,
            read_rule_cmd,
            save_rule_cmd,
            create_rule_cmd,
            delete_rule_cmd,
            preview_fingerprint_cmd,
            list_skills_cmd,
            save_skill_cmd,
            open_in_explorer_cmd,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Brain application");
}
