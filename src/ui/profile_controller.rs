use std::rc::Rc;
use std::sync::{Arc, Mutex};
use slint::{ComponentHandle, VecModel};

use super::bridge::{MainWindow, ProfileItem, KeybindItem};
use super::models::{update_ui_profile_list, update_ui_keybind_list};
use crate::lghub::{
    AppProfile, save_profiles, set_active_profile_id, get_active_profile_id,
    delete_profile_by_id, ButtonBinding, KeybindListener, save_bindings,
    save_dpi_presets,
};

pub struct ProfileControllerContext<'a> {
    pub app: &'a MainWindow,
    pub profile_model: &'a Rc<VecModel<ProfileItem>>,
    pub dpi_presets_model: &'a Rc<VecModel<i32>>,
    pub keybind_model: &'a Rc<VecModel<KeybindItem>>,
    pub profiles: &'a Arc<Mutex<Vec<AppProfile>>>,
    pub dpi_presets: &'a Arc<Mutex<Vec<u16>>>,
    pub keybinds: &'a Arc<Mutex<Vec<ButtonBinding>>>,
    pub listener: &'a Arc<KeybindListener>,
}

pub fn setup_profile_controller(ctx: &ProfileControllerContext) {
    let app = ctx.app;
    let profile_model = ctx.profile_model;
    let dpi_presets_model = ctx.dpi_presets_model;
    let keybind_model = ctx.keybind_model;
    let profiles_arc = ctx.profiles;
    let dpi_presets_arc = ctx.dpi_presets;
    let keybinds_arc = ctx.keybinds;
    let listener = ctx.listener;

    let weak = app.as_weak();
    let profile_list = profiles_arc.clone();
    app.on_next_app_profile(move || {
        let active_id = get_active_profile_id();
        let next = profile_list.lock().ok().and_then(|profiles| {
            if profiles.is_empty() { return None; }
            let index = profiles.iter().position(|p| p.id == active_id).unwrap_or(0);
            Some(profiles[(index + 1) % profiles.len()].id.clone())
        });
        if let (Some(app), Some(id)) = (weak.upgrade(), next) { app.invoke_switch_profile(id.into()); }
    });

    // Switch Profile Callback
    let app_weak = app.as_weak();
    let prof_ref = profiles_arc.clone();
    let prof_model_ref = profile_model.clone();
    let dpi_model_ref = dpi_presets_model.clone();
    let keybind_model_ref = keybind_model.clone();
    let dpi_arc = dpi_presets_arc.clone();
    let keybinds_arc_clone = keybinds_arc.clone();
    let listener_ref = listener.clone();

    app.on_switch_profile(move |id| {
        let id_str = id.as_str().trim();
        let target_profile = {
            if let Ok(profs) = prof_ref.lock() {
                profs.iter().find(|p| p.id == id_str).cloned()
            } else {
                None
            }
        };

        if let Some(p) = target_profile {
            let _ = set_active_profile_id(&p.id);

            // 1. Update DPI presets
            if let Ok(mut presets) = dpi_arc.lock() {
                *presets = p.dpi_presets.clone();
                let _ = save_dpi_presets(&presets);
            }
            dpi_model_ref.set_vec(p.dpi_presets.iter().map(|&x| x as i32).collect::<Vec<_>>());

            // 2. Update Keybindings & Listener
            if let Ok(mut binds) = keybinds_arc_clone.lock() {
                *binds = p.keybinds.clone();
                let _ = save_bindings(&binds);
                listener_ref.update_bindings(binds.clone());
            }
            update_ui_keybind_list(&keybind_model_ref, &keybinds_arc_clone);

            // 3. Update Profiles list UI
            if let Ok(profs) = prof_ref.lock() {
                update_ui_profile_list(&prof_model_ref, &profs, &p.id);
            }

            // 4. Update Window UI & Device DPI
            if let Some(app) = app_weak.upgrade() {
                app.set_active_profile_name(p.name.clone().into());
                app.invoke_set_dpi(p.current_dpi as i32);
                app.set_status_msg(format!("Активирован профиль: '{}'", p.name).into());
                println!("[+] Profile activated: '{}' (ID: {})", p.name, p.id);
            }
        }
    });

    // Create Profile Callback
    let app_weak = app.as_weak();
    let prof_ref = profiles_arc.clone();
    let prof_model_ref = profile_model.clone();
    let dpi_arc = dpi_presets_arc.clone();
    let keybinds_arc_clone = keybinds_arc.clone();

    app.on_create_profile(move |name, desc| {
        let name_str = name.trim().to_string();
        if name_str.is_empty() {
            return;
        }
        let desc_str = desc.trim().to_string();
        let new_id = format!("prof_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0));

        let current_presets = dpi_arc.lock().map(|g| g.clone()).unwrap_or_else(|_| vec![400, 800, 1600]);
        let current_binds = keybinds_arc_clone.lock().map(|g| g.clone()).unwrap_or_default();
        let current_dpi = current_presets.first().copied().unwrap_or(800);

        let new_profile = AppProfile {
            id: new_id.clone(),
            name: name_str.clone(),
            description: if desc_str.is_empty() { "Пользовательский профиль".into() } else { desc_str },
            dpi_presets: current_presets,
            current_dpi,
            rgb_mode: "static".into(),
            rgb_color: (0, 229, 255),
            keybinds: current_binds,
        };

        if let Ok(mut profs) = prof_ref.lock() {
            profs.push(new_profile);
            let _ = save_profiles(&profs);
            let active_id = get_active_profile_id();
            update_ui_profile_list(&prof_model_ref, &profs, &active_id);
        }

        if let Some(app) = app_weak.upgrade() {
            app.set_status_msg(format!("Создан новый профиль '{}'", name_str).into());
            println!("[+] Created new profile '{}'", name_str);
        }
    });

    // Duplicate Profile Callback
    let app_weak = app.as_weak();
    let prof_ref = profiles_arc.clone();
    let prof_model_ref = profile_model.clone();

    app.on_duplicate_profile(move |id| {
        let id_str = id.as_str().trim();
        let duplicate = {
            if let Ok(profs) = prof_ref.lock() {
                profs.iter().find(|p| p.id == id_str).cloned().map(|mut p| {
                    p.id = format!("prof_copy_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0));
                    p.name = format!("{} (Копия)", p.name);
                    p
                })
            } else {
                None
            }
        };

        if let Some(p) = duplicate {
            let p_name = p.name.clone();
            if let Ok(mut profs) = prof_ref.lock() {
                profs.push(p);
                let _ = save_profiles(&profs);
                let active_id = get_active_profile_id();
                update_ui_profile_list(&prof_model_ref, &profs, &active_id);
            }
            if let Some(app) = app_weak.upgrade() {
                app.set_status_msg(format!("Профиль '{}' успешно продублирован", p_name).into());
            }
        }
    });

    // Delete Profile Callback
    let app_weak = app.as_weak();
    let prof_ref = profiles_arc.clone();
    let prof_model_ref = profile_model.clone();

    app.on_delete_profile(move |id| {
        let id_str = id.as_str().trim();
        let mut deleted_name = String::new();
        let res = if let Ok(mut profs) = prof_ref.lock() {
            if let Some(p) = profs.iter().find(|p| p.id == id_str) {
                deleted_name = p.name.clone();
            }
            delete_profile_by_id(&mut profs, id_str)
        } else {
            Ok(false)
        };

        match res {
            Ok(true) => {
                if let Ok(profs) = prof_ref.lock() {
                    let active_id = get_active_profile_id();
                    update_ui_profile_list(&prof_model_ref, &profs, &active_id);
                }
                if let Some(app) = app_weak.upgrade() {
                    app.set_status_msg(format!("Профиль '{}' удален", deleted_name).into());
                }
            }
            Ok(false) => {}
            Err(e) => {
                if let Some(app) = app_weak.upgrade() {
                    app.set_status_msg(e.into());
                }
            }
        }
    });
}
