use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

use super::keybinds::{ButtonBinding, default_bindings};
use super::storage::{get_storage_path, atomic_write_file};

const PROFILES_FILE: &str = "profiles.json";
const ACTIVE_PROFILE_FILE: &str = "active_profile.txt";
const G9_CONFIG_FILE: &str = "g9_config.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct G9Config {
    pub locked: bool,
}

impl Default for G9Config {
    fn default() -> Self {
        Self { locked: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppProfile {
    pub id: String,
    pub name: String,
    pub description: String,
    pub dpi_presets: Vec<u16>,
    pub current_dpi: u16,
    pub rgb_mode: String,
    pub rgb_color: (u8, u8, u8),
    pub keybinds: Vec<ButtonBinding>,
}

fn get_profiles_path() -> PathBuf {
    get_storage_path(PROFILES_FILE)
}

fn get_active_profile_path() -> PathBuf {
    get_storage_path(ACTIVE_PROFILE_FILE)
}

/// Standard built-in default profiles
pub fn default_profiles() -> Vec<AppProfile> {
    let mut fps_binds = default_bindings();
    for b in fps_binds.iter_mut() {
        if b.button_id == "G4" {
            b.assigned_macro = "⚡ Быстрый клик ЛКМ (Toggle)".into();
        }
    }

    vec![
        AppProfile {
            id: "profile_desktop".into(),
            name: "Основной (Desktop)".into(),
            description: "Стандартный профиль для повседневной работы в Windows".into(),
            dpi_presets: vec![400, 800, 1600, 3200],
            current_dpi: 800,
            rgb_mode: "static".into(),
            rgb_color: (0, 229, 255), // Cyan
            keybinds: default_bindings(),
        },
        AppProfile {
            id: "profile_gaming".into(),
            name: "Игровой (Gaming FPS)".into(),
            description: "Оптимизировано для CS2, Valorant и шутеров: низкий DPI 400 и кликер на G4".into(),
            dpi_presets: vec![400, 800, 1200],
            current_dpi: 400,
            rgb_mode: "static".into(),
            rgb_color: (255, 23, 68), // Red
            keybinds: fps_binds,
        },
        AppProfile {
            id: "profile_work".into(),
            name: "Работа & Офис".into(),
            description: "Высокий DPI для мультимониторных рабочих станций и браузинга".into(),
            dpi_presets: vec![800, 1200, 1600, 2400],
            current_dpi: 1600,
            rgb_mode: "breathe".into(),
            rgb_color: (0, 206, 136), // Green
            keybinds: default_bindings(),
        },
    ]
}

/// Load profiles from disk, initializing defaults if missing
pub fn load_profiles() -> Vec<AppProfile> {
    load_profiles_from_path(&get_profiles_path())
}

pub fn load_profiles_from_path(path: &Path) -> Vec<AppProfile> {
    if !path.exists() {
        let defaults = default_profiles();
        let _ = save_profiles_to_path(&defaults, path);
        return defaults;
    }

    match fs::read_to_string(path) {
        Ok(json_str) => match serde_json::from_str::<Vec<AppProfile>>(&json_str) {
            Ok(list) if !list.is_empty() => list,
            _ => {
                let defaults = default_profiles();
                let _ = save_profiles_to_path(&defaults, path);
                defaults
            }
        },
        Err(_) => {
            let defaults = default_profiles();
            let _ = save_profiles_to_path(&defaults, path);
            defaults
        }
    }
}

/// Save profiles list to disk
pub fn save_profiles(profiles: &[AppProfile]) -> Result<(), String> {
    save_profiles_to_path(profiles, &get_profiles_path())
}

pub fn save_profiles_to_path(profiles: &[AppProfile], path: &Path) -> Result<(), String> {
    let json_str = serde_json::to_string_pretty(profiles)
        .map_err(|e| format!("Profiles serialization error: {e}"))?;
    atomic_write_file(path, &json_str)
}

/// Get current active profile ID
pub fn get_active_profile_id() -> String {
    let path = get_active_profile_path();
    if path.exists()
        && let Ok(id) = fs::read_to_string(&path) {
            let trimmed = id.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    "profile_desktop".to_string()
}

/// Set active profile ID
pub fn set_active_profile_id(id: &str) -> Result<(), String> {
    atomic_write_file(&get_active_profile_path(), id.trim())
}

/// Retrieve the currently active profile
pub fn get_active_profile(profiles: &[AppProfile]) -> AppProfile {
    let active_id = get_active_profile_id();
    profiles.iter()
        .find(|p| p.id == active_id)
        .cloned()
        .unwrap_or_else(|| {
            profiles.first().cloned().unwrap_or_else(|| default_profiles().remove(0))
        })
}

/// Update or add a profile
pub fn save_or_update_profile(profiles: &mut Vec<AppProfile>, profile: AppProfile) -> Result<(), String> {
    if let Some(pos) = profiles.iter().position(|p| p.id == profile.id) {
        profiles[pos] = profile;
    } else {
        profiles.push(profile);
    }
    save_profiles(profiles)
}

/// Delete a profile by ID (cannot delete if only 1 profile left)
pub fn delete_profile_by_id(profiles: &mut Vec<AppProfile>, id: &str) -> Result<bool, String> {
    if profiles.len() <= 1 {
        return Err("Невозможно удалить единственный профиль".into());
    }
    let orig_len = profiles.len();
    profiles.retain(|p| p.id != id);
    if profiles.len() < orig_len {
        save_profiles(profiles)?;
        if get_active_profile_id() == id
            && let Some(first) = profiles.first() {
                let _ = set_active_profile_id(&first.id);
            }
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Get G9 Lock setting (defaults to true)
pub fn get_g9_lock() -> bool {
    let path = get_storage_path(G9_CONFIG_FILE);
    if path.exists()
        && let Ok(content) = fs::read_to_string(&path)
        && let Ok(cfg) = serde_json::from_str::<G9Config>(&content) {
            return cfg.locked;
        }
    true
}

/// Set G9 Lock setting
pub fn set_g9_lock(locked: bool) -> Result<(), String> {
    let cfg = G9Config { locked };
    let json = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
    atomic_write_file(&get_storage_path(G9_CONFIG_FILE), &json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_profiles() {
        let p = default_profiles();
        assert!(p.len() >= 3);
        assert_eq!(p[0].id, "profile_desktop");
        assert_eq!(p[1].id, "profile_gaming");
        assert_eq!(p[2].id, "profile_work");
    }

    #[test]
    fn test_save_and_load_profiles() {
        let dir = std::env::temp_dir().join(format!("ghub_test_prof_{}", std::process::id()));
        let path = dir.join("test_profiles.json");
        let def = default_profiles();

        save_profiles_to_path(&def, &path).unwrap();
        let loaded = load_profiles_from_path(&path);
        let _ = fs::remove_dir_all(&dir);

        assert_eq!(loaded.len(), def.len());
        assert_eq!(loaded[0].name, def[0].name);
    }

    #[test]
    fn test_delete_profile() {
        let mut def = default_profiles();
        let initial_len = def.len();
        assert!(delete_profile_by_id(&mut def, "profile_work").unwrap());
        assert_eq!(def.len(), initial_len - 1);
    }

    #[test]
    fn test_g9_config() {
        let cfg = G9Config::default();
        assert!(cfg.locked);
        let serialized = serde_json::to_string(&cfg).unwrap();
        let deserialized: G9Config = serde_json::from_str(&serialized).unwrap();
        assert_eq!(cfg, deserialized);
    }
}
