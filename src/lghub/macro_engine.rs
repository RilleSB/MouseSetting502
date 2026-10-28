use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use super::db::{LgMacro, LgMacroComponent};

#[cfg(windows)]
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT,
    KEYEVENTF_KEYUP, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN,
    MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT,
};

#[cfg(windows)]
const XBUTTON1: u32 = 0x0001;
#[cfg(windows)]
const XBUTTON2: u32 = 0x0002;

#[derive(Default)]
struct TrackedState {
    pressed_mouse_buttons: HashSet<u8>,
    pressed_keys: HashSet<u16>,
}

#[cfg(windows)]
pub fn send_mouse_click(button: u8, down: bool) {
    let (dw_flags, mouse_data) = match (button, down) {
        (1, true) => (MOUSEEVENTF_LEFTDOWN, 0),
        (1, false) => (MOUSEEVENTF_LEFTUP, 0),
        (2, true) => (MOUSEEVENTF_RIGHTDOWN, 0),
        (2, false) => (MOUSEEVENTF_RIGHTUP, 0),
        (3, true) => (MOUSEEVENTF_MIDDLEDOWN, 0),
        (3, false) => (MOUSEEVENTF_MIDDLEUP, 0),
        (4, true) => (MOUSEEVENTF_XDOWN, XBUTTON1),
        (4, false) => (MOUSEEVENTF_XUP, XBUTTON1),
        (5, true) => (MOUSEEVENTF_XDOWN, XBUTTON2),
        (5, false) => (MOUSEEVENTF_XUP, XBUTTON2),
        _ => return,
    };

    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: mouse_data,
                dwFlags: dw_flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };

    unsafe {
        let sent = SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
        if sent == 0 {
            eprintln!("Warning: SendInput mouse event blocked or failed");
        }
    }
}

#[cfg(not(windows))]
pub fn send_mouse_click(_button: u8, _down: bool) {}

#[cfg(windows)]
pub fn send_key_event(vk_code: u16, down: bool) {
    let dw_flags = if down { 0 } else { KEYEVENTF_KEYUP };

    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk_code,
                wScan: 0,
                dwFlags: dw_flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };

    unsafe {
        let sent = SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
        if sent == 0 {
            eprintln!("Warning: SendInput key event blocked or failed for VK 0x{:02X}", vk_code);
        }
    }
}

#[cfg(not(windows))]
pub fn send_key_event(_vk_code: u16, _down: bool) {}

pub struct ActiveMacro {
    running: Arc<AtomicBool>,
    state: Arc<Mutex<TrackedState>>,
}

impl ActiveMacro {
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);

        // Immediately release all held keys and mouse buttons to prevent stuck input!
        if let Ok(mut state) = self.state.lock() {
            for &btn in &state.pressed_mouse_buttons {
                send_mouse_click(btn, false);
            }
            state.pressed_mouse_buttons.clear();

            for &vk in &state.pressed_keys {
                send_key_event(vk, false);
            }
            state.pressed_keys.clear();
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

pub fn play_macro_once(macro_data: &LgMacro) {
    let state = Arc::new(Mutex::new(TrackedState::default()));
    for comp in &macro_data.sequence {
        execute_component(comp, &state);
    }
    // Clean up
    if let Ok(mut s) = state.lock() {
        for &btn in &s.pressed_mouse_buttons {
            send_mouse_click(btn, false);
        }
        s.pressed_mouse_buttons.clear();
        for &vk in &s.pressed_keys {
            send_key_event(vk, false);
        }
        s.pressed_keys.clear();
    }
}

pub fn start_macro_loop(macro_data: LgMacro) -> ActiveMacro {
    let running = Arc::new(AtomicBool::new(true));
    let state = Arc::new(Mutex::new(TrackedState::default()));

    let r_clone = running.clone();
    let s_clone = state.clone();

    thread::spawn(move || {
        println!("[*] Macro loop started: '{}'...", macro_data.name);

        while r_clone.load(Ordering::SeqCst) {
            let mut total_delay = 0u64;

            for comp in &macro_data.sequence {
                if !r_clone.load(Ordering::SeqCst) {
                    break;
                }
                if let Some(ms) = comp.delay_ms {
                    total_delay += ms;
                }
                execute_component(comp, &s_clone);
            }

            // Safety rate-limit: enforce at least 10ms loop delay to prevent 100% CPU lockup
            if total_delay < 10 && r_clone.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(10 - total_delay));
            }
        }

        // On loop exit: release everything
        if let Ok(mut s) = s_clone.lock() {
            for &btn in &s.pressed_mouse_buttons {
                send_mouse_click(btn, false);
            }
            s.pressed_mouse_buttons.clear();

            for &vk in &s.pressed_keys {
                send_key_event(vk, false);
            }
            s.pressed_keys.clear();
        }

        println!("[*] Macro loop safely terminated: '{}'", macro_data.name);
    });

    ActiveMacro { running, state }
}

fn execute_component(comp: &LgMacroComponent, state: &Arc<Mutex<TrackedState>>) {
    if let (Some(btn), Some(down)) = (comp.mouse_button, comp.mouse_down) {
        if let Ok(mut s) = state.lock() {
            if down {
                s.pressed_mouse_buttons.insert(btn);
            } else {
                s.pressed_mouse_buttons.remove(&btn);
            }
        }
        send_mouse_click(btn, down);
    } else if let (Some(vk), Some(down)) = (comp.vk_code, comp.key_down) {
        if let Ok(mut s) = state.lock() {
            if down {
                s.pressed_keys.insert(vk);
            } else {
                s.pressed_keys.remove(&vk);
            }
        }
        send_key_event(vk, down);
    }

    if let Some(ms) = comp.delay_ms
        && ms > 0 {
            thread::sleep(Duration::from_millis(ms));
        }
}
