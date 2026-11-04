use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::Duration;

use super::button_actions::ButtonActions;
use super::macro_engine::send_key_event;
use super::storage::{atomic_write_file, get_storage_path as get_appdata_storage_path};

#[cfg(windows)]
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
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
    get_appdata_storage_path(DEFAULT_KEYBINDS_FILE)
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
            button_label: "G4 • Боковая назад (Макросы)".into(),
            default_action: "Назад (Браузер)".into(),
            vk_code: 0x05, // VK_XBUTTON1
            assigned_macro: "".into(),
            is_mouse_button: true,
        },
        ButtonBinding {
            button_id: "G5".into(),
            button_label: "G5 • Боковая вперед (Макросы)".into(),
            default_action: "Вперед (Браузер)".into(),
            vk_code: 0x06, // VK_XBUTTON2
            assigned_macro: "".into(),
            is_mouse_button: true,
        },
        ButtonBinding {
            button_id: "G6".into(),
            button_label: "G6 • Снайпер / DPI Shift [Аппаратная]".into(),
            default_action: "Прицеливание (DPI Shift в памяти G502)".into(),
            vk_code: 0x7C, // VK_F13
            assigned_macro: "".into(),
            is_mouse_button: true,
        },
        ButtonBinding {
            button_id: "G7".into(),
            button_label: "G7 • DPI Вниз [Аппаратная смена]".into(),
            default_action: "Уменьшить DPI (Память мыши)".into(),
            vk_code: 0x7D, // VK_F14
            assigned_macro: "__DPI_DOWN__".into(),
            is_mouse_button: true,
        },
        ButtonBinding {
            button_id: "G8".into(),
            button_label: "G8 • DPI Вверх [Аппаратная смена]".into(),
            default_action: "Увеличить DPI (Память мыши)".into(),
            vk_code: 0x7E, // VK_F15
            assigned_macro: "__DPI_UP__".into(),
            is_mouse_button: true,
        },
        ButtonBinding {
            button_id: "G9".into(),
            button_label: "G9 • Смена профиля [Аппаратная смена]".into(),
            default_action: "Цикл профилей в памяти G502".into(),
            vk_code: 0x7F, // VK_F16
            assigned_macro: "".into(),
            is_mouse_button: true,
        },
        ButtonBinding {
            button_id: "G10".into(),
            button_label: "G10 • Наклон колеса влево (Макросы)".into(),
            default_action: "Прокрутка влево".into(),
            vk_code: 0, // Intercepted via WM_MOUSEHWHEEL delta < 0
            assigned_macro: "".into(),
            is_mouse_button: true,
        },
        ButtonBinding {
            button_id: "G11".into(),
            button_label: "G11 • Наклон колеса вправо (Макросы)".into(),
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
            assigned_macro: "__DPI_DOWN__".into(),
            is_mouse_button: false,
        },
        ButtonBinding {
            button_id: "F10".into(),
            button_label: "Клавиша F10".into(),
            default_action: "Горячая клавиша F10".into(),
            vk_code: 0x79, // VK_F10
            assigned_macro: "__DPI_UP__".into(),
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
    atomic_write_file(path, &json_str)
}

