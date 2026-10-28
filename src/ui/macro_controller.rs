use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;
use slint::{ComponentHandle, VecModel};

use super::bridge::{MainWindow, MacroItem, KeybindItem};
use super::models::{update_ui_macro_list, update_ui_keybind_list};
use crate::lghub::{
    LgHubSettings, LgMacro, LgMacroComponent, macro_engine,
    custom_macros::{
        update_custom_macro, delete_macro, format_component,
    },
    keybinds::{
        ButtonBinding, KeybindListener, save_bindings, rename_assigned_macro,
    },
};

pub fn setup_macro_triggers(
    listener: &Arc<KeybindListener>,
    active_macro: &Arc<Mutex<Option<(String, macro_engine::ActiveMacro)>>>,
    custom_macros: &Arc<Mutex<Vec<LgMacro>>>,
    lghub_settings: &Arc<Option<LgHubSettings>>,
    app: &MainWindow,
) {
    let act_slot = active_macro.clone();
    let custom_ref = custom_macros.clone();
    let lghub_ref = lghub_settings.clone();
    let app_weak = app.as_weak();

    listener.set_callback(move |macro_name: String| {
        let mut slot = act_slot.lock().unwrap_or_else(|e| e.into_inner());

        if let Some((curr_name, active)) = slot.take() {
            active.stop();
            if curr_name == macro_name {
                let weak = app_weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        app.set_active_macro_name("".into());
                        app.set_status_msg(format!("Триггер: макрос '{}' остановлен", curr_name).into());
                    }
                });
                return;
            }
        }

        let target_macro = {
            let found = if let Ok(custom) = custom_ref.lock() {
                custom.iter().find(|m| m.name == macro_name).cloned()
            } else {
                None
            };
            found.or_else(|| {
                if let Some(ref data) = *lghub_ref {
                    data.cards.iter().find_map(|c| {
                        if c.name.as_deref() == Some(&macro_name)
                            || c.macro_data.as_ref().map(|m| m.name.as_str()) == Some(&macro_name)
                        {
                            c.macro_data.clone()
                        } else {
                            None
                        }
                    })
                } else {
                    None
                }
            })
        };

        if let Some(m) = target_macro {
            if m.is_toggle {
                let active = macro_engine::start_macro_loop(m);
                *slot = Some((macro_name.clone(), active));
                let weak = app_weak.clone();
                let m_name = macro_name.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        app.set_active_macro_name(m_name.clone().into());
                        app.set_status_msg(format!("Триггер: запущен автокликер '{}'", m_name).into());
                    }
                });
            } else {
                let weak = app_weak.clone();
                let m_name = macro_name.clone();
                thread::spawn(move || {
                    macro_engine::play_macro_once(&m);
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(app) = weak.upgrade() {
                            app.set_status_msg(format!("Триггер: макрос '{}' выполнен", m_name).into());
                        }
                    });
                });
            }
        }
    });

    // Emergency Panic Stop on Esc
    let panic_act_slot = active_macro.clone();
    let panic_app_weak = app.as_weak();
    listener.set_panic_callback(move || {
        let mut slot = panic_act_slot.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((curr_name, active)) = slot.take() {
            active.stop();
            let weak = panic_app_weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(app) = weak.upgrade() {
                    app.set_active_macro_name("".into());
                    app.set_status_msg(format!("⛔ Экстренная остановка (Esc): '{}' остановлен", curr_name).into());
                }
            });
        }
    });
}

pub struct MacroControllerContext<'a> {
    pub app: &'a MainWindow,
    pub macro_model: &'a Rc<VecModel<MacroItem>>,
    pub keybind_model: &'a Rc<VecModel<KeybindItem>>,
    pub builder_steps_model: &'a Rc<VecModel<slint::SharedString>>,
    pub custom_macros: &'a Arc<Mutex<Vec<LgMacro>>>,
    pub lghub_settings: &'a Arc<Option<LgHubSettings>>,
    pub keybinds: &'a Arc<Mutex<Vec<ButtonBinding>>>,
    pub active_macro: &'a Arc<Mutex<Option<(String, macro_engine::ActiveMacro)>>>,
    pub listener: &'a Arc<KeybindListener>,
}

