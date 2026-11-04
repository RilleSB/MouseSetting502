//! An editor draft never changes saved macros until the persistence transaction succeeds.
use super::bridge::MainWindow;
use crate::lghub::{KeybindListener, LgHubSettings, LgMacro, LgMacroComponent};
use crate::lghub::{
    db::{MacroMode, MacroPlayback},
    macro_engine::MacroPlayer,
};
use slint::{ComponentHandle, VecModel};
use std::{
    cell::RefCell,
    collections::BTreeSet,
    rc::Rc,
    sync::{Arc, Mutex},
    time::Instant,
};

#[derive(Default)]
struct Draft {
    phases: [Vec<LgMacroComponent>; 3],
    recording: bool,
    last_event: Option<Instant>,
    held: BTreeSet<(bool, u16)>,
    resume_listener: bool,
}

fn input(mouse: bool, code: u16, down: bool) -> LgMacroComponent {
    LgMacroComponent {
        explicit_delay: false,
        mouse_button: mouse.then_some(code as u8),
        mouse_down: mouse.then_some(down),
        vk_code: (!mouse).then_some(code),
        key_down: (!mouse).then_some(down),
        delay_ms: None,
    }
}
fn delay(ms: u64) -> LgMacroComponent {
    LgMacroComponent {
        explicit_delay: true,
        mouse_button: None,
        mouse_down: None,
        vk_code: None,
        key_down: None,
        delay_ms: Some(ms),
    }
}
fn split_delays(components: &[LgMacroComponent]) -> Vec<LgMacroComponent> {
    let mut result = Vec::new();
    for component in components {
        if component.mouse_button.is_some() || component.vk_code.is_some() {
            let mut event = component.clone();
            event.delay_ms = None;
            result.push(event);
            if let Some(ms) = component.delay_ms.filter(|ms| *ms > 0) {
                let mut pause = delay(ms);
                pause.explicit_delay = false;
                result.push(pause);
            }
        } else {
            result.push(component.clone());
        }
    }
    result
}
fn mode(index: i32) -> MacroMode {
    match index {
        1 => MacroMode::WhileHeld,
        2 => MacroMode::Toggle,
        3 => MacroMode::Sequence,
        _ => MacroMode::Once,
    }
}
fn mode_index(mode: MacroMode) -> i32 {
    match mode {
        MacroMode::Once => 0,
        MacroMode::WhileHeld => 1,
        MacroMode::Toggle => 2,
        MacroMode::Sequence => 3,
    }
}
fn label(event: &LgMacroComponent) -> String {
    if let Some(ms) = event.delay_ms {
        return format!("Пауза · {ms} мс");
    }
    let (name, down) = if let Some(button) = event.mouse_button {
        (
            match button {
                1 => "ЛКМ",
                2 => "ПКМ",
                3 => "СКМ",
                4 => "Назад",
                5 => "Вперёд",
                _ => "Мышь",
            }
            .into(),
            event.mouse_down.unwrap_or(false),
        )
    } else {
        (
            crate::lghub::custom_macros::vk_to_string(event.vk_code.unwrap_or(0)),
            event.key_down.unwrap_or(false),
        )
    };
    format!(
        "{name}\n{}",
        if down {
            "↓ Нажатие"
        } else {
            "↑ Отпускание"
        }
    )
}