/// Assign a macro to a specific button
pub fn assign_button_macro(
    bindings: &mut [ButtonBinding],
    button_id: &str,
    macro_name: &str,
) -> bool {
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
pub fn rename_assigned_macro(
    bindings: &mut [ButtonBinding],
    old_name: &str,
    new_name: &str,
) -> bool {
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
    trigger_tx: std::sync::mpsc::Sender<(String, u64, bool, String)>,
    panic_tx: std::sync::mpsc::Sender<u64>,
    actions: Arc<Mutex<ButtonActions>>,
    running: Arc<AtomicBool>,
    event_generation: Arc<AtomicU64>,
}

#[cfg(windows)]
static GLOBAL_HOOK: Mutex<Option<HookState>> = Mutex::new(None);

#[cfg(windows)]
static PRESSED_KEYS: Mutex<Option<HashSet<u16>>> = Mutex::new(None);

#[cfg(windows)]
fn check_and_dispatch_button(btn_id: &str, down: bool) -> bool {
    let guard = GLOBAL_HOOK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(state) = guard.as_ref() else {
        return false;
    };
    if !state.running.load(Ordering::SeqCst) {
        return false;
    }
    let generation = state.event_generation.load(Ordering::SeqCst);
    let assignment = state
        .bindings
        .read()
        .ok()
        .and_then(|binds| {
            binds
                .iter()
                .find(|b| b.button_id == btn_id)
                .map(|b| b.assigned_macro.clone())
        })
        .unwrap_or_default();
    let mut actions = state.actions.lock().unwrap_or_else(|e| e.into_inner());
    if !state.running.load(Ordering::SeqCst) {
        return false;
    }
    let event = actions.transition(btn_id, down, &assignment);
    for (vk, pressed) in event.keys {
        send_key_event(vk, pressed);
    }
    if let Some(action) = event.action {
        let _ = state
            .trigger_tx
            .send((action, generation, down, btn_id.to_string()));
    }
    event.suppress
}

#[cfg(windows)]
fn check_and_dispatch_vk(vk: u16, down: bool) -> bool {
    let id = {
        let guard = GLOBAL_HOOK.lock().unwrap_or_else(|e| e.into_inner());
        guard.as_ref().and_then(|state| {
            state.bindings.read().ok().and_then(|binds| {
                binds
                    .iter()
                    .find(|b| !b.is_mouse_button && b.vk_code == vk)
                    .map(|b| b.button_id.clone())
            })
        })
    };
    id.is_some_and(|id| check_and_dispatch_button(&id, down))
}

#[cfg(windows)]
fn dispatch_panic_stop() {
    let guard = GLOBAL_HOOK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(state) = guard.as_ref() {
        let generation = state
            .event_generation
            .fetch_add(1, Ordering::SeqCst)
            .wrapping_add(1);
        let _ = state.panic_tx.send(generation);
    }
}

#[cfg(windows)]
unsafe extern "system" fn low_level_mouse_proc(code: i32, wparam: usize, lparam: isize) -> isize {
    if code >= 0 && lparam != 0 {
        let ms = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
        // LLMHF_INJECTED = 1: ignore synthetic input from SendInput
        if (ms.flags & 1) == 0 {
            let btn_id = match wparam as u32 {
                WM_LBUTTONDOWN | WM_LBUTTONUP => Some("G1"),
                WM_RBUTTONDOWN | WM_RBUTTONUP => Some("G2"),
                WM_MBUTTONDOWN | WM_MBUTTONUP => Some("G3"),
                WM_XBUTTONDOWN | WM_XBUTTONUP => {
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

            if let Some(id) = btn_id {
                let msg = wparam as u32;
                let down = !matches!(
                    msg,
                    WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP
                );
                let suppress = check_and_dispatch_button(id, down);
                // Wheel tilt notifications are pulses, without a separate release.
                if msg == WM_MOUSEHWHEEL {
                    check_and_dispatch_button(id, false);
                }
                if suppress {
                    return 1;
                }
            }
        }
    }
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

#[cfg(windows)]
unsafe extern "system" fn low_level_keyboard_proc(
    code: i32,
    wparam: usize,
    lparam: isize,
) -> isize {
    if code >= 0 && lparam != 0 {
        let msg = wparam as u32;
        let kbd = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        // LLKHF_INJECTED = 0x10: ignore synthetic keyboard input
        if (kbd.flags & 0x10) == 0 {
            let vk = kbd.vkCode as u16;

            // Global emergency panic stop: Esc key (0x1B) immediately halts all running macros
            if (msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN) && vk == 0x1B {
                dispatch_panic_stop();
            }

            if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
                let mut guard = PRESSED_KEYS.lock().unwrap_or_else(|e| e.into_inner());
                let set = guard.get_or_insert_with(HashSet::new);
                set.insert(vk);
                drop(guard);
                // The router deduplicates repeats while still suppressing them.
                if check_and_dispatch_vk(vk, true) {
                    return 1;
                }
            } else if msg == WM_KEYUP || msg == WM_SYSKEYUP {
                let mut guard = PRESSED_KEYS.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(set) = guard.as_mut() {
                    set.remove(&vk);
                }
                drop(guard);
                if check_and_dispatch_vk(vk, false) {
                    return 1;
                }
            }
        }
    }
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

type CallbackFn = Arc<dyn Fn(String, u64, bool, String) + Send + Sync + 'static>;
type PanicFn = Arc<dyn Fn() + Send + Sync + 'static>;

/// Active background listener for global key and mouse button triggers
pub struct KeybindListener {
    actions: Arc<Mutex<ButtonActions>>,
    running: Arc<AtomicBool>,
    event_generation: Arc<AtomicU64>,
    thread_id: Arc<AtomicU32>,
    hook_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
    bindings: Arc<RwLock<Vec<ButtonBinding>>>,
    callback: Arc<RwLock<Option<CallbackFn>>>,
    panic_callback: Arc<RwLock<Option<PanicFn>>>,
    context_cancel: RwLock<Option<PanicFn>>,
}

impl KeybindListener {
    pub fn new(initial_bindings: Vec<ButtonBinding>) -> Self {
        Self {
            actions: Arc::new(Mutex::new(ButtonActions::default())),
            running: Arc::new(AtomicBool::new(false)),
            event_generation: Arc::new(AtomicU64::new(0)),
            thread_id: Arc::new(AtomicU32::new(0)),
            hook_thread: Mutex::new(None),
            bindings: Arc::new(RwLock::new(initial_bindings)),
            callback: Arc::new(RwLock::new(None)),
            panic_callback: Arc::new(RwLock::new(None)),
            context_cancel: RwLock::new(None),
        }
    }

    pub fn set_panic_callback<F>(&self, callback: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        if let Ok(mut lock) = self.panic_callback.write() {
            *lock = Some(Arc::new(callback));
        }
    }

    /// Device loss and context changes cannot leave a held-trigger macro running.
    pub fn set_context_cancel<F>(&self, callback: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        *self
            .context_cancel
            .write()
            .unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(callback));
    }

    pub fn set_callback<F>(&self, callback: F)
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        self.set_event_callback(move |name, _| callback(name));
    }

    /// Compatibility subscription: only activation edges are delivered.
    pub fn set_event_callback<F>(&self, callback: F)
    where
        F: Fn(String, u64) + Send + Sync + 'static,
    {
        self.set_transition_callback(move |name, generation, down, _| {
            if down {
                callback(name, generation);
            }
        });
    }

    pub fn set_transition_callback<F>(&self, callback: F)
    where
        F: Fn(String, u64, bool, String) + Send + Sync + 'static,
    {
        if let Ok(mut lock) = self.callback.write() {
            *lock = Some(Arc::new(callback));
        }
    }

    pub fn event_generation(&self) -> u64 {
        self.event_generation.load(Ordering::SeqCst)
    }

    pub fn invalidate_pending_events(&self) {
        self.event_generation.fetch_add(1, Ordering::SeqCst);
    }

    pub fn update_bindings(&self, new_bindings: Vec<ButtonBinding>) {
        self.invalidate_pending_events();
        if let Ok(mut lock) = self.bindings.write() {
            *lock = new_bindings;
        }
    }

    /// Dispatch a physical G6-G9 transition from MouseButtonSpy, not a fake F-key.
    pub fn hardware_button(&self, button: &str, down: bool, default: &str) {
        if !self.is_running() {
            return;
        }
        let generation = self.event_generation();
        let assignment = self
            .bindings
            .read()
            .ok()
            .and_then(|binds| {
                binds
                    .iter()
                    .find(|b| b.button_id == button)
                    .map(|b| b.assigned_macro.clone())
            })
            .filter(|a| !a.is_empty())
            .unwrap_or_else(|| default.into());
        let action = {
            let mut state = self.actions.lock().unwrap_or_else(|e| e.into_inner());
            if !self.is_running() {
                return;
            }
            let event = state.transition(button, down, &assignment);
            for (vk, pressed) in event.keys {
                send_key_event(vk, pressed);
            }
            event.action
        };
        if let Some(action) = action
            && let Ok(callback) = self.callback.read()
            && let Some(callback) = callback.as_ref()
        {
            callback(action, generation, down, button.to_string());
        }
    }

    pub fn release_buttons(&self) {
        self.invalidate_pending_events();
        let mut state = self.actions.lock().unwrap_or_else(|e| e.into_inner());
        for (vk, down) in state.release_all() {
            send_key_event(vk, down);
        }
        drop(state);
        let cancel = self
            .context_cancel
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(cancel) = cancel {
            cancel();
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Stops the background listener cleanly
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);

        self.release_buttons();
        let tid = self.thread_id.swap(0, Ordering::SeqCst);
        #[cfg(windows)]
        if tid != 0 {
            unsafe {
                PostThreadMessageW(tid, WM_QUIT, 0, 0);
            }
        }
        if let Some(handle) = self
            .hook_thread
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            let _ = handle.join();
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
        let cb_arc: CallbackFn = Arc::new(move |name, _, down, _| {
            if down {
                callback(name);
            }
        });
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
        let panic_cb_arc = self.panic_callback.clone();
        let actions_arc = self.actions.clone();
        let event_generation = self.event_generation.clone();

        #[cfg(windows)]
        {
            let (tx, rx) = std::sync::mpsc::channel::<(String, u64, bool, String)>();
            let (panic_tx, panic_rx) = std::sync::mpsc::channel::<u64>();

            // Single dedicated worker thread for triggered macros (Item 9)
            let cb_worker = callback.clone();
            let worker_generation = event_generation.clone();
            thread::spawn(move || {
                while let Ok((macro_name, generation, down, source)) = rx.recv() {
                    if worker_generation.load(Ordering::SeqCst) == generation {
                        cb_worker(macro_name, generation, down, source);
                    }
                }
            });

            // Single dedicated worker thread for emergency panic stop (Item 12)
            let panic_generation = event_generation.clone();
            let panic_running = running.clone();
            thread::spawn(move || {
                while let Ok(generation) = panic_rx.recv() {
                    if !panic_running.load(Ordering::SeqCst)
                        || panic_generation.load(Ordering::SeqCst) != generation
                    {
                        continue;
                    }
                    if let Ok(guard) = panic_cb_arc.read()
                        && let Some(cb) = guard.as_ref()
                    {
                        cb();
                    }
                }
            });

            let (ready_tx, ready_rx) = std::sync::mpsc::channel();
            let tx_for_thread = tx.clone();
            let panic_tx_for_thread = panic_tx.clone();

            let hook_thread = thread::spawn(move || {
                println!("[*] Global Keybind/Button Listener starting...");

                // Register with global hook state
                {
                    let mut guard = GLOBAL_HOOK.lock().unwrap_or_else(|e| e.into_inner());
                    *guard = Some(HookState {
                        bindings: bindings_arc.clone(),
                        trigger_tx: tx_for_thread.clone(),
                        panic_tx: panic_tx_for_thread.clone(),
                        actions: actions_arc.clone(),
                        running: running.clone(),
                        event_generation,
                    });
                }

                // Reset pressed keys state
                {
                    let mut guard = PRESSED_KEYS.lock().unwrap_or_else(|e| e.into_inner());
                    *guard = Some(HashSet::new());
                }

                let cur_tid = unsafe { GetCurrentThreadId() };
                thread_id_store.store(cur_tid, Ordering::SeqCst);

                // Attempt to install Windows Low-Level Hooks
                let mouse_hook = unsafe {
                    SetWindowsHookExW(
                        WH_MOUSE_LL,
                        Some(low_level_mouse_proc),
                        std::ptr::null_mut(),
                        0,
                    )
                };
                let kbd_hook = unsafe {
                    SetWindowsHookExW(
                        WH_KEYBOARD_LL,
                        Some(low_level_keyboard_proc),
                        std::ptr::null_mut(),
                        0,
                    )
                };

                if !mouse_hook.is_null() && !kbd_hook.is_null() {
                    println!(
                        "[+] Low-level Mouse & Keyboard hooks installed successfully (0% CPU event mode)."
                    );
                    let mut msg: MSG = unsafe { std::mem::zeroed() };
                    unsafe {
                        PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE);
                        let _ = ready_tx.send(true);
                        while running.load(Ordering::SeqCst)
                            && GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0
                        {
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
                    if !mouse_hook.is_null() {
                        unsafe {
                            UnhookWindowsHookEx(mouse_hook);
                        }
                    }
                    if !kbd_hook.is_null() {
                        unsafe {
                            UnhookWindowsHookEx(kbd_hook);
                        }
                    }
                    running.store(false, Ordering::SeqCst);
                    let _ = ready_tx.send(false);
                    eprintln!("[-] Windows input hooks could not be installed");
                }

                // Cleanup global hook state
                {
                    let mut guard = GLOBAL_HOOK.lock().unwrap_or_else(|e| e.into_inner());
                    *guard = None;
                }
                running.store(false, Ordering::SeqCst);
                for (vk, down) in actions_arc
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .release_all()
                {
                    send_key_event(vk, down);
                }
                thread_id_store.store(0, Ordering::SeqCst);
                println!("[*] Global Keybind/Button Listener stopped.");
            });
            *self.hook_thread.lock().unwrap_or_else(|e| e.into_inner()) = Some(hook_thread);
            if ready_rx.recv_timeout(Duration::from_secs(2)) != Ok(true) {
                self.stop();
            }
        }

        #[cfg(not(windows))]
        {
            let _ = (
                running,
                thread_id_store,
                bindings_arc,
                callback,
                panic_cb_arc,
                actions_arc,
                event_generation,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_subscription_delivers_release_with_original_action_and_source() {
        let listener = KeybindListener::new(default_bindings());
        let received = Arc::new(Mutex::new(Vec::new()));
        let output = received.clone();
        listener.set_transition_callback(move |name, _, down, source| {
            output.lock().unwrap().push((name, down, source))
        });
        listener.running.store(true, Ordering::SeqCst);
        listener.hardware_button("G6", true, "original");
        listener.hardware_button("G6", true, "repeat");
        listener.hardware_button("G6", false, "changed");
        assert_eq!(
            *received.lock().unwrap(),
            vec![
                ("original".into(), true, "G6".into()),
                ("original".into(), false, "G6".into())
            ]
        );
        listener.stop();
    }

    #[test]
    fn hardware_events_keep_their_generation_and_context_changes_invalidate_them() {
        let listener = KeybindListener::new(default_bindings());
        let events = Arc::new(Mutex::new(Vec::new()));
        let received = events.clone();
        listener.set_event_callback(move |name, generation| {
            received.lock().unwrap().push((name, generation));
        });
        listener.running.store(true, Ordering::SeqCst);
        let old = listener.event_generation();
        listener.hardware_button("G6", true, "macro");
        listener.release_buttons();
        let current = listener.event_generation();
        assert_ne!(old, current);
        listener.hardware_button("G6", true, "macro");
        assert_eq!(
            *events.lock().unwrap(),
            vec![("macro".into(), old), ("macro".into(), current)]
        );
        listener.update_bindings(default_bindings());
        assert_ne!(listener.event_generation(), current);
        listener.stop();
    }

    #[test]
    fn hardware_buttons_dispatch_real_edges_and_respect_listener_state() {
        let mut binds = default_bindings();
        assign_button_macro(&mut binds, "G6", "sniper macro");
        let listener = KeybindListener::new(binds);
        let received = Arc::new(Mutex::new(Vec::new()));
        let output = received.clone();
        listener.set_callback(move |action| output.lock().unwrap().push(action));
        listener.hardware_button("G6", true, "");
        assert!(received.lock().unwrap().is_empty());
        listener.running.store(true, Ordering::SeqCst);
        for _ in 0..3 {
            listener.hardware_button("G6", true, "");
            listener.hardware_button("G6", true, "");
            listener.hardware_button("G6", false, "");
        }
        assert_eq!(*received.lock().unwrap(), vec!["sniper macro"; 3]);
        listener.stop();
        listener.hardware_button("G6", true, "");
        assert_eq!(received.lock().unwrap().len(), 3);
    }

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
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test_keybinds.json");

        let mut binds = default_bindings();
        assign_button_macro(&mut binds, "G4", "Custom Clicker");

        save_bindings_to_path(&binds, &path).unwrap();
        let loaded = load_bindings_from_path(&path);

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
