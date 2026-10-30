use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use slint::{ComponentHandle, VecModel};

use super::bridge::MainWindow;
use crate::lghub::{get_next_preset, get_prev_preset, AppProfile, KeybindListener, set_g9_lock, get_active_profile_id, save_profiles};

pub const LOGITECH_VID: u16 = 0x046D;
pub const G502_PID: u16 = 0xC08B;
pub const G435_PID: u16 = 0x0ACB;

#[allow(clippy::enum_variant_names)]
pub enum DeviceCommand {
    SetDpi(u16),
    SetReportRate(u8),
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

    let tx = cmd_tx.clone();
    let weak = app.as_weak();
    app.on_set_report_rate(move |interval| {
        if let Some(app) = weak.upgrade() {
            if !app.get_device_connected() || !app.get_onboard_known() || app.get_onboard_enabled()
                || app.get_report_rate_busy() || !(1..=8).contains(&interval) { return; }
            app.set_report_rate_busy(true);
            if tx.send(DeviceCommand::SetReportRate(interval as u8)).is_err() {
                app.set_report_rate_busy(false);
                app.set_status_msg("Не удалось связаться с мышью".into());
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

    for add in [true, false] {
        let app_weak = app.as_weak();
        let presets_arc = dpi_presets.clone();
        let profiles_arc = profiles.clone();
        let model = dpi_presets_model.clone();
        let update = move |target: i32| {
            let Some(app) = app_weak.upgrade() else { return; };
            let dpi = ((target.clamp(100, 25600) + 25) / 50 * 50) as u16;
            let mut presets = presets_arc.lock().unwrap_or_else(|e| e.into_inner());
            let mut next = presets.clone();
            if add { if !next.contains(&dpi) { next.push(dpi); next.sort_unstable(); } }
            else if next.len() > 1 { next.retain(|value| *value != dpi); }
            if next.is_empty() { app.set_status_msg("Добавьте хотя бы один уровень DPI".into()); return; }
            let mut profiles = profiles_arc.lock().unwrap_or_else(|e| e.into_inner());
            let mut updated = profiles.clone();
            let active_id = get_active_profile_id();
            let Some(profile) = updated.iter_mut().find(|p| p.id == active_id) else {
                app.set_status_msg("Активный профиль не найден".into()); return;
            };
            profile.dpi_presets = next.clone();
            if !next.contains(&profile.current_dpi) { profile.current_dpi = next[0]; }
            if let Err(error) = save_profiles(&updated) {
                app.set_status_msg(format!("Не удалось сохранить DPI профиля: {error}").into()); return;
            }
            *profiles = updated;
            *presets = next.clone();
            model.set_vec(next.iter().map(|&value| value as i32).collect::<Vec<_>>());
            if add { app.set_custom_dpi_input("".into()); }
            app.set_status_msg("Список DPI активного профиля сохранён".into());
        };
        if add { app.on_add_dpi_preset(update); } else { app.on_remove_dpi_preset(update); }
    }

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
