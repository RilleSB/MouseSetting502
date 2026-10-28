use std::rc::Rc;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::thread;
use hidapi::HidApi;
use slint::{ModelRc, VecModel};

use crate::hidpp::HidppDevice;
use crate::hidpp::features::{
    get_device_name, get_current_dpi, set_dpi,
    set_led_effect, turn_off_all_leds, enable_host_mode,
    LedZone, LedEffect,
};
use crate::lghub::{
    LgHubSettings, LgMacro, LgMacroComponent, macro_engine,
    custom_macros::{
        load_custom_macros, update_custom_macro, delete_macro,
        format_component,
    },
    keybinds::{
        ButtonBinding, KeybindListener, load_bindings,
        save_bindings, assign_button_macro, clear_button_macro, rename_assigned_macro,
    },
};

const LOGITECH_VID: u16 = 0x046D;
const G502_PID: u16 = 0xC08B;
const G435_PID: u16 = 0x0ACB;

slint::include_modules!();

#[allow(clippy::enum_variant_names)]
enum DeviceCommand {
    SetDpi(u16),
    SetRgbStatic(u8, u8, u8),
    SetRgbCycle,
    SetRgbBreathe(u8, u8, u8),
    SetRgbOff,
}

fn update_ui_macro_list(
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

    // 2. G HUB imported macros
    if let Some(ref data) = **lghub_settings {
        for card in &data.cards {
            if let Some(m) = &card.macro_data
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

fn update_ui_keybind_list(
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

pub fn run() {
    let app = match MainWindow::new() {
        Ok(app) => app,
        Err(e) => {
            eprintln!("[!] Ошибка инициализации MainWindow: {e}");
            return;
        }
    };

    // Models created ONCE, mutated in-place (Item 11)
    let macro_model = Rc::new(VecModel::<MacroItem>::default());
    app.set_macro_items(ModelRc::from(macro_model.clone()));

    let keybind_model = Rc::new(VecModel::<KeybindItem>::default());
    app.set_keybind_items(ModelRc::from(keybind_model.clone()));

    let builder_steps_model = Rc::new(VecModel::<slint::SharedString>::default());
    app.set_builder_steps(ModelRc::from(builder_steps_model.clone()));

    // Load settings from G HUB SQLite db
    let lghub_settings = Arc::new(LgHubSettings::load_from_db().ok());

    // Load custom user macros from JSON (%APPDATA% / Item 6)
    let custom_macros = Arc::new(Mutex::new(load_custom_macros()));

    // Load keybindings from JSON (%APPDATA% / Item 6)
    let keybinds = Arc::new(Mutex::new(load_bindings()));

    // Populate initial models
    update_ui_macro_list(&macro_model, &lghub_settings, &custom_macros);
    update_ui_keybind_list(&keybind_model, &keybinds);

    // Active running macro state
    let active_macro: Arc<Mutex<Option<(String, macro_engine::ActiveMacro)>>> = Arc::new(Mutex::new(None));

    // Background Keybind Listener
    let initial_binds = if let Ok(guard) = keybinds.lock() {
        guard.clone()
    } else {
        Vec::new()
    };
    let listener = Arc::new(KeybindListener::new(initial_binds));

    // Wire background trigger callback to macro executor
    let act_slot = active_macro.clone();
    let custom_ref = custom_macros.clone();
    let lghub_ref = lghub_settings.clone();
    let app_weak = app.as_weak();

    listener.set_callback(move |macro_name: String| {
        let mut slot = act_slot.lock().unwrap_or_else(|e| e.into_inner());

        // If currently running this macro, stop it (toggle off)
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

        // Find macro in custom or lghub
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

    // Wire emergency panic stop on Esc (Item 12)
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

    // Explicit hook activation (Item 4: do NOT start automatically)
    app.set_listener_active(false);

    // HID Device worker channel
    let (cmd_tx, cmd_rx) = channel::<DeviceCommand>();

    // Initial Device Probing
    let api = HidApi::new().ok();
    let dev_info = api.as_ref().and_then(|a| {
        a.device_list()
            .find(|d| d.vendor_id() == LOGITECH_VID && d.product_id() == G502_PID && d.usage_page() == 0xFF00 && d.usage() == 0x0002)
    });

    let headset_found = api.as_ref().map(|a| {
        a.device_list().any(|d| d.vendor_id() == LOGITECH_VID && d.product_id() == G435_PID)
    }).unwrap_or(false);
    app.set_headset_connected(headset_found);

    if let (Some(api_ref), Some(info)) = (api.as_ref(), dev_info) {
        match api_ref.open_path(info.path()) {
            Ok(dev) => {
                let hidpp = Arc::new(HidppDevice::new(dev, 0xFF));
                let _ = enable_host_mode(&hidpp);

                if let Ok(name) = get_device_name(&hidpp) {
                    app.set_device_name(name.into());
                }
                app.set_device_connected(true);
                app.set_status_msg("Устройство подключено и готово к работе".into());

                if let Ok(dpi) = get_current_dpi(&hidpp, 0) {
                    app.set_current_dpi(dpi as i32);
                }

                let hid_worker = hidpp.clone();
                let app_weak = app.as_weak();
                thread::spawn(move || {
                    while let Ok(cmd) = cmd_rx.recv() {
                        match cmd {
                            DeviceCommand::SetDpi(dpi) => {
                                let res = set_dpi(&hid_worker, 0, dpi);
                                let weak = app_weak.clone();
                                let _ = slint::invoke_from_event_loop(move || {
                                    if let Some(app) = weak.upgrade() {
                                        match res {
                                            Ok(confirmed) => {
                                                app.set_current_dpi(confirmed as i32);
                                                app.set_status_msg(format!("Установлен DPI: {}", confirmed).into());
                                            }
                                            Err(e) => {
                                                app.set_status_msg(format!("Ошибка установки DPI: {}", e).into());
                                            }
                                        }
                                    }
                                });
                            }
                            DeviceCommand::SetRgbStatic(r, g, b) => {
                                let res = set_led_effect(&hid_worker, LedZone::Logo, LedEffect::Static { r, g, b }, false);
                                let weak = app_weak.clone();
                                let _ = slint::invoke_from_event_loop(move || {
                                    if let Some(app) = weak.upgrade() {
                                        match res {
                                            Ok(_) => app.set_status_msg(format!("Цвет установлен: RGB({}, {}, {})", r, g, b).into()),
                                            Err(e) => app.set_status_msg(format!("Ошибка RGB: {}", e).into()),
                                        }
                                    }
                                });
                            }
                            DeviceCommand::SetRgbCycle => {
                                let res = set_led_effect(&hid_worker, LedZone::Logo, LedEffect::ColorCycle { period_ms: 10000, brightness: 255 }, false);
                                let weak = app_weak.clone();
                                let _ = slint::invoke_from_event_loop(move || {
                                    if let Some(app) = weak.upgrade() {
                                        match res {
                                            Ok(_) => app.set_status_msg("Включен спектральный цикл".into()),
                                            Err(e) => app.set_status_msg(format!("Ошибка RGB: {}", e).into()),
                                        }
                                    }
                                });
                            }
                            DeviceCommand::SetRgbBreathe(r, g, b) => {
                                let res = set_led_effect(&hid_worker, LedZone::Logo, LedEffect::Breathing { r, g, b, period_ms: 3000 }, false);
                                let weak = app_weak.clone();
                                let _ = slint::invoke_from_event_loop(move || {
                                    if let Some(app) = weak.upgrade() {
                                        match res {
                                            Ok(_) => app.set_status_msg("Включен режим пульсации".into()),
                                            Err(e) => app.set_status_msg(format!("Ошибка RGB: {}", e).into()),
                                        }
                                    }
                                });
                            }
                            DeviceCommand::SetRgbOff => {
                                let res = turn_off_all_leds(&hid_worker, false);
                                let weak = app_weak.clone();
                                let _ = slint::invoke_from_event_loop(move || {
                                    if let Some(app) = weak.upgrade() {
                                        match res {
                                            Ok(_) => app.set_status_msg("Подсветка выключена".into()),
                                            Err(e) => app.set_status_msg(format!("Ошибка RGB: {}", e).into()),
                                        }
                                    }
                                });
                            }
                        }
                    }
                });
            }
            Err(e) => {
                app.set_device_connected(false);
                app.set_device_name("Ошибка доступа к мыши".into());
                app.set_status_msg(format!("Не удалось открыть устройство: {}", e).into());
            }
        }
    } else {
        app.set_device_connected(false);
        app.set_device_name("Устройство не найдено".into());
        app.set_status_msg("Мышь Logitech G502 HERO не обнаружена".into());
    }

    // Callbacks for DPI & RGB
    let tx = cmd_tx.clone();
    app.on_set_dpi(move |target| {
        let _ = tx.send(DeviceCommand::SetDpi(target as u16));
    });



    let tx = cmd_tx.clone();
    app.on_set_rgb_static(move |r, g, b| {
        let _ = tx.send(DeviceCommand::SetRgbStatic(r as u8, g as u8, b as u8));
    });

    let tx = cmd_tx.clone();
    app.on_set_rgb_cycle(move || {
        let _ = tx.send(DeviceCommand::SetRgbCycle);
    });

    let tx = cmd_tx.clone();
    app.on_set_rgb_breathe(move |r, g, b| {
        let _ = tx.send(DeviceCommand::SetRgbBreathe(r as u8, g as u8, b as u8));
    });

    let tx = cmd_tx.clone();
    app.on_set_rgb_off(move || {
        let _ = tx.send(DeviceCommand::SetRgbOff);
    });

    // Refresh devices callback
    let app_weak = app.as_weak();
    app.on_refresh_devices(move || {
        if let Some(app) = app_weak.upgrade() {
            let api = HidApi::new().ok();
            let found_g502 = api.as_ref().map(|a| {
                a.device_list().any(|d| d.vendor_id() == LOGITECH_VID && d.product_id() == G502_PID)
            }).unwrap_or(false);
            let found_g435 = api.as_ref().map(|a| {
                a.device_list().any(|d| d.vendor_id() == LOGITECH_VID && d.product_id() == G435_PID)
            }).unwrap_or(false);

            app.set_device_connected(found_g502);
            app.set_headset_connected(found_g435);
            app.set_status_msg("Устройства обновлены".into());
        }
    });

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

    // Keybinding Listener Toggle (Cleanly toggles background thread)
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
        if let Ok(mut binds) = keybinds_arc.lock() {
            assign_button_macro(&mut binds, btn_id.as_str(), macro_name.as_str());
            let _ = save_bindings(&binds);
            listener_ref.update_bindings(binds.clone());
        }
        update_ui_keybind_list(&keybind_model_ref, &keybinds_arc);
        if let Some(app) = app_weak.upgrade() {
            app.set_show_bind_picker(false);
            app.set_status_msg(format!("Кнопка {} привязана к макросу '{}'", btn_id, macro_name).into());
        }
    });

    // Clear Keybind
    let app_weak = app.as_weak();
    let keybinds_arc = keybinds.clone();
    let listener_ref = listener.clone();
    let keybind_model_ref = keybind_model.clone();
    app.on_clear_keybind(move |btn_id| {
        if let Ok(mut binds) = keybinds_arc.lock() {
            clear_button_macro(&mut binds, btn_id.as_str());
            let _ = save_bindings(&binds);
            listener_ref.update_bindings(binds.clone());
        }
        update_ui_keybind_list(&keybind_model_ref, &keybinds_arc);
        if let Some(app) = app_weak.upgrade() {
            app.set_show_bind_picker(false);
            app.set_status_msg(format!("Назначение кнопки {} сброшено на стандартное", btn_id).into());
        }
    });

    // Run Slint Application Loop (Item 10)
    if let Err(e) = app.run() {
        eprintln!("[!] Ошибка выполнения Slint: {e}");
    }

    // Clean up running macro and background listener on window close
    listener.stop();
    let mut slot = active_macro.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, active)) = slot.take() {
        active.stop();
    }
}
