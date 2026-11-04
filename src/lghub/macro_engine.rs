use super::db::{LgMacro, LgMacroComponent, MacroMode};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(windows)]
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN,
    MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_XDOWN,
    MOUSEEVENTF_XUP, MOUSEINPUT, MapVirtualKeyW, SendInput, keybd_event, mouse_event,
};

#[cfg(windows)]
const XBUTTON1: u32 = 0x0001;
#[cfg(windows)]
const XBUTTON2: u32 = 0x0002;

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
            // Robust fallback: direct driver-level mouse_event which bypasses UIPI restrictions
            mouse_event(dw_flags, 0, 0, mouse_data as i32, 0);
        }
    }
}

#[cfg(not(windows))]
pub fn send_mouse_click(_button: u8, _down: bool) {}

#[cfg(windows)]
pub fn send_key_event(vk_code: u16, down: bool) {
    let dw_flags = if down { 0 } else { KEYEVENTF_KEYUP }
        | if matches!(vk_code, 0x21..=0x28 | 0x2d | 0x2e | 0x5b..=0x5d | 0x6f | 0x90 | 0xa3 | 0xa5)
        {
            KEYEVENTF_EXTENDEDKEY
        } else {
            0
        };
    let scan_code = unsafe { MapVirtualKeyW(vk_code as u32, 0) as u16 };

    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk_code,
                wScan: scan_code,
                dwFlags: dw_flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };

    unsafe {
        let sent = SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
        if sent == 0 {
            // Robust fallback: direct keybd_event which bypasses UIPI restrictions and works in games
            keybd_event(vk_code as u8, scan_code as u8, dw_flags, 0);
        }
    }
}

#[cfg(not(windows))]
pub fn send_key_event(_vk_code: u16, _down: bool) {}

// All input from a run goes through this interface, including cleanup. Tests
// use a recorder and never inject real keyboard or mouse events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputButton {
    Key(u16),
    Mouse(u8),
}

trait InputSink: Send + Sync {
    fn send(&self, button: InputButton, down: bool);
}

struct SystemInput;
impl InputSink for SystemInput {
    fn send(&self, button: InputButton, down: bool) {
        match button {
            InputButton::Key(key) => send_key_event(key, down),
            InputButton::Mouse(button) => send_mouse_click(button, down),
        }
    }
}

struct RunState {
    running: bool,
    trigger_held: bool,
    held: Vec<InputButton>,
}

impl RunState {
    fn release(&mut self, sink: &dyn InputSink) {
        for button in self.held.drain(..).rev() {
            sink.send(button, false);
        }
    }
}

struct SharedRun {
    state: Mutex<RunState>,
    wake: Condvar,
    sink: Arc<dyn InputSink>,
}

static NEXT_RUN_ID: AtomicU64 = AtomicU64::new(1);

pub struct ActiveMacro {
    id: u64,
    shared: Arc<SharedRun>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
}

impl ActiveMacro {
    pub fn id(&self) -> u64 {
        self.id
    }

    /// On return all held input is released and the worker has terminated.
    /// The same mutex protects cancellation, tracking and actual input delivery,
    /// so a worker cannot send a late key-down after cleanup.
    pub fn stop(&self) {
        {
            let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
            state.running = false;
            state.release(self.shared.sink.as_ref());
            self.shared.wake.notify_all();
        }
        self.wait();
    }

    pub fn release_trigger(&self) {
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        state.trigger_held = false;
        self.shared.wake.notify_all();
    }

    pub fn is_running(&self) -> bool {
        self.shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .running
    }

