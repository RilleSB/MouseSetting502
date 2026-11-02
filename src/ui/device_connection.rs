use super::dpi_controller::DpiRequests;
use super::{
    bridge::MainWindow,
    device_worker::{G435_PID, G502_PID, LOGITECH_VID},
};
use crate::hidpp::features::report_rate::read_report_rate;
use crate::hidpp::features::{
    buttons::{button_bitmap, start_button_spy},
    profiles::{get_onboard_mode, set_onboard_mode},
};
use crate::hidpp::{
    HidppDevice,
    features::{get_current_dpi, get_current_onboard_profile, set_current_onboard_profile},
};
use crate::lghub::{AppProfile, KeybindListener};
use hidapi::HidApi;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub struct DeviceConnection {
    pub dev: HidppDevice,
    pub onboard: bool,
    pub known: bool,
    pub spy: Option<u8>,
    pub dpi: u16,
    pub slot: Option<u8>,
    bitmap: u16,
}

pub fn update_ui(weak: &slint::Weak<MainWindow>, f: impl FnOnce(MainWindow) + Send + 'static) {
    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(app) = weak.upgrade() {
            f(app);
        }
    });
}

pub fn refresh_report_rate(dev: &HidppDevice, weak: &slint::Weak<MainWindow>) {
    let data = read_report_rate(dev);
    update_ui(weak, move |app| {
        let (intervals, current) = data.unwrap_or_default();
        let intervals = intervals.into_iter().map(i32::from).collect::<Vec<_>>();
        app.set_report_rate_intervals(slint::ModelRc::new(slint::VecModel::from(intervals)));
        app.set_report_rate_interval(current as i32);
        app.set_report_rate_busy(false);
    });
}

impl DeviceConnection {
    pub fn open(
        weak: &slint::Weak<MainWindow>,
        requests: &Arc<DpiRequests>,
    ) -> Result<Self, String> {
        let observation = requests.observation();
        let api = HidApi::new().map_err(|e| e.to_string())?;
        let headset = api
            .device_list()
            .any(|d| d.vendor_id() == LOGITECH_VID && d.product_id() == G435_PID);
        update_ui(weak, move |app| app.set_headset_connected(headset));
        let info = api
            .device_list()
            .find(|d| {
                d.vendor_id() == LOGITECH_VID
                    && d.product_id() == G502_PID
                    && d.usage_page() == 0xff00
                    && d.usage() == 2
            })
            .ok_or("Мышь G502 HERO не найдена")?;
        let dev = HidppDevice::new(api.open_path(info.path()).map_err(|e| e.to_string())?, 0xff);
        // Opening a device must never force its memory mode.
        let onboard = get_onboard_mode(&dev)?;
        let dpi = get_current_dpi(&dev, 0)?;
        let slot = if onboard {
            get_current_onboard_profile(&dev).ok()
        } else {
            None
        };
        let name = crate::hidpp::features::get_device_name(&dev)
            .unwrap_or_else(|_| "Logitech G502 HERO".into());
        let dpi_requests = requests.clone();
        update_ui(weak, move |app| {
            app.set_device_connected(true);
            app.set_device_name(name.into());
            if dpi_requests.can_observe(observation) {
                app.set_current_dpi(dpi as i32);
            }
        });
        let mut connection = Self {
            dev,
            onboard,
            known: true,
            spy: None,
            dpi,
            slot,
            bitmap: 0,
        };
        let status = if onboard {
            "Внутренняя память включена. Для назначения G6–G9 выключите её в профилях.".into()
        } else {
            connection.start_spy().unwrap_or_else(|error| error)
        };
        connection.publish(weak);
        refresh_report_rate(&connection.dev, weak);
        let dpi_requests = requests.clone();
        update_ui(weak, move |app| {
            if dpi_requests.can_observe(observation) {
                app.set_status_msg(status.into());
            }
        });
        Ok(connection)
    }

    pub fn start_spy(&mut self) -> Result<String, String> {
        self.spy = None;
        self.spy = Some(
            start_button_spy(&self.dev).map_err(|e| format!("Не удалось включить G6–G9: {e}"))?,
        );
        Ok("Внутренняя память выключена. Прямой приём G6–G9 включён.".into())
    }

