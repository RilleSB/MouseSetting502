use crate::hidpp::device::HidppDevice;

const FEATURE_COLOR_LED_EFFECTS: u16 = 0x8070;
const FEATURE_ONBOARD_PROFILES: u16 = 0x8100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedZone {
    DpiLeds = 0x00,
    Logo = 0x01,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedEffect {
    Off,
    Static { r: u8, g: u8, b: u8 },
    Breathing { r: u8, g: u8, b: u8, period_ms: u16 },
    ColorCycle { period_ms: u16, brightness: u8 },
}

/// Switches mouse to Host Mode (software-controlled lighting).
/// Should be called once upon connection.
pub fn enable_host_mode(dev: &HidppDevice) -> Result<(), String> {
    let feat_idx = dev.get_feature_index(FEATURE_ONBOARD_PROFILES)?;
    // Function 1: SetMode (0x02 = Host / Software Mode)
    dev.call_function(feat_idx, 0x01, &[0x02])?;
    Ok(())
}

/// Queries the number of lighting zones (e.g. 2 for G502 HERO: DPI + Logo)
pub fn get_zone_count(dev: &HidppDevice) -> Result<u8, String> {
    let feat_idx = dev.get_feature_index(FEATURE_COLOR_LED_EFFECTS)?;
    let resp = dev.call_function(feat_idx, 0x00, &[])?;
    resp.params
        .first()
        .copied()
        .ok_or("Malformed zone info response".into())
}

pub fn set_led_effect(
    dev: &HidppDevice,
    zone: LedZone,
    effect: LedEffect,
    persistent: bool,
) -> Result<(), String> {
    let feat_idx = dev.get_feature_index(FEATURE_COLOR_LED_EFFECTS)?;
    let zone_idx = zone as u8;

    // Trailing persistence byte: 0x00 = RAM only (volatile), 0x01 = RAM & EEPROM (persistent)
    let p_byte = if persistent { 0x01 } else { 0x00 };

    let mut params = [0u8; 13];

    match effect {
        LedEffect::Off => {
            params[0] = zone_idx;
            params[1] = 0x00; // Effect 0: Off
            params[12] = p_byte;
        }
        LedEffect::Static { r, g, b } => {
            params[0] = zone_idx;
            params[1] = 0x01; // Effect 1: Fixed Color
            params[2] = r;
            params[3] = g;
            params[4] = b;
            params[5] = 0x02; // Fixed byte required by Logitech firmware
            params[12] = p_byte;
        }
        LedEffect::Breathing { r, g, b, period_ms } => {
            params[0] = zone_idx;
            params[1] = 0x02; // Effect 2: Pulse / Breathing
            params[2] = r;
            params[3] = g;
            params[4] = b;
            params[5] = (period_ms / 100).clamp(1, 255) as u8;
            params[12] = p_byte;
        }
        LedEffect::ColorCycle {
            period_ms,
            brightness,
        } => {
            params[0] = zone_idx;
            params[1] = 0x03; // Effect 3: Color Cycle
            params[7] = (period_ms >> 8) as u8;
            params[8] = (period_ms & 0xFF) as u8;
            params[9] = brightness;
            params[12] = p_byte;
        }
    }

    dev.call_function(feat_idx, 0x03, &params)?;
    Ok(())
}

/// Turns off all zones by querying zone count and iterating
pub fn turn_off_all_leds(dev: &HidppDevice, persistent: bool) -> Result<(), String> {
    let count = get_zone_count(dev).unwrap_or(2);
    for z in 0..count {
        let zone = match z {
            0 => LedZone::DpiLeds,
            _ => LedZone::Logo,
        };
        set_led_effect(dev, zone, LedEffect::Off, persistent)?;
    }
    Ok(())
}
