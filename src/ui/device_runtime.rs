use std::sync::{Arc, Mutex, mpsc::{Receiver, RecvTimeoutError}};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use crate::hidpp::features::{set_dpi, set_led_effect, turn_off_all_leds, LedEffect, LedZone};
use crate::lghub::{AppProfile, KeybindListener};
use super::{bridge::MainWindow, device_connection::{DeviceConnection, update_ui}, device_worker::DeviceCommand};

pub struct DeviceWorkerGuard(pub Arc<AtomicBool>);
impl Drop for DeviceWorkerGuard {
    fn drop(&mut self) { self.0.store(false, Ordering::SeqCst); }
}

pub fn start(weak: slint::Weak<MainWindow>, rx: Receiver<DeviceCommand>, profiles: Arc<Mutex<Vec<AppProfile>>>,
    locked: Arc<AtomicBool>, listener: Arc<KeybindListener>) -> DeviceWorkerGuard {
    let running = Arc::new(AtomicBool::new(true));
    let active = running.clone();
    std::thread::spawn(move || {
        let mut connection = connect(&weak);
        let mut last_poll = Instant::now();
        while active.load(Ordering::SeqCst) {
            match rx.recv_timeout(Duration::from_millis(8)) {
                Ok(DeviceCommand::Refresh) => {
                    listener.release_buttons();
                    connection = connect(&weak);
                }
                Ok(command) => {
                    if let Some(device) = connection.as_mut() {
                        let result = execute(device, command, &listener, &weak);
                        device.publish(&weak);
                        let status = result.unwrap_or_else(|e| format!("Ошибка: {e}"));
                        update_ui(&weak, move |app| app.set_status_msg(status.into()));
                    } else {
                        update_ui(&weak, |app| {
                            app.set_onboard_busy(false);
                            app.set_status_msg("Мышь недоступна. Подключите её и нажмите обновление устройств.".into());
                        });
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {}
            }
            if let Some(device) = connection.as_mut() {
                let result = device.poll_buttons(&listener, locked.load(Ordering::SeqCst)).and_then(|()| {
                    if last_poll.elapsed() >= Duration::from_millis(500) {
                        last_poll = Instant::now();
                        device.poll_state(&weak, &listener, &profiles, &locked)
                    } else { Ok(()) }
                });
                if let Err(error) = result {
                    listener.release_buttons();
                    connection = None;
                    disconnected(&weak, error);
                }
            }
        }
        listener.release_buttons();
    });
    DeviceWorkerGuard(running)
}

fn connect(weak: &slint::Weak<MainWindow>) -> Option<DeviceConnection> {
    match DeviceConnection::open(weak) {
        Ok(connection) => Some(connection),
        Err(error) => { disconnected(weak, error); None }
    }
}

fn disconnected(weak: &slint::Weak<MainWindow>, error: String) {
    update_ui(weak, move |app| {
        app.set_device_connected(false);
        app.set_onboard_known(false);
        app.set_onboard_busy(false);
        app.set_hardware_buttons_ready(false);
        app.set_device_name("G502 HERO недоступна".into());
        app.set_status_msg(error.into());
    });
}

fn execute(device: &mut DeviceConnection, command: DeviceCommand, listener: &KeybindListener,
    weak: &slint::Weak<MainWindow>) -> Result<String, String> {
    match command {
        DeviceCommand::SetDpi(dpi) => {
            let confirmed = set_dpi(&device.dev, 0, dpi)?;
            device.dpi = confirmed;
            update_ui(weak, move |app| app.set_current_dpi(confirmed as i32));
            Ok(format!("Установлен DPI: {confirmed}"))
        }
        DeviceCommand::SetMemoryMode(onboard) => device.set_memory(onboard, listener),
        DeviceCommand::SetRgbStatic(r, g, b) => {
            set_led_effect(&device.dev, LedZone::Logo, LedEffect::Static { r, g, b }, false)?;
            Ok(format!("Цвет установлен: RGB({r}, {g}, {b})"))
        }
        DeviceCommand::SetRgbCycle => {
            set_led_effect(&device.dev, LedZone::Logo, LedEffect::ColorCycle { period_ms: 10000, brightness: 255 }, false)?;
            Ok("Включён спектральный цикл".into())
        }
        DeviceCommand::SetRgbBreathe(r, g, b) => {
            set_led_effect(&device.dev, LedZone::Logo, LedEffect::Breathing { r, g, b, period_ms: 3000 }, false)?;
            Ok("Включён эффект дыхания".into())
        }
        DeviceCommand::SetRgbOff => {
            turn_off_all_leds(&device.dev, false)?;
            Ok("Подсветка выключена".into())
        }
        DeviceCommand::SyncAllOnboardSlots(dpi) => {
            crate::hidpp::features::sync_all_onboard_profiles_dpi(&device.dev, dpi)?;
            Ok("Профили синхронизированы".into())
        }
        DeviceCommand::Refresh => unreachable!(),
    }
}
