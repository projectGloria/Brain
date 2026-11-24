use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub brain_repo_path: Option<String>,
    pub scan_roots: Vec<String>,
    pub custom_repos: Vec<String>,
}

pub fn master_brain_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "Could not resolve user home directory".to_string())?;
    Ok(home.join(".brain"))
}

pub fn legacy_skill_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "Could not resolve user home directory".to_string())?;
    Ok(home
        .join(".gemini")
        .join("config")
        .join("skills")
        .join("agent-context-generator"))
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

    for c in candidates {
        if is_valid_brain_repo(&c) {
            return Some(c);
        }
    }
    None
}

pub fn load_or_init_config() -> Result<AppConfig, String> {
    let path = config_file_path()?;
    let mut cfg = if path.is_file() {
        let raw = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read config {}: {e}", path.display()))?;
        serde_json::from_str::<AppConfig>(&raw).unwrap_or_default()
    } else {
        AppConfig::default()
    };

    let needs_repo_detect = match &cfg.brain_repo_path {
        Some(p) => !is_valid_brain_repo(Path::new(p)),
        None => true,
    };

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
    fs::write(&path, json).map_err(|e| format!("Failed to write config {}: {e}", path.display()))
}