#[derive(Clone)]
pub(super) struct DraftUi {
    weak: slint::Weak<MainWindow>,
    data: Rc<RefCell<Draft>>,
    model: Rc<VecModel<slint::SharedString>>,
    listener: Arc<KeybindListener>,
    player: Arc<MacroPlayer>,
}
impl DraftUi {
    fn phase(app: &MainWindow) -> usize {
        app.get_builder_phase().clamp(0, 2) as usize
    }
    fn redraw(&self) {
        if let Some(app) = self.weak.upgrade() {
            let data = self.data.borrow();
            self.model.set_vec(
                data.phases[Self::phase(&app)]
                    .iter()
                    .map(|e| {
                        let mut e = e.clone();
                        if app.get_builder_standard_delay()
                            && !e.explicit_delay
                            && e.mouse_button.is_none()
                            && e.vk_code.is_none()
                        {
                            e.delay_ms = app.get_builder_default_delay().parse().ok();
                        }
                        label(&e).into()
                    })
                    .collect::<Vec<_>>(),
            );
            app.set_builder_phase_counts(slint::ModelRc::new(VecModel::from(
                data.phases
                    .iter()
                    .map(|p| p.len() as i32)
                    .collect::<Vec<_>>(),
            )));
        }
    }
    fn open(&self, target: Option<LgMacro>, imported: bool) {
        let Some(app) = self.weak.upgrade() else {
            return;
        };
        let resume_listener = if app.get_show_macro_builder() {
            self.data.borrow().resume_listener
        } else {
            self.listener.is_running()
        };
        self.player.stop();
        self.listener.stop();
        app.set_listener_active(false);
        app.set_active_macro_name("".into());
        *self.data.borrow_mut() = Draft {
            resume_listener,
            ..Draft::default()
        };
        app.set_builder_phase(1);
        app.set_builder_selected_step(-1);
        app.set_builder_error("".into());
        app.set_builder_recording(false);
        app.set_builder_key_input("A".into());
        app.set_builder_delay_input("50".into());
        if let Some(data) = target {
            let index = mode_index(data.mode());
            app.set_builder_editing_original(if imported {
                "".into()
            } else {
                data.name.clone().into()
            });
            app.set_builder_name(if imported {
                format!("{} (копия)", data.name).into()
            } else {
                data.name.clone().into()
            });
            app.set_builder_mode(index);
            app.set_builder_is_toggle(index == 2);
            app.set_builder_standard_delay(
                data.playback
                    .as_ref()
                    .and_then(|p| p.standard_delay_ms)
                    .is_some(),
            );
            app.set_builder_default_delay(
                data.playback
                    .as_ref()
                    .and_then(|p| p.standard_delay_ms)
                    .unwrap_or(50)
                    .to_string()
                    .into(),
            );
            let mut draft = self.data.borrow_mut();
            draft.phases[1] = split_delays(&data.sequence);
            if let Some(spec) = data.playback {
                draft.phases[0] = split_delays(&spec.on_press);
                draft.phases[2] = split_delays(&spec.on_release);
            }
        } else {
            app.set_builder_editing_original("".into());
            app.set_builder_name("Новый макрос".into());
            app.set_builder_mode(0);
            app.set_builder_is_toggle(false);
            app.set_builder_standard_delay(true);
            app.set_builder_default_delay("50".into());
        }
        self.redraw();
        app.set_show_macro_builder(true);
    }
    fn stop_recording(&self) {
        if let Some(app) = self.weak.upgrade() {
            let mut data = self.data.borrow_mut();
            let held = std::mem::take(&mut data.held);
            for (mouse, key) in held.into_iter().rev() {
                data.phases[Self::phase(&app)].push(input(mouse, key, false));
            }
            data.recording = false;
            data.last_event = None;
            app.set_builder_recording(false);
        }
        self.redraw();
    }
    fn close(&self) {
        self.stop_recording();
        if let Some(app) = self.weak.upgrade() {
            app.set_show_macro_builder(false);
            if self.data.borrow().resume_listener {
                self.listener.restart();
            }
            app.set_listener_active(self.listener.is_running());
        }
    }
    fn append(&self, event: LgMacroComponent) {
        if let Some(app) = self.weak.upgrade() {
            let mut data = self.data.borrow_mut();
            let steps = &mut data.phases[Self::phase(&app)];
            if (event.mouse_button.is_some() || event.vk_code.is_some())
                && steps
                    .last()
                    .is_some_and(|e| e.mouse_button.is_some() || e.vk_code.is_some())
            {
                let mut pause = delay(0);
                pause.explicit_delay = false;
                steps.push(pause);
            }
            steps.push(event);
            app.set_builder_error("".into());
        }
        self.redraw();
    }
    fn capture(&self, mouse: bool, code: u16, down: bool) {
        let Some(app) = self.weak.upgrade() else {
            return;
        };
        let mut data = self.data.borrow_mut();
        if !data.recording {
            return;
        }
        if down {
            if !data.held.insert((mouse, code)) {
                return;
            }
        } else if !data.held.remove(&(mouse, code)) {
            return;
        }
        let now = Instant::now();
        if let Some(last) = data.last_event {
            let mut pause =
                delay(now.duration_since(last).as_millis().min(u64::MAX as u128) as u64);
            pause.explicit_delay = false;
            data.phases[Self::phase(&app)].push(pause);
        }
        data.last_event = Some(now);
        data.phases[Self::phase(&app)].push(input(mouse, code, down));
        drop(data);
        self.redraw();
    }
    pub fn build(&self, app: &MainWindow, name: &str) -> Result<LgMacro, String> {
        self.stop_recording();
        let data = self.data.borrow();
        let mode = mode(app.get_builder_mode());
        if if mode == MacroMode::Sequence {
            data.phases.iter().all(Vec::is_empty)
        } else {
            data.phases[1].is_empty()
        } {
            return Err("Добавьте действие или задержку — пустой макрос не сохраняется".into());
        }
        let standard_delay_ms = if app.get_builder_standard_delay() {
            Some(parse_delay(app.get_builder_default_delay().as_str())?)
        } else {
            None
        };
        Ok(LgMacro {
            name: name.into(),
            macro_type: match mode {
                MacroMode::Once => "once",
                MacroMode::WhileHeld => "while_held",
                MacroMode::Toggle => "toggle",
                MacroMode::Sequence => "staged",
            }
            .into(),
            action_name: Some(name.into()),
            is_toggle: mode == MacroMode::Toggle,
            sequence: data.phases[1].clone(),
            playback: Some(MacroPlayback {
                mode,
                on_press: data.phases[0].clone(),
                on_release: data.phases[2].clone(),
                standard_delay_ms,
            }),
        })
    }
}
fn set_pause_after(steps: &mut Vec<LgMacroComponent>, index: usize, ms: u64) -> Option<usize> {
    let event = steps.get(index)?;
    let pause_index = if event.mouse_button.is_none() && event.vk_code.is_none() {
        index
    } else {
        index + 1
    };
    if steps
        .get(pause_index)
        .is_some_and(|e| e.mouse_button.is_none() && e.vk_code.is_none())
    {
        steps[pause_index] = delay(ms);
    } else {
        steps.insert(pause_index, delay(ms));
    }
    Some(pause_index)
}

