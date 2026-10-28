use std::rc::Rc;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::thread;
use hidapi::HidApi;
use slint::{ComponentHandle, VecModel};

use super::bridge::MainWindow;
use crate::hidpp::HidppDevice;
use crate::hidpp::features::{
    get_device_name, get_current_dpi, set_dpi,
    set_led_effect, turn_off_all_leds,
    LedZone, LedEffect,
};
use crate::lghub::{add_dpi_preset, remove_dpi_preset, get_next_preset, get_prev_preset};

pub const LOGITECH_VID: u16 = 0x046D;
pub const G502_PID: u16 = 0xC08B;
pub const G435_PID: u16 = 0x0ACB;

#[allow(clippy::enum_variant_names)]
pub enum DeviceCommand {
    SetDpi(u16),
    SetRgbStatic(u8, u8, u8),
    SetRgbCycle,
    SetRgbBreathe(u8, u8, u8),
    SetRgbOff,
}

pub fn init_device_worker(
    app: &MainWindow,
    dpi_presets_model: &Rc<VecModel<i32>>,
    dpi_presets: &Arc<Mutex<Vec<u16>>>,
) {
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
                // Keep Onboard Mode (0x01) so G7/G8 physical DPI buttons operate in hardware
                if let Ok(feat_idx) = hidpp.get_feature_index(0x8100) {
                    let _ = hidpp.call_function(feat_idx, 0x01, &[0x01]);
                }

                if let Ok(name) = get_device_name(&hidpp) {
                    app.set_device_name(name.into());
                }
                app.set_device_connected(true);
                app.set_status_msg("Устройство подключено и готово к работе".into());

                let initial_dpi = get_current_dpi(&hidpp, 0).unwrap_or(800);
                app.set_current_dpi(initial_dpi as i32);

                // Background sync loop for hardware DPI changes (G7 / G8 physical clicks)
                let sync_worker = hidpp.clone();
                let sync_weak = app.as_weak();
                thread::spawn(move || {
                    let mut last_dpi = initial_dpi;
                    loop {
                        thread::sleep(std::time::Duration::from_millis(150));
                        if let Ok(current) = get_current_dpi(&sync_worker, 0)
                            && current != last_dpi {
                                last_dpi = current;
                                let weak = sync_weak.clone();
                                let _ = slint::invoke_from_event_loop(move || {
                                    if let Some(app) = weak.upgrade() {
                                        app.set_current_dpi(current as i32);
                                        app.set_status_msg(format!("Аппаратный DPI мыши: {}", current).into());
                                    }
                                });
                        }
                    }
                });

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
                spawn_offline_cmd_handler(cmd_rx, app.as_weak());
            }
        }
    } else {
        app.set_device_connected(false);
        app.set_device_name("Устройство не найдено".into());
        app.set_status_msg("Мышь Logitech G502 HERO не обнаружена (автономный режим)".into());
        spawn_offline_cmd_handler(cmd_rx, app.as_weak());
    }

    // Callbacks for DPI & RGB
    let tx = cmd_tx.clone();
    app.on_set_dpi(move |target| {
        let _ = tx.send(DeviceCommand::SetDpi(target as u16));
    });

    let app_weak = app.as_weak();
    let tx = cmd_tx.clone();
    let presets_arc = dpi_presets.clone();
    app.on_dpi_up(move || {
        if let Some(app) = app_weak.upgrade() {
            let curr = app.get_current_dpi() as u16;
            let next = {
                let p = presets_arc.lock().unwrap_or_else(|e| e.into_inner());
                get_next_preset(&p, curr)
            };
            let _ = tx.send(DeviceCommand::SetDpi(next));
        }
    });

    let app_weak = app.as_weak();
    let tx = cmd_tx.clone();
    let presets_arc = dpi_presets.clone();
    app.on_dpi_down(move || {
        if let Some(app) = app_weak.upgrade() {
            let curr = app.get_current_dpi() as u16;
            let prev = {
                let p = presets_arc.lock().unwrap_or_else(|e| e.into_inner());
                get_prev_preset(&p, curr)
            };
            let _ = tx.send(DeviceCommand::SetDpi(prev));
        }
    });

    let app_weak = app.as_weak();
    let tx = cmd_tx.clone();
    let presets_arc = dpi_presets.clone();
    let model = dpi_presets_model.clone();
    app.on_add_dpi_preset(move |target| {
        let dpi_val = ((target + 25) / 50 * 50).clamp(100, 25600) as u16;
        let mut p = presets_arc.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(()) = add_dpi_preset(&mut p, dpi_val) {
            let new_list: Vec<i32> = p.iter().map(|&x| x as i32).collect();
            model.set_vec(new_list);
            let _ = tx.send(DeviceCommand::SetDpi(dpi_val));
            if let Some(app) = app_weak.upgrade() {
                app.set_custom_dpi_input("".into());
                app.set_status_msg(format!("Добавлен пресет DPI: {}", dpi_val).into());
            }
        }
    });

    let app_weak = app.as_weak();
    let presets_arc = dpi_presets.clone();
    let model = dpi_presets_model.clone();
    app.on_remove_dpi_preset(move |target| {
        let mut p = presets_arc.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(()) = remove_dpi_preset(&mut p, target as u16) {
            let new_list: Vec<i32> = p.iter().map(|&x| x as i32).collect();
            model.set_vec(new_list);
            if let Some(app) = app_weak.upgrade() {
                app.set_status_msg(format!("Удален пресет DPI: {}", target).into());
            }
        }
    });

    let app_weak = app.as_weak();
    let tx = cmd_tx.clone();
    app.on_apply_custom_dpi(move |s| {
        let str_val = s.trim();
        if let Ok(parsed) = str_val.parse::<i32>() {
            let clamped = ((parsed + 25) / 50 * 50).clamp(100, 25600) as u16;
            let _ = tx.send(DeviceCommand::SetDpi(clamped));
            if let Some(app) = app_weak.upgrade() {
                app.set_custom_dpi_input("".into());
            }
        } else if let Some(app) = app_weak.upgrade() {
            app.set_status_msg("Введите корректное число DPI (100 - 25600)".into());
        }
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
}

fn spawn_offline_cmd_handler(cmd_rx: std::sync::mpsc::Receiver<DeviceCommand>, app_weak: slint::Weak<MainWindow>) {
    thread::spawn(move || {
        while let Ok(cmd) = cmd_rx.recv() {
            if let DeviceCommand::SetDpi(dpi) = cmd {
                let weak = app_weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        app.set_current_dpi(dpi as i32);
                        app.set_status_msg(format!("Установлен DPI: {} (автономный режим)", dpi).into());
                    }
                });
            }
        }
    });
}
