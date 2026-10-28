use std::fs;
use std::path::{Path, PathBuf};
use super::db::{LgMacro, LgMacroComponent};

const DEFAULT_MACROS_FILE: &str = "user_macros.json";

fn get_storage_path() -> PathBuf {
    PathBuf::from(DEFAULT_MACROS_FILE)
}

/// Returns standard starter presets if user has no saved macros yet
pub fn default_preset_macros() -> Vec<LgMacro> {
    vec![
        LgMacro {
            name: "⚡ Быстрый клик ЛКМ (Toggle)".into(),
            macro_type: "toggle".into(),
            action_name: Some("Auto-Clicker Left Button".into()),
            is_toggle: true,
            sequence: vec![
                LgMacroComponent {
                    mouse_button: Some(1),
                    mouse_down: Some(true),
                    vk_code: None,
                    key_down: None,
                    delay_ms: Some(15),
                },
                LgMacroComponent {
                    mouse_button: Some(1),
                    mouse_down: Some(false),
                    vk_code: None,
                    key_down: None,
                    delay_ms: Some(15),
                },
            ],
        },
        LgMacro {
            name: "🎯 Тройной выстрел (Burst 3x)".into(),
            macro_type: "sequence".into(),
            action_name: Some("3x Left Click Burst".into()),
            is_toggle: false,
            sequence: vec![
                LgMacroComponent { mouse_button: Some(1), mouse_down: Some(true), vk_code: None, key_down: None, delay_ms: Some(15) },
                LgMacroComponent { mouse_button: Some(1), mouse_down: Some(false), vk_code: None, key_down: None, delay_ms: Some(40) },
                LgMacroComponent { mouse_button: Some(1), mouse_down: Some(true), vk_code: None, key_down: None, delay_ms: Some(15) },
                LgMacroComponent { mouse_button: Some(1), mouse_down: Some(false), vk_code: None, key_down: None, delay_ms: Some(40) },
                LgMacroComponent { mouse_button: Some(1), mouse_down: Some(true), vk_code: None, key_down: None, delay_ms: Some(15) },
                LgMacroComponent { mouse_button: Some(1), mouse_down: Some(false), vk_code: None, key_down: None, delay_ms: Some(10) },
            ],
        },
        LgMacro {
            name: "🚀 Прыжок с приседанием (Crouch Jump)".into(),
            macro_type: "sequence".into(),
            action_name: Some("Space + Ctrl Jump".into()),
            is_toggle: false,
            sequence: vec![
                // VK_SPACE = 0x20, VK_CONTROL = 0x11
                LgMacroComponent { mouse_button: None, mouse_down: None, vk_code: Some(0x20), key_down: Some(true), delay_ms: Some(10) },
                LgMacroComponent { mouse_button: None, mouse_down: None, vk_code: Some(0x11), key_down: Some(true), delay_ms: Some(60) },
                LgMacroComponent { mouse_button: None, mouse_down: None, vk_code: Some(0x20), key_down: Some(false), delay_ms: Some(10) },
                LgMacroComponent { mouse_button: None, mouse_down: None, vk_code: Some(0x11), key_down: Some(false), delay_ms: Some(10) },
            ],
        },
        LgMacro {
            name: "🔄 Спам ПКМ (Toggle)".into(),
            macro_type: "toggle".into(),
            action_name: Some("Auto-Clicker Right Button".into()),
            is_toggle: true,
            sequence: vec![
                LgMacroComponent {
                    mouse_button: Some(2),
                    mouse_down: Some(true),
                    vk_code: None,
                    key_down: None,
                    delay_ms: Some(25),
                },
                LgMacroComponent {
                    mouse_button: Some(2),
                    mouse_down: Some(false),
                    vk_code: None,
                    key_down: None,
                    delay_ms: Some(25),
                },
            ],
        },
    ]
}

/// Load custom user macros from disk, initializing defaults if none exist
pub fn load_custom_macros() -> Vec<LgMacro> {
    load_custom_macros_from_path(&get_storage_path())
}