fn parse_delay(text: &str) -> Result<u64, String> {
    text.trim()
        .parse::<u64>()
        .ok()
        .filter(|ms| *ms <= 3_600_000)
        .ok_or_else(|| "Задержка должна быть целым числом от 0 до 3600000 мс".into())
}
fn key_code(text: &str) -> Option<u16> {
    use slint::platform::Key;
    let ch = text.chars().next()?;
    for (key, vk) in [
        (Key::Return, 13),
        (Key::Tab, 9),
        (Key::Backspace, 8),
        (Key::Shift, 0xa0),
        (Key::ShiftR, 0xa1),
        (Key::Control, 0xa2),
        (Key::ControlR, 0xa3),
        (Key::Alt, 0xa4),
        (Key::AltGr, 0xa5),
        (Key::LeftArrow, 37),
        (Key::UpArrow, 38),
        (Key::RightArrow, 39),
        (Key::DownArrow, 40),
        (Key::Delete, 46),
        (Key::Home, 36),
        (Key::End, 35),
        (Key::PageUp, 33),
        (Key::PageDown, 34),
    ] {
        if ch == char::from(key) {
            return Some(vk);
        }
    }
    if ch == ' ' {
        return Some(32);
    }
    if (char::from(Key::F1)..=char::from(Key::F24)).contains(&ch) {
        return Some(0x70 + (ch as u32 - char::from(Key::F1) as u32) as u16);
    }
    #[cfg(windows)]
    if !ch.is_control() && ch as u32 <= u16::MAX as u32 {
        let mapped =
            unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::VkKeyScanW(ch as u16) };
        if mapped != -1 {
            return Some((mapped as u16) & 0xff);
        }
    }
    if ch.is_ascii_alphanumeric() {
        return Some(ch.to_ascii_uppercase() as u16);
    }
    // Unsupported layout/special keys remain visible as an error, never a different key.
    None
}

