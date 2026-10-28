use std::rc::Rc;
use std::sync::{Arc, Mutex};
use hidapi::HidApi;
use slint::{ModelRc, VecModel};

use crate::hidpp::HidppDevice;
use crate::hidpp::features::{
    get_device_name, get_current_dpi, set_dpi,
    set_led_effect, LedZone, LedEffect,
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
        in-out property <string> device_name: "G502 HERO Gaming Mouse";
        in-out property <int> current_dpi: 800;
        in-out property <string> status_msg: "Подключено";
        in-out property <color> current_color: #00e5ff;
        in-out property <string> active_macro_name: "";
        in-out property <[string]> macro_list: ["lkm", "pkm", "traxmine", "ad", "rgp", "snayp"];

        callback set_dpi(int);
        callback set_rgb_static(int, int, int);
        callback set_rgb_cycle();
        callback set_rgb_breathe(int, int, int);
        callback set_rgb_off();
        callback toggle_macro(string);

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
                            background: #00e676;
                            y: 25px;
                        }

                        Text {
                            text: root.device_name;
                            font-size: 13px;
                            font-weight: 600;
                            color: #e0e6ed;
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
                                    text: "RAM: ~12 MB";
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
                                text: "Управление подсветкой LIGHTSYNC (Зона 1: Логотип G)";
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

                            Text {
                                text: "Макросы из базы Logitech G HUB";
                                font-size: 18px;
                                font-weight: 700;
                                color: #ffffff;
                            }

                            Text {
                                text: "Привязка на мыши: G9 -> 'lkm' (Автокликер ЛКМ), G6 Снайпер -> 'pkm' (Автокликер ПКМ)";
                                font-size: 12px;
                                color: #00e5ff;
                            }

                            HorizontalLayout {
                                spacing: 12px;

                                for name in root.macro_list : Rectangle {
                                    height: 54px;
                                    border-radius: 6px;
                                    background: root.active_macro_name == name ? #00e5ff : #161920;
                                    border-width: 1px;
                                    border-color: root.active_macro_name == name ? #00e5ff : #242a36;

                                    HorizontalLayout {
                                        padding-left: 14px;
                                        padding-right: 14px;
                                        alignment: space-between;

                                        VerticalLayout {
                                            alignment: center;
                                            Text {
                                                text: name;
                                                font-size: 14px;
                                                font-weight: 700;
                                                color: root.active_macro_name == name ? #000000 : #ffffff;
                                            }
                                            Text {
                                                text: root.active_macro_name == name ? "РАБОТАЕТ" : "Нажмите для теста";
                                                font-size: 10px;
                                                color: root.active_macro_name == name ? #003344 : #6c788c;
                                            }
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
                        text: "Logitech HID++ 2.0 Direct Native Driver";
                        font-size: 11px;
                        color: #434c59;
                        vertical-alignment: center;
                    }
                }
            }
        }
    }
}

