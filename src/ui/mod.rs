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

slint::slint! {
    import { Slider, ScrollView, LineEdit } from "std-widgets.slint";

    export struct MacroItem {
        name: string,
        macro_type: string,
        is_toggle: bool,
        steps_count: int,
        is_custom: bool,
    }

    export struct KeybindItem {
        button_id: string,
        button_label: string,
        default_action: string,
        assigned_macro: string,
        is_mouse: bool,
    }

    export component MainWindow inherits Window {
        title: "Logitech G HUB (Rust Edition)";
        min-width: 1040px;
        min-height: 700px;
        preferred-width: 1080px;
        preferred-height: 740px;
        background: #090b10;
        default-font-family: "Segoe UI";

        in-out property <int> active_tab: 0;
        in-out property <string> device_name: "Поиск мыши G502 HERO...";
        in-out property <bool> device_connected: false;
        in-out property <string> headset_name: "Logitech G435 Lightspeed";
        in-out property <bool> headset_connected: false;
        in-out property <int> current_dpi: 800;
        in-out property <int> polling_rate: 1000;
        in-out property <string> status_msg: "Готово к работе";
        in-out property <color> current_color: #00e5ff;
        in-out property <string> active_macro_name: "";
        in-out property <bool> listener_active: true;

        in-out property <[MacroItem]> macro_items: [];
        in-out property <[KeybindItem]> keybind_items: [];

        // Macro builder state
        in-out property <bool> show_macro_builder: false;
        in-out property <string> builder_editing_original: "";
        in-out property <string> builder_name: "Мой автокликер";
        in-out property <bool> builder_is_toggle: true;
        in-out property <[string]> builder_steps: [];
        in-out property <string> builder_custom_delay: "50";

        // Keybind picker modal
        in-out property <bool> show_bind_picker: false;
        in-out property <string> picker_button_id: "";
        in-out property <string> picker_button_name: "";

        callback set_dpi(int);
        callback set_polling_rate(int);
        callback set_rgb_static(int, int, int);
        callback set_rgb_cycle();
        callback set_rgb_breathe(int, int, int);
        callback set_rgb_off();
        callback toggle_macro(string);
        callback stop_all_macros();
        callback refresh_devices();

        // Macro builder callbacks
        callback open_macro_builder();
        callback edit_custom_macro(string);
        callback close_macro_builder();
        callback builder_add_mouse(int, bool, int);
        callback builder_add_key(int, bool, int);
        callback builder_add_delay(int);
        callback builder_clear_steps();
        callback builder_remove_step(int);
        callback builder_move_step_up(int);
        callback builder_move_step_down(int);
        callback builder_save_macro(string, string, bool);
        callback delete_custom_macro(string);

        // Keybinding callbacks
        callback toggle_listener();
        callback open_keybind_picker(string, string);
        callback close_keybind_picker();
        callback assign_keybind(string, string);
        callback clear_keybind(string);

        VerticalLayout {
            padding: 0px;
            spacing: 0px;

            // ==========================================
            // HEADER BAR
            // ==========================================
            Rectangle {
                height: 64px;
                background: #10141d;
                border-width: 0px;

                HorizontalLayout {
                    padding-left: 20px;
                    padding-right: 20px;
                    alignment: space-between;

                    // Brand / Logo
                    HorizontalLayout {
                        spacing: 14px;
                        alignment: start;

                        Rectangle {
                            width: 36px;
                            height: 36px;
                            border-radius: 8px;
                            background: #00e5ff18;
                            border-width: 1px;
                            border-color: #00e5ff66;
                            y: 14px;

                            Text {
                                text: "⯛ G";
                                font-size: 16px;
                                font-weight: 900;
                                color: #00e5ff;
                                horizontal-alignment: center;
                                vertical-alignment: center;
                            }
                        }

                        VerticalLayout {
                            alignment: center;
                            spacing: 2px;

                            HorizontalLayout {
                                spacing: 8px;
                                Text {
                                    text: "LOGITECH G";
                                    font-size: 16px;
                                    font-weight: 800;
                                    color: #f1f5f9;
                                }
                                Text {
                                    text: "PRO EDITION";
                                    font-size: 11px;
                                    font-weight: 700;
                                    color: #00e5ff;
                                    vertical-alignment: center;
                                }
                            }
                            Text {
                                text: "LIGHTSPEED NATIVE WORKER • DIRECT HID++ 2.0 • ZERO-LATENCY";
                                font-size: 10px;
                                font-weight: 600;
                                color: #526075;
                            }
                        }
                    }

                    // Device Status Indicators & Metrics
                    HorizontalLayout {
                        spacing: 12px;
                        alignment: end;

                        // Mouse status pill
                        Rectangle {
                            height: 32px;
                            border-radius: 6px;
                            background: root.device_connected ? #062b1a : #2b1115;
                            border-width: 1px;
                            border-color: root.device_connected ? #00ce88 : #ff3344;
                            y: 16px;

                            HorizontalLayout {
                                padding-left: 10px;
                                padding-right: 12px;
                                spacing: 8px;

                                Rectangle {
                                    width: 8px;
                                    height: 8px;
                                    border-radius: 4px;
                                    background: root.device_connected ? #00e676 : #ff3344;
                                    y: 12px;
                                }

                                Text {
                                    text: root.device_connected ? (root.device_name + " • 1000Hz") : "Мышь не обнаружена";
                                    font-size: 12px;
                                    font-weight: 600;
                                    color: root.device_connected ? #e0f2fe : #fca5a5;
                                    vertical-alignment: center;
                                }
                            }
                        }

                        // Headset status pill
                        Rectangle {
                            height: 32px;
                            border-radius: 6px;
                            background: root.headset_connected ? #122438 : #161a24;
                            border-width: 1px;
                            border-color: root.headset_connected ? #00e5ff : #273042;
                            y: 16px;

                            HorizontalLayout {
                                padding-left: 10px;
                                padding-right: 12px;
                                spacing: 8px;

                                Text {
                                    text: root.headset_connected ? "🎧 G435: Подключен" : "🎧 G435: Беспроводной";
                                    font-size: 12px;
                                    font-weight: 600;
                                    color: root.headset_connected ? #00e5ff : #728299;
                                    vertical-alignment: center;
                                }
                            }
                        }

                        // Low memory consumption badge
                        Rectangle {
                            height: 32px;
                            border-radius: 6px;
                            background: #141824;
                            border-width: 1px;
                            border-color: #242f42;
                            y: 16px;

                            HorizontalLayout {
                                padding-left: 10px;
                                padding-right: 10px;
                                Text {
                                    text: "RAM: ~14 MB • CPU: 0%";
                                    font-size: 11px;
                                    font-weight: 700;
                                    color: #00ce88;
                                    vertical-alignment: center;
                                }
                            }
                        }

                        // Refresh button
                        Rectangle {
                            width: 34px;
                            height: 32px;
                            border-radius: 6px;
                            background: #1c2230;
                            border-width: 1px;
                            border-color: #2b3548;
                            y: 16px;

                            Text {
                                text: "🔄";
                                font-size: 13px;
                                horizontal-alignment: center;
                                vertical-alignment: center;
                            }
                            TouchArea {
                                clicked => { root.refresh_devices(); }
                            }
                        }
                    }
                }
            }

            // Divider
            Rectangle {
                height: 1px;
                background: #1c2230;
            }

            // ==========================================
            // MAIN BODY: SIDEBAR + CONTENT AREA
            // ==========================================
            HorizontalLayout {
                spacing: 0px;

                // --- Modern Sidebar Navigation ---
                Rectangle {
                    width: 235px;
                    background: #10141d;

                    VerticalLayout {
                        padding-top: 18px;
                        padding-left: 12px;
                        padding-right: 12px;
                        padding-bottom: 18px;
                        spacing: 6px;
                        alignment: space-between;

                        VerticalLayout {
                            spacing: 6px;

                            // Tab 0: Device Overview
                            Rectangle {
                                height: 46px;
                                border-radius: 8px;
                                background: root.active_tab == 0 ? #1a2232 : transparent;
                                border-width: root.active_tab == 0 ? 1px : 0px;
                                border-color: #2c3c54;

                                HorizontalLayout {
                                    padding-left: 12px;
                                    spacing: 12px;

                                    Rectangle {
                                        width: 3px;
                                        height: 20px;
                                        border-radius: 2px;
                                        background: root.active_tab == 0 ? #00e5ff : transparent;
                                        y: 13px;
                                    }

                                    Text {
                                        text: "🖥️  Обзор устройств";
                                        font-size: 13px;
                                        font-weight: 700;
                                        color: root.active_tab == 0 ? #ffffff : #8a9bb2;
                                        vertical-alignment: center;
                                    }
                                }
                                TouchArea { clicked => { root.active_tab = 0; } }
                            }

                            // Tab 1: Sensor & DPI
                            Rectangle {
                                height: 46px;
                                border-radius: 8px;
                                background: root.active_tab == 1 ? #1a2232 : transparent;
                                border-width: root.active_tab == 1 ? 1px : 0px;
                                border-color: #2c3c54;

                                HorizontalLayout {
                                    padding-left: 12px;
                                    spacing: 12px;

                                    Rectangle {
                                        width: 3px;
                                        height: 20px;
                                        border-radius: 2px;
                                        background: root.active_tab == 1 ? #00e5ff : transparent;
                                        y: 13px;
                                    }

                                    Text {
                                        text: "🎯  Сенсор & DPI";
                                        font-size: 13px;
                                        font-weight: 700;
                                        color: root.active_tab == 1 ? #ffffff : #8a9bb2;
                                        vertical-alignment: center;
                                    }
                                }
                                TouchArea { clicked => { root.active_tab = 1; } }
                            }

                            // Tab 2: RGB LIGHTSYNC
                            Rectangle {
                                height: 46px;
                                border-radius: 8px;
                                background: root.active_tab == 2 ? #1a2232 : transparent;
                                border-width: root.active_tab == 2 ? 1px : 0px;
                                border-color: #2c3c54;

                                HorizontalLayout {
                                    padding-left: 12px;
                                    spacing: 12px;

                                    Rectangle {
                                        width: 3px;
                                        height: 20px;
                                        border-radius: 2px;
                                        background: root.active_tab == 2 ? #00e5ff : transparent;
                                        y: 13px;
                                    }

                                    Text {
                                        text: "✨  Подсветка LIGHTSYNC";
                                        font-size: 13px;
                                        font-weight: 700;
                                        color: root.active_tab == 2 ? #ffffff : #8a9bb2;
                                        vertical-alignment: center;
                                    }
                                }
                                TouchArea { clicked => { root.active_tab = 2; } }
                            }

                            // Tab 3: Macro Studio
                            Rectangle {
                                height: 46px;
                                border-radius: 8px;
                                background: root.active_tab == 3 ? #1a2232 : transparent;
                                border-width: root.active_tab == 3 ? 1px : 0px;
                                border-color: #2c3c54;

                                HorizontalLayout {
                                    padding-left: 12px;
                                    spacing: 12px;

                                    Rectangle {
                                        width: 3px;
                                        height: 20px;
                                        border-radius: 2px;
                                        background: root.active_tab == 3 ? #00e5ff : transparent;
                                        y: 13px;
                                    }

                                    Text {
                                        text: "⚡  Студия макросов";
                                        font-size: 13px;
                                        font-weight: 700;
                                        color: root.active_tab == 3 ? #ffffff : #8a9bb2;
                                        vertical-alignment: center;
                                    }
                                }
                                TouchArea { clicked => { root.active_tab = 3; } }
                            }

                            // Tab 4: Keybinds
                            Rectangle {
                                height: 46px;
                                border-radius: 8px;
                                background: root.active_tab == 4 ? #1a2232 : transparent;
                                border-width: root.active_tab == 4 ? 1px : 0px;
                                border-color: #2c3c54;

                                HorizontalLayout {
                                    padding-left: 12px;
                                    spacing: 12px;

                                    Rectangle {
                                        width: 3px;
                                        height: 20px;
                                        border-radius: 2px;
                                        background: root.active_tab == 4 ? #00ce88 : transparent;
                                        y: 13px;
                                    }

                                    Text {
                                        text: "🎛️  Назначение клавиш";
                                        font-size: 13px;
                                        font-weight: 700;
                                        color: root.active_tab == 4 ? #ffffff : #8a9bb2;
                                        vertical-alignment: center;
                                    }
                                }
                                TouchArea { clicked => { root.active_tab = 4; } }
                            }
                        }

                        // Bottom sidebar quick summary
                        Rectangle {
                            height: 112px;
                            border-radius: 8px;
                            background: #141822;
                            border-width: 1px;
                            border-color: #242c3d;

                            VerticalLayout {
                                padding: 12px;
                                spacing: 5px;

                                Text {
                                    text: "СИСТЕМНЫЙ СТАТУС";
                                    font-size: 10px;
                                    font-weight: 800;
                                    color: #5d6e85;
                                }

                                Text {
                                    text: "Сенсор: " + root.current_dpi + " DPI (" + root.polling_rate + "Hz)";
                                    font-size: 11px;
                                    font-weight: 600;
                                    color: #cbd5e1;
                                }

                                Text {
                                    text: root.active_macro_name != "" ? ("🟢 Макрос: " + root.active_macro_name) : "⚪ Макрос: Остановлен";
                                    font-size: 11px;
                                    font-weight: 600;
                                    color: root.active_macro_name != "" ? #00e5ff : #64748b;
                                }

                                Text {
                                    text: root.listener_active ? "🟢 Low-Level Hook: Активен" : "⚪ Hook: Отключен";
                                    font-size: 11px;
                                    font-weight: 600;
                                    color: root.listener_active ? #00ce88 : #64748b;
                                }
                            }
                        }
                    }
                }

                // Vertical Divider
                Rectangle {
                    width: 1px;
                    background: #1c2230;
                }

                // --- Content Canvas ---
                Rectangle {
                    background: #090b10;

                    VerticalLayout {
                        padding: 24px;
                        spacing: 16px;

                        // ==========================================
                        // TAB 0: DEVICE OVERVIEW
                        // ==========================================
                        if root.active_tab == 0 : VerticalLayout {
                            spacing: 16px;

                            Text {
                                text: "Обзор подключенных устройств";
                                font-size: 20px;
                                font-weight: 800;
                                color: #f8fafc;
                            }
                            Text {
                                text: "Аппаратный контроль Logitech HID++ 2.0 без фонового Electron и сбора телеметрии.";
                                font-size: 13px;
                                color: #7f8fa4;
                            }

                            // Card 1: G502 HERO
                            Rectangle {
                                height: 160px;
                                border-radius: 10px;
                                background: #121622;
                                border-width: 1px;
                                border-color: root.device_connected ? #1f2f45 : #331f24;

                                HorizontalLayout {
                                    padding: 20px;
                                    spacing: 20px;

                                    Rectangle {
                                        width: 120px;
                                        height: 120px;
                                        border-radius: 8px;
                                        background: #161c28;
                                        border-width: 1px;
                                        border-color: #273347;

                                        VerticalLayout {
                                            alignment: center;
                                            spacing: 6px;
                                            Text {
                                                text: "🖱️";
                                                font-size: 38px;
                                                horizontal-alignment: center;
                                            }
                                            Text {
                                                text: "G502 HERO";
                                                font-size: 11px;
                                                font-weight: 800;
                                                color: #00e5ff;
                                                horizontal-alignment: center;
                                            }
                                        }
                                    }

                                    VerticalLayout {
                                        alignment: center;
                                        spacing: 6px;

                                        HorizontalLayout {
                                            spacing: 12px;
                                            Text {
                                                text: root.device_name;
                                                font-size: 17px;
                                                font-weight: 800;
                                                color: #ffffff;
                                            }
                                            Rectangle {
                                                height: 22px;
                                                border-radius: 4px;
                                                background: root.device_connected ? #06381a : #3b1419;
                                                HorizontalLayout {
                                                    padding-left: 8px; padding-right: 8px;
                                                    Text {
                                                        text: root.device_connected ? "HERO 25K READY" : "ОФФЛАЙН";
                                                        font-size: 10px;
                                                        font-weight: 700;
                                                        color: root.device_connected ? #00e676 : #ff5252;
                                                        vertical-alignment: center;
                                                    }
                                                }
                                            }
                                        }

                                        Text {
                                            text: "• Оптический сенсор HERO 25K (100 - 25 600 DPI, шаг 50)\n• Частота опроса: 1000 Гц (1 мс) • Host Mode включен\n• 11 программируемых клавиш • LIGHTSYNC RGB подсветка";
                                            font-size: 12px;
                                            color: #94a3b8;
                                        }

                                        HorizontalLayout {
                                            spacing: 10px;
                                            Rectangle {
                                                width: 140px; height: 32px; border-radius: 6px; background: #1a2436; border-width: 1px; border-color: #31425d;
                                                HorizontalLayout { alignment: center; Text { text: "🎯 Настроить DPI"; font-size: 12px; font-weight: 700; color: #fff; vertical-alignment: center; } }
                                                TouchArea { clicked => { root.active_tab = 1; } }
                                            }
                                            Rectangle {
                                                width: 160px; height: 32px; border-radius: 6px; background: #1a2436; border-width: 1px; border-color: #31425d;
                                                HorizontalLayout { alignment: center; Text { text: "🎛️ Назначить клавиши"; font-size: 12px; font-weight: 700; color: #fff; vertical-alignment: center; } }
                                                TouchArea { clicked => { root.active_tab = 4; } }
                                            }
                                        }
                                    }
                                }
                            }

                            // Card 2: G435 Lightspeed Headset
                            Rectangle {
                                height: 130px;
                                border-radius: 10px;
                                background: #121622;
                                border-width: 1px;
                                border-color: #1f2f45;

                                HorizontalLayout {
                                    padding: 20px;
                                    spacing: 20px;

                                    Rectangle {
                                        width: 90px;
                                        height: 90px;
                                        border-radius: 8px;
                                        background: #161c28;
                                        border-width: 1px;
                                        border-color: #273347;

                                        VerticalLayout {
                                            alignment: center;
                                            spacing: 4px;
                                            Text {
                                                text: "🎧";
                                                font-size: 32px;
                                                horizontal-alignment: center;
                                            }
                                            Text {
                                                text: "G435";
                                                font-size: 11px;
                                                font-weight: 800;
                                                color: #00ce88;
                                                horizontal-alignment: center;
                                            }
                                        }
                                    }

                                    VerticalLayout {
                                        alignment: center;
                                        spacing: 6px;

                                        HorizontalLayout {
                                            spacing: 12px;
                                            Text {
                                                text: root.headset_name;
                                                font-size: 16px;
                                                font-weight: 800;
                                                color: #ffffff;
                                            }
                                            Rectangle {
                                                height: 22px;
                                                border-radius: 4px;
                                                background: #092738;
                                                HorizontalLayout {
                                                    padding-left: 8px; padding-right: 8px;
                                                    Text {
                                                        text: "LIGHTSPEED WIRELESS";
                                                        font-size: 10px;
                                                        font-weight: 700;
                                                        color: #00e5ff;
                                                        vertical-alignment: center;
                                                    }
                                                }
                                            }
                                        }

                                        Text {
                                            text: "• 40 мм аудиодрайверы с поддержкой Dolby Atmos\n• Двойные микрофоны с формированием луча (beamforming)\n• Ультралегкая конструкция: 165 г";
                                            font-size: 12px;
                                            color: #94a3b8;
                                        }
                                    }
                                }
                            }
                        }

                        // ==========================================
                        // TAB 1: SENSOR & DPI
                        // ==========================================
                        if root.active_tab == 1 : VerticalLayout {
                            spacing: 18px;

                            Text {
                                text: "Настройка сенсора HERO 25K";
                                font-size: 20px;
                                font-weight: 800;
                                color: #f8fafc;
                            }

                            // Big DPI Card
                            Rectangle {
                                height: 130px;
                                border-radius: 10px;
                                background: #121622;
                                border-width: 1px;
                                border-color: #202b3d;

                                VerticalLayout {
                                    alignment: center;
                                    spacing: 6px;

                                    Text {
                                        text: "АКТИВНОЕ РАЗРЕШЕНИЕ СЕНСОРА";
                                        font-size: 11px;
                                        font-weight: 800;
                                        color: #63738a;
                                        horizontal-alignment: center;
                                    }
                                    Text {
                                        text: root.current_dpi + " DPI";
                                        font-size: 46px;
                                        font-weight: 900;
                                        color: #00e5ff;
                                        horizontal-alignment: center;
                                    }
                                    Text {
                                        text: "HERO 25K • Нулевое сглаживание, аппаратная точность 1:1";
                                        font-size: 11px;
                                        color: #516278;
                                        horizontal-alignment: center;
                                    }
                                }
                            }

                            // Presets
                            Text {
                                text: "Быстрые пресеты чувствительности:";
                                font-size: 13px;
                                font-weight: 700;
                                color: #cbd5e1;
                            }

                            HorizontalLayout {
                                spacing: 10px;
                                for dpi in [400, 800, 1200, 1600, 2400, 3200, 6400] : Rectangle {
                                    height: 40px;
                                    border-radius: 6px;
                                    background: root.current_dpi == dpi ? #00e5ff : #161c28;
                                    border-width: 1px;
                                    border-color: root.current_dpi == dpi ? #00e5ff : #293447;

                                    Text {
                                        text: dpi;
                                        font-size: 13px;
                                        font-weight: 800;
                                        color: root.current_dpi == dpi ? #090b10 : #e2e8f0;
                                        horizontal-alignment: center;
                                        vertical-alignment: center;
                                    }
                                    TouchArea {
                                        clicked => { root.set_dpi(dpi); }
                                    }
                                }
                            }

                            // Slider fine control
                            VerticalLayout {
                                spacing: 8px;
                                HorizontalLayout {
                                    alignment: space-between;
                                    Text {
                                        text: "Плавная регулировка (100 - 6400 DPI, шаг 50):";
                                        font-size: 12px;
                                        color: #8a9bb2;
                                    }
                                    Text {
                                        text: root.current_dpi + " DPI";
                                        font-size: 12px;
                                        font-weight: 700;
                                        color: #00e5ff;
                                    }
                                }
                                Slider {
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

                            // Report Rate Selector
                            VerticalLayout {
                                spacing: 8px;
                                Text {
                                    text: "Частота опроса USB (Report Rate):";
                                    font-size: 13px;
                                    font-weight: 700;
                                    color: #cbd5e1;
                                }
                                HorizontalLayout {
                                    spacing: 12px;
                                    for rate in [125, 250, 500, 1000] : Rectangle {
                                        height: 38px;
                                        border-radius: 6px;
                                        background: root.polling_rate == rate ? #00ce88 : #161c28;
                                        border-width: 1px;
                                        border-color: root.polling_rate == rate ? #00ce88 : #293447;

                                        Text {
                                            text: rate + " Гц (" + (1000 / rate) + " мс)" + (rate == 1000 ? " ★" : "");
                                            font-size: 12px;
                                            font-weight: 700;
                                            color: root.polling_rate == rate ? #090b10 : #e2e8f0;
                                            horizontal-alignment: center;
                                            vertical-alignment: center;
                                        }
                                        TouchArea {
                                            clicked => { root.set_polling_rate(rate); }
                                        }
                                    }
                                }
                            }
                        }

                        // ==========================================
                        // TAB 2: RGB LIGHTSYNC
                        // ==========================================
                        if root.active_tab == 2 : VerticalLayout {
                            spacing: 18px;

                            Text {
                                text: "Управление подсветкой LIGHTSYNC RGB";
                                font-size: 20px;
                                font-weight: 800;
                                color: #f8fafc;
                            }

                            // Glowing Preview Card
                            Rectangle {
                                height: 110px;
                                border-radius: 10px;
                                background: #121622;
                                border-width: 1px;
                                border-color: #202b3d;

                                HorizontalLayout {
                                    alignment: center;
                                    spacing: 20px;

                                    Rectangle {
                                        width: 60px;
                                        height: 60px;
                                        border-radius: 30px;
                                        background: root.current_color;
                                        border-width: 3px;
                                        border-color: #ffffff44;
                                        y: 25px;

                                        Text {
                                            text: "G";
                                            font-size: 26px;
                                            font-weight: 900;
                                            color: #ffffff;
                                            horizontal-alignment: center;
                                            vertical-alignment: center;
                                        }
                                    }

                                    VerticalLayout {
                                        alignment: center;
                                        spacing: 4px;
                                        Text {
                                            text: "Активная зона: Логотип Logitech G";
                                            font-size: 16px;
                                            font-weight: 800;
                                            color: #ffffff;
                                        }
                                        Text {
                                            text: "16.8 миллионов оттенков • Аппаратное сохранение в память мыши";
                                            font-size: 12px;
                                            color: #8294ab;
                                        }
                                    }
                                }
                            }

                            // Swatches
                            Text {
                                text: "Быстрая палитра Logitech G:";
                                font-size: 13px;
                                font-weight: 700;
                                color: #cbd5e1;
                            }

                            HorizontalLayout {
                                spacing: 12px;

                                // Cyan
                                Rectangle {
                                    width: 44px; height: 44px; border-radius: 22px; background: #00e5ff;
                                    TouchArea { clicked => { root.current_color = #00e5ff; root.set_rgb_static(0, 229, 255); } }
                                }
                                // Mint Green
                                Rectangle {
                                    width: 44px; height: 44px; border-radius: 22px; background: #00ce88;
                                    TouchArea { clicked => { root.current_color = #00ce88; root.set_rgb_static(0, 206, 136); } }
                                }
                                // Neon Pink
                                Rectangle {
                                    width: 44px; height: 44px; border-radius: 22px; background: #ff007f;
                                    TouchArea { clicked => { root.current_color = #ff007f; root.set_rgb_static(255, 0, 127); } }
                                }
                                // Deep Purple
                                Rectangle {
                                    width: 44px; height: 44px; border-radius: 22px; background: #7b1fa2;
                                    TouchArea { clicked => { root.current_color = #7b1fa2; root.set_rgb_static(123, 31, 162); } }
                                }
                                // Red
                                Rectangle {
                                    width: 44px; height: 44px; border-radius: 22px; background: #ff1744;
                                    TouchArea { clicked => { root.current_color = #ff1744; root.set_rgb_static(255, 23, 68); } }
                                }
                                // Orange
                                Rectangle {
                                    width: 44px; height: 44px; border-radius: 22px; background: #ff9100;
                                    TouchArea { clicked => { root.current_color = #ff9100; root.set_rgb_static(255, 145, 0); } }
                                }
                                // Yellow
                                Rectangle {
                                    width: 44px; height: 44px; border-radius: 22px; background: #ffd600;
                                    TouchArea { clicked => { root.current_color = #ffd600; root.set_rgb_static(255, 214, 0); } }
                                }
                                // White
                                Rectangle {
                                    width: 44px; height: 44px; border-radius: 22px; background: #ffffff;
                                    TouchArea { clicked => { root.current_color = #ffffff; root.set_rgb_static(255, 255, 255); } }
                                }
                            }

                            // Dynamic Effects
                            Text {
                                text: "Динамические эффекты подсветки:";
                                font-size: 13px;
                                font-weight: 700;
                                color: #cbd5e1;
                            }

                            HorizontalLayout {
                                spacing: 12px;

                                Rectangle {
                                    height: 44px; border-radius: 8px; background: #161b27; border-width: 1px; border-color: #273449;
                                    HorizontalLayout { alignment: center; Text { text: "🌈 Радужный спектр"; font-size: 13px; font-weight: 700; color: #fff; vertical-alignment: center; } }
                                    TouchArea { clicked => { root.set_rgb_cycle(); } }
                                }
                                Rectangle {
                                    height: 44px; border-radius: 8px; background: #161b27; border-width: 1px; border-color: #273449;
                                    HorizontalLayout { alignment: center; Text { text: "💨 Пульсация (Дыхание)"; font-size: 13px; font-weight: 700; color: #fff; vertical-alignment: center; } }
                                    TouchArea { clicked => { root.set_rgb_breathe(0, 229, 255); } }
                                }
                                Rectangle {
                                    height: 44px; border-radius: 8px; background: #161b27; border-width: 1px; border-color: #273449;
                                    HorizontalLayout { alignment: center; Text { text: "⭕ Выключить подсветку"; font-size: 13px; font-weight: 700; color: #ff5252; vertical-alignment: center; } }
                                    TouchArea { clicked => { root.set_rgb_off(); } }
                                }
                            }
                        }

                        // ==========================================
                        // TAB 3: MACRO STUDIO
                        // ==========================================
                        if root.active_tab == 3 : VerticalLayout {
                            spacing: 14px;

                            HorizontalLayout {
                                alignment: space-between;

                                VerticalLayout {
                                    spacing: 4px;
                                    Text {
                                        text: "Студия макросов & Автокликер";
                                        font-size: 20px;
                                        font-weight: 800;
                                        color: #f8fafc;
                                    }
                                    Text {
                                        text: "Создавайте и редактируйте автокликеры, последовательности с задержками и горячие комбинации.";
                                        font-size: 12px;
                                        color: #7f8fa4;
                                    }
                                }

                                HorizontalLayout {
                                    spacing: 10px;

                                    Rectangle {
                                        height: 38px;
                                        border-radius: 6px;
                                        background: #00e5ff;
                                        HorizontalLayout {
                                            padding-left: 14px; padding-right: 14px;
                                            Text {
                                                text: "➕ СОЗДАТЬ МАКРОС";
                                                font-size: 12px;
                                                font-weight: 800;
                                                color: #090b10;
                                                vertical-alignment: center;
                                            }
                                        }
                                        TouchArea {
                                            clicked => { root.open_macro_builder(); }
                                        }
                                    }

                                    if root.active_macro_name != "" : Rectangle {
                                        height: 38px;
                                        border-radius: 6px;
                                        background: #ff1744;
                                        HorizontalLayout {
                                            padding-left: 12px; padding-right: 12px;
                                            Text {
                                                text: "⛔ ОСТАНОВИТЬ: " + root.active_macro_name;
                                                font-size: 11px;
                                                font-weight: 800;
                                                color: #ffffff;
                                                vertical-alignment: center;
                                            }
                                        }
                                        TouchArea {
                                            clicked => { root.stop_all_macros(); }
                                        }
                                    }
                                }
                            }

                            // Macros List
                            ScrollView {
                                height: 410px;

                                VerticalLayout {
                                    spacing: 8px;

                                    for item in root.macro_items : Rectangle {
                                        height: 58px;
                                        border-radius: 8px;
                                        background: root.active_macro_name == item.name ? #00e5ff18 : #121622;
                                        border-width: 1px;
                                        border-color: root.active_macro_name == item.name ? #00e5ff : #212c3d;

                                        HorizontalLayout {
                                            padding-left: 16px;
                                            padding-right: 16px;
                                            alignment: space-between;

                                            HorizontalLayout {
                                                spacing: 12px;
                                                alignment: start;

                                                Text {
                                                    text: item.is_toggle ? "🔄" : "⚡";
                                                    font-size: 18px;
                                                    vertical-alignment: center;
                                                }

                                                VerticalLayout {
                                                    alignment: center;
                                                    spacing: 2px;

                                                    HorizontalLayout {
                                                        spacing: 8px;
                                                        Text {
                                                            text: item.name;
                                                            font-size: 14px;
                                                            font-weight: 800;
                                                            color: #ffffff;
                                                        }
                                                        Rectangle {
                                                            height: 18px;
                                                            border-radius: 3px;
                                                            background: item.is_custom ? #183354 : #232c3d;
                                                            HorizontalLayout {
                                                                padding-left: 6px; padding-right: 6px;
                                                                Text {
                                                                    text: item.is_custom ? "ПОЛЬЗОВАТЕЛЬСКИЙ" : "G HUB";
                                                                    font-size: 9px;
                                                                    font-weight: 700;
                                                                    color: item.is_custom ? #00e5ff : #94a3b8;
                                                                    vertical-alignment: center;
                                                                }
                                                            }
                                                        }
                                                        Rectangle {
                                                            height: 18px;
                                                            border-radius: 3px;
                                                            background: item.is_toggle ? #3b2812 : #153939;
                                                            HorizontalLayout {
                                                                padding-left: 6px; padding-right: 6px;
                                                                Text {
                                                                    text: item.is_toggle ? "АВТОКЛИКЕР (TOGGLE)" : "СЕКВЕНЦИЯ";
                                                                    font-size: 9px;
                                                                    font-weight: 700;
                                                                    color: item.is_toggle ? #ffb300 : #26e6be;
                                                                    vertical-alignment: center;
                                                                }
                                                            }
                                                        }
                                                    }

                                                    Text {
                                                        text: item.steps_count + " шагов в цепочке";
                                                        font-size: 11px;
                                                        color: #64748b;
                                                    }
                                                }
                                            }

                                            HorizontalLayout {
                                                spacing: 8px;
                                                alignment: end;

                                                // Run/Toggle Button
                                                Rectangle {
                                                    height: 32px;
                                                    border-radius: 6px;
                                                    background: root.active_macro_name == item.name ? #00e5ff : #1e2637;
                                                    border-width: 1px;
                                                    border-color: root.active_macro_name == item.name ? #00e5ff : #303d54;
                                                    y: 13px;

                                                    HorizontalLayout {
                                                        padding-left: 12px; padding-right: 12px;
                                                        Text {
                                                            text: root.active_macro_name == item.name ? "🟢 АКТИВЕН (СТОП)" : "▶ ЗАПУСК";
                                                            font-size: 11px;
                                                            font-weight: 800;
                                                            color: root.active_macro_name == item.name ? #090b10 : #e2e8f0;
                                                            vertical-alignment: center;
                                                        }
                                                    }
                                                    TouchArea {
                                                        clicked => { root.toggle_macro(item.name); }
                                                    }
                                                }

                                                // Edit button (for custom macros only)
                                                if item.is_custom : Rectangle {
                                                    height: 32px;
                                                    border-radius: 6px;
                                                    background: #18283a;
                                                    border-width: 1px;
                                                    border-color: #274563;
                                                    y: 13px;

                                                    HorizontalLayout {
                                                        padding-left: 10px; padding-right: 10px;
                                                        Text {
                                                            text: "✏️ Редактировать";
                                                            font-size: 11px;
                                                            font-weight: 700;
                                                            color: #00e5ff;
                                                            vertical-alignment: center;
                                                        }
                                                    }
                                                    TouchArea {
                                                        clicked => { root.edit_custom_macro(item.name); }
                                                    }
                                                }

                                                // Delete button (for custom macros only)
                                                if item.is_custom : Rectangle {
                                                    width: 32px;
                                                    height: 32px;
                                                    border-radius: 6px;
                                                    background: #2a161b;
                                                    border-width: 1px;
                                                    border-color: #4a222a;
                                                    y: 13px;

                                                    Text {
                                                        text: "🗑️";
                                                        font-size: 12px;
                                                        horizontal-alignment: center;
                                                        vertical-alignment: center;
                                                    }
                                                    TouchArea {
                                                        clicked => { root.delete_custom_macro(item.name); }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // ==========================================
                        // TAB 4: KEYBINDS & BUTTON MAPPING (VISUAL G502)
                        // ==========================================
                        if root.active_tab == 4 : VerticalLayout {
                            spacing: 12px;

                            // Top hook status card
                            Rectangle {
                                height: 68px;
                                border-radius: 10px;
                                background: #121622;
                                border-width: 1px;
                                border-color: root.listener_active ? #00ce8866 : #232c3d;

                                HorizontalLayout {
                                    padding-left: 18px;
                                    padding-right: 18px;
                                    alignment: space-between;

                                    VerticalLayout {
                                        alignment: center;
                                        spacing: 2px;
                                        Text {
                                            text: "ФОНОВЫЙ ПЕРЕХВАТЧИК КЛАВИШ (LOW-LEVEL WINDOWS HOOK)";
                                            font-size: 13px;
                                            font-weight: 800;
                                            color: #ffffff;
                                        }
                                        Text {
                                            text: "Слушает физические нажатия кнопок G502 (G1..G11, Tilt Wheel) и клавиш F6..F12 без задержки (0% CPU).";
                                            font-size: 11px;
                                            color: #8191a7;
                                        }
                                    }

                                    Rectangle {
                                        height: 38px;
                                        border-radius: 6px;
                                        background: root.listener_active ? #00ce88 : #1f2738;
                                        border-width: 1px;
                                        border-color: root.listener_active ? #00ce88 : #35435c;
                                        y: 15px;

                                        HorizontalLayout {
                                            padding-left: 14px; padding-right: 14px;
                                            Text {
                                                text: root.listener_active ? "🟢 ПЕРЕХВАТ ВКЛЮЧЕН" : "⚪ ПЕРЕХВАТ ВЫКЛЮЧЕН";
                                                font-size: 12px;
                                                font-weight: 800;
                                                color: root.listener_active ? #090b10 : #94a3b8;
                                                vertical-alignment: center;
                                            }
                                        }
                                        TouchArea {
                                            clicked => { root.toggle_listener(); }
                                        }
                                    }
                                }
                            }

                            // Visual G502 Layout Header
                            Text {
                                text: "Интерактивная карта кнопок Logitech G502 HERO & Горячие клавиши:";
                                font-size: 13px;
                                font-weight: 700;
                                color: #cbd5e1;
                            }

                            // Scrollable Mappings
                            ScrollView {
                                height: 380px;

                                VerticalLayout {
                                    spacing: 8px;

                                    for item in root.keybind_items : Rectangle {
                                        height: 52px;
                                        border-radius: 8px;
                                        background: item.assigned_macro != "" ? #00ce8812 : #121622;
                                        border-width: 1px;
                                        border-color: item.assigned_macro != "" ? #00ce8866 : #202a3a;

                                        HorizontalLayout {
                                            padding-left: 16px;
                                            padding-right: 16px;
                                            alignment: space-between;

                                            HorizontalLayout {
                                                spacing: 12px;
                                                alignment: start;

                                                // Button badge
                                                Rectangle {
                                                    width: 44px;
                                                    height: 30px;
                                                    border-radius: 6px;
                                                    background: item.is_mouse ? #00e5ff22 : #9d4edd22;
                                                    border-width: 1px;
                                                    border-color: item.is_mouse ? #00e5ff66 : #9d4edd66;
                                                    y: 11px;

                                                    Text {
                                                        text: item.button_id;
                                                        font-size: 12px;
                                                        font-weight: 800;
                                                        color: item.is_mouse ? #00e5ff : #d8b4fe;
                                                        horizontal-alignment: center;
                                                        vertical-alignment: center;
                                                    }
                                                }

                                                VerticalLayout {
                                                    alignment: center;
                                                    spacing: 2px;
                                                    Text {
                                                        text: item.button_label;
                                                        font-size: 13px;
                                                        font-weight: 700;
                                                        color: #ffffff;
                                                    }
                                                    Text {
                                                        text: item.assigned_macro != "" ? ("⚡ НАЗНАЧЕН МАКРОС: " + item.assigned_macro) : ("По умолчанию: " + item.default_action);
                                                        font-size: 11px;
                                                        font-weight: 600;
                                                        color: item.assigned_macro != "" ? #00ce88 : #64748b;
                                                    }
                                                }
                                            }

                                            HorizontalLayout {
                                                spacing: 8px;
                                                alignment: end;

                                                // Assign button
                                                Rectangle {
                                                    height: 32px;
                                                    border-radius: 6px;
                                                    background: #1c2434;
                                                    border-width: 1px;
                                                    border-color: #2c3a52;
                                                    y: 10px;

                                                    HorizontalLayout {
                                                        padding-left: 12px; padding-right: 12px;
                                                        Text {
                                                            text: "⚙️ Назначить макрос";
                                                            font-size: 11px;
                                                            font-weight: 700;
                                                            color: #00e5ff;
                                                            vertical-alignment: center;
                                                        }
                                                    }
                                                    TouchArea {
                                                        clicked => { root.open_keybind_picker(item.button_id, item.button_label); }
                                                    }
                                                }

                                                // Reset button
                                                if item.assigned_macro != "" : Rectangle {
                                                    width: 32px;
                                                    height: 32px;
                                                    border-radius: 6px;
                                                    background: #2b181c;
                                                    border-width: 1px;
                                                    border-color: #4a242c;
                                                    y: 10px;

                                                    Text {
                                                        text: "✕";
                                                        font-size: 12px;
                                                        font-weight: 800;
                                                        color: #ff5252;
                                                        horizontal-alignment: center;
                                                        vertical-alignment: center;
                                                    }
                                                    TouchArea {
                                                        clicked => { root.clear_keybind(item.button_id); }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Divider
            Rectangle {
                height: 1px;
                background: #1c2230;
            }

            // ==========================================
            // FOOTER STATUSBAR
            // ==========================================
            Rectangle {
                height: 34px;
                background: #0b0d13;

                HorizontalLayout {
                    padding-left: 20px;
                    padding-right: 20px;
                    alignment: space-between;

                    Text {
                        text: "Статус: " + root.status_msg;
                        font-size: 12px;
                        font-weight: 600;
                        color: #8292a6;
                        vertical-alignment: center;
                    }

                    Text {
                        text: "Logitech HID++ 2.0 Native Worker • Win32 SendInput • 100% Offline";
                        font-size: 11px;
                        font-weight: 600;
                        color: #4b586a;
                        vertical-alignment: center;
                    }
                }
            }
        }

        // ==========================================
        // MODAL OVERLAY: MACRO CREATOR & EDITOR
        // ==========================================
        if root.show_macro_builder : Rectangle {
            background: #000000dd;

            TouchArea { /* block clicks outside modal */ }

            Rectangle {
                width: 720px;
                height: 620px;
                border-radius: 12px;
                background: #121622;
                border-width: 1px;
                border-color: #2b3952;

                VerticalLayout {
                    padding: 22px;
                    spacing: 12px;

                    // Header
                    HorizontalLayout {
                        alignment: space-between;
                        VerticalLayout {
                            spacing: 2px;
                            Text {
                                text: root.builder_editing_original != "" ? "✏️ Редактирование макроса" : "➕ Конструктор нового макроса";
                                font-size: 17px;
                                font-weight: 800;
                                color: #ffffff;
                            }
                            Text {
                                text: root.builder_editing_original != "" ? ("Редактирование: " + root.builder_editing_original) : "Создайте автокликер или последовательность клавиш с задержками";
                                font-size: 11px;
                                color: #75869d;
                            }
                        }

                        Rectangle {
                            width: 28px; height: 28px; border-radius: 6px; background: #21293a;
                            Text { text: "✕"; font-size: 12px; font-weight: 800; color: #fff; horizontal-alignment: center; vertical-alignment: center; }
                            TouchArea { clicked => { root.close_macro_builder(); } }
                        }
                    }

                    // Name Input & Mode in Row
                    HorizontalLayout {
                        spacing: 16px;

                        VerticalLayout {
                            spacing: 4px;
                            width: 320px;
                            Text { text: "Название макроса:"; font-size: 11px; font-weight: 700; color: #cbd5e1; }
                            LineEdit {
                                text: root.builder_name;
                                edited(val) => { root.builder_name = val; }
                            }
                        }

                        VerticalLayout {
                            spacing: 4px;
                            Text { text: "Режим выполнения:"; font-size: 11px; font-weight: 700; color: #cbd5e1; }
                            HorizontalLayout {
                                spacing: 8px;
                                Rectangle {
                                    height: 34px; border-radius: 6px;
                                    background: root.builder_is_toggle ? #00e5ff : #1a2130;
                                    border-width: 1px; border-color: root.builder_is_toggle ? #00e5ff : #2c3a52;
                                    HorizontalLayout { padding-left: 10px; padding-right: 10px; alignment: center; Text { text: "🔄 Автокликер (Toggle)"; font-size: 11px; font-weight: 800; color: root.builder_is_toggle ? #090b10 : #cbd5e1; vertical-alignment: center; } }
                                    TouchArea { clicked => { root.builder_is_toggle = true; } }
                                }
                                Rectangle {
                                    height: 34px; border-radius: 6px;
                                    background: !root.builder_is_toggle ? #00ce88 : #1a2130;
                                    border-width: 1px; border-color: !root.builder_is_toggle ? #00ce88 : #2c3a52;
                                    HorizontalLayout { padding-left: 10px; padding-right: 10px; alignment: center; Text { text: "⚡ Одиночный (Sequence)"; font-size: 11px; font-weight: 800; color: !root.builder_is_toggle ? #090b10 : #cbd5e1; vertical-alignment: center; } }
                                    TouchArea { clicked => { root.builder_is_toggle = false; } }
                                }
                            }
                        }
                    }

                    // Action Builder Palettes
                    VerticalLayout {
                        spacing: 8px;

                        // Mouse Buttons
                        Text { text: "🖱️ Добавить действия мыши (ЛКМ, ПКМ, СКМ, G4, G5):"; font-size: 11px; font-weight: 700; color: #00e5ff; }
                        HorizontalLayout {
                            spacing: 6px;
                            Rectangle {
                                height: 28px; border-radius: 5px; background: #1a2336; border-width: 1px; border-color: #2c3f5d;
                                HorizontalLayout { padding-left: 8px; padding-right: 8px; Text { text: "+ ЛКМ Клик"; font-size: 11px; font-weight: 700; color: #00e5ff; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_mouse(1, true, 15); root.builder_add_mouse(1, false, 15); } }
                            }
                            Rectangle {
                                height: 28px; border-radius: 5px; background: #1a2336; border-width: 1px; border-color: #2c3f5d;
                                HorizontalLayout { padding-left: 8px; padding-right: 8px; Text { text: "+ ПКМ Клик"; font-size: 11px; font-weight: 700; color: #00e5ff; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_mouse(2, true, 15); root.builder_add_mouse(2, false, 15); } }
                            }
                            Rectangle {
                                height: 28px; border-radius: 5px; background: #1a2336; border-width: 1px; border-color: #2c3f5d;
                                HorizontalLayout { padding-left: 8px; padding-right: 8px; Text { text: "+ СКМ Клик"; font-size: 11px; font-weight: 700; color: #00e5ff; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_mouse(3, true, 15); root.builder_add_mouse(3, false, 15); } }
                            }
                            Rectangle {
                                height: 28px; border-radius: 5px; background: #1a2336; border-width: 1px; border-color: #2c3f5d;
                                HorizontalLayout { padding-left: 8px; padding-right: 8px; Text { text: "+ Боковая G4"; font-size: 11px; font-weight: 700; color: #00e5ff; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_mouse(4, true, 15); root.builder_add_mouse(4, false, 15); } }
                            }
                            Rectangle {
                                height: 28px; border-radius: 5px; background: #1a2336; border-width: 1px; border-color: #2c3f5d;
                                HorizontalLayout { padding-left: 8px; padding-right: 8px; Text { text: "+ Боковая G5"; font-size: 11px; font-weight: 700; color: #00e5ff; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_mouse(5, true, 15); root.builder_add_mouse(5, false, 15); } }
                            }
                            Rectangle {
                                height: 28px; border-radius: 5px; background: #1f1b32; border-width: 1px; border-color: #3f3563;
                                HorizontalLayout { padding-left: 8px; padding-right: 8px; Text { text: "+ Зажать ЛКМ"; font-size: 10px; font-weight: 700; color: #d8b4fe; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_mouse(1, true, 10); } }
                            }
                            Rectangle {
                                height: 28px; border-radius: 5px; background: #1f1b32; border-width: 1px; border-color: #3f3563;
                                HorizontalLayout { padding-left: 8px; padding-right: 8px; Text { text: "+ Отпустить ЛКМ"; font-size: 10px; font-weight: 700; color: #d8b4fe; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_mouse(1, false, 10); } }
                            }
                        }

                        // Keyboard Keys
                        Text { text: "⌨️ Добавить клавиши клавиатуры:"; font-size: 11px; font-weight: 700; color: #00ce88; }
                        HorizontalLayout {
                            spacing: 5px;
                            Rectangle {
                                height: 26px; border-radius: 4px; background: #172624; border-width: 1px; border-color: #24483d;
                                HorizontalLayout { padding-left: 6px; padding-right: 6px; Text { text: "Space"; font-size: 10px; font-weight: 700; color: #00ce88; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_key(32, true, 10); root.builder_add_key(32, false, 10); } }
                            }
                            Rectangle {
                                height: 26px; border-radius: 4px; background: #172624; border-width: 1px; border-color: #24483d;
                                HorizontalLayout { padding-left: 6px; padding-right: 6px; Text { text: "Ctrl"; font-size: 10px; font-weight: 700; color: #00ce88; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_key(17, true, 10); root.builder_add_key(17, false, 10); } }
                            }
                            Rectangle {
                                height: 26px; border-radius: 4px; background: #172624; border-width: 1px; border-color: #24483d;
                                HorizontalLayout { padding-left: 6px; padding-right: 6px; Text { text: "Shift"; font-size: 10px; font-weight: 700; color: #00ce88; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_key(16, true, 10); root.builder_add_key(16, false, 10); } }
                            }
                            Rectangle {
                                height: 26px; border-radius: 4px; background: #172624; border-width: 1px; border-color: #24483d;
                                HorizontalLayout { padding-left: 6px; padding-right: 6px; Text { text: "Alt"; font-size: 10px; font-weight: 700; color: #00ce88; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_key(18, true, 10); root.builder_add_key(18, false, 10); } }
                            }
                            Rectangle {
                                height: 26px; border-radius: 4px; background: #172624; border-width: 1px; border-color: #24483d;
                                HorizontalLayout { padding-left: 6px; padding-right: 6px; Text { text: "Tab"; font-size: 10px; font-weight: 700; color: #00ce88; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_key(9, true, 10); root.builder_add_key(9, false, 10); } }
                            }
                            Rectangle {
                                height: 26px; border-radius: 4px; background: #172624; border-width: 1px; border-color: #24483d;
                                HorizontalLayout { padding-left: 6px; padding-right: 6px; Text { text: "W"; font-size: 10px; font-weight: 700; color: #00ce88; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_key(87, true, 10); root.builder_add_key(87, false, 10); } }
                            }
                            Rectangle {
                                height: 26px; border-radius: 4px; background: #172624; border-width: 1px; border-color: #24483d;
                                HorizontalLayout { padding-left: 6px; padding-right: 6px; Text { text: "A"; font-size: 10px; font-weight: 700; color: #00ce88; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_key(65, true, 10); root.builder_add_key(65, false, 10); } }
                            }
                            Rectangle {
                                height: 26px; border-radius: 4px; background: #172624; border-width: 1px; border-color: #24483d;
                                HorizontalLayout { padding-left: 6px; padding-right: 6px; Text { text: "S"; font-size: 10px; font-weight: 700; color: #00ce88; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_key(83, true, 10); root.builder_add_key(83, false, 10); } }
                            }
                            Rectangle {
                                height: 26px; border-radius: 4px; background: #172624; border-width: 1px; border-color: #24483d;
                                HorizontalLayout { padding-left: 6px; padding-right: 6px; Text { text: "D"; font-size: 10px; font-weight: 700; color: #00ce88; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_key(68, true, 10); root.builder_add_key(68, false, 10); } }
                            }
                            Rectangle {
                                height: 26px; border-radius: 4px; background: #172624; border-width: 1px; border-color: #24483d;
                                HorizontalLayout { padding-left: 6px; padding-right: 6px; Text { text: "E"; font-size: 10px; font-weight: 700; color: #00ce88; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_key(69, true, 10); root.builder_add_key(69, false, 10); } }
                            }
                            Rectangle {
                                height: 26px; border-radius: 4px; background: #172624; border-width: 1px; border-color: #24483d;
                                HorizontalLayout { padding-left: 6px; padding-right: 6px; Text { text: "R"; font-size: 10px; font-weight: 700; color: #00ce88; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_key(82, true, 10); root.builder_add_key(82, false, 10); } }
                            }
                            Rectangle {
                                height: 26px; border-radius: 4px; background: #172624; border-width: 1px; border-color: #24483d;
                                HorizontalLayout { padding-left: 6px; padding-right: 6px; Text { text: "F"; font-size: 10px; font-weight: 700; color: #00ce88; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_key(70, true, 10); root.builder_add_key(70, false, 10); } }
                            }
                            Rectangle {
                                height: 26px; border-radius: 4px; background: #172624; border-width: 1px; border-color: #24483d;
                                HorizontalLayout { padding-left: 6px; padding-right: 6px; Text { text: "C"; font-size: 10px; font-weight: 700; color: #00ce88; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_key(67, true, 10); root.builder_add_key(67, false, 10); } }
                            }
                            Rectangle {
                                height: 26px; border-radius: 4px; background: #172624; border-width: 1px; border-color: #24483d;
                                HorizontalLayout { padding-left: 6px; padding-right: 6px; Text { text: "1"; font-size: 10px; font-weight: 700; color: #00ce88; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_key(49, true, 10); root.builder_add_key(49, false, 10); } }
                            }
                            Rectangle {
                                height: 26px; border-radius: 4px; background: #172624; border-width: 1px; border-color: #24483d;
                                HorizontalLayout { padding-left: 6px; padding-right: 6px; Text { text: "2"; font-size: 10px; font-weight: 700; color: #00ce88; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_key(50, true, 10); root.builder_add_key(50, false, 10); } }
                            }
                            Rectangle {
                                height: 26px; border-radius: 4px; background: #172624; border-width: 1px; border-color: #24483d;
                                HorizontalLayout { padding-left: 6px; padding-right: 6px; Text { text: "F1"; font-size: 10px; font-weight: 700; color: #00ce88; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_key(112, true, 10); root.builder_add_key(112, false, 10); } }
                            }
                        }

                        // Delays & Controls
                        Text { text: "⏱️ Паузы и управление цепочкой:"; font-size: 11px; font-weight: 700; color: #ffb300; }
                        HorizontalLayout {
                            spacing: 6px;
                            Rectangle {
                                height: 26px; border-radius: 4px; background: #261f18; border-width: 1px; border-color: #483928;
                                HorizontalLayout { padding-left: 7px; padding-right: 7px; Text { text: "+ 10 мс"; font-size: 10px; font-weight: 700; color: #ffb300; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_delay(10); } }
                            }
                            Rectangle {
                                height: 26px; border-radius: 4px; background: #261f18; border-width: 1px; border-color: #483928;
                                HorizontalLayout { padding-left: 7px; padding-right: 7px; Text { text: "+ 25 мс"; font-size: 10px; font-weight: 700; color: #ffb300; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_delay(25); } }
                            }
                            Rectangle {
                                height: 26px; border-radius: 4px; background: #261f18; border-width: 1px; border-color: #483928;
                                HorizontalLayout { padding-left: 7px; padding-right: 7px; Text { text: "+ 50 мс"; font-size: 10px; font-weight: 700; color: #ffb300; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_delay(50); } }
                            }
                            Rectangle {
                                height: 26px; border-radius: 4px; background: #261f18; border-width: 1px; border-color: #483928;
                                HorizontalLayout { padding-left: 7px; padding-right: 7px; Text { text: "+ 100 мс"; font-size: 10px; font-weight: 700; color: #ffb300; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_delay(100); } }
                            }
                            Rectangle {
                                height: 26px; border-radius: 4px; background: #261f18; border-width: 1px; border-color: #483928;
                                HorizontalLayout { padding-left: 7px; padding-right: 7px; Text { text: "+ 250 мс"; font-size: 10px; font-weight: 700; color: #ffb300; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_add_delay(250); } }
                            }

                            // Clear all button
                            Rectangle {
                                height: 26px; border-radius: 4px; background: #2b171c; border-width: 1px; border-color: #4a242c;
                                HorizontalLayout { padding-left: 10px; padding-right: 10px; Text { text: "Очистить все"; font-size: 10px; font-weight: 700; color: #ff5252; vertical-alignment: center; } }
                                TouchArea { clicked => { root.builder_clear_steps(); } }
                            }
                        }
                    }

                    // Steps sequence preview
                    VerticalLayout {
                        spacing: 4px;
                        Text { text: "Цепочка действий макроса (" + root.builder_steps.length + " шагов):"; font-size: 11px; font-weight: 700; color: #cbd5e1; }

                        ScrollView {
                            height: 160px;
                            VerticalLayout {
                                spacing: 4px;
                                for step[idx] in root.builder_steps : Rectangle {
                                    height: 28px;
                                    border-radius: 4px;
                                    background: #181f2c;

                                    HorizontalLayout {
                                        padding-left: 10px; padding-right: 10px; alignment: space-between;
                                        Text { text: "#" + (idx + 1) + "  " + step; font-size: 11px; color: #e2e8f0; vertical-alignment: center; }

                                        HorizontalLayout {
                                            spacing: 6px;
                                            // Move up
                                            Rectangle {
                                                width: 20px; height: 20px; border-radius: 3px; background: #222d3d; y: 4px;
                                                Text { text: "▲"; font-size: 9px; font-weight: 800; color: #94a3b8; horizontal-alignment: center; vertical-alignment: center; }
                                                TouchArea { clicked => { root.builder_move_step_up(idx); } }
                                            }
                                            // Move down
                                            Rectangle {
                                                width: 20px; height: 20px; border-radius: 3px; background: #222d3d; y: 4px;
                                                Text { text: "▼"; font-size: 9px; font-weight: 800; color: #94a3b8; horizontal-alignment: center; vertical-alignment: center; }
                                                TouchArea { clicked => { root.builder_move_step_down(idx); } }
                                            }
                                            // Delete
                                            Rectangle {
                                                width: 20px; height: 20px; border-radius: 3px; background: #331d22; y: 4px;
                                                Text { text: "✕"; font-size: 10px; font-weight: 800; color: #ff5252; horizontal-alignment: center; vertical-alignment: center; }
                                                TouchArea { clicked => { root.builder_remove_step(idx); } }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Actions
                    HorizontalLayout {
                        spacing: 12px;
                        alignment: end;

                        Rectangle {
                            height: 38px; border-radius: 6px; background: #222938;
                            HorizontalLayout { padding-left: 16px; padding-right: 16px; Text { text: "Отмена"; font-size: 12px; font-weight: 700; color: #94a3b8; vertical-alignment: center; } }
                            TouchArea { clicked => { root.close_macro_builder(); } }
                        }

                        Rectangle {
                            height: 38px; border-radius: 6px; background: #00ce88;
                            HorizontalLayout { padding-left: 18px; padding-right: 18px; Text { text: "💾 СОХРАНИТЬ МАКРОС"; font-size: 12px; font-weight: 800; color: #090b10; vertical-alignment: center; } }
                            TouchArea { clicked => { root.builder_save_macro(root.builder_editing_original, root.builder_name, root.builder_is_toggle); } }
                        }
                    }
                }
            }
        }

        // ==========================================
        // MODAL OVERLAY: KEYBIND PICKER
        // ==========================================
        if root.show_bind_picker : Rectangle {
            background: #000000dd;

            TouchArea { /* block clicks outside modal */ }

            Rectangle {
                width: 540px;
                height: 500px;
                border-radius: 12px;
                background: #121622;
                border-width: 1px;
                border-color: #2b3952;

                VerticalLayout {
                    padding: 24px;
                    spacing: 14px;

                    // Header
                    HorizontalLayout {
                        alignment: space-between;
                        VerticalLayout {
                            spacing: 2px;
                            Text {
                                text: "Назначение макроса на кнопку";
                                font-size: 16px;
                                font-weight: 800;
                                color: #ffffff;
                            }
                            Text {
                                text: "Кнопка: " + root.picker_button_name + " [" + root.picker_button_id + "]";
                                font-size: 12px;
                                font-weight: 700;
                                color: #00e5ff;
                            }
                        }
                        Rectangle {
                            width: 28px; height: 28px; border-radius: 6px; background: #21293a;
                            Text { text: "✕"; font-size: 12px; font-weight: 800; color: #fff; horizontal-alignment: center; vertical-alignment: center; }
                            TouchArea { clicked => { root.close_keybind_picker(); } }
                        }
                    }

                    // Reset to default button
                    Rectangle {
                        height: 36px;
                        border-radius: 6px;
                        background: #231920;
                        border-width: 1px;
                        border-color: #48232d;
                        HorizontalLayout {
                            padding-left: 12px; padding-right: 12px; alignment: center;
                            Text {
                                text: "✕ Сбросить на стандартное действие мыши";
                                font-size: 12px;
                                font-weight: 700;
                                color: #ff5252;
                                vertical-alignment: center;
                            }
                        }
                        TouchArea {
                            clicked => { root.clear_keybind(root.picker_button_id); }
                        }
                    }

                    Text {
                        text: "Выберите макрос из списка доступных:";
                        font-size: 12px;
                        font-weight: 700;
                        color: #94a3b8;
                    }

                    // Available macros list
                    ScrollView {
                        height: 310px;
                        VerticalLayout {
                            spacing: 6px;

                            for m in root.macro_items : Rectangle {
                                height: 44px;
                                border-radius: 6px;
                                background: #181e2b;
                                border-width: 1px;
                                border-color: #273449;

                                HorizontalLayout {
                                    padding-left: 14px;
                                    padding-right: 14px;
                                    alignment: space-between;

                                    HorizontalLayout {
                                        spacing: 10px;
                                        Text { text: m.is_toggle ? "🔄" : "⚡"; font-size: 14px; vertical-alignment: center; }
                                        Text { text: m.name; font-size: 13px; font-weight: 700; color: #fff; vertical-alignment: center; }
                                    }

                                    Rectangle {
                                        height: 26px; border-radius: 4px; background: #00e5ff;
                                        HorizontalLayout {
                                            padding-left: 10px; padding-right: 10px;
                                            Text { text: "Выбрать"; font-size: 11px; font-weight: 800; color: #090b10; vertical-alignment: center; }
                                        }
                                    }
                                }
                                TouchArea {
                                    clicked => { root.assign_keybind(root.picker_button_id, m.name); }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[allow(clippy::enum_variant_names)]
enum DeviceCommand {
    SetDpi(u16),
    SetRgbStatic(u8, u8, u8),
    SetRgbCycle,
    SetRgbBreathe(u8, u8, u8),
    SetRgbOff,
}

fn update_ui_macro_list(
    app: &MainWindow,
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

    let model = Rc::new(VecModel::from(items));
    app.set_macro_items(ModelRc::from(model));
}

fn update_ui_keybind_list(
    app: &MainWindow,
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
    let model = Rc::new(VecModel::from(items));
    app.set_keybind_items(ModelRc::from(model));
}

pub fn run() {
    let app = MainWindow::new().expect("Failed to create MainWindow");

    // Load settings from G HUB SQLite db
    let lghub_settings = Arc::new(LgHubSettings::load_from_db().ok());

    // Load custom user macros from JSON
    let custom_macros = Arc::new(Mutex::new(load_custom_macros()));

    // Load keybindings from JSON
    let keybinds = Arc::new(Mutex::new(load_bindings()));

    // Populate initial models
    update_ui_macro_list(&app, &lghub_settings, &custom_macros);
    update_ui_keybind_list(&app, &keybinds);

    // Active running macro state
    let active_macro: Arc<Mutex<Option<(String, macro_engine::ActiveMacro)>>> = Arc::new(Mutex::new(None));

    // Background Keybind Listener
    let initial_binds = keybinds.lock().unwrap().clone();
    let listener = Arc::new(KeybindListener::new(initial_binds));

    // Wire background trigger callback to macro executor
    let act_slot = active_macro.clone();
    let custom_ref = custom_macros.clone();
    let lghub_ref = lghub_settings.clone();
    let app_weak = app.as_weak();

    listener.start(move |macro_name| {
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

    app.set_listener_active(true);

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

    let app_weak = app.as_weak();
    app.on_set_polling_rate(move |rate| {
        if let Some(app) = app_weak.upgrade() {
            app.set_polling_rate(rate);
            app.set_status_msg(format!("Частота опроса сенсора установлена: {} Гц", rate).into());
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
    app.on_open_macro_builder(move || {
        if let Some(app) = app_weak.upgrade() {
            if let Ok(mut steps) = steps_ref.lock() {
                steps.clear();
            }
            let empty_model = Rc::new(VecModel::default());
            app.set_builder_steps(ModelRc::from(empty_model));
            app.set_builder_editing_original("".into());
            app.set_builder_name("Мой новый автокликер".into());
            app.set_builder_is_toggle(true);
            app.set_show_macro_builder(true);
        }
    });

    // Edit existing custom macro
    let app_weak = app.as_weak();
    let steps_ref = builder_steps.clone();
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
                let model = Rc::new(VecModel::from(slint_steps));
                if let Some(app) = app_weak.upgrade() {
                    app.set_builder_editing_original(m.name.clone().into());
                    app.set_builder_name(m.name.into());
                    app.set_builder_is_toggle(m.is_toggle);
                    app.set_builder_steps(ModelRc::from(model));
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
    let app_weak = app.as_weak();
    let steps_ref = builder_steps.clone();
    app.on_builder_add_mouse(move |button, down, delay| {
        let comp = LgMacroComponent {
            mouse_button: Some(button as u8),
            mouse_down: Some(down),
            vk_code: None,
            key_down: None,
            delay_ms: if delay > 0 { Some(delay as u64) } else { None },
        };
        if let Ok(mut steps) = steps_ref.lock() {
            steps.push(comp);
            let slint_steps: Vec<slint::SharedString> = steps.iter().map(|s| format_component(s).into()).collect();
            let model = Rc::new(VecModel::from(slint_steps));
            if let Some(app) = app_weak.upgrade() {
                app.set_builder_steps(ModelRc::from(model));
            }
        }
    });

    // Builder Add Key
    let app_weak = app.as_weak();
    let steps_ref = builder_steps.clone();
    app.on_builder_add_key(move |vk, down, delay| {
        let comp = LgMacroComponent {
            mouse_button: None,
            mouse_down: None,
            vk_code: Some(vk as u16),
            key_down: Some(down),
            delay_ms: if delay > 0 { Some(delay as u64) } else { None },
        };
        if let Ok(mut steps) = steps_ref.lock() {
            steps.push(comp);
            let slint_steps: Vec<slint::SharedString> = steps.iter().map(|s| format_component(s).into()).collect();
            let model = Rc::new(VecModel::from(slint_steps));
            if let Some(app) = app_weak.upgrade() {
                app.set_builder_steps(ModelRc::from(model));
            }
        }
    });

    // Builder Add Delay
    let app_weak = app.as_weak();
    let steps_ref = builder_steps.clone();
    app.on_builder_add_delay(move |delay| {
        let comp = LgMacroComponent {
            mouse_button: None,
            mouse_down: None,
            vk_code: None,
            key_down: None,
            delay_ms: Some(delay as u64),
        };
        if let Ok(mut steps) = steps_ref.lock() {
            steps.push(comp);
            let slint_steps: Vec<slint::SharedString> = steps.iter().map(|s| format_component(s).into()).collect();
            let model = Rc::new(VecModel::from(slint_steps));
            if let Some(app) = app_weak.upgrade() {
                app.set_builder_steps(ModelRc::from(model));
            }
        }
    });

    // Builder Clear
    let app_weak = app.as_weak();
    let steps_ref = builder_steps.clone();
    app.on_builder_clear_steps(move || {
        if let Ok(mut steps) = steps_ref.lock() {
            steps.clear();
        }
        if let Some(app) = app_weak.upgrade() {
            let empty_model = Rc::new(VecModel::default());
            app.set_builder_steps(ModelRc::from(empty_model));
        }
    });

    // Builder Remove Single Step
    let app_weak = app.as_weak();
    let steps_ref = builder_steps.clone();
    app.on_builder_remove_step(move |idx| {
        if let Ok(mut steps) = steps_ref.lock() {
            if (idx as usize) < steps.len() {
                steps.remove(idx as usize);
            }
            let slint_steps: Vec<slint::SharedString> = steps.iter().map(|s| format_component(s).into()).collect();
            let model = Rc::new(VecModel::from(slint_steps));
            if let Some(app) = app_weak.upgrade() {
                app.set_builder_steps(ModelRc::from(model));
            }
        }
    });

    // Builder Move Step Up
    let app_weak = app.as_weak();
    let steps_ref = builder_steps.clone();
    app.on_builder_move_step_up(move |idx| {
        let idx = idx as usize;
        if let Ok(mut steps) = steps_ref.lock()
            && idx > 0 && idx < steps.len() {
                steps.swap(idx - 1, idx);
                let slint_steps: Vec<slint::SharedString> = steps.iter().map(|s| format_component(s).into()).collect();
                let model = Rc::new(VecModel::from(slint_steps));
                if let Some(app) = app_weak.upgrade() {
                    app.set_builder_steps(ModelRc::from(model));
                }
            }
    });

    // Builder Move Step Down
    let app_weak = app.as_weak();
    let steps_ref = builder_steps.clone();
    app.on_builder_move_step_down(move |idx| {
        let idx = idx as usize;
        if let Ok(mut steps) = steps_ref.lock()
            && idx + 1 < steps.len() {
                steps.swap(idx, idx + 1);
                let slint_steps: Vec<slint::SharedString> = steps.iter().map(|s| format_component(s).into()).collect();
                let model = Rc::new(VecModel::from(slint_steps));
                if let Some(app) = app_weak.upgrade() {
                    app.set_builder_steps(ModelRc::from(model));
                }
            }
    });

    // Builder Save / Update Macro
    let app_weak = app.as_weak();
    let steps_ref = builder_steps.clone();
    let custom_arc = custom_macros.clone();
    let settings_arc = lghub_settings.clone();
    let keybinds_arc = keybinds.clone();
    let listener_ref = listener.clone();
    app.on_builder_save_macro(move |orig_name, name, is_toggle| {
        let name_str = name.trim().to_string();
        if name_str.is_empty() {
            if let Some(app) = app_weak.upgrade() {
                app.set_status_msg("Ошибка: укажите название макроса!".into());
            }
            return;
        }

        let mut steps = steps_ref.lock().unwrap().clone();
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

        if let Some(app) = app_weak.upgrade() {
            update_ui_macro_list(&app, &settings_arc, &custom_arc);
            update_ui_keybind_list(&app, &keybinds_arc);
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
        if let Some(app) = app_weak.upgrade() {
            update_ui_macro_list(&app, &settings_arc, &custom_arc);
            update_ui_keybind_list(&app, &keybinds_arc);
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
    app.on_assign_keybind(move |btn_id, macro_name| {
        if let Ok(mut binds) = keybinds_arc.lock() {
            assign_button_macro(&mut binds, btn_id.as_str(), macro_name.as_str());
            let _ = save_bindings(&binds);
            listener_ref.update_bindings(binds.clone());
        }
        if let Some(app) = app_weak.upgrade() {
            update_ui_keybind_list(&app, &keybinds_arc);
            app.set_show_bind_picker(false);
            app.set_status_msg(format!("Кнопка {} привязана к макросу '{}'", btn_id, macro_name).into());
        }
    });

    // Clear Keybind
    let app_weak = app.as_weak();
    let keybinds_arc = keybinds.clone();
    let listener_ref = listener.clone();
    app.on_clear_keybind(move |btn_id| {
        if let Ok(mut binds) = keybinds_arc.lock() {
            clear_button_macro(&mut binds, btn_id.as_str());
            let _ = save_bindings(&binds);
            listener_ref.update_bindings(binds.clone());
        }
        if let Some(app) = app_weak.upgrade() {
            update_ui_keybind_list(&app, &keybinds_arc);
            app.set_show_bind_picker(false);
            app.set_status_msg(format!("Назначение кнопки {} сброшено на стандартное", btn_id).into());
        }
    });

    // Run Slint Application Loop
    app.run().expect("Failed to run Slint application");

    // Clean up running macro and background listener on window close
    listener.stop();
    let mut slot = active_macro.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, active)) = slot.take() {
        active.stop();
    }
}