pub(super) fn setup(
    app: &MainWindow,
    model: Rc<VecModel<slint::SharedString>>,
    custom: Arc<Mutex<Vec<LgMacro>>>,
    imported: Arc<Option<LgHubSettings>>,
    listener: Arc<KeybindListener>,
    player: Arc<MacroPlayer>,
) -> DraftUi {
    let ui = DraftUi {
        weak: app.as_weak(),
        data: Rc::new(RefCell::new(Draft::default())),
        model,
        listener,
        player,
    };
    let x = ui.clone();
    app.on_open_macro_builder(move || x.open(None, false));
    let x = ui.clone();
    app.on_edit_custom_macro(move |name| {
        let saved = custom
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|m| m.name == name.as_str())
            .cloned();
        if let Some(data) = saved {
            x.open(Some(data), false);
        } else if let Some(data) = imported
            .as_ref()
            .as_ref()
            .and_then(|s| {
                s.cards
                    .iter()
                    .filter_map(|c| c.macro_data.as_ref())
                    .find(|m| m.name == name.as_str())
            })
            .cloned()
        {
            x.open(Some(data), true);
        }
    });
    let x = ui.clone();
    app.on_close_macro_builder(move || x.close());
    let x = ui.clone();
    app.on_builder_refresh_timing(move || x.redraw());
    let x = ui.clone();
    app.on_builder_set_phase(move |phase| {
        x.stop_recording();
        if let Some(a) = x.weak.upgrade() {
            a.set_builder_phase(phase.clamp(0, 2));
            a.set_builder_selected_step(-1);
        }
        x.redraw();
    });
    let x = ui.clone();
    app.on_builder_add_mouse(move |button, down, _| {
        if (1..=5).contains(&button) {
            x.append(input(true, button as u16, down));
        }
    });
    let x = ui.clone();
    app.on_builder_add_key(move |key, down, _| {
        if (1..=254).contains(&key) {
            x.append(input(false, key as u16, down));
        }
    });
    let x = ui.clone();
    app.on_builder_add_chord(move |text, kind| {
        match crate::lghub::button_actions::parse_chord(text.as_str()) {
            Ok(keys) => {
                if kind != 2 {
                    for key in &keys {
                        x.append(input(false, *key, true));
                    }
                }
                if kind != 1 {
                    for key in keys.into_iter().rev() {
                        x.append(input(false, key, false));
                    }
                }
            }
            Err(error) => {
                if let Some(a) = x.weak.upgrade() {
                    a.set_builder_error(error.into());
                }
            }
        }
    });
    let x = ui.clone();
    app.on_builder_add_delay(move |ms| {
        if (0..=3_600_000).contains(&ms) {
            x.append(delay(ms as u64));
        }
    });
    let x = ui.clone();
    app.on_builder_insert_delay(move |text| match parse_delay(text.as_str()) {
        Ok(ms) => x.append(delay(ms)),
        Err(e) => {
            if let Some(a) = x.weak.upgrade() {
                a.set_builder_error(e.into());
            }
        }
    });
    let x = ui.clone();
    app.on_builder_clear_steps(move || {
        x.stop_recording();
        if let Some(a) = x.weak.upgrade() {
            x.data.borrow_mut().phases[DraftUi::phase(&a)].clear();
            a.set_builder_selected_step(-1);
        }
        x.redraw();
    });
    let x = ui.clone();
    app.on_builder_remove_step(move |idx| {
        if let Some(a) = x.weak.upgrade() {
            let mut d = x.data.borrow_mut();
            let steps = &mut d.phases[DraftUi::phase(&a)];
            if idx >= 0 && (idx as usize) < steps.len() {
                steps.remove(idx as usize);
            }
            a.set_builder_selected_step(-1);
        }
        x.redraw();
    });
    for up in [true, false] {
        let x = ui.clone();
        let handler = move |idx: i32| {
            if let Some(a) = x.weak.upgrade() {
                let mut d = x.data.borrow_mut();
                let steps = &mut d.phases[DraftUi::phase(&a)];
                let other = if up { idx - 1 } else { idx + 1 };
                if idx >= 0
                    && other >= 0
                    && (idx as usize) < steps.len()
                    && (other as usize) < steps.len()
                {
                    steps.swap(idx as usize, other as usize);
                    a.set_builder_selected_step(other);
                }
            }
            x.redraw();
        };
        if up {
            app.on_builder_move_step_up(handler);
        } else {
            app.on_builder_move_step_down(handler);
        }
    }
    let x = ui.clone();
    app.on_builder_select_step(move |idx| {
        if let Some(a) = x.weak.upgrade() {
            a.set_builder_selected_step(idx);
            if let Some(step) = x.data.borrow().phases[DraftUi::phase(&a)].get(idx as usize) {
                a.set_builder_delay_input(step.delay_ms.unwrap_or(50).to_string().into());
            }
        }
    });
    let x = ui.clone();
    app.on_builder_update_delay(move |idx, text| {
        if let Some(a) = x.weak.upgrade() {
            match parse_delay(text.as_str()) {
                Ok(ms) => {
                    let mut d = x.data.borrow_mut();
                    let steps = &mut d.phases[DraftUi::phase(&a)];
                    if let Some(index) = set_pause_after(steps, idx as usize, ms) {
                        a.set_builder_selected_step(index as i32);
                    }
                    a.set_builder_error("".into());
                }
                Err(e) => a.set_builder_error(e.into()),
            }
        }
        x.redraw();
    });
    let x = ui.clone();
    app.on_builder_toggle_recording(move || {
        if x.data.borrow().recording {
            x.stop_recording();
        } else {
            x.data.borrow_mut().recording = true;
            x.data.borrow_mut().last_event = None;
            if let Some(a) = x.weak.upgrade() {
                a.set_builder_recording(true);
                a.set_builder_error("".into());
            }
        }
    });
    let x = ui.clone();
    app.on_builder_capture_key(move |text, down| {
        if text.starts_with(char::from(slint::platform::Key::Escape)) {
            x.stop_recording();
            return;
        }
        if let Some(code) = key_code(text.as_str()) {
            x.capture(false, code, down);
        } else if let Some(a) = x.weak.upgrade() {
            a.set_builder_error("Эту клавишу добавьте по имени в поле сочетания".into());
        }
    });
    let x = ui.clone();
    app.on_builder_capture_mouse(move |button, down| {
        if (1..=5).contains(&button) {
            x.capture(true, button as u16, down);
        }
    });
    ui
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editing_pause_after_event_replaces_existing_gap_instead_of_adding_delays() {
        let mut steps = vec![input(false, 65, true), delay(1), input(false, 65, false)];
        assert_eq!(set_pause_after(&mut steps, 0, 20), Some(1));
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[1].delay_ms, Some(20));
        assert!(steps[1].explicit_delay);
        assert_eq!(set_pause_after(&mut steps, 1, 5), Some(1));
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[1].delay_ms, Some(5));
    }

    #[test]
    fn recording_resolves_function_and_modifier_keys_without_treating_them_as_text() {
        use slint::platform::Key;
        assert_eq!(key_code(&slint::SharedString::from(Key::F12)), Some(0x7b));
        assert_eq!(
            key_code(&slint::SharedString::from(Key::ControlR)),
            Some(0xa3)
        );
    }
    #[test]
    fn split_legacy_timing_keeps_order_and_exact_values() {
        let mut event = input(false, 65, true);
        event.delay_ms = Some(1);
        let split = split_delays(&[event, delay(0), input(false, 65, false)]);
        assert_eq!(split.len(), 4);
        assert_eq!(split[0].delay_ms, None);
        assert_eq!(split[1].delay_ms, Some(1));
        assert_eq!(split[2].delay_ms, Some(0));
        assert_eq!(split[3].key_down, Some(false));
    }
    #[test]
    fn delay_validation_does_not_turn_invalid_input_into_an_action() {
        for value in ["-1", "1.5", "abc", "3600001", ""] {
            assert!(parse_delay(value).is_err());
        }
        assert_eq!(parse_delay("1").unwrap(), 1);
        assert_eq!(parse_delay("0").unwrap(), 0);
    }
}