pub fn run() {
    let app = MainWindow::new().expect("Failed to create MainWindow");

    let api = HidApi::new().expect("Failed to initialize HID");
    let info = api
        .device_list()
        .find(|d| d.vendor_id() == LOGITECH_VID && d.product_id() == G502_PID && d.usage_page() == 0xFF00 && d.usage() == 0x0002);

    let dev_arc: Option<Arc<HidppDevice>> = if let Some(i) = info {
        if let Ok(dev) = api.open_path(i.path()) {
            let hidpp = Arc::new(HidppDevice::new(dev, 0xFF));
            if let Ok(name) = get_device_name(&hidpp) {
                app.set_device_name(name.into());
            }
            if let Ok(dpi) = get_current_dpi(&hidpp, 0) {
                app.set_current_dpi(dpi as i32);
            }
            Some(hidpp)
        } else {
            None
        }
    } else {
        None
    };

    // Load macros from G HUB database
    let lghub_data = LgHubSettings::load_from_db().ok();
    if let Some(ref data) = lghub_data {
        let mut names: Vec<slint::SharedString> = Vec::new();
        for card in &data.cards {
            if let Some(m) = &card.macro_data {
                if m.is_toggle || m.name == "lkm" || m.name == "pkm" || m.name == "traxmine" || m.name == "ad" {
                    names.push(m.name.clone().into());
                }
            }
        }
        if !names.is_empty() {
            let model = Rc::new(VecModel::from(names));
            app.set_macro_list(ModelRc::from(model));
        }
    }

    // Callbacks
    let dev_dpi = dev_arc.clone();
    let app_weak = app.as_weak();
    app.on_set_dpi(move |target| {
        if let Some(ref dev) = dev_dpi {
            let _ = set_dpi(dev, 0, target as u16);
            if let Some(app) = app_weak.upgrade() {
                app.set_current_dpi(target);
                app.set_status_msg(format!("Установлен DPI: {}", target).into());
            }
        }
    });

    let dev_rgb = dev_arc.clone();
    let app_weak = app.as_weak();
    app.on_set_rgb_static(move |r, g, b| {
        if let Some(ref dev) = dev_rgb {
            let _ = set_led_effect(dev, LedZone::Logo, LedEffect::Static { r: r as u8, g: g as u8, b: b as u8 });
            if let Some(app) = app_weak.upgrade() {
                app.set_status_msg(format!("Цвет установлен: RGB({}, {}, {})", r, g, b).into());
            }
        }
    });

    let dev_cycle = dev_arc.clone();
    let app_weak = app.as_weak();
    app.on_set_rgb_cycle(move || {
        if let Some(ref dev) = dev_cycle {
            let _ = set_led_effect(dev, LedZone::Logo, LedEffect::ColorCycle { period_ms: 10000, brightness: 255 });
            if let Some(app) = app_weak.upgrade() {
                app.set_status_msg("Включен радужный спектральный цикл".into());
            }
        }
    });

    let dev_breathe = dev_arc.clone();
    let app_weak = app.as_weak();
    app.on_set_rgb_breathe(move |r, g, b| {
        if let Some(ref dev) = dev_breathe {
            let _ = set_led_effect(dev, LedZone::Logo, LedEffect::Breathing { r: r as u8, g: g as u8, b: b as u8, period_ms: 3000 });
            if let Some(app) = app_weak.upgrade() {
                app.set_status_msg("Включен режим пульсации (дыхание)".into());
            }
        }
    });

    let dev_off = dev_arc.clone();
    let app_weak = app.as_weak();
    app.on_set_rgb_off(move || {
        if let Some(ref dev) = dev_off {
            let _ = set_led_effect(dev, LedZone::All, LedEffect::Off);
            if let Some(app) = app_weak.upgrade() {
                app.set_status_msg("Подсветка выключена".into());
            }
        }
    });

    // Toggle active macro
    let active_macro_slot: Arc<Mutex<Option<macro_engine::ActiveMacro>>> = Arc::new(Mutex::new(None));
    let slot_clone = active_macro_slot.clone();
    let app_weak = app.as_weak();
    app.on_toggle_macro(move |name| {
        let mut slot = slot_clone.lock().unwrap();
        if let Some(active) = slot.take() {
            active.stop();
            if let Some(app) = app_weak.upgrade() {
                app.set_active_macro_name("".into());
                app.set_status_msg("Макрос остановлен".into());
            }
        } else {
            if let Ok(settings) = LgHubSettings::load_from_db() {
                let target = settings.cards.iter().find(|c| {
                    name == c.name.as_deref().unwrap_or("")
                        || name == c.macro_data.as_ref().map(|m| m.name.as_str()).unwrap_or("")
                });
                if let Some(card) = target {
                    if let Some(m) = &card.macro_data {
                        let active = macro_engine::start_macro_loop(m.clone());
                        *slot = Some(active);
                        if let Some(app) = app_weak.upgrade() {
                            app.set_active_macro_name(name.clone());
                            app.set_status_msg(format!("Запущен макрос '{}' (автокликер)", name).into());
                        }
                    }
                }
            }
        }
    });

    app.run().expect("Failed to run Slint application");
}
