#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent_runs;
mod brain_core;
mod clis;
mod config;
mod dna;
mod gitignore;
mod release;
mod rules;
mod skills;

use brain_core::{
    check_repo_with, compute_brain_fingerprint, detect_stubs, error_status, get_brain_files,
    install_brain, normalize_path, read_utf8, remove_legacy_skill, sync_project, InstallResult,
    RepoStatus, ScanContext, SyncResult,
};
use clis::{CliActionResult, CliConfigDocument, CliInfo, CliLatest};
use config::{
    is_valid_brain_repo, legacy_skill_dir, load_or_init_config, master_brain_dir, save_config,
    AppConfig,
};
use dna::{DnaApplyResult, DnaProjectInspect, DnaProjectSummary};
use gitignore::{GitignoreApplyResult, GitignoreRepoStatus, GitignoreTemplate};
use rules::{
    create_rule, delete_rule, list_rules, preview_fingerprint, read_rule, resolve_brain_dir,
    save_rule, FingerprintPreview, RuleDocument, RuleSummary,
};
use serde::Serialize;
use skills::{list_skills, save_skill, tool_targets, SkillEntry};
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
    /// False when a Brain repo path is configured but no longer looks like the repo.
    brain_repo_valid: bool,
    master_fingerprint: String,
    repo_fingerprint: String,
    master_in_sync_with_repo: bool,
    skills_in_sync: bool,
    stub_files: Vec<String>,
    legacy_skill_present: bool,
    legacy_skill_path: String,
    tool_paths: Vec<ToolPathInfo>,
}

#[derive(Debug, Clone, Serialize)]
struct BatchSyncItem {
    repo_path: String,
    ok: bool,
    error: Option<String>,
    result: Option<SyncResult>,
}

/// Runs blocking filesystem / process work off the UI thread.
async fn blocking<T, F>(f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| format!("Background task failed: {e}"))?
}

struct Paths {
    cfg: AppConfig,
    master_dir: PathBuf,
    fallback_brain: Option<PathBuf>,
    brain_repo: Option<PathBuf>,
}

fn paths() -> Result<Paths, String> {
    let cfg = load_or_init_config()?;
    let master_dir = master_brain_dir()?;
    let brain_repo = cfg
        .brain_repo_path
        .as_deref()
        .filter(|p| !p.trim().is_empty())
        .map(PathBuf::from);
    let fallback_brain = brain_repo.as_ref().map(|p| p.join("brain"));
    Ok(Paths {
        cfg,
        master_dir,
        fallback_brain,
        brain_repo,
    })
}

impl Paths {
    fn scan_context(&self) -> ScanContext {
        let template = gitignore::load_template_text(self.brain_repo.as_deref());
        ScanContext::new(
            &self.master_dir,
            self.fallback_brain.as_deref(),
            self.brain_repo.as_deref(),
            template,
        )
    }

    fn brain_dir(&self) -> Result<PathBuf, String> {
        resolve_brain_dir(self.cfg.brain_repo_path.as_deref(), &self.master_dir)
    }
}

fn build_app_state() -> Result<AppStateSnapshot, String> {
    let p = paths()?;
    let master_exists = p.master_dir.is_dir();
    let brain_repo_valid = p.brain_repo.as_deref().map(is_valid_brain_repo).unwrap_or(true);

    let repo_brain_dir = p.fallback_brain.clone().filter(|d| d.is_dir());

    let effective_dir = if master_exists {
        p.master_dir.clone()
    } else if let Some(ref rbd) = repo_brain_dir {
        rbd.clone()
    } else {
        p.master_dir.clone()
    };

    let master_fingerprint = compute_brain_fingerprint(&effective_dir, None)?;
    let repo_fingerprint = if let Some(ref rbd) = repo_brain_dir {
        compute_brain_fingerprint(rbd, None)?
    } else {
        master_fingerprint.clone()
    };

    let skills = list_skills(p.brain_repo.as_deref()).unwrap_or_default();
    let skills_in_sync = skills.iter().all(|s| s.status == "SYNCED");

    let master_in_sync_with_repo = match &repo_brain_dir {
        Some(_) => master_exists && master_fingerprint == repo_fingerprint && skills_in_sync,
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

    let tool_paths = tool_targets()?
        .into_iter()
        .map(|(tool, label, path)| ToolPathInfo {
            tool: tool.to_string(),
            label: label.to_string(),
            exists: path.is_dir(),
            path: path.to_string_lossy().to_string(),
        })
        .collect();

    Ok(AppStateSnapshot {
        config: p.cfg,
        master_dir: p.master_dir.to_string_lossy().to_string(),
        master_exists,
        brain_dir: repo_brain_dir.map(|d| d.to_string_lossy().to_string()),
        brain_repo_valid,
        master_fingerprint,
        repo_fingerprint,
        master_in_sync_with_repo,
        skills_in_sync,
        stub_files,
        legacy_skill_present,
        legacy_skill_path: legacy_dir.to_string_lossy().to_string(),
        tool_paths,
    })
}

fn clean_list(items: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    items
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|s| normalize_path(Path::new(&s)))
        .filter(|s| seen.insert(s.to_lowercase()))
        .collect()
}

