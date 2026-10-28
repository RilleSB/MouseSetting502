use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};


#[cfg(windows)]
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
#[cfg(windows)]
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const DEFAULT_KEYBINDS_FILE: &str = "keybinds.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ButtonBinding {
    pub button_id: String,
    pub button_label: String,
    pub default_action: String,
    pub vk_code: u16,
    pub assigned_macro: String,
    pub is_mouse_button: bool,
}

fn get_storage_path() -> PathBuf {
    PathBuf::from(DEFAULT_KEYBINDS_FILE)
}

/// Returns the standard default bindings for Logitech G502 HERO and common hotkeys
pub fn default_bindings() -> Vec<ButtonBinding> {
    vec![
        ButtonBinding {
            button_id: "G1".into(),
            button_label: "G1 • Основная (ЛКМ)".into(),
            default_action: "Основной клик".into(),
            vk_code: 0x01, // VK_LBUTTON
            assigned_macro: "".into(),
            is_mouse_button: true,
        },
        ButtonBinding {
            button_id: "G2".into(),
            button_label: "G2 • Вторичная (ПКМ)".into(),
            default_action: "Контекстный клик".into(),
            vk_code: 0x02, // VK_RBUTTON
            assigned_macro: "".into(),
            is_mouse_button: true,
        },
        ButtonBinding {
            button_id: "G3".into(),
            button_label: "G3 • Колесо (СКМ)".into(),
            default_action: "Клик колеса".into(),
            vk_code: 0x04, // VK_MBUTTON
            assigned_macro: "".into(),
            is_mouse_button: true,
        },
        ButtonBinding {
            button_id: "G4".into(),
            button_label: "G4 • Боковая назад".into(),
            default_action: "Назад (Браузер)".into(),
            vk_code: 0x05, // VK_XBUTTON1
            assigned_macro: "⚡ Быстрый клик ЛКМ (Toggle)".into(),
            is_mouse_button: true,
        },
        ButtonBinding {
            button_id: "G5".into(),
            button_label: "G5 • Боковая вперед".into(),
            default_action: "Вперед (Браузер)".into(),
            vk_code: 0x06, // VK_XBUTTON2
            assigned_macro: "🎯 Тройной выстрел (Burst 3x)".into(),
            is_mouse_button: true,
        },
        ButtonBinding {
            button_id: "G6".into(),
            button_label: "G6 • Снайпер (DPI Shift)".into(),
            default_action: "Прицеливание (DPI Shift)".into(),
            vk_code: 0x7C, // VK_F13
            assigned_macro: "".into(),
            is_mouse_button: true,
        },
        ButtonBinding {
            button_id: "G7".into(),
            button_label: "G7 • DPI Вниз".into(),
            default_action: "Уменьшить DPI".into(),
            vk_code: 0x7D, // VK_F14
            assigned_macro: "".into(),
            is_mouse_button: true,
        },
        ButtonBinding {
            button_id: "G8".into(),
            button_label: "G8 • DPI Вверх".into(),
            default_action: "Увеличить DPI".into(),
            vk_code: 0x7E, // VK_F15
            assigned_macro: "".into(),
            is_mouse_button: true,
        },
        ButtonBinding {
            button_id: "G9".into(),
            button_label: "G9 • Смена профиля".into(),
            default_action: "Цикл профилей G502".into(),
            vk_code: 0x7F, // VK_F16
            assigned_macro: "".into(),
            is_mouse_button: true,
        },
        ButtonBinding {
            button_id: "G10".into(),
            button_label: "G10 • Наклон колеса влево".into(),
            default_action: "Прокрутка влево".into(),
            vk_code: 0, // Intercepted via WM_MOUSEHWHEEL delta < 0
            assigned_macro: "".into(),
            is_mouse_button: true,
        },
        ButtonBinding {
            button_id: "G11".into(),
            button_label: "G11 • Наклон колеса вправо".into(),
            default_action: "Прокрутка вправо".into(),
            vk_code: 0, // Intercepted via WM_MOUSEHWHEEL delta > 0
            assigned_macro: "".into(),
            is_mouse_button: true,
        },
        ButtonBinding {
            button_id: "F6".into(),
            button_label: "Клавиша F6".into(),
            default_action: "Горячая клавиша F6".into(),
            vk_code: 0x75, // VK_F6
            assigned_macro: "".into(),
            is_mouse_button: false,
        },
        ButtonBinding {
            button_id: "F7".into(),
            button_label: "Клавиша F7".into(),
            default_action: "Горячая клавиша F7".into(),
            vk_code: 0x76, // VK_F7
            assigned_macro: "".into(),
            is_mouse_button: false,
        },
        ButtonBinding {
            button_id: "F8".into(),
            button_label: "Клавиша F8".into(),
            default_action: "Горячая клавиша F8".into(),
            vk_code: 0x77, // VK_F8
            assigned_macro: "".into(),
            is_mouse_button: false,
        },
        ButtonBinding {
            button_id: "F9".into(),
            button_label: "Клавиша F9".into(),
            default_action: "Горячая клавиша F9".into(),
            vk_code: 0x78, // VK_F9
            assigned_macro: "".into(),
            is_mouse_button: false,
        },
        ButtonBinding {
            button_id: "F10".into(),
            button_label: "Клавиша F10".into(),
            default_action: "Горячая клавиша F10".into(),
            vk_code: 0x79, // VK_F10
            assigned_macro: "".into(),
            is_mouse_button: false,
        },
        ButtonBinding {
            button_id: "F11".into(),
            button_label: "Клавиша F11".into(),
            default_action: "Горячая клавиша F11".into(),
            vk_code: 0x7A, // VK_F11
            assigned_macro: "".into(),
            is_mouse_button: false,
        },
        ButtonBinding {
            button_id: "F12".into(),
            button_label: "Клавиша F12".into(),
            default_action: "Горячая клавиша F12".into(),
            vk_code: 0x7B, // VK_F12
            assigned_macro: "".into(),
            is_mouse_button: false,
        },
    ]
}

