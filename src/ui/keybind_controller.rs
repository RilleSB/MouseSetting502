use std::rc::Rc;
use std::sync::{Arc, Mutex};
use slint::{ComponentHandle, VecModel};

use super::bridge::{MainWindow, KeybindItem};
use super::models::update_ui_keybind_list;
use crate::lghub::keybinds::{
    ButtonBinding, KeybindListener, assign_button_macro,
    clear_button_macro, save_bindings,
};

pub fn setup_keybind_controller(
    app: &MainWindow,
    keybind_model: &Rc<VecModel<KeybindItem>>,
    keybinds: &Arc<Mutex<Vec<ButtonBinding>>>,
    listener: &Arc<KeybindListener>,
) {
    // Keybinding Listener Toggle (Explicit trigger on button click)
    let app_weak = app.as_weak();
    let listener_ref = listener.clone();
    let keybinds_arc = keybinds.clone();
    app.on_toggle_listener(move || {
        // If enabling, ensure bindings are up to date
        if !listener_ref.is_running()
            && let Ok(binds) = keybinds_arc.lock() {
                listener_ref.update_bindings(binds.clone());
            }
        let now_running = listener_ref.toggle();
        if let Some(app) = app_weak.upgrade() {
            app.set_listener_active(now_running);
            if now_running {
                app.set_status_msg("Фоновый перехватчик клавиш (Windows Hook) включен".into());
            } else {
                app.set_status_msg("Фоновый перехватчик клавиш отключен".into());
            }
        }
    });

    // Open/Close Keybind Picker
    let app_weak = app.as_weak();
    app.on_open_keybind_picker(move |btn_id, btn_label| {
        if let Some(app) = app_weak.upgrade() {
            app.set_picker_button_id(btn_id);
            app.set_picker_button_name(btn_label);
            app.set_show_bind_picker(true);
        }
    });

    let app_weak = app.as_weak();
    app.on_close_keybind_picker(move || {
        if let Some(app) = app_weak.upgrade() {
            app.set_show_bind_picker(false);
        }
    });

    // Assign Keybind
    let app_weak = app.as_weak();
    let keybinds_arc = keybinds.clone();
    let listener_ref = listener.clone();
    let keybind_model_ref = keybind_model.clone();
    app.on_assign_keybind(move |btn_id, macro_name| {
        let btn_str = btn_id.as_str().trim();
        let macro_str = macro_name.as_str().trim();
        if btn_str.is_empty() {
            eprintln!("[-] Warning: assign_keybind called with empty button_id");
            return;
        }

        let assigned = if let Ok(mut binds) = keybinds_arc.lock() {
            let res = assign_button_macro(&mut binds, btn_str, macro_str);
            let _ = save_bindings(&binds);
            listener_ref.update_bindings(binds.clone());
            res
        } else {
            false
        };

        update_ui_keybind_list(&keybind_model_ref, &keybinds_arc);

        if let Some(app) = app_weak.upgrade() {
            app.set_show_bind_picker(false);
            if assigned {
                println!("[+] Keybind assigned: button '{}' -> macro '{}'", btn_str, macro_str);
                app.set_status_msg(format!("Кнопка {} привязана к макросу '{}'", btn_str, macro_str).into());
            } else {
                eprintln!("[-] Warning: button '{}' not found in keybindings list", btn_str);
                app.set_status_msg(format!("Ошибка: кнопка {} не найдена", btn_str).into());
            }
        }
    });

    // Clear Keybind
    let app_weak = app.as_weak();
    let keybinds_arc = keybinds.clone();
    let listener_ref = listener.clone();
    let keybind_model_ref = keybind_model.clone();
    app.on_clear_keybind(move |btn_id| {
        let btn_str = btn_id.as_str().trim();
        if let Ok(mut binds) = keybinds_arc.lock() {
            clear_button_macro(&mut binds, btn_str);
            let _ = save_bindings(&binds);
            listener_ref.update_bindings(binds.clone());
        }
        update_ui_keybind_list(&keybind_model_ref, &keybinds_arc);
        if let Some(app) = app_weak.upgrade() {
            app.set_show_bind_picker(false);
            println!("[*] Keybind cleared for button '{}'", btn_str);
            app.set_status_msg(format!("Назначение кнопки {} сброшено на стандартное", btn_str).into());
        }
    });
}
