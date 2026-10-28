use std::rc::Rc;
use std::sync::{Arc, Mutex};
use slint::VecModel;

use super::bridge::{MacroItem, KeybindItem, ProfileItem};
use crate::lghub::{LgHubSettings, LgMacro, ButtonBinding, AppProfile};

pub fn update_ui_macro_list(
    macro_model: &Rc<VecModel<MacroItem>>,
    lghub_settings: &Arc<Option<LgHubSettings>>,
    custom_macros: &Arc<Mutex<Vec<LgMacro>>>,
) {
    let mut items = Vec::new();

    // 1. Custom user-created macros first
    if let Ok(custom) = custom_macros.lock() {
        for m in custom.iter() {
            items.push(MacroItem {
                name: m.name.clone().into(),
                macro_type: m.macro_type.clone().into(),
                is_toggle: m.is_toggle,
                steps_count: m.sequence.len() as i32,
                is_custom: true,
            });
        }
    }

    // 2. G HUB imported macros (only valid macros with recorded steps)
    if let Some(ref data) = **lghub_settings {
        for card in &data.cards {
            if let Some(m) = &card.macro_data
                && !m.sequence.is_empty()
                && !m.name.trim().is_empty()
                && m.name != "Unnamed"
                && !m.name.starts_with("APPLET_")
                && !items.iter().any(|item| item.name.as_str() == m.name.as_str()) {
                    items.push(MacroItem {
                        name: m.name.clone().into(),
                        macro_type: m.macro_type.clone().into(),
                        is_toggle: m.is_toggle,
                        steps_count: m.sequence.len() as i32,
                        is_custom: false,
                    });
                }
        }
    }

    macro_model.set_vec(items);
}

pub fn update_ui_keybind_list(
    keybind_model: &Rc<VecModel<KeybindItem>>,
    keybinds: &Arc<Mutex<Vec<ButtonBinding>>>,
) {
    let mut items = Vec::new();
    if let Ok(binds) = keybinds.lock() {
        for b in binds.iter() {
            items.push(KeybindItem {
                button_id: b.button_id.clone().into(),
                button_label: b.button_label.clone().into(),
                default_action: b.default_action.clone().into(),
                assigned_macro: b.assigned_macro.clone().into(),
                is_mouse: b.is_mouse_button,
            });
        }
    }
    keybind_model.set_vec(items);
}

pub fn update_ui_profile_list(
    profile_model: &Rc<VecModel<ProfileItem>>,
    profiles: &[AppProfile],
    active_id: &str,
) {
    let mut items = Vec::new();
    for p in profiles {
        let presets_str = p.dpi_presets.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(" / ");
        let active_keybinds = p.keybinds.iter().filter(|b| !b.assigned_macro.trim().is_empty()).count();
        items.push(ProfileItem {
            id: p.id.clone().into(),
            name: p.name.clone().into(),
            description: p.description.clone().into(),
            dpi_info: format!("{} DPI (Пресеты: {})", p.current_dpi, presets_str).into(),
            keybinds_count: active_keybinds as i32,
            is_active: p.id == active_id,
        });
    }
    profile_model.set_vec(items);
}
