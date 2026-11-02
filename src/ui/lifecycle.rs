pub(super) fn install_close_handler(window: &slint::Window, tray_available: bool) {
    window.on_close_requested(move || {
        if !tray_available {
            // run_event_loop_until_quit deliberately survives hidden windows.
            // Without a tray, hiding alone would strand the running process.
            let _ = slint::quit_event_loop();
        }
        slint::CloseRequestResponse::HideWindow
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
    use slint::platform::{EventLoopProxy, Platform, WindowAdapter, WindowEvent};
    use slint::{ComponentHandle, PlatformError};
    use std::collections::VecDeque;
    use std::rc::Rc;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };
    use std::time::{Duration, Instant};

    type UiEvent = Box<dyn FnOnce() + Send>;
    #[derive(Clone, Default)]
    struct HeadlessLoop {
        quit: Arc<AtomicBool>,
        events: Arc<Mutex<VecDeque<UiEvent>>>,
    }
    impl EventLoopProxy for HeadlessLoop {
        fn quit_event_loop(&self) -> Result<(), slint::EventLoopError> {
            self.quit.store(true, Ordering::SeqCst);
            Ok(())
        }
        fn invoke_from_event_loop(&self, event: UiEvent) -> Result<(), slint::EventLoopError> {
            self.events.lock().unwrap().push_back(event);
            Ok(())
        }
    }
    struct HeadlessPlatform(HeadlessLoop);
    impl Platform for HeadlessPlatform {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
            Ok(MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer))
        }
        fn new_event_loop_proxy(&self) -> Option<Box<dyn EventLoopProxy>> {
            Some(Box::new(self.0.clone()))
        }
        fn run_event_loop(&self) -> Result<(), PlatformError> {
            self.0.quit.store(false, Ordering::SeqCst);
            let started = Instant::now();
            while !self.0.quit.load(Ordering::SeqCst) {
                let event = self.0.events.lock().unwrap().pop_front();
                if let Some(event) = event {
                    event();
                }
                slint::platform::update_timers_and_animations();
                if started.elapsed() > Duration::from_secs(2) {
                    return Err(PlatformError::Other(
                        "Close failed to exit the event loop".into(),
                    ));
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            Ok(())
        }
    }

    #[test]
    fn close_can_reopen_with_tray_and_exits_the_event_loop_without_tray() {
        // Exercise the real Slint CloseRequested event and event loop without
        // opening OS windows, loading settings, installing hooks, or using HID.
        let event_loop = HeadlessLoop::default();
        slint::platform::set_platform(Box::new(HeadlessPlatform(event_loop.clone()))).unwrap();
        for tray_available in [true, false] {
            let app = crate::ui::bridge::MainWindow::new().unwrap();
            install_close_handler(app.window(), tray_available);
            app.show().unwrap();
            let weak = app.as_weak();
            let quitting = event_loop.quit.clone();
            slint::Timer::single_shot(Duration::ZERO, move || {
                let app = weak.upgrade().unwrap();
                app.window().dispatch_event(WindowEvent::CloseRequested);
                assert!(!app.window().is_visible());
                if tray_available {
                    assert!(!quitting.load(Ordering::SeqCst));
                    app.show().unwrap();
                    assert!(app.window().is_visible());
                    // Equivalent to the explicit tray-menu exit action.
                    slint::quit_event_loop().unwrap();
                }
            });
            slint::run_event_loop_until_quit().unwrap();
            assert!(event_loop.quit.load(Ordering::SeqCst));
        }
    }
}