pub fn load_custom_macros_from_path(path: &Path) -> Vec<LgMacro> {
    if !path.exists() {
        let defaults = default_preset_macros();
        let _ = save_custom_macros_to_path(&defaults, path);
        return defaults;
    }

    match fs::read_to_string(path) {
        Ok(json_str) => match serde_json::from_str::<Vec<LgMacro>>(&json_str) {
            Ok(list) => {
                if list.is_empty() {
                    let defaults = default_preset_macros();
                    let _ = save_custom_macros_to_path(&defaults, path);
                    defaults
                } else {
                    list
                }
            }
            Err(e) => {
                eprintln!("[!] Warning: failed to parse custom macros JSON: {e}");
                default_preset_macros()
            }
        },
        Err(e) => {
            eprintln!("[!] Warning: failed to read custom macros file: {e}");
            default_preset_macros()
        }
    }
}

/// Save custom user macros to disk
pub fn save_custom_macros(macros: &[LgMacro]) -> Result<(), String> {
    save_custom_macros_to_path(macros, &get_storage_path())
}

pub fn save_custom_macros_to_path(macros: &[LgMacro], path: &Path) -> Result<(), String> {
    let json_str = serde_json::to_string_pretty(macros)
        .map_err(|e| format!("JSON serialization error: {e}"))?;
    fs::write(path, json_str).map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
    Ok(())
}

/// Add or update a custom macro in the list and save
pub fn save_or_update_macro(list: &mut Vec<LgMacro>, new_macro: LgMacro) -> Result<(), String> {
    if let Some(existing) = list.iter_mut().find(|m| m.name == new_macro.name) {
        *existing = new_macro;
    } else {
        list.push(new_macro);
    }
    save_custom_macros(list)
}

/// Update an existing macro, replacing old_name if it was renamed
pub fn update_custom_macro(list: &mut Vec<LgMacro>, old_name: &str, new_macro: LgMacro) -> Result<(), String> {
    if !old_name.is_empty()
        && let Some(pos) = list.iter().position(|m| m.name == old_name) {
            list[pos] = new_macro;
            return save_custom_macros(list);
        }
    save_or_update_macro(list, new_macro)
}