/// Load keybindings from file or create defaults
pub fn load_bindings() -> Vec<ButtonBinding> {
    load_bindings_from_path(&get_storage_path())
}

pub fn load_bindings_from_path(path: &Path) -> Vec<ButtonBinding> {
    if !path.exists() {
        let def = default_bindings();
        let _ = save_bindings_to_path(&def, path);
        return def;
    }

    match fs::read_to_string(path) {
        Ok(json_str) => match serde_json::from_str::<Vec<ButtonBinding>>(&json_str) {
            Ok(list) => {
                if list.is_empty() {
                    let def = default_bindings();
                    let _ = save_bindings_to_path(&def, path);
                    def
                } else {
                    list
                }
            }
            Err(e) => {
                eprintln!("[!] Warning: failed to parse keybinds JSON: {e}");
                default_bindings()
            }
        },
        Err(e) => {
            eprintln!("[!] Warning: failed to read keybinds file: {e}");
            default_bindings()
        }
    }
}

/// Save bindings to disk
pub fn save_bindings(bindings: &[ButtonBinding]) -> Result<(), String> {
    save_bindings_to_path(bindings, &get_storage_path())
}

pub fn save_bindings_to_path(bindings: &[ButtonBinding], path: &Path) -> Result<(), String> {
    let json_str = serde_json::to_string_pretty(bindings)
        .map_err(|e| format!("JSON serialization error: {e}"))?;
    fs::write(path, json_str).map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
    Ok(())
}

/// Assign a macro to a specific button
pub fn assign_button_macro(bindings: &mut [ButtonBinding], button_id: &str, macro_name: &str) -> bool {
    if let Some(b) = bindings.iter_mut().find(|b| b.button_id == button_id) {
        b.assigned_macro = macro_name.to_string();
        true
    } else {
        false
    }
}

/// Clear macro assignment for a button
pub fn clear_button_macro(bindings: &mut [ButtonBinding], button_id: &str) -> bool {
    assign_button_macro(bindings, button_id, "")
}

/// Update keybinding macro name when a custom macro is renamed
pub fn rename_assigned_macro(bindings: &mut [ButtonBinding], old_name: &str, new_name: &str) -> bool {
    let mut updated = false;
    for b in bindings.iter_mut() {
        if b.assigned_macro == old_name {
            b.assigned_macro = new_name.to_string();
            updated = true;
        }
    }
    updated
}

// Global dispatcher state for Windows Low-Level Hooks
#[cfg(windows)]
struct HookState {
    bindings: Arc<RwLock<Vec<ButtonBinding>>>,
    callback: Arc<dyn Fn(String) + Send + Sync + 'static>,
    last_trigger: Mutex<HashMap<String, Instant>>,
}

#[cfg(windows)]
static GLOBAL_HOOK: Mutex<Option<HookState>> = Mutex::new(None);