    pub fn wait(&self) {
        let mut worker = self.worker.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(handle) = worker.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for ActiveMacro {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Start a cancellable run, honoring its sequence/toggle setting.
pub fn start_macro(macro_data: LgMacro) -> ActiveMacro {
    let repeat = macro_data.is_toggle;
    start_with_sink(macro_data, repeat, Arc::new(SystemInput))
}

pub fn start_macro_loop(macro_data: LgMacro) -> ActiveMacro {
    start_with_sink(macro_data, true, Arc::new(SystemInput))
}

/// Blocking CLI compatibility wrapper; GUI runs must retain the ActiveMacro.
pub fn play_macro_once(macro_data: &LgMacro) {
    let mut data = macro_data.clone();
    if let Some(spec) = data.playback.as_mut() {
        if spec.mode == MacroMode::Sequence {
            let mut sequence = spec.on_press.clone();
            sequence.extend(data.sequence);
            sequence.extend(spec.on_release.clone());
            data.sequence = sequence;
        }
        spec.mode = MacroMode::Once;
    }
    let active = start_with_sink(data, false, Arc::new(SystemInput));
    active.wait();
}

fn start_with_sink(macro_data: LgMacro, repeat: bool, sink: Arc<dyn InputSink>) -> ActiveMacro {
    let shared = Arc::new(SharedRun {
        state: Mutex::new(RunState {
            running: true,
            trigger_held: true,
            held: Vec::new(),
        }),
        wake: Condvar::new(),
        sink,
    });
    let run = shared.clone();
    let worker = thread::spawn(move || {
        let mode = if macro_data.playback.is_some() {
            macro_data.mode()
        } else if repeat {
            MacroMode::Toggle
        } else {
            MacroMode::Once
        };
        let standard = macro_data
            .playback
            .as_ref()
            .and_then(|p| p.standard_delay_ms);
        if mode == MacroMode::Sequence {
            let spec = macro_data
                .playback
                .as_ref()
                .expect("sequence has playback metadata");
            if execute_sequence(&spec.on_press, &run, standard, false, false) {
                repeat_sequence(&macro_data.sequence, &run, standard, true);
                // A normal trigger release runs the release phase; emergency cancellation does not.
                let running = run.state.lock().unwrap_or_else(|e| e.into_inner()).running;
                if running {
                    execute_sequence(&spec.on_release, &run, standard, false, false);
                }
            }
        } else if matches!(mode, MacroMode::Toggle | MacroMode::WhileHeld) {
            repeat_sequence(
                &macro_data.sequence,
                &run,
                standard,
                mode == MacroMode::WhileHeld,
            );
        } else {
            execute_sequence(&macro_data.sequence, &run, standard, false, false);
        }
        let mut state = run.state.lock().unwrap_or_else(|e| e.into_inner());
        state.release(run.sink.as_ref());
        state.running = false;
        run.wake.notify_all();
    });
    ActiveMacro {
        id: NEXT_RUN_ID.fetch_add(1, Ordering::Relaxed),
        shared,
        worker: Mutex::new(Some(worker)),
    }
}

fn repeat_sequence(
    components: &[LgMacroComponent],
    run: &SharedRun,
    standard: Option<u64>,
    held: bool,
) {
    loop {
        let started = Instant::now();
        if !execute_sequence(components, run, standard, held, true) {
            break;
        }
        // Explicit 1 ms event delays must not be silently stretched to a 10 ms cycle.
        // Only a cycle with no positive delays needs a small cancellable yield.
        if !(0..components.len())
            .any(|index| effective_delay(components, index, standard, true) > 0)
            && !wait_delay_mode(
                run,
                Duration::from_millis(1).saturating_sub(started.elapsed()),
                held,
            )
        {
            break;
        }
    }
}

fn effective_delay(
    components: &[LgMacroComponent],
    index: usize,
    standard: Option<u64>,
    repeat: bool,
) -> u64 {
    let component = &components[index];
    let Some(standard) = standard else {
        return component.delay_ms.unwrap_or(0);
    };
    if component.mouse_button.is_none() && component.vk_code.is_none() {
        return if component.explicit_delay {
            component.delay_ms.unwrap_or(0)
        } else {
            standard
        };
    }
    let next_is_delay = components
        .get(index + 1)
        .is_some_and(|next| next.mouse_button.is_none() && next.vk_code.is_none());
    if next_is_delay || (index + 1 == components.len() && !repeat) {
        0
    } else {
        standard
    }
}

fn execute_sequence(
    components: &[LgMacroComponent],
    run: &SharedRun,
    standard: Option<u64>,
    held: bool,
    repeat: bool,
) -> bool {
    for (index, component) in components.iter().enumerate() {
        let mut component = component.clone();
        component.delay_ms = Some(effective_delay(components, index, standard, repeat));
        if !execute_component(&component, run, held) {
            return false;
        }
    }
    true
}

fn execute_component(component: &LgMacroComponent, run: &SharedRun, held: bool) -> bool {
    {
        let mut state = run.state.lock().unwrap_or_else(|e| e.into_inner());
        if !state.running || (held && !state.trigger_held) {
            return false;
        }
        let input =
            if let (Some(button), Some(down)) = (component.mouse_button, component.mouse_down) {
                Some((InputButton::Mouse(button), down))
            } else if let (Some(key), Some(down)) = (component.vk_code, component.key_down) {
                Some((InputButton::Key(key), down))
            } else {
                None
            };
        if let Some((button, down)) = input {
            if down && !state.held.contains(&button) {
                state.held.push(button);
            }
            if !down {
                state.held.retain(|held| *held != button);
            }
            run.sink.send(button, down);
        }
    }
    wait_delay_mode(
        run,
        Duration::from_millis(component.delay_ms.unwrap_or(0)),
        held,
    )
}

fn wait_delay_mode(run: &SharedRun, delay: Duration, held: bool) -> bool {
    let started = Instant::now();
    let mut state = run.state.lock().unwrap_or_else(|e| e.into_inner());
    while state.running && (!held || state.trigger_held) {
        let remaining = delay.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return true;
        }
        // Bound OS timeout values even for a u64::MAX delay loaded from JSON.
        let (next, _) = run
            .wake
            .wait_timeout(state, remaining.min(Duration::from_secs(60)))
            .unwrap_or_else(|e| e.into_inner());
        state = next;
    }
    false
}

struct Playback {
    generation: u64,
    active: Option<(String, ActiveMacro)>,
    trigger_source: Option<String>,
    trigger_mode: MacroMode,
}

/// Owns the single active GUI run. Cancellation also invalidates queued triggers.
pub struct MacroPlayer {
    playback: Mutex<Playback>,
    sink: Arc<dyn InputSink>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PlaybackChange {
    Started,
    Stopped,
    Stale,
    Released,
}

impl Default for MacroPlayer {
    fn default() -> Self {
        Self::with_sink(Arc::new(SystemInput))
    }
}

impl MacroPlayer {
    fn with_sink(sink: Arc<dyn InputSink>) -> Self {
        Self {
            playback: Mutex::new(Playback {
                generation: 0,
                active: None,
                trigger_source: None,
                trigger_mode: MacroMode::Once,
            }),
            sink,
        }
    }

    pub fn generation(&self) -> u64 {
        self.playback
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .generation
    }

    pub fn is_idle_at(&self, generation: u64) -> bool {
        let playback = self.playback.lock().unwrap_or_else(|e| e.into_inner());
        playback.generation == generation && playback.active.is_none()
    }

    pub fn toggle(&self, macro_data: LgMacro, expected: u64) -> PlaybackChange {
        self.change(macro_data, expected, false, None)
    }

    pub fn trigger(&self, macro_data: LgMacro, expected: u64) -> PlaybackChange {
        self.change(macro_data, expected, true, None)
    }

    pub fn press(&self, macro_data: LgMacro, expected: u64, source: String) -> PlaybackChange {
        self.change(macro_data, expected, true, Some(source))
    }

    pub fn release(&self, source: &str, expected: u64) -> PlaybackChange {
        let playback = self.playback.lock().unwrap_or_else(|e| e.into_inner());
        if playback.generation != expected {
            return PlaybackChange::Stale;
        }
        if playback.trigger_source.as_deref() == Some(source)
            && matches!(
                playback.trigger_mode,
                MacroMode::WhileHeld | MacroMode::Sequence
            )
            && let Some((_, active)) = &playback.active
        {
            active.release_trigger();
            PlaybackChange::Released
        } else {
            PlaybackChange::Stale
        }
    }

    fn change(
        &self,
        macro_data: LgMacro,
        expected: u64,
        restart_sequence: bool,
        source: Option<String>,
    ) -> PlaybackChange {
        let mut playback = self.playback.lock().unwrap_or_else(|e| e.into_inner());
        if playback.generation != expected {
            return PlaybackChange::Stale;
        }
        if let Some((name, active)) = playback.active.take() {
            let toggle_off = name == macro_data.name
                && active.is_running()
                && (!restart_sequence || macro_data.mode() == MacroMode::Toggle);
            active.stop();
            if toggle_off {
                playback.generation = playback.generation.wrapping_add(1);
                return PlaybackChange::Stopped;
            }
        }
        let name = macro_data.name.clone();
        let repeat = macro_data.is_toggle;
        playback.trigger_source = source;
        playback.trigger_mode = macro_data.mode();
        playback.active = Some((name, start_with_sink(macro_data, repeat, self.sink.clone())));
        PlaybackChange::Started
    }

    pub fn stop(&self) -> u64 {
        let mut playback = self.playback.lock().unwrap_or_else(|e| e.into_inner());
        playback.generation = playback.generation.wrapping_add(1);
        if let Some((_, active)) = playback.active.take() {
            active.stop();
        }
        playback.generation
    }

    /// Editing one macro must not stop an unrelated active run.
    pub fn stop_if_name(&self, name: &str) -> bool {
        let mut playback = self.playback.lock().unwrap_or_else(|e| e.into_inner());
        if playback
            .active
            .as_ref()
            .is_some_and(|(active, _)| active.trim() == name.trim())
        {
            playback.generation = playback.generation.wrapping_add(1);
            if let Some((_, active)) = playback.active.take() {
                active.stop();
            }
            true
        } else {
            false
        }
    }

    /// Called by the UI timer. No detached completion callback can clear a newer run.
    pub fn take_finished(&self) -> Option<String> {
        let mut playback = self.playback.lock().unwrap_or_else(|e| e.into_inner());
        if playback
            .active
            .as_ref()
            .is_some_and(|(_, run)| !run.is_running())
        {
            playback.active.take().map(|(name, _)| name)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{self, Receiver};

    type Event = (InputButton, bool);
    struct Recorder {
        events: Mutex<Vec<Event>>,
        sent: mpsc::Sender<Event>,
    }
    impl InputSink for Recorder {
        fn send(&self, button: InputButton, down: bool) {
            self.events.lock().unwrap().push((button, down));
            let _ = self.sent.send((button, down));
        }
    }
    fn recorder() -> (Arc<Recorder>, Receiver<Event>) {
        let (sent, received) = mpsc::channel();
        (
            Arc::new(Recorder {
                events: Mutex::new(Vec::new()),
                sent,
            }),
            received,
        )
    }
    fn key(key: u16, down: bool, delay: u64) -> LgMacroComponent {
        LgMacroComponent {
            explicit_delay: false,
            vk_code: Some(key),
            key_down: Some(down),
            delay_ms: Some(delay),
            mouse_button: None,
            mouse_down: None,
        }
    }
    fn data(name: &str, repeat: bool, sequence: Vec<LgMacroComponent>) -> LgMacro {
        LgMacro {
            playback: None,
            name: name.into(),
            macro_type: if repeat { "toggle" } else { "sequence" }.into(),
            is_toggle: repeat,
            sequence,
            action_name: None,
        }
    }
    fn receive(receiver: &Receiver<Event>) -> Event {
        receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("macro event timed out")
    }

    fn configured(
        mode: MacroMode,
        sequence: Vec<LgMacroComponent>,
        press: Vec<LgMacroComponent>,
        release: Vec<LgMacroComponent>,
    ) -> LgMacro {
        let mut data = data("configured", mode == MacroMode::Toggle, sequence);
        data.playback = Some(super::super::db::MacroPlayback {
            mode,
            on_press: press,
            on_release: release,
            standard_delay_ms: None,
        });
        data
    }

    #[test]
    fn hold_release_interrupts_long_delay_and_balances_input() {
        let (sink, events) = recorder();
        let player = MacroPlayer::with_sink(sink.clone());
        let data = configured(
            MacroMode::WhileHeld,
            vec![key(65, true, u64::MAX)],
            vec![],
            vec![],
        );
        assert_eq!(player.press(data, 0, "G6".into()), PlaybackChange::Started);
        assert_eq!(receive(&events), (InputButton::Key(65), true));
        // Releasing a different source cannot terminate this run.
        assert_eq!(player.release("G7", 0), PlaybackChange::Stale);
        assert_eq!(player.release("G6", 0), PlaybackChange::Released);
        assert_eq!(receive(&events), (InputButton::Key(65), false));
        player.stop();
        assert_eq!(sink.events.lock().unwrap().len(), 2);
    }

    #[test]
    fn staged_release_cancels_hold_delay_and_runs_release_phase_once() {
        let (sink, events) = recorder();
        let active = start_with_sink(
            configured(
                MacroMode::Sequence,
                vec![key(65, true, u64::MAX)],
                vec![key(17, true, 0)],
                vec![key(65, false, 0), key(17, false, 0)],
            ),
            false,
            sink.clone(),
        );
        assert_eq!(receive(&events), (InputButton::Key(17), true));
        assert_eq!(receive(&events), (InputButton::Key(65), true));
        active.release_trigger();
        active.release_trigger();
        active.wait();
        assert_eq!(
            *sink.events.lock().unwrap(),
            vec![
                (InputButton::Key(17), true),
                (InputButton::Key(65), true),
                (InputButton::Key(65), false),
                (InputButton::Key(17), false)
            ]
        );
    }

    #[test]
    fn quick_tap_completes_press_phase_skips_hold_and_runs_release() {
        let (sink, events) = recorder();
        let active = start_with_sink(
            configured(
                MacroMode::Sequence,
                vec![key(66, true, 0)],
                vec![key(17, true, 20), key(65, true, 0)],
                vec![key(65, false, 0), key(17, false, 0)],
            ),
            false,
            sink.clone(),
        );
        receive(&events);
        active.release_trigger();
        active.wait();
        assert_eq!(
            *sink.events.lock().unwrap(),
            vec![
                (InputButton::Key(17), true),
                (InputButton::Key(65), true),
                (InputButton::Key(65), false),
                (InputButton::Key(17), false)
            ]
        );
    }

    #[test]
    fn emergency_stop_skips_release_actions_but_releases_held_input() {
        let (sink, events) = recorder();
        let active = start_with_sink(
            configured(
                MacroMode::Sequence,
                vec![key(65, true, u64::MAX)],
                vec![],
                vec![key(66, true, 0)],
            ),
            false,
            sink.clone(),
        );
        receive(&events);
        active.stop();
        assert_eq!(
            *sink.events.lock().unwrap(),
            vec![(InputButton::Key(65), true), (InputButton::Key(65), false)]
        );
    }

    #[test]
    fn standard_delay_overrides_recorded_pause_but_preserves_explicit_pause() {
        let (sink, events) = recorder();
        let recorded = LgMacroComponent {
            explicit_delay: false,
            mouse_button: None,
            mouse_down: None,
            vk_code: None,
            key_down: None,
            delay_ms: Some(u64::MAX),
        };
        let mut manual = recorded.clone();
        manual.explicit_delay = true;
        let mut data = configured(
            MacroMode::Once,
            vec![
                key(65, true, 0),
                recorded,
                key(65, false, 0),
                manual,
                key(66, true, 0),
            ],
            vec![],
            vec![],
        );
        data.playback.as_mut().unwrap().standard_delay_ms = Some(0);
        let active = start_with_sink(data, false, sink.clone());
        assert_eq!(receive(&events), (InputButton::Key(65), true));
        assert_eq!(receive(&events), (InputButton::Key(65), false));
        assert!(events.recv_timeout(Duration::from_millis(30)).is_err());
        active.stop();
        assert_eq!(sink.events.lock().unwrap().len(), 2);
    }

    #[test]
    fn empty_hold_section_waits_for_release_instead_of_finishing() {
        let (sink, events) = recorder();
        let active = start_with_sink(
            configured(
                MacroMode::Sequence,
                vec![],
                vec![key(17, true, 0)],
                vec![key(17, false, 0)],
            ),
            false,
            sink.clone(),
        );
        receive(&events);
        assert!(active.is_running());
        assert!(events.recv_timeout(Duration::from_millis(20)).is_err());
        active.release_trigger();
        active.wait();
        assert_eq!(
            *sink.events.lock().unwrap(),
            vec![(InputButton::Key(17), true), (InputButton::Key(17), false)]
        );
    }

    #[test]
    fn natural_completion_releases_unmatched_input_in_reverse_order() {
        let (sink, _) = recorder();
        let mouse = LgMacroComponent {
            explicit_delay: false,
            mouse_button: Some(1),
            mouse_down: Some(true),
            vk_code: None,
            key_down: None,
            delay_ms: None,
        };
        let active = start_with_sink(
            data("once", false, vec![key(17, true, 0), mouse]),
            false,
            sink.clone(),
        );
        active.wait();
        assert!(!active.is_running());
        active.stop();
        assert_eq!(
            *sink.events.lock().unwrap(),
            vec![
                (InputButton::Key(17), true),
                (InputButton::Mouse(1), true),
                (InputButton::Mouse(1), false),
                (InputButton::Key(17), false),
            ]
        );
    }

    #[test]
    fn cancellation_wakes_huge_delays_in_both_playback_modes() {
        for repeat in [false, true] {
            let (sink, received) = recorder();
            let active = start_with_sink(
                data(
                    "long",
                    repeat,
                    vec![key(65, true, u64::MAX), key(66, true, 0)],
                ),
                repeat,
                sink.clone(),
            );
            assert_eq!(receive(&received), (InputButton::Key(65), true));
            let started = Instant::now();
            active.stop();
            assert!(started.elapsed() < Duration::from_secs(1));
            assert!(!active.is_running());
            assert_eq!(
                *sink.events.lock().unwrap(),
                vec![(InputButton::Key(65), true), (InputButton::Key(65), false),]
            );
        }
    }

    #[test]
    fn simultaneous_stops_release_each_button_exactly_once() {
        let (sink, received) = recorder();
        let active = Arc::new(start_with_sink(
            data("held", true, vec![key(17, true, u64::MAX)]),
            true,
            sink.clone(),
        ));
        receive(&received);
        let stoppers: Vec<_> = (0..8)
            .map(|_| {
                let active = active.clone();
                thread::spawn(move || active.stop())
            })
            .collect();
        for stopper in stoppers {
            stopper.join().unwrap();
        }
        assert_eq!(
            *sink.events.lock().unwrap(),
            vec![(InputButton::Key(17), true), (InputButton::Key(17), false),]
        );
    }

    #[test]
    fn dropping_the_handle_cancels_and_joins_the_run() {
        let (sink, received) = recorder();
        let active = start_with_sink(
            data(
                "drop",
                false,
                vec![key(65, true, u64::MAX), key(66, true, 0)],
            ),
            false,
            sink.clone(),
        );
        receive(&received);
        drop(active);
        assert_eq!(
            *sink.events.lock().unwrap(),
            vec![(InputButton::Key(65), true), (InputButton::Key(65), false),]
        );
    }

    #[test]
    fn empty_toggle_is_cancellable_and_run_ids_are_unique() {
        let (sink, _) = recorder();
        let first = start_with_sink(data("empty", true, vec![]), true, sink.clone());
        let second = start_with_sink(data("empty", true, vec![]), true, sink);
        assert_ne!(first.id(), second.id());
        first.stop();
        second.stop();
        assert!(!first.is_running());
        assert!(!second.is_running());
    }

    #[test]
    fn stop_cannot_race_input_delivery_and_leave_a_late_key_down() {
        struct BlockingSink {
            recorder: Arc<Recorder>,
            entered: mpsc::Sender<()>,
            gate: (Mutex<bool>, Condvar),
        }
        impl InputSink for BlockingSink {
            fn send(&self, button: InputButton, down: bool) {
                if down {
                    self.entered.send(()).unwrap();
                    let (lock, wake) = &self.gate;
                    let mut open = lock.lock().unwrap();
                    while !*open {
                        open = wake.wait(open).unwrap();
                    }
                }
                self.recorder.send(button, down);
            }
        }
        let (recorded, _) = recorder();
        let (entered_tx, entered_rx) = mpsc::channel();
        let sink = Arc::new(BlockingSink {
            recorder: recorded.clone(),
            entered: entered_tx,
            gate: (Mutex::new(false), Condvar::new()),
        });
        let active = Arc::new(start_with_sink(
            data(
                "race",
                false,
                vec![key(65, true, u64::MAX), key(66, true, 0)],
            ),
            false,
            sink.clone(),
        ));
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let stopping = active.clone();
        let (done_tx, done_rx) = mpsc::channel();
        let stopper = thread::spawn(move || {
            stopping.stop();
            done_tx.send(()).unwrap();
        });
        let returned_before_input_finished =
            done_rx.recv_timeout(Duration::from_millis(50)).is_ok();
        *sink.gate.0.lock().unwrap() = true;
        sink.gate.1.notify_all();
        stopper.join().unwrap();
        assert!(!returned_before_input_finished);
        assert_eq!(
            *recorded.events.lock().unwrap(),
            vec![(InputButton::Key(65), true), (InputButton::Key(65), false),]
        );
    }

    #[test]
    fn replacing_a_run_releases_old_input_before_starting_new_input() {
        let (sink, received) = recorder();
        let player = MacroPlayer::with_sink(sink.clone());
        assert_eq!(
            player.toggle(data("old", false, vec![key(65, true, u64::MAX)]), 0),
            PlaybackChange::Started
        );
        receive(&received);
        assert_eq!(
            player.toggle(data("new", false, vec![key(66, true, u64::MAX)]), 0),
            PlaybackChange::Started
        );
        assert_eq!(receive(&received), (InputButton::Key(65), false));
        assert_eq!(receive(&received), (InputButton::Key(66), true));
        player.stop();
        assert_eq!(
            *sink.events.lock().unwrap(),
            vec![
                (InputButton::Key(65), true),
                (InputButton::Key(65), false),
                (InputButton::Key(66), true),
                (InputButton::Key(66), false),
            ]
        );
    }

    #[test]
    fn stop_invalidates_queued_triggers_and_old_ui_notifications() {
        let (sink, received) = recorder();
        let player = MacroPlayer::with_sink(sink);
        let old_generation = player.generation();
        let stopped_generation = player.stop();
        assert_eq!(
            player.toggle(data("stale", false, vec![key(65, true, 0)]), old_generation),
            PlaybackChange::Stale
        );
        assert!(player.is_idle_at(stopped_generation));
        assert_eq!(
            player.toggle(
                data("new", false, vec![key(66, true, u64::MAX)]),
                stopped_generation
            ),
            PlaybackChange::Started
        );
        receive(&received);
        assert!(!player.is_idle_at(stopped_generation));
        player.stop();
    }

    #[test]
    fn same_toggle_stops_but_same_sequence_restarts_and_finished_runs_are_reaped() {
        let (sink, received) = recorder();
        let player = MacroPlayer::with_sink(sink.clone());
        let toggle = data("toggle", true, vec![key(65, true, u64::MAX)]);
        assert_eq!(player.toggle(toggle.clone(), 0), PlaybackChange::Started);
        receive(&received);
        assert_eq!(player.toggle(toggle, 0), PlaybackChange::Stopped);
        let once = data("once", false, vec![key(66, true, u64::MAX)]);
        let generation = player.generation();
        assert_eq!(
            player.trigger(once.clone(), generation),
            PlaybackChange::Started
        );
        assert_eq!(receive(&received), (InputButton::Key(65), false));
        assert_eq!(receive(&received), (InputButton::Key(66), true));
        assert_eq!(player.trigger(once, generation), PlaybackChange::Started);
        assert_eq!(receive(&received), (InputButton::Key(66), false));
        assert_eq!(receive(&received), (InputButton::Key(66), true));
        player.stop();
        let generation = player.generation();
        player.toggle(
            data("done", false, vec![key(67, true, 0), key(67, false, 0)]),
            generation,
        );
        player
            .playback
            .lock()
            .unwrap()
            .active
            .as_ref()
            .unwrap()
            .1
            .wait();
        assert_eq!(player.take_finished(), Some("done".into()));
        assert_eq!(player.take_finished(), None);
    }

    #[test]
    fn rapid_start_stop_keeps_all_input_balanced() {
        let (sink, received) = recorder();
        let player = MacroPlayer::with_sink(sink.clone());
        for _ in 0..50 {
            player.toggle(
                data("rapid", false, vec![key(65, true, u64::MAX)]),
                player.generation(),
            );
            assert_eq!(receive(&received), (InputButton::Key(65), true));
            player.stop();
            assert_eq!(receive(&received), (InputButton::Key(65), false));
        }
        assert_eq!(sink.events.lock().unwrap().len(), 100);
    }

    #[test]
    fn stopping_by_name_releases_only_the_matching_run() {
        let (sink, received) = recorder();
        let player = MacroPlayer::with_sink(sink.clone());
        player.trigger(
            data("held", true, vec![key(65, true, u64::MAX)]),
            player.generation(),
        );
        assert_eq!(receive(&received), (InputButton::Key(65), true));
        let generation = player.generation();
        assert!(!player.stop_if_name("different"));
        assert_eq!(player.generation(), generation);
        assert_eq!(sink.events.lock().unwrap().len(), 1);
        assert!(player.stop_if_name("held"));
        assert_eq!(receive(&received), (InputButton::Key(65), false));
        assert!(!player.stop_if_name("held"));
        assert_eq!(sink.events.lock().unwrap().len(), 2);
    }
    #[test]
    fn active_sequence_card_stops_instead_of_restarting() {
        let (sink, received) = recorder();
        let player = MacroPlayer::with_sink(sink.clone());
        let once = data("once", false, vec![key(65, true, u64::MAX)]);
        assert_eq!(player.toggle(once.clone(), 0), PlaybackChange::Started);
        receive(&received);
        assert_eq!(player.toggle(once, 0), PlaybackChange::Stopped);
        assert!(player.is_idle_at(player.generation()));
        assert_eq!(
            *sink.events.lock().unwrap(),
            vec![(InputButton::Key(65), true), (InputButton::Key(65), false),]
        );
    }
}