#[tauri::command]
async fn get_app_state() -> Result<AppStateSnapshot, String> {
    blocking(build_app_state).await
}

#[tauri::command]
async fn update_config(config: AppConfig) -> Result<AppStateSnapshot, String> {
    blocking(move || {
        let mut config = config;
        config.brain_repo_path = config
            .brain_repo_path
            .map(|r| r.trim().to_string())
            .filter(|r| !r.is_empty())
            .map(|r| normalize_path(Path::new(&r)));
        if let Some(ref repo) = config.brain_repo_path {
            if !is_valid_brain_repo(Path::new(repo)) {
                return Err(format!(
                    "'{repo}' does not look like the Brain repository (missing brain/init.md or install-brain.bat)."
                ));
            }
        }
        config.scan_roots = clean_list(config.scan_roots);
        config.custom_repos = clean_list(config.custom_repos);
        save_config(&config)?;
        build_app_state()
    })
    .await
}

fn candidate_repos(p: &Paths) -> Vec<PathBuf> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut out: Vec<PathBuf> = Vec::new();
    let mut push_if_new = |path: PathBuf, require_dir: bool| {
        if require_dir && !path.is_dir() {
            return;
        }
        if seen.insert(normalize_path(&path).to_lowercase()) {
            out.push(path);
        }
    };

    if let Some(ref br) = p.brain_repo {
        push_if_new(br.clone(), true);
    }

    for root_str in &p.cfg.scan_roots {
        let root = Path::new(root_str);
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };
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
            push_if_new(path, true);
        }
    }

    // Custom repos are listed even when missing, so the user sees an ERROR row.
    for custom_str in &p.cfg.custom_repos {
        push_if_new(PathBuf::from(custom_str), false);
    }
    out
}

#[tauri::command]
async fn scan_repos() -> Result<Vec<RepoStatus>, String> {
    blocking(|| {
        let p = paths()?;
        let ctx = p.scan_context();
        let mut results: Vec<RepoStatus> = candidate_repos(&p)
            .into_iter()
            .map(|repo| check_repo_with(&repo, &ctx).unwrap_or_else(|e| error_status(&repo, e)))
            .collect();
        results.sort_by(|a, b| {
            b.is_brain_repo
                .cmp(&a.is_brain_repo)
                .then_with(|| a.repo.to_ascii_lowercase().cmp(&b.repo.to_ascii_lowercase()))
        });
        Ok(results)
    })
    .await
}

#[tauri::command]
async fn check_repo_cmd(repo_path: String) -> Result<RepoStatus, String> {
    blocking(move || {
        let p = paths()?;
        check_repo_with(Path::new(&repo_path), &p.scan_context())
    })
    .await
}

#[tauri::command]
async fn sync_repo_cmd(repo_path: String) -> Result<SyncResult, String> {
    blocking(move || {
        let p = paths()?;
        sync_project(Path::new(&repo_path), &p.master_dir, p.fallback_brain.as_deref())
    })
    .await
}

#[tauri::command]
async fn sync_all_repos_cmd(repo_paths: Vec<String>) -> Result<Vec<BatchSyncItem>, String> {
    blocking(move || {
        let p = paths()?;
        Ok(repo_paths
            .into_iter()
            .map(|rp| {
                match sync_project(Path::new(&rp), &p.master_dir, p.fallback_brain.as_deref()) {
                    Ok(r) => BatchSyncItem { repo_path: rp, ok: true, error: None, result: Some(r) },
                    Err(e) => BatchSyncItem { repo_path: rp, ok: false, error: Some(e), result: None },
                }
            })
            .collect())
    })
    .await
}

#[tauri::command]
async fn install_brain_cmd() -> Result<InstallResult, String> {
    blocking(|| {
        let p = paths()?;
        install_brain(p.brain_repo.as_deref(), &p.master_dir)
    })
    .await
}

#[tauri::command]
async fn remove_legacy_skill_cmd() -> Result<bool, String> {
    blocking(remove_legacy_skill).await
}

#[tauri::command]
async fn list_rules_cmd() -> Result<Vec<RuleSummary>, String> {
    blocking(|| list_rules(&paths()?.brain_dir()?)).await
}

#[tauri::command]
async fn read_rule_cmd(name: String) -> Result<RuleDocument, String> {
    blocking(move || read_rule(&paths()?.brain_dir()?, &name)).await
}

