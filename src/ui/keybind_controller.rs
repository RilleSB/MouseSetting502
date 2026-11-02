use super::bridge::{KeybindItem, MainWindow, ProfileItem};
use super::models::{update_ui_keybind_list, update_ui_profile_list};
use crate::lghub::button_actions::{action_label, encode_chord};
use crate::lghub::keybinds::{ButtonBinding, KeybindListener};
use crate::lghub::{AppProfile, get_active_profile_id};
use slint::{ComponentHandle, VecModel};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

pub fn setup_keybind_controller(
    app: &MainWindow,
    keybind_model: &Rc<VecModel<KeybindItem>>,
    keybinds: &Arc<Mutex<Vec<ButtonBinding>>>,
    listener: &Arc<KeybindListener>,
    profiles: &Arc<Mutex<Vec<AppProfile>>>,
    profile_model: &Rc<VecModel<ProfileItem>>,
) {
    let weak = app.as_weak();
    let listener_ref = listener.clone();
    let binds = keybinds.clone();
    app.on_toggle_listener(move || {
        if !listener_ref.is_running()
            && let Ok(binds) = binds.lock()
        {
            listener_ref.update_bindings(binds.clone());
        }
        let running = listener_ref.toggle();
        if let Some(app) = weak.upgrade() {
            if !running {
                app.invoke_stop_all_macros();
            }
            app.set_listener_active(running);
            app.set_status_msg(
                if running {
                    "Программные назначения включены"
                } else {
                    "Программные назначения выключены"
                }
                .into(),
            );
        }
    });

    let weak = app.as_weak();
    app.on_open_keybind_picker(move |id, label| {
        if let Some(app) = weak.upgrade() {
            app.set_picker_button_id(id);
            app.set_picker_button_name(label);
            app.set_picker_error("".into());
            app.set_show_bind_picker(true);
        }
    });
    let weak = app.as_weak();
    app.on_close_keybind_picker(move || {
        if let Some(app) = weak.upgrade() {
            app.set_show_bind_picker(false);
        }
    });

    let weak = app.as_weak();
    let binds = keybinds.clone();
    let listener = listener.clone();
    let model = keybind_model.clone();
    let profiles = profiles.clone();
    let profile_model = profile_model.clone();
    app.on_assign_keybind(move |id, action| {
        let Some(app) = weak.upgrade() else {
            return;
        };
        let id = id.trim();
        if !action.is_empty()
            && matches!(id, "G6" | "G7" | "G8" | "G9")
            && (!app.get_onboard_known()
                || app.get_onboard_enabled()
                || !app.get_hardware_buttons_ready())
        {
            app.set_picker_error(
                "Для G6–G9 подключите мышь и выключите внутреннюю память во вкладке профилей."
                    .into(),
            );
            return;
        }
        match super::binding_persistence::store_assignment(
            id,
            action.as_str(),
            &binds,
            &profiles,
            &listener,
        ) {
            Ok(()) => {
                update_ui_keybind_list(&model, &binds);
                if let Ok(profiles) = profiles.lock() {
                    update_ui_profile_list(&profile_model, &profiles, &get_active_profile_id());
                }
                app.set_show_bind_picker(false);
                let label = if action.is_empty() {
                    "Стандартное действие".into()
                } else {
                    action_label(action.as_str())
                };
                let suffix = if listener.is_running() {
                    ""
                } else {
                    " (перехватчик выключен)"
                };
                app.set_status_msg(format!("{id}: {label}{suffix}").into());
            }
            Err(error) => {
                app.set_picker_error(error.clone().into());
                app.set_status_msg(format!("Назначение не сохранено: {error}").into());
            }
        }
    });

    let weak = app.as_weak();
    app.on_assign_keyboard(move |id, text| {
        if let Some(app) = weak.upgrade() {
            match encode_chord(text.as_str()) {
                Ok(action) => app.invoke_assign_keybind(id, action.into()),
                Err(error) => app.set_picker_error(error.into()),
            }
        }
    });
    let weak = app.as_weak();
    app.on_clear_keybind(move |id| {
        if let Some(app) = weak.upgrade() {
            app.invoke_assign_keybind(id, "".into());
        }
    });
}
