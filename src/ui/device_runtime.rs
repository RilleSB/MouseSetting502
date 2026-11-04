use super::dpi_controller::DpiRequests;
use super::{
    bridge::MainWindow,
    device_connection::{DeviceConnection, refresh_report_rate, update_ui},
    device_worker::DeviceCommand,
};
use crate::hidpp::features::{LedEffect, LedZone, set_dpi, set_led_effect};
use crate::lghub::{AppProfile, KeybindListener};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{
    Arc, Mutex,
    mpsc::{Receiver, RecvTimeoutError},
};
use std::time::{Duration, Instant};

pub struct DeviceWorkerGuard {
    running: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl DeviceWorkerGuard {
    fn spawn(run: impl FnOnce(Arc<AtomicBool>) + Send + 'static) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let active = running.clone();
        Self {
            running,
            worker: Some(std::thread::spawn(move || run(active))),
        }
    }
}
impl Drop for DeviceWorkerGuard {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub fn start(
    weak: slint::Weak<MainWindow>,
    rx: Receiver<DeviceCommand>,
    profiles: Arc<Mutex<Vec<AppProfile>>>,
    locked: Arc<AtomicBool>,
    listener: Arc<KeybindListener>,
    requests: Arc<DpiRequests>,
) -> DeviceWorkerGuard {
    DeviceWorkerGuard::spawn(move |active| {
        if !active.load(Ordering::SeqCst) {
            listener.release_buttons();
            return;
        }
        let mut connection = connect(&weak, &requests);
        let store = crate::lghub::profile_store::ProfileStore::user();
        let mut last_poll = Instant::now();
        while active.load(Ordering::SeqCst) {
            let received = rx.recv_timeout(Duration::from_millis(8));
            if !active.load(Ordering::SeqCst) {
                break;
            }
            match received {
                Ok(DeviceCommand::Refresh) => {
                    listener.release_buttons();
                    connection = connect(&weak, &requests);
                }
                Ok(DeviceCommand::SetDpi(request)) => {
                    let completion = requests.execute(request, &profiles, &store, |dpi| {
                        let device = connection.as_mut().ok_or("Мышь недоступна")?;
                        set_dpi(&device.dev, 0, dpi)
                    });
                    if let Some(completion) = completion {
                        if let Some(confirmed) = completion.confirmed()
                            && let Some(device) = connection.as_mut()
                        {
                            device.dpi = confirmed;
                        }
                        let requests = requests.clone();
                        update_ui(&weak, move |app| {
                            app.invoke_refresh_profile_models();
                            if !requests.can_publish(
                                &completion.request,
                                app.get_active_profile_id().as_str(),
                            ) {
                                return;
                            }
                            if let Some(confirmed) = completion.confirmed() {
                                app.set_current_dpi(i32::from(confirmed));
                            }
                            app.set_status_msg(completion.status().into());
                            requests.acknowledge(&completion.request);
                        });
                    }
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
                            app.set_report_rate_busy(false);
                            app.set_status_msg(
                                "Мышь недоступна. Подключите её и нажмите обновление устройств."
                                    .into(),
                            );
                        });
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {}
            }
            if !active.load(Ordering::SeqCst) {
                break;
            }
            if let Some(device) = connection.as_mut() {
                let result = device
                    .poll_buttons(&listener, locked.load(Ordering::SeqCst))
                    .and_then(|()| {
                        if last_poll.elapsed() >= Duration::from_millis(500) {
                            last_poll = Instant::now();
                            device.poll_state(&weak, &listener, &profiles, &locked, &requests)
                        } else {
                            Ok(())
                        }
                    });
                if let Err(error) = result {
                    listener.release_buttons();
                    connection = None;
                    disconnected(&weak, error);
                }
            }
        }
        listener.release_buttons();
    })
}

fn connect(
    weak: &slint::Weak<MainWindow>,
    requests: &Arc<DpiRequests>,
) -> Option<DeviceConnection> {
    match DeviceConnection::open(weak, requests) {
        Ok(connection) => Some(connection),
        Err(error) => {
            disconnected(weak, error);
            None
        }
    }
}

fn disconnected(weak: &slint::Weak<MainWindow>, error: String) {
    update_ui(weak, move |app| {
        app.set_device_connected(false);
        app.set_onboard_known(false);
        app.set_report_rate_busy(false);
        app.set_report_rate_interval(0);
        app.set_report_rate_intervals(Default::default());
        app.set_onboard_busy(false);
        app.set_hardware_buttons_ready(false);
        app.set_device_name("G502 HERO недоступна".into());
        app.set_status_msg(error.into());
    });
}

fn execute(
    device: &mut DeviceConnection,
    command: DeviceCommand,
    listener: &KeybindListener,
    weak: &slint::Weak<MainWindow>,
) -> Result<String, String> {
    match command {
        DeviceCommand::SetReportRate(interval) => {
            let result = if device.onboard || !device.known {
                Err("Выключите внутреннюю память перед изменением частоты".into())
            } else {
                crate::hidpp::features::report_rate::set_report_rate(&device.dev, interval).map(
                    |(_, confirmed)| format!("Частота опроса: {} Гц", 1000 / u32::from(confirmed)),
                )
            };
            refresh_report_rate(&device.dev, weak);
            result
        }
        DeviceCommand::SetMemoryMode(onboard) => {
            let result = device.set_memory(onboard, listener);
            refresh_report_rate(&device.dev, weak);
            result
        }
        DeviceCommand::SetLighting { zone, effect } => {
            if device.onboard || !device.known {
                return Err(
                    "Выключите внутреннюю память в профилях перед настройкой подсветки".into(),
                );
            }
            let zones = match zone {
                Some(zone) => vec![zone],
                None => {
                    let count = crate::hidpp::features::get_zone_count(&device.dev)?;
                    [LedZone::DpiLeds, LedZone::Logo]
                        .into_iter()
                        .take(count.min(2) as usize)
                        .collect()
                }
            };
            let mut errors = Vec::new();
            for zone in zones {
                if let Err(error) = set_led_effect(&device.dev, zone, effect, false) {
                    errors.push(format!("{zone:?}: {error}"));
                }
            }
            if !errors.is_empty() {
                return Err(errors.join("; "));
            }
            Ok(if effect == LedEffect::Off {
                "Подсветка выключена"
            } else {
                "Эффект подсветки применён"
            }
            .into())
        }
        DeviceCommand::SyncAllOnboardSlots(dpi) => {
            crate::hidpp::features::sync_all_onboard_profiles_dpi(&device.dev, dpi)?;
            Ok("Профили синхронизированы".into())
        }
        DeviceCommand::Refresh | DeviceCommand::SetDpi(_) => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn dropping_guard_requests_stop_and_waits_for_in_flight_work_to_finish() {
        let (started_tx, started_rx) = mpsc::channel();
        let (finish_tx, finish_rx) = mpsc::channel();
        let (dropped_tx, dropped_rx) = mpsc::channel();
        let completed = Arc::new(AtomicBool::new(false));
        let done = completed.clone();
        let guard = DeviceWorkerGuard::spawn(move |running| {
            started_tx.send(()).unwrap();
            finish_rx.recv().unwrap();
            assert!(!running.load(Ordering::SeqCst));
            done.store(true, Ordering::SeqCst);
        });
        let running = guard.running.clone();
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let stopper = std::thread::spawn(move || {
            drop(guard);
            dropped_tx.send(()).unwrap();
        });
        let started = Instant::now();
        while running.load(Ordering::SeqCst) && started.elapsed() < Duration::from_secs(2) {
            std::thread::yield_now();
        }
        // Always unblock and join before asserting, even if the regression occurs.
        let returned_too_early = dropped_rx.recv_timeout(Duration::from_millis(30)).is_ok();
        finish_tx.send(()).unwrap();
        stopper.join().unwrap();
        assert!(!running.load(Ordering::SeqCst));
        assert!(!returned_too_early);
        assert!(completed.load(Ordering::SeqCst));
    }
}