    pub fn set_memory(
        &mut self,
        onboard: bool,
        listener: &KeybindListener,
    ) -> Result<String, String> {
        listener.release_buttons();
        self.bitmap = 0;
        let result = set_onboard_mode(&self.dev, onboard);
        let actual = get_onboard_mode(&self.dev);
        self.known = actual.is_ok();
        self.onboard = actual?;
        self.slot = if self.onboard {
            get_current_onboard_profile(&self.dev).ok()
        } else {
            None
        };
        result?;
        if self.onboard {
            Ok("Внутренняя память включена; G6–G9 используют настройки мыши.".into())
        } else {
            self.start_spy()
        }
    }

    pub fn publish(&self, weak: &slint::Weak<MainWindow>) {
        let (onboard, known, ready, slot) = (
            self.onboard,
            self.known,
            self.spy.is_some(),
            self.slot.unwrap_or(0),
        );
        update_ui(weak, move |app| {
            app.set_onboard_enabled(onboard);
            app.set_onboard_known(known);
            app.set_onboard_busy(false);
            app.set_hardware_buttons_ready(ready);
            app.set_active_onboard_slot(slot as i32);
        });
    }

    pub fn poll_buttons(
        &mut self,
        listener: &KeybindListener,
        g9_locked: bool,
    ) -> Result<(), String> {
        for _ in 0..64 {
            let Some(packet) = self.dev.read_notification(0)? else {
                break;
            };
            let Some(bitmap) = self.spy.and_then(|index| button_bitmap(&packet, index)) else {
                continue;
            };
            let changed = self.bitmap ^ bitmap;
            self.bitmap = bitmap;
            if self.onboard || !self.known {
                continue;
            }
            for (bit, id, default) in [
                (5, "G6", ""),
                (6, "G7", "__DPI_DOWN__"),
                (7, "G8", "__DPI_UP__"),
                (8, "G9", if g9_locked { "" } else { "__PROFILE_NEXT__" }),
            ] {
                if changed & (1 << bit) != 0 {
                    listener.hardware_button(id, bitmap & (1 << bit) != 0, default);
                }
            }
        }
        Ok(())
    }

    pub fn poll_state(
        &mut self,
        weak: &slint::Weak<MainWindow>,
        listener: &KeybindListener,
        profiles: &Arc<Mutex<Vec<AppProfile>>>,
        locked: &AtomicBool,
        requests: &Arc<DpiRequests>,
    ) -> Result<(), String> {
        let observation = requests.observation();
        let onboard = get_onboard_mode(&self.dev)?;
        if !self.known || onboard != self.onboard {
            self.known = true;
            self.onboard = onboard;
            self.slot = None;
            listener.release_buttons();
            self.bitmap = 0;
            if !onboard {
                let result = self.start_spy();
                if let Err(error) = result {
                    update_ui(weak, move |app| app.set_status_msg(error.into()));
                }
            }
            self.publish(weak);
            refresh_report_rate(&self.dev, weak);
        }
        if onboard {
            let slot = get_current_onboard_profile(&self.dev)?;
            if let Some(previous) = self.slot.filter(|previous| *previous != slot) {
                if locked.load(Ordering::SeqCst) {
                    set_current_onboard_profile(&self.dev, previous)?;
                } else {
                    self.slot = Some(slot);
                    let id = profiles
                        .lock()
                        .ok()
                        .and_then(|p| p.get(slot.saturating_sub(1) as usize).map(|p| p.id.clone()));
                    update_ui(weak, move |app| {
                        if let Some(id) = id {
                            app.invoke_switch_profile(id.into());
                        }
                    });
                }
            } else {
                self.slot = Some(slot);
            }
        }
        let dpi = get_current_dpi(&self.dev, 0)?;
        self.dpi = dpi;
        let requests = requests.clone();
        update_ui(weak, move |app| {
            if requests.can_observe(observation) {
                app.set_current_dpi(dpi as i32);
            }
        });
        self.publish(weak);
        Ok(())
    }
}
