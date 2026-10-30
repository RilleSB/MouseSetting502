use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use slint::{ComponentHandle, VecModel};

use super::bridge::MainWindow;
use crate::lghub::{add_dpi_preset, remove_dpi_preset, get_next_preset, get_prev_preset, AppProfile, KeybindListener, set_g9_lock};

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
    SetMemoryMode(bool),
    Refresh,
    SyncAllOnboardSlots(u16),
}

pub fn init_device_worker(
    app: &MainWindow,
    dpi_presets_model: &Rc<VecModel<i32>>,
    dpi_presets: &Arc<Mutex<Vec<u16>>>,
    profiles: &Arc<Mutex<Vec<AppProfile>>>,
    g9_locked: &Arc<AtomicBool>,
    listener: &Arc<KeybindListener>,
) -> super::device_runtime::DeviceWorkerGuard {
    let (cmd_tx, cmd_rx) = channel::<DeviceCommand>();

    app.set_g9_locked(g9_locked.load(Ordering::SeqCst));
    let worker = super::device_runtime::start(app.as_weak(), cmd_rx, profiles.clone(), g9_locked.clone(), listener.clone());
    let tx = cmd_tx.clone();
    let weak = app.as_weak();
    app.on_set_memory_mode(move |onboard| {
        if let Some(app) = weak.upgrade() {
            if !app.get_device_connected() || !app.get_onboard_known() || app.get_onboard_busy() { return; }
            app.set_onboard_busy(true);
            if tx.send(DeviceCommand::SetMemoryMode(onboard)).is_err() {
                app.set_onboard_busy(false);
                app.set_status_msg("Обработчик устройства остановлен".into());
            }
        }
    });

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

    // Callbacks for G9 & Onboard Memory
    let g9_lock_clone = g9_locked.clone();
    let app_weak = app.as_weak();
    app.on_toggle_g9_lock(move || {
        let current = g9_lock_clone.load(Ordering::SeqCst);
        let new_val = !current;
        g9_lock_clone.store(new_val, Ordering::SeqCst);
        let _ = set_g9_lock(new_val);
        if let Some(app) = app_weak.upgrade() {
            app.set_g9_locked(new_val);
            if new_val {
                app.set_status_msg("🔒 Кнопка G9 заблокирована: переключение профилей отключено".into());
            } else {
                app.set_status_msg("🔄 Кнопка G9 синхронизирована с профилями приложения".into());
            }
        }
    });

    let tx = cmd_tx.clone();
    let app_weak = app.as_weak();
    app.on_sync_all_onboard_memory(move || {
        if let Some(app) = app_weak.upgrade() {
            let dpi = app.get_current_dpi() as u16;
            let _ = tx.send(DeviceCommand::SyncAllOnboardSlots(dpi));
        }
    });

    let tx = cmd_tx;
    app.on_refresh_devices(move || { let _ = tx.send(DeviceCommand::Refresh); });
    worker
}