#[cfg(windows)]
fn check_and_dispatch_button(btn_id: &str) -> bool {
    let guard = GLOBAL_HOOK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(state) = guard.as_ref() {
        let macro_name = {
            if let Ok(binds) = state.bindings.read() {
                binds.iter().find(|b| b.button_id == btn_id && !b.assigned_macro.trim().is_empty())
                    .map(|b| b.assigned_macro.clone())
            } else {
                None
            }
        };

        if let Some(name) = macro_name {
            let mut last = state.last_trigger.lock().unwrap_or_else(|e| e.into_inner());
            let now = Instant::now();
            let debounce = Duration::from_millis(160);
            let allow = match last.get(&name) {
                Some(&prev) => now.duration_since(prev) > debounce,
                None => true,
            };
            if allow {
                last.insert(name.clone(), now);
                let cb = state.callback.clone();
                thread::spawn(move || {
                    cb(name);
                });
            }
            return true;
        }
    }
    false
}

#[cfg(windows)]
fn check_and_dispatch_vk(vk: u16) -> bool {
    let guard = GLOBAL_HOOK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(state) = guard.as_ref() {
        let macro_name = {
            if let Ok(binds) = state.bindings.read() {
                binds.iter().find(|b| b.vk_code == vk && !b.assigned_macro.trim().is_empty())
                    .map(|b| b.assigned_macro.clone())
            } else {
                None
            }
        };

        if let Some(name) = macro_name {
            let mut last = state.last_trigger.lock().unwrap_or_else(|e| e.into_inner());
            let now = Instant::now();
            let debounce = Duration::from_millis(160);
            let allow = match last.get(&name) {
                Some(&prev) => now.duration_since(prev) > debounce,
                None => true,
            };
            if allow {
                last.insert(name.clone(), now);
                let cb = state.callback.clone();
                thread::spawn(move || {
                    cb(name);
                });
            }
            return true;
        }
    }
    false
}

