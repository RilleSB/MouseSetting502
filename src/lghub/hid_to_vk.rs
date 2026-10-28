/// Maps a USB HID Usage code (Usage Page 0x07 - Keyboard/Keypad) to a Windows Virtual-Key (VK) code.
pub fn hid_usage_to_vk(usage: u32) -> Option<u16> {
    match usage {
        // A..Z (HID 0x04..0x1D -> VK_A..VK_Z: 0x41..0x5A)
        0x04..=0x1D => Some((0x41 + (usage - 0x04)) as u16),

        // 1..9 (HID 0x1E..0x26 -> VK_1..VK_9: 0x31..0x39)
        0x1E..=0x26 => Some((0x31 + (usage - 0x1E)) as u16),
        // 0 (HID 0x27 -> VK_0: 0x30)
        0x27 => Some(0x30),

        // Enter, Escape, Backspace, Tab, Space
        0x28 => Some(0x0D), // VK_RETURN
        0x29 => Some(0x1B), // VK_ESCAPE
        0x2A => Some(0x08), // VK_BACK
        0x2B => Some(0x09), // VK_TAB
        0x2C => Some(0x20), // VK_SPACE

        // Punctuation
        0x2D => Some(0xBD), // VK_OEM_MINUS
        0x2E => Some(0xBB), // VK_OEM_PLUS
        0x2F => Some(0xDB), // VK_OEM_4 ([)
        0x30 => Some(0xDD), // VK_OEM_6 (])
        0x31 => Some(0xDC), // VK_OEM_5 (\)
        0x33 => Some(0xBA), // VK_OEM_1 (;)
        0x34 => Some(0xDE), // VK_OEM_7 (')
        0x35 => Some(0xC0), // VK_OEM_3 (`)
        0x36 => Some(0xBC), // VK_OEM_COMMA (,)
        0x37 => Some(0xBE), // VK_OEM_PERIOD (.)
        0x38 => Some(0xBF), // VK_OEM_2 (/)

        // CapsLock
        0x39 => Some(0x14), // VK_CAPITAL

        // F1..F12 (HID 0x3A..0x45 -> VK_F1..VK_F12: 0x70..0x7B)
        0x3A..=0x45 => Some((0x70 + (usage - 0x3A)) as u16),

        // Navigation & Editing
        0x46 => Some(0x2C), // VK_SNAPSHOT (PrintScreen)
        0x47 => Some(0x91), // VK_SCROLL
        0x48 => Some(0x13), // VK_PAUSE
        0x49 => Some(0x2D), // VK_INSERT
        0x4A => Some(0x24), // VK_HOME
        0x4B => Some(0x21), // VK_PRIOR (PageUp)
        0x4C => Some(0x2E), // VK_DELETE
        0x4D => Some(0x23), // VK_END
        0x4E => Some(0x22), // VK_NEXT (PageDown)
        0x4F => Some(0x27), // VK_RIGHT
        0x50 => Some(0x25), // VK_LEFT
        0x51 => Some(0x28), // VK_DOWN
        0x52 => Some(0x26), // VK_UP

        // Keypad
        0x53 => Some(0x90), // VK_NUMLOCK
        0x54 => Some(0x6F), // VK_DIVIDE
        0x55 => Some(0x6A), // VK_MULTIPLY
        0x56 => Some(0x6D), // VK_SUBTRACT
        0x57 => Some(0x6B), // VK_ADD
        0x58 => Some(0x0D), // VK_RETURN
        0x59..=0x61 => Some((0x61 + (usage - 0x59)) as u16), // VK_NUMPAD1..VK_NUMPAD9
        0x62 => Some(0x60), // VK_NUMPAD0
        0x63 => Some(0x6E), // VK_DECIMAL

        // F13..F24 (HID 0x68..0x73 -> VK_F13..VK_F24: 0x7C..0x87)
        0x68..=0x73 => Some((0x7C + (usage - 0x68)) as u16),

        // Modifiers (HID 224..231 / 0xE0..0xE7)
        224 => Some(0xA2), // VK_LCONTROL
        225 => Some(0xA0), // VK_LSHIFT
        226 => Some(0xA4), // VK_LMENU (LAlt)
        227 => Some(0x5B), // VK_LWIN
        228 => Some(0xA3), // VK_RCONTROL
        229 => Some(0xA1), // VK_RSHIFT
        230 => Some(0xA5), // VK_RMENU (RAlt)
        231 => Some(0x5C), // VK_RWIN

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_letters() {
        assert_eq!(hid_usage_to_vk(0x04), Some(0x41)); // 'A'
        assert_eq!(hid_usage_to_vk(16), Some(0x4D));   // 'M' (Usage 16)
        assert_eq!(hid_usage_to_vk(0x1D), Some(0x5A)); // 'Z'
    }

    #[test]
    fn test_numbers() {
        assert_eq!(hid_usage_to_vk(0x1E), Some(0x31)); // '1'
        assert_eq!(hid_usage_to_vk(0x27), Some(0x30)); // '0'
    }

    #[test]
    fn test_modifiers() {
        assert_eq!(hid_usage_to_vk(224), Some(0xA2)); // VK_LCONTROL
        assert_eq!(hid_usage_to_vk(225), Some(0xA0)); // VK_LSHIFT
        assert_eq!(hid_usage_to_vk(226), Some(0xA4)); // VK_LMENU (LAlt)
        assert_eq!(hid_usage_to_vk(227), Some(0x5B)); // VK_LWIN
        assert_eq!(hid_usage_to_vk(228), Some(0xA3)); // VK_RCONTROL
        assert_eq!(hid_usage_to_vk(231), Some(0x5C)); // VK_RWIN
    }

    #[test]
    fn test_function_keys() {
        assert_eq!(hid_usage_to_vk(0x3A), Some(0x70)); // F1
        assert_eq!(hid_usage_to_vk(0x45), Some(0x7B)); // F12
        assert_eq!(hid_usage_to_vk(0x68), Some(0x7C)); // F13
        assert_eq!(hid_usage_to_vk(0x73), Some(0x87)); // F24
    }

    #[test]
    fn test_unknown() {
        assert_eq!(hid_usage_to_vk(0xFFFF), None);
        assert_eq!(hid_usage_to_vk(0x00), None);
    }
}
