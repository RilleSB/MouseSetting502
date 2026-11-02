use std::collections::HashMap;

pub const KEY_PREFIX: &str = "__KEY__:";

/// A saved chord uses virtual-key numbers, independent of the UI language.
pub fn parse_chord(text: &str) -> Result<Vec<u16>, String> {
    let mut keys = Vec::new();
    for part in text.split('+') {
        let name = part.trim().to_ascii_uppercase();
        let key = match name.as_str() {
            "CTRL" | "CONTROL" => 0x11,
            "SHIFT" => 0x10,
            "ALT" => 0x12,
            "WIN" => 0x5b,
            "SPACE" | "ПРОБЕЛ" => 0x20,
            "TAB" => 9,
            "ENTER" => 13,
            "ESC" | "ESCAPE" => 27,
            "BACKSPACE" => 8,
            "DELETE" | "DEL" => 0x2e,
            "INSERT" | "INS" => 0x2d,
            "HOME" => 0x24,
            "END" => 0x23,
            "PAGEUP" | "PGUP" => 0x21,
            "PAGEDOWN" | "PGDN" => 0x22,
            "LEFT" => 0x25,
            "UP" => 0x26,
            "RIGHT" => 0x27,
            "DOWN" => 0x28,
            _ if name.len() == 1 && name.as_bytes()[0].is_ascii_alphanumeric() => {
                name.as_bytes()[0] as u16
            }
            _ if name.starts_with('F') => name[1..]
                .parse::<u16>()
                .ok()
                .filter(|n| (1..=24).contains(n))
                .map(|n| 0x6f + n)
                .ok_or_else(|| format!("Неизвестная клавиша: {part}"))?,
            _ => return Err(format!("Неизвестная клавиша: {part}. Пример: Ctrl+Shift+A")),
        };
        if keys.contains(&key) {
            return Err(format!("Клавиша повторяется: {part}"));
        }
        keys.push(key);
    }
    Ok(keys)
}

pub fn encode_chord(text: &str) -> Result<String, String> {
    Ok(format!(
        "{KEY_PREFIX}{}",
        parse_chord(text)?
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join(",")
    ))
}

pub fn decode_chord(action: &str) -> Option<Vec<u16>> {
    let keys = action
        .strip_prefix(KEY_PREFIX)?
        .split(',')
        .map(str::parse::<u16>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if keys.is_empty() || keys.iter().any(|key| !(1..=254).contains(key)) {
        return None;
    }
    Some(keys)
}

pub fn action_label(action: &str) -> String {
    let Some(keys) = decode_chord(action) else {
        return action.into();
    };
    keys.iter()
        .map(|&key| match key {
            0x11 => "Ctrl".into(),
            0x10 => "Shift".into(),
            0x12 => "Alt".into(),
            0x5b => "Win".into(),
            0x20 => "Space".into(),
            9 => "Tab".into(),
            13 => "Enter".into(),
            27 => "Esc".into(),
            8 => "Backspace".into(),
            0x2e => "Delete".into(),
            0x2d => "Insert".into(),
            0x24 => "Home".into(),
            0x23 => "End".into(),
            0x21 => "PageUp".into(),
            0x22 => "PageDown".into(),
            0x25 => "Left".into(),
            0x26 => "Up".into(),
            0x27 => "Right".into(),
            0x28 => "Down".into(),
            0x70..=0x87 => format!("F{}", key - 0x6f),
            _ => char::from_u32(key as u32).unwrap_or('?').to_string(),
        })
        .collect::<Vec<_>>()
        .join("+")
}

#[derive(Default)]
pub struct ButtonActions {
    pressed: HashMap<String, String>,
    held_keys: HashMap<u16, usize>,
}

#[derive(Default, Debug, PartialEq)]
pub struct Dispatch {
    pub suppress: bool,
    pub keys: Vec<(u16, bool)>,
    pub action: Option<String>,
}

impl ButtonActions {
    pub fn transition(&mut self, button: &str, down: bool, assignment: &str) -> Dispatch {
        let mut result = Dispatch::default();
        let action = if down {
            if let Some(saved) = self.pressed.get(button) {
                result.suppress = !saved.is_empty();
                return result;
            }
            self.pressed.insert(button.into(), assignment.into());
            assignment.to_string()
        } else {
            let Some(saved) = self.pressed.remove(button) else {
                return result;
            };
            saved
        };
        result.suppress = !action.is_empty();
        if let Some(mut keys) = decode_chord(&action) {
            if !down {
                keys.reverse();
            }
            for key in keys {
                if down {
                    let count = self.held_keys.entry(key).or_default();
                    if *count == 0 {
                        result.keys.push((key, true));
                    }
                    *count += 1;
                } else if let Some(count) = self.held_keys.get_mut(&key) {
                    *count -= 1;
                    if *count == 0 {
                        self.held_keys.remove(&key);
                        result.keys.push((key, false));
                    }
                }
            }
        } else if down && !action.is_empty() {
            result.action = Some(action);
        }
        result
    }

    pub fn release_all(&mut self) -> Vec<(u16, bool)> {
        self.pressed.clear();
        self.held_keys
            .drain()
            .map(|(key, _)| (key, false))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chord_validation_and_labels() {
        assert_eq!(
            action_label(&encode_chord("ctrl+Shift+a").unwrap()),
            "Ctrl+Shift+A"
        );
        for input in ["", "Ctrl+", "F25", "Ctrl+Ctrl", "Unknown"] {
            assert!(encode_chord(input).is_err());
        }
    }

    #[test]
    fn release_uses_original_assignment_and_preserves_shared_modifiers() {
        let mut router = ButtonActions::default();
        let a = encode_chord("Ctrl+A").unwrap();
        let b = encode_chord("Ctrl+B").unwrap();
        assert_eq!(
            router.transition("G4", true, &a).keys,
            vec![(17, true), (65, true)]
        );
        assert!(router.transition("G4", true, &a).keys.is_empty());
        assert_eq!(router.transition("G5", true, &b).keys, vec![(66, true)]);
        assert_eq!(
            router.transition("G4", false, "different").keys,
            vec![(65, false)]
        );
        assert_eq!(
            router.transition("G5", false, "").keys,
            vec![(66, false), (17, false)]
        );
        assert!(router.release_all().is_empty());
    }

    #[test]
    fn rapid_presses_are_not_dropped_and_stop_releases_keys() {
        let mut router = ButtonActions::default();
        for _ in 0..3 {
            assert_eq!(
                router.transition("G6", true, "macro").action.as_deref(),
                Some("macro")
            );
            assert!(router.transition("G6", false, "").suppress);
        }
        router.transition("G6", true, &encode_chord("Shift").unwrap());
        assert_eq!(router.release_all(), vec![(16, false)]);
    }
}