#[cfg(windows)]
unsafe extern "system" fn low_level_mouse_proc(code: i32, wparam: usize, lparam: isize) -> isize {
    if code >= 0 && lparam != 0 {
        let ms = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
        // LLMHF_INJECTED = 1: ignore synthetic input from SendInput
        if (ms.flags & 1) == 0 {
            let btn_id = match wparam as u32 {
                WM_LBUTTONDOWN => Some("G1"),
                WM_RBUTTONDOWN => Some("G2"),
                WM_MBUTTONDOWN => Some("G3"),
                WM_XBUTTONDOWN => {
                    let xbtn = (ms.mouseData >> 16) as u16;
                    if xbtn == 1 {
                        Some("G4")
                    } else if xbtn == 2 {
                        Some("G5")
                    } else {
                        None
                    }
                }
                WM_MOUSEHWHEEL => {
                    let delta = (ms.mouseData >> 16) as i16;
                    if delta < 0 {
                        Some("G10") // Tilt Left
                    } else if delta > 0 {
                        Some("G11") // Tilt Right
                    } else {
                        None
                    }
                }
                _ => None,
            };

            if let Some(id) = btn_id
                && check_and_dispatch_button(id) {
                    // Suppress default action for side buttons / tilt if macro assigned
                    if id == "G4" || id == "G5" || id == "G10" || id == "G11" {
                        return 1;
                    }
                }
        }
    }
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

#[cfg(windows)]
unsafe extern "system" fn low_level_keyboard_proc(code: i32, wparam: usize, lparam: isize) -> isize {
    if code >= 0 && lparam != 0 {
        let msg = wparam as u32;
        if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
            let kbd = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
            // LLKHF_INJECTED = 0x10: ignore synthetic keyboard input
            if (kbd.flags & 0x10) == 0 {
                let vk = kbd.vkCode as u16;
                if check_and_dispatch_vk(vk) {
                    // Key trigger fired macro
                }
            }
        }
    }
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

type CallbackFn = Arc<dyn Fn(String) + Send + Sync + 'static>;

/// Active background listener for global key and mouse button triggers
pub struct KeybindListener {
    running: Arc<AtomicBool>,
    thread_id: Arc<AtomicU32>,
    bindings: Arc<RwLock<Vec<ButtonBinding>>>,
    callback: Arc<RwLock<Option<CallbackFn>>>,
}

impl KeybindListener {
    pub fn new(initial_bindings: Vec<ButtonBinding>) -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            thread_id: Arc::new(AtomicU32::new(0)),
            bindings: Arc::new(RwLock::new(initial_bindings)),
            callback: Arc::new(RwLock::new(None)),
        }
    }

    pub fn update_bindings(&self, new_bindings: Vec<ButtonBinding>) {
        if let Ok(mut lock) = self.bindings.write() {
            *lock = new_bindings;
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Stops the background listener cleanly
    pub fn stop(&self) {
        if !self.running.swap(false, Ordering::SeqCst) {
            return;
        }

        let tid = self.thread_id.swap(0, Ordering::SeqCst);
        #[cfg(windows)]
        if tid != 0 {
            unsafe {
                PostThreadMessageW(tid, WM_QUIT, 0, 0);
            }
        }
    }

    /// Toggles listener state on or off. Returns true if now running, false if stopped.
    pub fn toggle(&self) -> bool {
        if self.is_running() {
            self.stop();
            false
        } else {
            self.restart();
            self.is_running()
        }
    }

    /// Restart the background listener with the saved callback
    pub fn restart(&self) {
        let cb_opt = {
            if let Ok(guard) = self.callback.read() {
                guard.clone()
            } else {
                None
            }
        };

        if let Some(cb) = cb_opt {
            self.start_internal(cb);
        }
    }

    /// Start the background listener loop.
    pub fn start<F>(&self, callback: F)
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        let cb_arc: CallbackFn = Arc::new(callback);
        if let Ok(mut lock) = self.callback.write() {
            *lock = Some(cb_arc.clone());
        }
        self.start_internal(cb_arc);
    }

    fn start_internal(&self, callback: CallbackFn) {
        if self.running.swap(true, Ordering::SeqCst) {
            // Already running
            return;
        }

        let running = self.running.clone();
        let thread_id_store = self.thread_id.clone();
        let bindings_arc = self.bindings.clone();

        #[cfg(windows)]
        {
            let cb_for_thread = callback.clone();
            thread::spawn(move || {
                println!("[*] Global Keybind/Button Listener starting...");

                // Register with global hook state
                {
                    let mut guard = GLOBAL_HOOK.lock().unwrap_or_else(|e| e.into_inner());
                    *guard = Some(HookState {
                        bindings: bindings_arc.clone(),
                        callback: cb_for_thread,
                        last_trigger: Mutex::new(HashMap::new()),
                    });
                }

                let cur_tid = unsafe { GetCurrentThreadId() };
                thread_id_store.store(cur_tid, Ordering::SeqCst);

                // Attempt to install Windows Low-Level Hooks
                let mouse_hook = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(low_level_mouse_proc), std::ptr::null_mut(), 0) };
                let kbd_hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_keyboard_proc), std::ptr::null_mut(), 0) };

                if !mouse_hook.is_null() && !kbd_hook.is_null() {
                    println!("[+] Low-level Mouse & Keyboard hooks installed successfully (0% CPU event mode).");
                    let mut msg: MSG = unsafe { std::mem::zeroed() };
                    unsafe {
                        while running.load(Ordering::SeqCst) && GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                            TranslateMessage(&msg);
                            DispatchMessageW(&msg);
                        }
                        if !mouse_hook.is_null() {
                            UnhookWindowsHookEx(mouse_hook);
                        }
                        if !kbd_hook.is_null() {
                            UnhookWindowsHookEx(kbd_hook);
                        }
                    }
                } else {
                    // Fallback to polling loop if running in headless test/non-desktop session
                    if !mouse_hook.is_null() { unsafe { UnhookWindowsHookEx(mouse_hook); } }
                    if !kbd_hook.is_null() { unsafe { UnhookWindowsHookEx(kbd_hook); } }
                    println!("[*] Hooks not available; running in polling fallback mode.");

                    let mut key_states: HashMap<u16, bool> = HashMap::new();
                    let mut last_trigger_time: HashMap<String, Instant> = HashMap::new();
                    let debounce_dur = Duration::from_millis(160);

                    while running.load(Ordering::SeqCst) {
                        let active_pairs: Vec<(u16, String)> = {
                            if let Ok(binds) = bindings_arc.read() {
                                binds
                                    .iter()
                                    .filter(|b| b.vk_code > 0 && !b.assigned_macro.trim().is_empty())
                                    .map(|b| (b.vk_code, b.assigned_macro.clone()))
                                    .collect()
                            } else {
                                Vec::new()
                            }
                        };

                        for (vk, macro_name) in active_pairs {
                            let is_down = check_key_down(vk);
                            let was_down = key_states.get(&vk).copied().unwrap_or(false);

                            if is_down && !was_down {
                                let now = Instant::now();
                                let allow = match last_trigger_time.get(&macro_name) {
                                    Some(&t) => now.duration_since(t) > debounce_dur,
                                    None => true,
                                };

                                if allow {
                                    last_trigger_time.insert(macro_name.clone(), now);
                                    let cb = callback.clone();
                                    let m = macro_name.clone();
                                    thread::spawn(move || {
                                        cb(m);
                                    });
                                }
                            }
                            key_states.insert(vk, is_down);
                        }
                        thread::sleep(Duration::from_millis(10));
                    }
                }

                // Cleanup global hook state
                {
                    let mut guard = GLOBAL_HOOK.lock().unwrap_or_else(|e| e.into_inner());
                    *guard = None;
                }
                running.store(false, Ordering::SeqCst);
                thread_id_store.store(0, Ordering::SeqCst);
                println!("[*] Global Keybind/Button Listener stopped.");
            });
        }

        #[cfg(not(windows))]
        {
            let _ = (running, thread_id_store, bindings_arc, callback);
        }
    }
}

