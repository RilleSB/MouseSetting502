use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use super::db::{LgMacro, LgMacroComponent};

const INPUT_MOUSE: u32 = 0;
const INPUT_KEYBOARD: u32 = 1;

const MOUSEEVENTF_LEFTDOWN: u32 = 0x0002;
const MOUSEEVENTF_LEFTUP: u32 = 0x0004;
const MOUSEEVENTF_RIGHTDOWN: u32 = 0x0008;
const MOUSEEVENTF_RIGHTUP: u32 = 0x0010;
const MOUSEEVENTF_MIDDLEDOWN: u32 = 0x0020;
const MOUSEEVENTF_MIDDLEUP: u32 = 0x0040;

const KEYEVENTF_KEYUP: u32 = 0x0002;

#[repr(C)]
#[derive(Clone, Copy)]
struct MOUSEINPUT {
    dx: i32,
    dy: i32,
    mouse_data: u32,
    dw_flags: u32,
    time: u32,
    dw_extra_info: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct KEYBDINPUT {
    w_vk: u16,
    w_scan: u16,
    dw_flags: u32,
    time: u32,
    dw_extra_info: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
union INPUT_UNION {
    mi: MOUSEINPUT,
    ki: KEYBDINPUT,
    padding: [u64; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct INPUT {
    r#type: u32,
    u: INPUT_UNION,
}

unsafe extern "system" {
    fn SendInput(cInputs: u32, pInputs: *const INPUT, cbSize: i32) -> u32;
}

pub fn send_mouse_click(button: u8, down: bool) {
    let dw_flags = match (button, down) {
        (1, true) => MOUSEEVENTF_LEFTDOWN,
        (1, false) => MOUSEEVENTF_LEFTUP,
        (2, true) => MOUSEEVENTF_RIGHTDOWN,
        (2, false) => MOUSEEVENTF_RIGHTUP,
        (3, true) => MOUSEEVENTF_MIDDLEDOWN,
        (3, false) => MOUSEEVENTF_MIDDLEUP,
        _ => return,
    };

    let mut input = INPUT {
        r#type: INPUT_MOUSE,
        u: INPUT_UNION {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouse_data: 0,
                dw_flags,
                time: 0,
                dw_extra_info: 0,
            },
        },
    };

    unsafe {
        SendInput(1, &mut input, std::mem::size_of::<INPUT>() as i32);
    }
}

pub fn send_key_event(vk_code: u16, down: bool) {
    let dw_flags = if down { 0 } else { KEYEVENTF_KEYUP };

    let mut input = INPUT {
        r#type: INPUT_KEYBOARD,
        u: INPUT_UNION {
            ki: KEYBDINPUT {
                w_vk: vk_code,
                w_scan: 0,
                dw_flags,
                time: 0,
                dw_extra_info: 0,
            },
        },
    };

    unsafe {
        SendInput(1, &mut input, std::mem::size_of::<INPUT>() as i32);
    }
}

pub struct ActiveMacro {
    running: Arc<AtomicBool>,
}

impl ActiveMacro {
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

pub fn play_macro_once(macro_data: &LgMacro) {
    for comp in &macro_data.sequence {
        execute_component(comp);
    }
}

pub fn start_macro_loop(macro_data: LgMacro) -> ActiveMacro {
    let running = Arc::new(AtomicBool::new(true));
    let r_clone = running.clone();

    thread::spawn(move || {
        println!("[*] Macro loop started: '{}'...", macro_data.name);
        while r_clone.load(Ordering::SeqCst) {
            for comp in &macro_data.sequence {
                if !r_clone.load(Ordering::SeqCst) {
                    break;
                }
                execute_component(comp);
            }
        }
        println!("[*] Macro loop stopped: '{}'", macro_data.name);
    });

    ActiveMacro { running }
}

fn execute_component(comp: &LgMacroComponent) {
    if let (Some(btn), Some(down)) = (comp.mouse_button, comp.mouse_down) {
        send_mouse_click(btn, down);
    } else if let (Some(vk), Some(down)) = (comp.key_code, comp.key_down) {
        send_key_event(vk as u16, down);
    }

    if let Some(ms) = comp.delay_ms {
        if ms > 0 {
            thread::sleep(Duration::from_millis(ms));
        }
    }
}