/// Delete a custom macro by name and save
pub fn delete_macro(list: &mut Vec<LgMacro>, name: &str) -> Result<bool, String> {
    let original_len = list.len();
    list.retain(|m| m.name != name);
    if list.len() < original_len {
        save_custom_macros(list)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Convert a component to human-readable Russian label
pub fn format_component(comp: &LgMacroComponent) -> String {
    if let (Some(btn), Some(down)) = (comp.mouse_button, comp.mouse_down) {
        let btn_name = match btn {
            1 => "ЛКМ (Левая)",
            2 => "ПКМ (Правая)",
            3 => "СКМ (Колесо)",
            4 => "G4 (Боковая назад)",
            5 => "G5 (Боковая вперед)",
            _ => "Мышь",
        };
        let action = if down { "Нажатие (Вниз)" } else { "Отпускание (Вверх)" };
        let delay = comp.delay_ms.map(|d| format!(" [{} мс]", d)).unwrap_or_default();
        format!("🖱️ {} {}{}", btn_name, action, delay)
    } else if let (Some(vk), Some(down)) = (comp.vk_code, comp.key_down) {
        let key_name = vk_to_string(vk);
        let action = if down { "Нажатие" } else { "Отпускание" };
        let delay = comp.delay_ms.map(|d| format!(" [{} мс]", d)).unwrap_or_default();
        format!("⌨️ Клавиша '{}' {}{}", key_name, action, delay)
    } else if let Some(delay) = comp.delay_ms {
        format!("⏱️ Пауза {} мс", delay)
    } else {
        "Действие".into()
    }
}

/// Convert VK code to human readable name
pub fn vk_to_string(vk: u16) -> String {
    match vk {
        0x01 => "ЛКМ".into(),
        0x02 => "ПКМ".into(),
        0x04 => "СКМ".into(),
        0x05 => "XButton1 (G4)".into(),
        0x06 => "XButton2 (G5)".into(),
        0x08 => "Backspace".into(),
        0x09 => "Tab".into(),
        0x0D => "Enter".into(),
        0x10 => "Shift".into(),
        0x11 => "Ctrl".into(),
        0x12 => "Alt".into(),
        0x14 => "Caps Lock".into(),
        0x1B => "Esc".into(),
        0x20 => "Пробел".into(),
        0x25 => "Стрелка Влево".into(),
        0x26 => "Стрелка Вверх".into(),
        0x27 => "Стрелка Вправо".into(),
        0x28 => "Стрелка Вниз".into(),
        0x2D => "Insert".into(),
        0x2E => "Delete".into(),
        0x30..=0x39 => format!("{}", (vk as u8) as char),
        0x41..=0x5A => format!("{}", (vk as u8) as char),
        0x70..=0x87 => format!("F{}", vk - 0x70 + 1),
        0x60..=0x69 => format!("Num {}", vk - 0x60),
        _ => format!("VK 0x{:02X}", vk),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_presets() {
        let presets = default_preset_macros();
        assert!(presets.len() >= 3);
        assert!(presets.iter().any(|m| m.is_toggle));
        assert!(presets.iter().any(|m| !m.is_toggle));
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("ghub_test_macros_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test_macros.json");

        let initial = vec![
            LgMacro {
                name: "Test Macro 1".into(),
                macro_type: "sequence".into(),
                action_name: None,
                is_toggle: false,
                sequence: vec![
                    LgMacroComponent { mouse_button: Some(1), mouse_down: Some(true), vk_code: None, key_down: None, delay_ms: Some(50) }
                ],
            }
        ];

        save_custom_macros_to_path(&initial, &path).unwrap();
        let loaded = load_custom_macros_from_path(&path);
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "Test Macro 1");
        assert_eq!(loaded[0].sequence.len(), 1);
        assert_eq!(loaded[0].sequence[0].mouse_button, Some(1));
    }

    #[test]
    fn test_add_and_delete_macro() {
        let mut list = default_preset_macros();
        let original_len = list.len();

        let new_m = LgMacro {
            name: "Super Jump".into(),
            macro_type: "sequence".into(),
            action_name: None,
            is_toggle: false,
            sequence: vec![],
        };

        // Add
        if let Some(existing) = list.iter_mut().find(|m| m.name == new_m.name) {
            *existing = new_m;
        } else {
            list.push(new_m);
        }
        assert_eq!(list.len(), original_len + 1);

        // Delete
        list.retain(|m| m.name != "Super Jump");
        assert_eq!(list.len(), original_len);
    }

    #[test]
    fn test_format_component() {
        let c1 = LgMacroComponent {
            mouse_button: Some(1),
            mouse_down: Some(true),
            vk_code: None,
            key_down: None,
            delay_ms: Some(25),
        };
        let s1 = format_component(&c1);
        assert!(s1.contains("ЛКМ"));
        assert!(s1.contains("25 мс"));

        let c2 = LgMacroComponent {
            mouse_button: None,
            mouse_down: None,
            vk_code: Some(0x20),
            key_down: Some(true),
            delay_ms: Some(10),
        };
        let s2 = format_component(&c2);
        assert!(s2.contains("Пробел"));

        let c3 = LgMacroComponent {
            mouse_button: None,
            mouse_down: None,
            vk_code: None,
            key_down: None,
            delay_ms: Some(100),
        };
        let s3 = format_component(&c3);
        assert!(s3.contains("100 мс"));
    }

    #[test]
    fn test_update_custom_macro_with_rename() {
        let mut list = default_preset_macros();
        let orig_name = &list[0].name.clone();

        let mut updated = list[0].clone();
        updated.name = "Renamed Macro".into();
        updated.sequence.push(LgMacroComponent {
            mouse_button: Some(2),
            mouse_down: Some(true),
            vk_code: None,
            key_down: None,
            delay_ms: Some(30),
        });

        // Test update without disk write error
        if let Some(pos) = list.iter().position(|m| m.name == *orig_name) {
            list[pos] = updated;
        }
        assert!(list.iter().any(|m| m.name == "Renamed Macro"));
        assert!(!list.iter().any(|m| m.name == *orig_name));
    }
}