#[tauri::command]
async fn save_rule_cmd(name: String, raw_content: String) -> Result<RuleDocument, String> {
    blocking(move || save_rule(&paths()?.brain_dir()?, &name, &raw_content)).await
}

#[tauri::command]
async fn create_rule_cmd(name: String) -> Result<RuleDocument, String> {
    blocking(move || create_rule(&paths()?.brain_dir()?, &name)).await
}

#[tauri::command]
async fn delete_rule_cmd(name: String) -> Result<(), String> {
    blocking(move || delete_rule(&paths()?.brain_dir()?, &name)).await
}

#[tauri::command]
async fn preview_fingerprint_cmd(
    file_name: Option<String>,
    draft_content: Option<String>,
) -> Result<FingerprintPreview, String> {
    blocking(move || {
        preview_fingerprint(&paths()?.brain_dir()?, file_name.as_deref(), draft_content.as_deref())
    })
    .await
}

#[tauri::command]
async fn list_skills_cmd() -> Result<Vec<SkillEntry>, String> {
    blocking(|| list_skills(paths()?.brain_repo.as_deref())).await
}

#[tauri::command]
async fn save_skill_cmd(tool: String, skill_name: String, content: String) -> Result<SkillEntry, String> {
    blocking(move || {
        let p = paths()?;
        let repo_root = p
            .brain_repo
            .ok_or_else(|| "Brain repository path is not configured.".to_string())?;
        save_skill(&repo_root, &tool, &skill_name, &content)
    })
    .await
}

#[tauri::command]
async fn open_in_explorer_cmd(path: String) -> Result<(), String> {
    blocking(move || {
        let target = PathBuf::from(&path);
        if !target.exists() {
            return Err(format!("Path '{path}' does not exist."));
        }
        let mut cmd = Command::new("explorer.exe");
        if target.is_file() {
            // Reveal the file instead of launching it with its default handler.
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.raw_arg(format!("/select,\"{}\"", target.display()));
            }
            #[cfg(not(windows))]
            cmd.arg(&target);
        } else {
            cmd.arg(&target);
        }
        cmd.spawn().map_err(|e| format!("Failed to open Explorer: {e}"))?;
        Ok(())
    })
    .await
}

// --- .gitignore ---

#[tauri::command]
async fn read_gitignore_template_cmd() -> Result<GitignoreTemplate, String> {
    blocking(|| gitignore::read_template(paths()?.brain_repo.as_deref())).await
}

#[tauri::command]
async fn save_gitignore_template_cmd(content: String) -> Result<GitignoreTemplate, String> {
    blocking(move || gitignore::save_template(paths()?.brain_repo.as_deref(), &content)).await
}

#[tauri::command]
async fn gitignore_status_cmd(repo_path: String) -> Result<GitignoreRepoStatus, String> {
    blocking(move || gitignore::repo_status(Path::new(&repo_path), paths()?.brain_repo.as_deref()))
        .await
}

#[tauri::command]
async fn apply_gitignore_cmd(repo_path: String) -> Result<GitignoreApplyResult, String> {
    blocking(move || {
        let p = paths()?;
        Ok(gitignore::apply_many(&[repo_path], p.brain_repo.as_deref()).remove(0))
    })
    .await
}

#[tauri::command]
async fn apply_gitignore_many_cmd(repo_paths: Vec<String>) -> Result<Vec<GitignoreApplyResult>, String> {
    blocking(move || Ok(gitignore::apply_many(&repo_paths, paths()?.brain_repo.as_deref()))).await
}

// --- CLIs ---

#[tauri::command]
async fn list_clis_cmd() -> Result<Vec<CliInfo>, String> {
    blocking(|| Ok(clis::list())).await
}

#[tauri::command]
async fn cli_latest_versions_cmd(force: Option<bool>) -> Result<Vec<CliLatest>, String> {
    blocking(move || Ok(clis::latest_versions(force.unwrap_or(false)))).await
}

#[tauri::command]
async fn install_cli_cmd(id: String) -> Result<CliActionResult, String> {
    blocking(move || clis::install(&id)).await
}

#[tauri::command]
async fn update_cli_cmd(id: String) -> Result<CliActionResult, String> {
    blocking(move || clis::update(&id)).await
}

#[tauri::command]
async fn read_cli_config_cmd(id: String, key: String) -> Result<CliConfigDocument, String> {
    blocking(move || clis::read_config(&id, &key)).await
}

#[tauri::command]
async fn save_cli_config_cmd(id: String, key: String, content: String) -> Result<CliConfigDocument, String> {
    blocking(move || clis::save_config(&id, &key, &content)).await
}

// --- Agent runs ---