pub fn setup_macro_controller(ctx: &MacroControllerContext) {
    let app = ctx.app;
    let macro_model = ctx.macro_model;
    let keybind_model = ctx.keybind_model;
    let builder_steps_model = ctx.builder_steps_model;
    let custom_macros = ctx.custom_macros;
    let lghub_settings = ctx.lghub_settings;
    let keybinds = ctx.keybinds;
    let active_macro = ctx.active_macro;
    let listener = ctx.listener;
    // Stop all macros callback
    let macro_slot = active_macro.clone();
    let app_weak = app.as_weak();
    app.on_stop_all_macros(move || {
        let mut slot = macro_slot.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, active)) = slot.take() {
            active.stop();
        }
        if let Some(app) = app_weak.upgrade() {
            app.set_active_macro_name("".into());
            app.set_status_msg("Все макросы остановлены".into());
        }
    });

    // Toggle macro execution callback
    let macro_slot = active_macro.clone();
    let settings_arc = lghub_settings.clone();
    let custom_arc = custom_macros.clone();
    let app_weak = app.as_weak();
    app.on_toggle_macro(move |name| {
        let mut slot = macro_slot.lock().unwrap_or_else(|e| e.into_inner());

        // Check if currently running the same macro -> stop it
        if let Some((active_name, active)) = slot.take() {
            active.stop();
            if active_name == name.as_str() {
                if let Some(app) = app_weak.upgrade() {
                    app.set_active_macro_name("".into());
                    app.set_status_msg(format!("Макрос '{}' остановлен", name).into());
                }
                return;
            }
        }

        // Search custom macros first
        let custom_target = if let Ok(custom) = custom_arc.lock() {
            custom.iter().find(|m| m.name == name.as_str()).cloned()
        } else {
            None
        };

        if let Some(m) = custom_target {
            if m.is_toggle {
                let active = macro_engine::start_macro_loop(m);
                *slot = Some((name.to_string(), active));
                if let Some(app) = app_weak.upgrade() {
                    app.set_active_macro_name(name.clone());
                    app.set_status_msg(format!("Запущен автокликер '{}'", name).into());
                }
            } else {
                let weak = app_weak.clone();
                let m_name = name.clone();
                thread::spawn(move || {
                    macro_engine::play_macro_once(&m);
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(app) = weak.upgrade() {
                            app.set_status_msg(format!("Макрос '{}' выполнен", m_name).into());
                        }
                    });
                });
            }
            return;
        }

        // Search G HUB imported macros
        if let Some(ref data) = *settings_arc {
            let target = data.cards.iter().find(|c| {
                name == c.name.as_deref().unwrap_or("")
                    || name == c.macro_data.as_ref().map(|m| m.name.as_str()).unwrap_or("")
            });

            if let Some(card) = target
                && let Some(m) = &card.macro_data {
                    if m.is_toggle {
                        let active = macro_engine::start_macro_loop(m.clone());
                        *slot = Some((name.to_string(), active));
                        if let Some(app) = app_weak.upgrade() {
                            app.set_active_macro_name(name.clone());
                            app.set_status_msg(format!("Запущен автокликер '{}'", name).into());
                        }
                    } else {
                        let m_clone = m.clone();
                        let weak = app_weak.clone();
                        let m_name = name.clone();
                        thread::spawn(move || {
                            macro_engine::play_macro_once(&m_clone);
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(app) = weak.upgrade() {
                                    app.set_status_msg(format!("Макрос '{}' выполнен", m_name).into());
                                }
                            });
                        });
                    }
                    return;
                }
        }

        if let Some(app) = app_weak.upgrade() {
            app.set_status_msg(format!("Макрос '{}' не найден", name).into());
        }
    });

    // Macro Builder in-memory sequence state
    let builder_steps: Arc<Mutex<Vec<LgMacroComponent>>> = Arc::new(Mutex::new(Vec::new()));

    // Open builder for new macro
    let app_weak = app.as_weak();
    let steps_ref = builder_steps.clone();
    let steps_model = builder_steps_model.clone();
    app.on_open_macro_builder(move || {
        if let Some(app) = app_weak.upgrade() {
            if let Ok(mut steps) = steps_ref.lock() {
                steps.clear();
            }
            steps_model.set_vec(Vec::new());
            app.set_builder_editing_original("".into());
            app.set_builder_name("Мой новый автокликер".into());
            app.set_builder_is_toggle(true);
            app.set_show_macro_builder(true);
        }
    });

    // Edit existing custom macro
    let app_weak = app.as_weak();
    let steps_ref = builder_steps.clone();
    let steps_model = builder_steps_model.clone();
    let custom_arc = custom_macros.clone();
    app.on_edit_custom_macro(move |name| {
        let target = if let Ok(custom) = custom_arc.lock() {
            custom.iter().find(|m| m.name == name.as_str()).cloned()
        } else {
            None
        };

        if let Some(m) = target
            && let Ok(mut steps) = steps_ref.lock() {
                *steps = m.sequence.clone();
                let slint_steps: Vec<slint::SharedString> = steps.iter().map(|s| format_component(s).into()).collect();
                steps_model.set_vec(slint_steps);
                if let Some(app) = app_weak.upgrade() {
                    app.set_builder_editing_original(m.name.clone().into());
                    app.set_builder_name(m.name.into());
                    app.set_builder_is_toggle(m.is_toggle);
                    app.set_show_macro_builder(true);
                }
            }
    });

    let app_weak = app.as_weak();
    app.on_close_macro_builder(move || {
        if let Some(app) = app_weak.upgrade() {
            app.set_show_macro_builder(false);
        }
    });

    // Builder Add Mouse
    let steps_ref = builder_steps.clone();
    let steps_model = builder_steps_model.clone();
    app.on_builder_add_mouse(move |button, down, delay| {
        let comp = LgMacroComponent {
            mouse_button: Some(button as u8),
            mouse_down: Some(down),
            vk_code: None,
            key_down: None,
            delay_ms: if delay > 0 { Some(delay as u64) } else { None },
        };
        if let Ok(mut steps) = steps_ref.lock() {
            steps_model.push(format_component(&comp).into());
            steps.push(comp);
        }
    });

    // Builder Add Key
    let steps_ref = builder_steps.clone();
    let steps_model = builder_steps_model.clone();
    app.on_builder_add_key(move |vk, down, delay| {
        let comp = LgMacroComponent {
            mouse_button: None,
            mouse_down: None,
            vk_code: Some(vk as u16),
            key_down: Some(down),
            delay_ms: if delay > 0 { Some(delay as u64) } else { None },
        };
        if let Ok(mut steps) = steps_ref.lock() {
            steps_model.push(format_component(&comp).into());
            steps.push(comp);
        }
    });

    // Builder Add Delay
    let steps_ref = builder_steps.clone();
    let steps_model = builder_steps_model.clone();
    app.on_builder_add_delay(move |delay| {
        let comp = LgMacroComponent {
            mouse_button: None,
            mouse_down: None,
            vk_code: None,
            key_down: None,
            delay_ms: Some(delay as u64),
        };
        if let Ok(mut steps) = steps_ref.lock() {
            steps_model.push(format_component(&comp).into());
            steps.push(comp);
        }
    });

    // Builder Clear
    let steps_ref = builder_steps.clone();
    let steps_model = builder_steps_model.clone();
    app.on_builder_clear_steps(move || {
        if let Ok(mut steps) = steps_ref.lock() {
            steps.clear();
        }
        steps_model.set_vec(Vec::new());
    });

    // Builder Remove Single Step
    let steps_ref = builder_steps.clone();
    let steps_model = builder_steps_model.clone();
    app.on_builder_remove_step(move |idx| {
        if let Ok(mut steps) = steps_ref.lock() {
            let i = idx as usize;
            if i < steps.len() {
                steps.remove(i);
                steps_model.remove(i);
            }
        }
    });

    // Builder Move Step Up
    let steps_ref = builder_steps.clone();
    let steps_model = builder_steps_model.clone();
    app.on_builder_move_step_up(move |idx| {
        let idx = idx as usize;
        if let Ok(mut steps) = steps_ref.lock()
            && idx > 0 && idx < steps.len() {
                steps.swap(idx - 1, idx);
                let slint_steps: Vec<slint::SharedString> = steps.iter().map(|s| format_component(s).into()).collect();
                steps_model.set_vec(slint_steps);
            }
    });

    // Builder Move Step Down
    let steps_ref = builder_steps.clone();
    let steps_model = builder_steps_model.clone();
    app.on_builder_move_step_down(move |idx| {
        let idx = idx as usize;
        if let Ok(mut steps) = steps_ref.lock()
            && idx + 1 < steps.len() {
                steps.swap(idx, idx + 1);
                let slint_steps: Vec<slint::SharedString> = steps.iter().map(|s| format_component(s).into()).collect();
                steps_model.set_vec(slint_steps);
            }
    });

    // Builder Save / Update Macro
    let app_weak = app.as_weak();
    let steps_ref = builder_steps.clone();
    let custom_arc = custom_macros.clone();
    let settings_arc = lghub_settings.clone();
    let keybinds_arc = keybinds.clone();
    let listener_ref = listener.clone();
    let macro_model_ref = macro_model.clone();
    let keybind_model_ref = keybind_model.clone();
    app.on_builder_save_macro(move |orig_name, name, is_toggle| {
        let name_str = name.trim().to_string();
        if name_str.is_empty() {
            if let Some(app) = app_weak.upgrade() {
                app.set_status_msg("Ошибка: укажите название макроса!".into());
            }
            return;
        }

        let mut steps = steps_ref.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if steps.is_empty() {
            steps.push(LgMacroComponent { mouse_button: Some(1), mouse_down: Some(true), vk_code: None, key_down: None, delay_ms: Some(20) });
            steps.push(LgMacroComponent { mouse_button: Some(1), mouse_down: Some(false), vk_code: None, key_down: None, delay_ms: Some(20) });
        }

        let new_macro = LgMacro {
            name: name_str.clone(),
            macro_type: if is_toggle { "toggle".into() } else { "sequence".into() },
            action_name: Some(name_str.clone()),
            is_toggle,
            sequence: steps,
        };

        if let Ok(mut custom) = custom_arc.lock() {
            let orig_str = orig_name.trim();
            let _ = update_custom_macro(&mut custom, orig_str, new_macro);

            // If macro was renamed, update any keybindings that referenced the old name
            if !orig_str.is_empty() && orig_str != name_str
                && let Ok(mut binds) = keybinds_arc.lock()
                    && rename_assigned_macro(&mut binds, orig_str, &name_str) {
                        let _ = save_bindings(&binds);
                        listener_ref.update_bindings(binds.clone());
                    }
        }

        update_ui_macro_list(&macro_model_ref, &settings_arc, &custom_arc);
        update_ui_keybind_list(&keybind_model_ref, &keybinds_arc);
        if let Some(app) = app_weak.upgrade() {
            app.set_show_macro_builder(false);
            app.set_status_msg(format!("Макрос '{}' успешно сохранен", name_str).into());
        }
    });

    // Delete Custom Macro
    let app_weak = app.as_weak();
    let custom_arc = custom_macros.clone();
    let settings_arc = lghub_settings.clone();
    let keybinds_arc = keybinds.clone();
    let listener_ref = listener.clone();
    let macro_model_ref = macro_model.clone();
    let keybind_model_ref = keybind_model.clone();
    app.on_delete_custom_macro(move |name| {
        if let Ok(mut custom) = custom_arc.lock() {
            let _ = delete_macro(&mut custom, name.as_str());
        }
        // Also clear any keybinds referencing this deleted macro
        if let Ok(mut binds) = keybinds_arc.lock() {
            let mut changed = false;
            for b in binds.iter_mut() {
                if b.assigned_macro == name.as_str() {
                    b.assigned_macro = "".into();
                    changed = true;
                }
            }
            if changed {
                let _ = save_bindings(&binds);
                listener_ref.update_bindings(binds.clone());
            }
        }
        update_ui_macro_list(&macro_model_ref, &settings_arc, &custom_arc);
        update_ui_keybind_list(&keybind_model_ref, &keybinds_arc);
        if let Some(app) = app_weak.upgrade() {
            app.set_status_msg(format!("Пользовательский макрос '{}' удален", name).into());
        }
    });
}
