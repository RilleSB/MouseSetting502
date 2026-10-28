use std::env;
use std::path::PathBuf;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LgCard {
    pub id: String,
    pub name: Option<String>,
    pub attribute: Option<String>,
    pub macro_data: Option<LgMacro>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LgMacroType {
    Keystroke,
    Sequence,
    Text,
    Action,
    Unknown(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LgKeystroke {
    pub code: Option<u32>,
    pub modifiers: Option<Vec<u32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LgMacroComponent {
    pub mouse_button: Option<u8>,
    pub mouse_down: Option<bool>,
    pub key_code: Option<u32>,
    pub key_down: Option<bool>,
    pub delay_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LgMacro {
    pub name: String,
    pub macro_type: String,
    pub action_name: Option<String>,
    pub is_toggle: bool,
    pub sequence: Vec<LgMacroComponent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LgAssignment {
    pub slot_id: String,
    pub card_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LgProfile {
    pub name: String,
    pub app_id: String,
    pub active_for_application: bool,
    pub assignments: Vec<LgAssignment>,
}

pub struct LgHubSettings {
    pub profiles: Vec<LgProfile>,
    pub cards: Vec<LgCard>,
}

impl LgHubSettings {
    pub fn default_db_path() -> Result<PathBuf, String> {
        let local_app_data = env::var("LOCALAPPDATA").map_err(|_| "LOCALAPPDATA env var not found")?;
        let p = PathBuf::from(local_app_data).join("LGHUB").join("settings.db");
        if p.exists() {
            Ok(p)
        } else {
            Err(format!("G HUB settings.db not found at {:?}", p))
        }
    }

    pub fn load_from_db() -> Result<Self, String> {
        let path = Self::default_db_path()?;
        let conn = Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| format!("Failed to open {}: {e}", path.display()))?;

        let mut stmt = conn
            .prepare("SELECT file FROM data WHERE _id=1")
            .map_err(|e| format!("Query prepare failed: {e}"))?;

        let blob: Vec<u8> = stmt
            .query_row([], |row| row.get(0))
            .map_err(|e| format!("Failed to read settings blob: {e}"))?;

        let json_str = String::from_utf8(blob).map_err(|e| format!("Blob is not valid UTF-8: {e}"))?;
        let root: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| format!("JSON parse error: {e}"))?;

        let mut cards = Vec::new();
        if let Some(cards_arr) = root.get("cards").and_then(|c| c.get("cards")).and_then(|v| v.as_array()) {
            for c in cards_arr {
                let id = c.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                let name = c.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
                let attribute = c.get("attribute").and_then(|v| v.as_str()).map(|s| s.to_string());

                let mut macro_data = None;
                if let Some(m) = c.get("macro") {
                    let m_name = name.clone().unwrap_or_else(|| "Unnamed".into());
                    let m_type = m.get("type").and_then(|v| v.as_str()).unwrap_or("UNKNOWN").to_string();
                    let action_name = m.get("actionName").and_then(|v| v.as_str()).map(|s| s.to_string());

                    let mut components = Vec::new();
                    let mut is_toggle = false;

                    // Sequence parsing (toggleSequence or simpleSequence or components)
                    if let Some(seq) = m.get("sequence") {
                        if let Some(toggle_seq) = seq.get("toggleSequence").and_then(|ts| ts.get("components")).and_then(|v| v.as_array()) {
                            is_toggle = true;
                            for comp in toggle_seq {
                                parse_component(comp, &mut components);
                            }
                        } else if let Some(simple) = seq.get("simpleSequence").and_then(|ss| ss.get("components")).and_then(|v| v.as_array()) {
                            for comp in simple {
                                parse_component(comp, &mut components);
                            }
                        }
                    } else if let Some(keystroke) = m.get("keystroke") {
                        if let Some(code) = keystroke.get("code").and_then(|v| v.as_u64()) {
                            components.push(LgMacroComponent {
                                mouse_button: None,
                                mouse_down: None,
                                key_code: Some(code as u32),
                                key_down: Some(true),
                                delay_ms: None,
                            });
                            components.push(LgMacroComponent {
                                mouse_button: None,
                                mouse_down: None,
                                key_code: Some(code as u32),
                                key_down: Some(false),
                                delay_ms: None,
                            });
                        }
                    }

                    macro_data = Some(LgMacro {
                        name: m_name,
                        macro_type: m_type,
                        action_name,
                        is_toggle,
                        sequence: components,
                    });
                }

                cards.push(LgCard {
                    id,
                    name,
                    attribute,
                    macro_data,
                });
            }
        }

        let mut profiles = Vec::new();
        if let Some(profs_arr) = root.get("profiles").and_then(|p| p.get("profiles")).and_then(|v| v.as_array()) {
            for p in profs_arr {
                let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("Default").to_string();
                let app_id = p.get("applicationId").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let active = p.get("activeForApplication").and_then(|v| v.as_bool()).unwrap_or(false);

                let mut assignments = Vec::new();
                if let Some(assign_arr) = p.get("assignments").and_then(|v| v.as_array()) {
                    for a in assign_arr {
                        let slot_id = a.get("slotId").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                        let card_id = a.get("cardId").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                        assignments.push(LgAssignment { slot_id, card_id });
                    }
                }

                profiles.push(LgProfile {
                    name,
                    app_id,
                    active_for_application: active,
                    assignments,
                });
            }
        }

        Ok(Self { profiles, cards })
    }

    pub fn find_card(&self, card_id: &str) -> Option<&LgCard> {
        self.cards.iter().find(|c| c.id == card_id)
    }
}

fn parse_component(comp: &serde_json::Value, out: &mut Vec<LgMacroComponent>) {
    if let Some(mouse) = comp.get("mouse") {
        if let Some(btn) = mouse.get("button") {
            let hid_usage = btn.get("hidUsage").and_then(|v| v.as_str()).and_then(|s| s.parse::<u8>().ok()).unwrap_or(1);
            let is_down = btn.get("isDown").and_then(|v| v.as_bool()).unwrap_or(false);
            out.push(LgMacroComponent {
                mouse_button: Some(hid_usage),
                mouse_down: Some(is_down),
                key_code: None,
                key_down: None,
                delay_ms: None,
            });
        }
    } else if let Some(delay) = comp.get("delay") {
        let ms = delay.get("durationMs").and_then(|v| v.as_u64()).unwrap_or(1);
        out.push(LgMacroComponent {
            mouse_button: None,
            mouse_down: None,
            key_code: None,
            key_down: None,
            delay_ms: Some(ms),
        });
    }
}
