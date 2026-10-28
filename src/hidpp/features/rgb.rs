use crate::hidpp::device::HidppDevice;

const FEATURE_COLOR_LED_EFFECTS: u16 = 0x8070;
const FEATURE_ONBOARD_PROFILES: u16 = 0x8100;

#[derive(Debug, Clone, Copy)]
pub enum LedZone {
    DpiLeds = 0x00,
    Logo = 0x01,
    All = 0xFF,
}

#[derive(Debug, Clone, Copy)]
pub enum LedEffect {
    Off,
    Static { r: u8, g: u8, b: u8 },
    Breathing { r: u8, g: u8, b: u8, period_ms: u16 },
    ColorCycle { period_ms: u16, brightness: u8 },
}

/// Switches mouse to Host Mode (software-controlled lighting)
pub fn enable_host_mode(dev: &HidppDevice) -> Result<(), String> {
    if let Ok(feat_idx) = dev.get_feature_index(FEATURE_ONBOARD_PROFILES) {
        // Function 1: SetMode (0x02 = Host / Software Mode)
        let _ = dev.call_function(feat_idx, 0x01, &[0x02]);
    }
    Ok(())
}

pub fn set_led_effect(dev: &HidppDevice, zone: LedZone, effect: LedEffect) -> Result<(), String> {
    // Ensure mouse is in Host Mode so software commands take effect
    let _ = enable_host_mode(dev);

    let feat_idx = dev.get_feature_index(FEATURE_COLOR_LED_EFFECTS)?;
    let zone_idx = zone as u8;

    let mut params = [0u8; 13];

    match effect {
        LedEffect::Off => {
            params[0] = zone_idx;
            params[1] = 0x00; // Effect 0: Off
            params[12] = 0x01; // Persistence
        }
        LedEffect::Static { r, g, b } => {
            params[0] = zone_idx;
            params[1] = 0x01; // Effect 1: Fixed Color
            params[2] = r;
            params[3] = g;
            params[4] = b;
            params[5] = 0x02; // Fixed byte required by Logitech firmware
            params[12] = 0x01; // Persistence: RAM & EEPROM
        }
        LedEffect::Breathing { r, g, b, period_ms } => {
            params[0] = zone_idx;
            params[1] = 0x02; // Effect 2: Pulse / Breathing
            params[2] = r;
            params[3] = g;
            params[4] = b;
            params[5] = (period_ms / 100).clamp(1, 255) as u8; // Speed / Period byte
            params[12] = 0x01;
        }
        LedEffect::ColorCycle { period_ms, brightness } => {
            params[0] = zone_idx;
            params[1] = 0x03; // Effect 3: Color Cycle
            params[2] = 0x00;
            params[3] = 0x00;
            params[4] = 0x00;
            params[7] = (period_ms >> 8) as u8;
            params[8] = (period_ms & 0xFF) as u8;
            params[9] = brightness;
            params[12] = 0x01;
        }
    }

    let _resp = dev.call_function(feat_idx, 0x03, &params)?;
    Ok(())
}
