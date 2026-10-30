//! Windows notification-area integration. The tray icon stays alive while the
//! Slint event loop runs, so closing the main window only hides it.

#[cfg(windows)]
use slint::{ComponentHandle, Timer, TimerMode, Weak};

#[cfg(windows)]
use super::bridge::MainWindow;

#[cfg(windows)]
pub struct TrayIntegration {
    _icon: tray_icon::TrayIcon,
    _menu: tray_icon::menu::Menu,
    _show_item: tray_icon::menu::MenuItem,
    _quit_item: tray_icon::menu::MenuItem,
    _event_timer: Timer,
}

#[cfg(windows)]
impl TrayIntegration {
    pub fn new(app: &MainWindow) -> Result<Self, String> {
        use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
        use tray_icon::menu::{Menu, MenuEvent, MenuItem};

        let menu = Menu::new();
        let show_item = MenuItem::with_id("show-window", "Открыть Mouse Settings 502", true, None);
        let quit_item = MenuItem::with_id("quit-app", "Выход", true, None);
        menu.append(&show_item).map_err(|e| format!("не удалось создать меню трея: {e}"))?;
        menu.append(&quit_item).map_err(|e| format!("не удалось создать меню трея: {e}"))?;

        // ID 1 is the icon resource embedded from icon.ico by build.rs.
        let icon = Icon::from_resource(1, None)
            .map_err(|e| format!("не удалось загрузить иконку трея: {e}"))?;
        let tray_icon = TrayIconBuilder::new()
            .with_tooltip("Mouse Settings 502")
            .with_menu(Box::new(menu.clone()))
            .with_icon(icon)
            .build()
            .map_err(|e| format!("не удалось добавить значок в трей: {e}"))?;

        let weak: Weak<MainWindow> = app.as_weak();
        let show_id = show_item.id().clone();
        let quit_id = quit_item.id().clone();
        let event_timer = Timer::default();
        event_timer.start(TimerMode::Repeated, std::time::Duration::from_millis(100), move || {
            let mut show_window = false;
            let mut quit = false;

            while let Ok(event) = MenuEvent::receiver().try_recv() {
                if event.id == show_id { show_window = true; }
                if event.id == quit_id { quit = true; }
            }
            while let Ok(event) = TrayIconEvent::receiver().try_recv() {
                if matches!(event,
                    TrayIconEvent::DoubleClick { button: MouseButton::Left, .. }
                    | TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }
                ) {
                    show_window = true;
                }
            }

            if quit {
                slint::quit_event_loop().ok();
            } else if show_window {
                if let Some(app) = weak.upgrade() {
                    let _ = app.show();
                }
            }
        });

        Ok(Self {
            _icon: tray_icon,
            _menu: menu,
            _show_item: show_item,
            _quit_item: quit_item,
            _event_timer: event_timer,
        })
    }
}

#[cfg(not(windows))]
pub struct TrayIntegration;

#[cfg(not(windows))]
impl TrayIntegration {
    pub fn new(_: &super::bridge::MainWindow) -> Result<Self, String> { Ok(Self) }
}
