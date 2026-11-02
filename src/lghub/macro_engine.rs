use super::db::{LgMacro, LgMacroComponent};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(windows)]
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN,
    MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT, MapVirtualKeyW, SendInput,
    keybd_event, mouse_event,
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
    let dw_flags = if down { 0 } else { KEYEVENTF_KEYUP };
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
    let active = start_with_sink(macro_data.clone(), false, Arc::new(SystemInput));
    active.wait();
}

fn start_with_sink(macro_data: LgMacro, repeat: bool, sink: Arc<dyn InputSink>) -> ActiveMacro {
    let shared = Arc::new(SharedRun {
        state: Mutex::new(RunState {
            running: true,
            held: Vec::new(),
        }),
        wake: Condvar::new(),
        sink,
    });
    let run = shared.clone();
    let worker = thread::spawn(move || {
        loop {
            let started = Instant::now();
            let mut completed = true;
            for component in &macro_data.sequence {
                if !execute_component(component, &run) {
                    completed = false;
                    break;
                }
            }
            if !repeat || !completed {
                break;
            }
            // Empty/no-delay loops still wait, and that wait is cancellable.
            if !wait_delay(
                &run,
                Duration::from_millis(10).saturating_sub(started.elapsed()),
            ) {
                break;
            }
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

fn execute_component(component: &LgMacroComponent, run: &SharedRun) -> bool {
    {
        let mut state = run.state.lock().unwrap_or_else(|e| e.into_inner());
        if !state.running {
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
    wait_delay(run, Duration::from_millis(component.delay_ms.unwrap_or(0)))
}

fn wait_delay(run: &SharedRun, delay: Duration) -> bool {
    let started = Instant::now();
    let mut state = run.state.lock().unwrap_or_else(|e| e.into_inner());
    while state.running {
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
        self.change(macro_data, expected, false)
    }

    pub fn trigger(&self, macro_data: LgMacro, expected: u64) -> PlaybackChange {
        self.change(macro_data, expected, true)
    }

    fn change(&self, macro_data: LgMacro, expected: u64, restart_sequence: bool) -> PlaybackChange {
        let mut playback = self.playback.lock().unwrap_or_else(|e| e.into_inner());
        if playback.generation != expected {
            return PlaybackChange::Stale;
        }
        if let Some((name, active)) = playback.active.take() {
            let toggle_off = name == macro_data.name
                && active.is_running()
                && (!restart_sequence || macro_data.is_toggle);
            active.stop();
            if toggle_off {
                playback.generation = playback.generation.wrapping_add(1);
                return PlaybackChange::Stopped;
            }
        }
        let name = macro_data.name.clone();
        let repeat = macro_data.is_toggle;
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
            vk_code: Some(key),
            key_down: Some(down),
            delay_ms: Some(delay),
            mouse_button: None,
            mouse_down: None,
        }
    }
    fn data(name: &str, repeat: bool, sequence: Vec<LgMacroComponent>) -> LgMacro {
        LgMacro {
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

    #[test]
    fn natural_completion_releases_unmatched_input_in_reverse_order() {
        let (sink, _) = recorder();
        let mouse = LgMacroComponent {
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
