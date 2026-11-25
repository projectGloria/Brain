use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AppConfig {
    pub brain_repo_path: Option<String>,
    pub scan_roots: Vec<String>,
    pub custom_repos: Vec<String>,
}

pub fn master_brain_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "Could not resolve user home directory".to_string())?;
    Ok(home.join(".brain"))
}

/// Both places the retired `agent-context-generator` Antigravity skill may live.
pub fn legacy_skill_dirs() -> Result<Vec<PathBuf>, String> {
    let home = dirs::home_dir().ok_or_else(|| "Could not resolve user home directory".to_string())?;
    let gemini = home.join(".gemini");
    Ok(vec![
        gemini.join("config").join("skills").join("agent-context-generator"),
        gemini.join("antigravity").join("skills").join("agent-context-generator"),
    ])
}

/// The first legacy skill folder that exists, or the primary location when none does.
pub fn legacy_skill_dir() -> Result<PathBuf, String> {
    let dirs = legacy_skill_dirs()?;
    Ok(dirs
        .iter()
        .find(|d| d.exists())
        .cloned()
        .unwrap_or_else(|| dirs[0].clone()))
}

pub fn config_file_path() -> Result<PathBuf, String> {
    let cfg = dirs::config_dir().ok_or_else(|| "Could not resolve %APPDATA% directory".to_string())?;
    Ok(cfg.join("Brain").join("config.json"))
}

pub fn is_valid_brain_repo(path: &Path) -> bool {
    path.is_dir()
        && path.join("brain").join("init.md").is_file()
        && path.join("install-brain.bat").is_file()
}

pub fn detect_brain_repo() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Ok(cwd) = std::env::current_dir() {
        let mut cur = Some(cwd.as_path());
        for _ in 0..6 {
            if let Some(p) = cur {
                candidates.push(p.to_path_buf());
                cur = p.parent();
            }
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        let mut cur = exe.parent();
        for _ in 0..6 {
            if let Some(p) = cur {
                candidates.push(p.to_path_buf());
                cur = p.parent();
            }
        }
    }

    candidates.into_iter().find(|c| is_valid_brain_repo(c))
}

pub fn load_or_init_config() -> Result<AppConfig, String> {
    let path = config_file_path()?;
    let mut cfg = if path.is_file() {
        let raw = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read config {}: {e}", path.display()))?;
        match serde_json::from_str::<AppConfig>(raw.trim_start_matches('\u{FEFF}')) {
            Ok(c) => c,
            Err(_) => {
                // Keep the unreadable file for the user instead of silently overwriting it.
                let _ = fs::copy(&path, path.with_extension("json.bad"));
                AppConfig::default()
            }
        }
    } else {
        AppConfig::default()
    };

    // Only auto-detect when no path was ever set; a user-chosen path that went
    // invalid is reported in the UI rather than silently replaced.
    let needs_repo_detect = cfg
        .brain_repo_path
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .is_empty();

    let mut changed = !path.is_file();
    if needs_repo_detect {
        if let Some(detected) = detect_brain_repo() {
            let detected_str = detected.to_string_lossy().to_string();
            cfg.brain_repo_path = Some(detected_str);
            if cfg.scan_roots.is_empty() {
                if let Some(parent) = detected.parent() {
                    if parent.is_dir() {
                        cfg.scan_roots.push(parent.to_string_lossy().to_string());
                    }
                }
            }
            changed = true;
        }
    }

    if changed {
        let _ = save_config(&cfg);
    }

    Ok(cfg)
}

pub fn save_config(cfg: &AppConfig) -> Result<(), String> {
    let path = config_file_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config directory: {e}"))?;
    }
    let json = serde_json::to_string_pretty(cfg)
        .map_err(|e| format!("Failed to serialize config: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| format!("Failed to write config {}: {e}", tmp.display()))?;
    fs::rename(&tmp, &path).map_err(|e| format!("Failed to write config {}: {e}", path.display()))
}
