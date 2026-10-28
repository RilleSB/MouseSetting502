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
use crate::lghub::{LgHubSettings, macro_engine};

const LOGITECH_VID: u16 = 0x046D;
const G502_PID: u16 = 0xC08B;

slint::slint! {
    import { Button, Slider, ScrollView } from "std-widgets.slint";

    export component MainWindow inherits Window {
        title: "Logitech G HUB (Rust Edition)";
        min-width: 820px;
        min-height: 560px;
        preferred-width: 860px;
        preferred-height: 600px;
        background: #0f1115;
        default-font-family: "Segoe UI";

        in-out property <int> active_tab: 0;
        in-out property <string> device_name: "Поиск устройства...";
        in-out property <bool> device_connected: false;
        in-out property <int> current_dpi: 800;
        in-out property <string> status_msg: "Инициализация...";
        in-out property <color> current_color: #00e5ff;
        in-out property <string> active_macro_name: "";
        in-out property <[string]> macro_list: [];

        callback set_dpi(int);
        callback set_rgb_static(int, int, int);
        callback set_rgb_cycle();
        callback set_rgb_breathe(int, int, int);
        callback set_rgb_off();
        callback toggle_macro(string);
        callback stop_all_macros();

        VerticalLayout {
            padding: 0px;
            spacing: 0px;

            // --- Top Header ---
            Rectangle {
                height: 60px;
                background: #161920;

                HorizontalLayout {
                    padding-left: 24px;
                    padding-right: 24px;
                    spacing: 16px;
                    alignment: space-between;

                    HorizontalLayout {
                        spacing: 12px;
                        alignment: start;

                        Text {
                            text: "🦀 G-HUB";
                            font-size: 20px;
                            font-weight: 800;
                            color: #00e5ff;
                            vertical-alignment: center;
                        }

                        Text {
                            text: "RUST EDITION";
                            font-size: 11px;
                            font-weight: 700;
                            color: #6c788c;
                            vertical-alignment: center;
                        }
                    }

                    HorizontalLayout {
                        spacing: 8px;
                        alignment: end;

                        Rectangle {
                            width: 10px;
                            height: 10px;
                            border-radius: 5px;
                            background: root.device_connected ? #00e676 : #ff3d00;
                            y: 25px;
                        }

                        Text {
                            text: root.device_name;
                            font-size: 13px;
                            font-weight: 600;
                            color: root.device_connected ? #e0e6ed : #8a96a8;
                            vertical-alignment: center;
                        }

                        Rectangle {
                            width: 12px;
                        }

                        Rectangle {
                            height: 26px;
                            border-radius: 4px;
                            background: #212631;
                            HorizontalLayout {
                                padding-left: 10px;
                                padding-right: 10px;
                                Text {
                                    text: "RAM: ~18 MB";
                                    font-size: 11px;
                                    color: #00e5ff;
                                    font-weight: 600;
                                    vertical-alignment: center;
                                }
                            }
                        }
                    }
                }
            }

            // --- Main Body (Sidebar + Content) ---
            HorizontalLayout {
                spacing: 0px;

                // --- Sidebar ---
                Rectangle {
                    width: 200px;
                    background: #13161c;

                    VerticalLayout {
                        padding-top: 20px;
                        padding-left: 12px;
                        padding-right: 12px;
                        spacing: 8px;
                        alignment: start;

                        // Tab 0: DPI
                        Rectangle {
                            height: 44px;
                            border-radius: 6px;
                            background: root.active_tab == 0 ? #1f2530 : transparent;
                            border-width: root.active_tab == 0 ? 1px : 0px;
                            border-color: #00e5ff;

                            HorizontalLayout {
                                padding-left: 14px;
                                spacing: 10px;
                                Text {
                                    text: "🎯  Чувствительность (DPI)";
                                    font-size: 13px;
                                    font-weight: 600;
                                    color: root.active_tab == 0 ? #ffffff : #8a96a8;
                                    vertical-alignment: center;
                                }
                            }
                            TouchArea {
                                clicked => { root.active_tab = 0; }
                            }
                        }

                        // Tab 1: RGB
                        Rectangle {
                            height: 44px;
                            border-radius: 6px;
                            background: root.active_tab == 1 ? #1f2530 : transparent;
                            border-width: root.active_tab == 1 ? 1px : 0px;
                            border-color: #00e5ff;

                            HorizontalLayout {
                                padding-left: 14px;
                                spacing: 10px;
                                Text {
                                    text: "✨  Подсветка LIGHTSYNC";
                                    font-size: 13px;
                                    font-weight: 600;
                                    color: root.active_tab == 1 ? #ffffff : #8a96a8;
                                    vertical-alignment: center;
                                }
                            }
                            TouchArea {
                                clicked => { root.active_tab = 1; }
                            }
                        }

                        // Tab 2: Macros
                        Rectangle {
                            height: 44px;
                            border-radius: 6px;
                            background: root.active_tab == 2 ? #1f2530 : transparent;
                            border-width: root.active_tab == 2 ? 1px : 0px;
                            border-color: #00e5ff;

                            HorizontalLayout {
                                padding-left: 14px;
                                spacing: 10px;
                                Text {
                                    text: "⚡  Макросы G HUB";
                                    font-size: 13px;
                                    font-weight: 600;
                                    color: root.active_tab == 2 ? #ffffff : #8a96a8;
                                    vertical-alignment: center;
                                }
                            }
                            TouchArea {
                                clicked => { root.active_tab = 2; }
                            }
                        }
                    }
                }

                // --- Content Area ---
                Rectangle {
                    background: #0f1115;

                    VerticalLayout {
                        padding: 30px;
                        spacing: 20px;

                        // === TAB 0: DPI ===
                        if root.active_tab == 0 : VerticalLayout {
                            spacing: 24px;

                            Text {
                                text: "Настройка сенсора HERO";
                                font-size: 18px;
                                font-weight: 700;
                                color: #ffffff;
                            }

                            // Big DPI Card
                            Rectangle {
                                height: 130px;
                                border-radius: 8px;
                                background: #161920;
                                border-width: 1px;
                                border-color: #242a36;

                                VerticalLayout {
                                    alignment: center;
                                    spacing: 8px;
                                    Text {
                                        text: "ТЕКУЩИЙ АКТИВНЫЙ DPI";
                                        font-size: 11px;
                                        font-weight: 700;
                                        color: #6c788c;
                                        horizontal-alignment: center;
                                    }
                                    Text {
                                        text: root.current_dpi + " DPI";
                                        font-size: 42px;
                                        font-weight: 800;
                                        color: #00e5ff;
                                        horizontal-alignment: center;
                                    }
                                }
                            }

                            // Presets
                            Text {
                                text: "Быстрые пресеты:";
                                font-size: 13px;
                                font-weight: 600;
                                color: #9aa7b8;
                            }

                            HorizontalLayout {
                                spacing: 10px;
                                for dpi in [400, 800, 1200, 1600, 2400, 3200] : Rectangle {
                                    height: 38px;
                                    border-radius: 6px;
                                    background: root.current_dpi == dpi ? #00e5ff : #1f2530;
                                    border-width: 1px;
                                    border-color: root.current_dpi == dpi ? #00e5ff : #2d3545;

                                    Text {
                                        text: dpi;
                                        font-size: 13px;
                                        font-weight: 700;
                                        color: root.current_dpi == dpi ? #000000 : #e0e6ed;
                                        horizontal-alignment: center;
                                        vertical-alignment: center;
                                    }
                                    TouchArea {
                                        clicked => {
                                            root.set_dpi(dpi);
                                        }
                                    }
                                }
                            }

                            // Slider
                            VerticalLayout {
                                spacing: 8px;
                                Text {
                                    text: "Плавная регулировка (100 - 6400 DPI):";
                                    font-size: 12px;
                                    color: #8391a3;
                                }
                                dpi_slider := Slider {
                                    minimum: 100;
                                    maximum: 6400;
                                    value: root.current_dpi;
                                    changed(v) => {
                                        root.current_dpi = Math.round(v / 50) * 50;
                                    }
                                    released(v) => {
                                        root.set_dpi(root.current_dpi);
                                    }
                                }
                            }
                        }

                        // === TAB 1: RGB ===
                        if root.active_tab == 1 : VerticalLayout {
                            spacing: 20px;

                            Text {
                                text: "Управление подсветкой LIGHTSYNC (Логотип G)";
                                font-size: 18px;
                                font-weight: 700;
                                color: #ffffff;
                            }

                            // Preview Card
                            Rectangle {
                                height: 100px;
                                border-radius: 8px;
                                background: #161920;
                                border-width: 1px;
                                border-color: #242a36;

                                HorizontalLayout {
                                    alignment: center;
                                    spacing: 16px;
                                    Rectangle {
                                        width: 50px;
                                        height: 50px;
                                        border-radius: 25px;
                                        background: root.current_color;
                                        y: 25px;
                                    }
                                    Text {
                                        text: "Активный цвет логотипа";
                                        font-size: 16px;
                                        font-weight: 700;
                                        color: #ffffff;
                                        vertical-alignment: center;
                                    }
                                }
                            }

                            Text {
                                text: "Палитра быстрых цветов:";
                                font-size: 13px;
                                font-weight: 600;
                                color: #9aa7b8;
                            }

                            // Color Buttons
                            HorizontalLayout {
                                spacing: 12px;

                                // Cyan
                                Rectangle {
                                    width: 44px; height: 44px; border-radius: 22px; background: #00e5ff;
                                    TouchArea { clicked => { root.current_color = #00e5ff; root.set_rgb_static(0, 229, 255); } }
                                }
                                // Neon Pink
                                Rectangle {
                                    width: 44px; height: 44px; border-radius: 22px; background: #ff007f;
                                    TouchArea { clicked => { root.current_color = #ff007f; root.set_rgb_static(255, 0, 127); } }
                                }
                                // Purple
                                Rectangle {
                                    width: 44px; height: 44px; border-radius: 22px; background: #7b1fa2;
                                    TouchArea { clicked => { root.current_color = #7b1fa2; root.set_rgb_static(123, 31, 162); } }
                                }
                                // Red
                                Rectangle {
                                    width: 44px; height: 44px; border-radius: 22px; background: #ff1744;
                                    TouchArea { clicked => { root.current_color = #ff1744; root.set_rgb_static(255, 23, 68); } }
                                }
                                // Green
                                Rectangle {
                                    width: 44px; height: 44px; border-radius: 22px; background: #00e676;
                                    TouchArea { clicked => { root.current_color = #00e676; root.set_rgb_static(0, 230, 118); } }
                                }
                                // Orange
                                Rectangle {
                                    width: 44px; height: 44px; border-radius: 22px; background: #ff9100;
                                    TouchArea { clicked => { root.current_color = #ff9100; root.set_rgb_static(255, 145, 0); } }
                                }
                                // White
                                Rectangle {
                                    width: 44px; height: 44px; border-radius: 22px; background: #ffffff;
                                    TouchArea { clicked => { root.current_color = #ffffff; root.set_rgb_static(255, 255, 255); } }
                                }
                            }

                            Text {
                                text: "Динамические эффекты:";
                                font-size: 13px;
                                font-weight: 600;
                                color: #9aa7b8;
                            }

                            HorizontalLayout {
                                spacing: 14px;

                                // Cycle
                                Rectangle {
                                    height: 42px; border-radius: 6px; background: #1f2530; border-width: 1px; border-color: #2d3545;
                                    HorizontalLayout { alignment: center; Text { text: "🌈 Радужный цикл"; font-size: 13px; font-weight: 600; color: #fff; vertical-alignment: center; } }
                                    TouchArea { clicked => { root.set_rgb_cycle(); } }
                                }

                                // Breathing
                                Rectangle {
                                    height: 42px; border-radius: 6px; background: #1f2530; border-width: 1px; border-color: #2d3545;
                                    HorizontalLayout { alignment: center; Text { text: "💨 Пульсация (Дыхание)"; font-size: 13px; font-weight: 600; color: #fff; vertical-alignment: center; } }
                                    TouchArea { clicked => { root.set_rgb_breathe(0, 229, 255); } }
                                }

                                // Off
                                Rectangle {
                                    height: 42px; border-radius: 6px; background: #1f2530; border-width: 1px; border-color: #2d3545;
                                    HorizontalLayout { alignment: center; Text { text: "⭕ Выключить подсветку"; font-size: 13px; font-weight: 600; color: #ff5252; vertical-alignment: center; } }
                                    TouchArea { clicked => { root.set_rgb_off(); } }
                                }
                            }
                        }

                        // === TAB 2: MACROS ===
                        if root.active_tab == 2 : VerticalLayout {
                            spacing: 16px;

                            HorizontalLayout {
                                alignment: space-between;
                                Text {
                                    text: "Макросы из базы Logitech G HUB";
                                    font-size: 18px;
                                    font-weight: 700;
                                    color: #ffffff;
                                }
                                if root.active_macro_name != "" : Rectangle {
                                    height: 28px;
                                    border-radius: 4px;
                                    background: #ff1744;
                                    HorizontalLayout {
                                        padding-left: 10px; padding-right: 10px;
                                        Text {
                                            text: "⛔ ОСТАНОВИТЬ ВСЕ МАКРОСЫ";
                                            font-size: 11px;
                                            font-weight: 700;
                                            color: #fff;
                                            vertical-alignment: center;
                                        }
                                    }
                                    TouchArea {
                                        clicked => { root.stop_all_macros(); }
                                    }
                                }
                            }

                            Text {
                                text: "Нажмите на макрос для включения/выключения автокликера:";
                                font-size: 12px;
                                color: #8a96a8;
                            }

                            ScrollView {
                                height: 260px;
                                VerticalLayout {
                                    spacing: 8px;
                                    for name in root.macro_list : Rectangle {
                                        height: 46px;
                                        border-radius: 6px;
                                        background: root.active_macro_name == name ? #00e5ff : #161920;
                                        border-width: 1px;
                                        border-color: root.active_macro_name == name ? #00e5ff : #242a36;

                                        HorizontalLayout {
                                            padding-left: 16px;
                                            padding-right: 16px;
                                            alignment: space-between;

                                            Text {
                                                text: name;
                                                font-size: 14px;
                                                font-weight: 700;
                                                color: root.active_macro_name == name ? #000000 : #ffffff;
                                                vertical-alignment: center;
                                            }

                                            Text {
                                                text: root.active_macro_name == name ? "🟢 АКТИВЕН" : "Нажмите для запуска";
                                                font-size: 11px;
                                                font-weight: 600;
                                                color: root.active_macro_name == name ? #003344 : #5f6d80;
                                                vertical-alignment: center;
                                            }
                                        }
                                        TouchArea {
                                            clicked => {
                                                root.toggle_macro(name);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // --- Footer Statusbar ---
            Rectangle {
                height: 32px;
                background: #0b0d10;

                HorizontalLayout {
                    padding-left: 24px;
                    padding-right: 24px;
                    alignment: space-between;

                    Text {
                        text: "Статус: " + root.status_msg;
                        font-size: 12px;
                        color: #7b889b;
                        vertical-alignment: center;
                    }

                    Text {
                        text: "Logitech HID++ 2.0 Native Worker";
                        font-size: 11px;
                        color: #434c59;
                        vertical-alignment: center;
                    }
                }
            }
        }
    }
}

enum DeviceCommand {
    SetDpi(u16),
    SetRgbStatic(u8, u8, u8),
    SetRgbCycle,
    SetRgbBreathe(u8, u8, u8),
    SetRgbOff,
}

pub fn run() {
    let app = MainWindow::new().expect("Failed to create MainWindow");

    // Preload G HUB settings once at startup into memory
    let lghub_settings = Arc::new(LgHubSettings::load_from_db().ok());

    // Dynamically populate all unique custom macros from G HUB
    if let Some(ref data) = *lghub_settings {
        let mut macro_names = Vec::new();
        for card in &data.cards {
            if let Some(m) = &card.macro_data {
                if !macro_names.contains(&m.name) {
                    macro_names.push(m.name.clone());
                }
            }
        }
        let slint_names: Vec<slint::SharedString> = macro_names.into_iter().map(|s| s.into()).collect();
        let model = Rc::new(VecModel::from(slint_names));
        app.set_macro_list(ModelRc::from(model));
    }

    // Try opening device
    let api = HidApi::new().ok();
    let dev_info = api.as_ref().and_then(|a| {
        a.device_list()
            .find(|d| d.vendor_id() == LOGITECH_VID && d.product_id() == G502_PID && d.usage_page() == 0xFF00 && d.usage() == 0x0002)
    });

    let (cmd_tx, cmd_rx) = channel::<DeviceCommand>();

    if let (Some(api_ref), Some(info)) = (api.as_ref(), dev_info) {
        match api_ref.open_path(info.path()) {
            Ok(dev) => {
                let hidpp = Arc::new(HidppDevice::new(dev, 0xFF));
                let _ = enable_host_mode(&hidpp); // Call once on connect

                if let Ok(name) = get_device_name(&hidpp) {
                    app.set_device_name(name.into());
                }
                app.set_device_connected(true);
                app.set_status_msg("Устройство подключено и готово к работе".into());

                if let Ok(dpi) = get_current_dpi(&hidpp, 0) {
                    app.set_current_dpi(dpi as i32);
                }

                // Dedicated background worker for HID I/O to NEVER freeze the UI
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

    // Callbacks to worker channel
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

    // Macro execution controller with proper state and switching
    let active_macro: Arc<Mutex<Option<(String, macro_engine::ActiveMacro)>>> = Arc::new(Mutex::new(None));

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

    let macro_slot = active_macro.clone();
    let settings_arc = lghub_settings.clone();
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

        // Switching to new macro
        if let Some(ref data) = *settings_arc {
            let target = data.cards.iter().find(|c| {
                name == c.name.as_deref().unwrap_or("")
                    || name == c.macro_data.as_ref().map(|m| m.name.as_str()).unwrap_or("")
            });

            if let Some(card) = target {
                if let Some(m) = &card.macro_data {
                    let active = macro_engine::start_macro_loop(m.clone());
                    *slot = Some((name.to_string(), active));
                    if let Some(app) = app_weak.upgrade() {
                        app.set_active_macro_name(name.clone());
                        app.set_status_msg(format!("Запущен макрос '{}' (автокликер)", name).into());
                    }
                    return;
                }
            }
        }

        if let Some(app) = app_weak.upgrade() {
            app.set_status_msg(format!("Макрос '{}' не найден в базе данных", name).into());
        }
    });

    app.run().expect("Failed to run Slint application");

    // Clean up any running macros on window close
    let mut slot = active_macro.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, active)) = slot.take() {
        active.stop();
    }
}