/// Query hardware key down state on Windows via GetAsyncKeyState
#[cfg(windows)]
fn check_key_down(vk: u16) -> bool {
    unsafe {
        let state = GetAsyncKeyState(vk as i32);
        (state as u16 & 0x8000) != 0
    }
}

#[cfg(not(windows))]
fn check_key_down(_vk: u16) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_bindings() {
        let binds = default_bindings();
        assert!(binds.len() >= 10);
        assert!(binds.iter().any(|b| b.button_id == "G1"));
        assert!(binds.iter().any(|b| b.button_id == "G4"));
        assert!(binds.iter().any(|b| b.button_id == "G5"));
        assert!(binds.iter().any(|b| b.button_id == "G6"));
        assert!(binds.iter().any(|b| b.button_id == "G10"));
        assert!(binds.iter().any(|b| b.button_id == "G11"));
        assert!(binds.iter().any(|b| b.button_id == "F6"));
        assert!(binds.iter().any(|b| b.button_id == "F12"));
    }

    #[test]
    fn test_assign_and_clear_macro() {
        let mut binds = default_bindings();
        let changed = assign_button_macro(&mut binds, "G6", "Sniper Macro");
        assert!(changed);
        let g6 = binds.iter().find(|b| b.button_id == "G6").unwrap();
        assert_eq!(g6.assigned_macro, "Sniper Macro");

        let cleared = clear_button_macro(&mut binds, "G6");
        assert!(cleared);
        let g6 = binds.iter().find(|b| b.button_id == "G6").unwrap();
        assert_eq!(g6.assigned_macro, "");
    }

    #[test]
    fn test_rename_assigned_macro() {
        let mut binds = default_bindings();
        assign_button_macro(&mut binds, "G4", "Old Name");
        let renamed = rename_assigned_macro(&mut binds, "Old Name", "New Name");
        assert!(renamed);
        let g4 = binds.iter().find(|b| b.button_id == "G4").unwrap();
        assert_eq!(g4.assigned_macro, "New Name");
    }

    #[test]
    fn test_keybinds_save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("ghub_test_keybinds_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test_keybinds.json");

        let mut binds = default_bindings();
        assign_button_macro(&mut binds, "G4", "Custom Clicker");

        save_bindings_to_path(&binds, &path).unwrap();
        let loaded = load_bindings_from_path(&path);
        let _ = fs::remove_dir_all(&dir);

        assert_eq!(loaded.len(), binds.len());
        let g4 = loaded.iter().find(|b| b.button_id == "G4").unwrap();
        assert_eq!(g4.assigned_macro, "Custom Clicker");
    }

    #[test]
    fn test_listener_start_stop() {
        let binds = default_bindings();
        let listener = KeybindListener::new(binds);
        assert!(!listener.is_running());

        listener.start(|_m| {});
        assert!(listener.is_running());

        listener.stop();
        thread::sleep(Duration::from_millis(25));
        assert!(!listener.is_running());
    }

    #[test]
    fn test_listener_restart_and_toggle() {
        let binds = default_bindings();
        let listener = KeybindListener::new(binds);
        listener.start(|_m| {});
        assert!(listener.is_running());

        // Stop
        listener.stop();
        thread::sleep(Duration::from_millis(25));
        assert!(!listener.is_running());

        // Restart with saved callback!
        listener.restart();
        assert!(listener.is_running());

        // Toggle to off
        let state = listener.toggle();
        assert!(!state);
        thread::sleep(Duration::from_millis(25));
        assert!(!listener.is_running());

        // Toggle to on
        let state2 = listener.toggle();
        assert!(state2);
        assert!(listener.is_running());

        listener.stop();
    }
}