#[tauri::command]
async fn start_agent_run(
    app: tauri::AppHandle,
    req: agent_runs::AgentRunRequest,
) -> Result<agent_runs::AgentRunStarted, String> {
    blocking(move || agent_runs::start(app, req)).await
}

#[tauri::command]
async fn cancel_agent_run(run_id: String) -> Result<bool, String> {
    blocking(move || agent_runs::cancel(&run_id)).await
}

#[tauri::command]
async fn cli_auth_status(id: String) -> Result<agent_runs::AuthStatus, String> {
    blocking(move || agent_runs::auth_status(&id)).await
}

#[tauri::command]
async fn open_cli_login(id: String) -> Result<(), String> {
    blocking(move || agent_runs::open_login(&id)).await
}

#[tauri::command]
async fn cli_models_cmd(id: String, force: Option<bool>) -> Result<agent_runs::ModelCatalog, String> {
    blocking(move || agent_runs::model_catalog(&id, force.unwrap_or(false))).await
}

// --- DNA (gloriaTools analyzer & generator) ---

#[tauri::command]
async fn dna_scan_repos_cmd(repo_paths: Vec<String>) -> Result<Vec<DnaProjectSummary>, String> {
    blocking(move || Ok(dna::scan_repos(&repo_paths))).await
}

#[tauri::command]
async fn dna_inspect_repo_cmd(repo_path: String) -> Result<DnaProjectInspect, String> {
    blocking(move || Ok(dna::inspect_repo(Path::new(&repo_path)))).await
}

#[tauri::command]
async fn dna_create_tools_cmd(
    repo_path: String,
    tool_names: Option<Vec<String>>,
) -> Result<DnaApplyResult, String> {
    blocking(move || Ok(dna::create_tools(Path::new(&repo_path), tool_names.as_deref()))).await
}

#[tauri::command]
async fn dna_create_tools_many_cmd(repo_paths: Vec<String>) -> Result<Vec<DnaApplyResult>, String> {
    blocking(move || {
        Ok(repo_paths
            .iter()
            .map(|p| dna::create_tools(Path::new(p), None))
            .collect())
    })
    .await
}

// --- Releases ---

#[tauri::command]
async fn release_preflight_cmd(repo_path: String) -> Result<release::ReleasePreflight, String> {
    blocking(move || release::preflight(Path::new(&repo_path))).await
}

#[tauri::command]
async fn release_bootstrap_cmd(repo_path: String) -> Result<Vec<String>, String> {
    blocking(move || release::bootstrap_gloria_setup(Path::new(&repo_path), None)).await
}

#[tauri::command]
async fn start_release_cmd(
    app: tauri::AppHandle,
    req: release::ReleaseRequest,
) -> Result<release::ReleaseStarted, String> {
    blocking(move || release::start(app, req)).await
}

#[tauri::command]
async fn cancel_release_cmd(run_id: String) -> Result<bool, String> {
    blocking(move || release::cancel(&run_id)).await
}

#[tauri::command]
async fn open_url_cmd(url: String) -> Result<(), String> {
    blocking(move || {
        let trimmed = url.trim();
        if !trimmed.starts_with("https://") && !trimmed.starts_with("http://") {
            return Err("Only http(s) URLs can be opened.".to_string());
        }
        #[cfg(windows)]
        let mut cmd = {
            let mut c = Command::new("explorer.exe");
            c.arg(trimmed);
            c
        };
        #[cfg(target_os = "macos")]
        let mut cmd = {
            let mut c = Command::new("open");
            c.arg(trimmed);
            c
        };
        #[cfg(all(not(windows), not(target_os = "macos")))]
        let mut cmd = {
            let mut c = Command::new("xdg-open");
            c.arg(trimmed);
            c
        };
        cmd.spawn()
            .map_err(|e| format!("Failed to open URL: {e}"))?;
        Ok(())
    })
    .await
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
            read_gitignore_template_cmd,
            save_gitignore_template_cmd,
            gitignore_status_cmd,
            apply_gitignore_cmd,
            apply_gitignore_many_cmd,
            list_clis_cmd,
            cli_latest_versions_cmd,
            install_cli_cmd,
            update_cli_cmd,
            read_cli_config_cmd,
            save_cli_config_cmd,
            start_agent_run,
            cancel_agent_run,
            cli_models_cmd,
            cli_auth_status,
            open_cli_login,
            dna_scan_repos_cmd,
            dna_inspect_repo_cmd,
            dna_create_tools_cmd,
            dna_create_tools_many_cmd,
            release_preflight_cmd,
            release_bootstrap_cmd,
            start_release_cmd,
            cancel_release_cmd,
            open_url_cmd,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Brain application")
        .run(|_app, event| {
            // No agent or release should keep running after the window is gone.
            if let tauri::RunEvent::Exit = event {
                agent_runs::cancel_all();
                release::cancel_all();
            }
        });
}
