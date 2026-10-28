use crate::hidpp::device::HidppDevice;

const FEATURE_COLOR_LED_EFFECTS: u16 = 0x8070;

#[derive(Debug, Clone, Copy)]
pub enum LedZone {
    Logo = 0x00,
    DpiIndicators = 0x01,
    All = 0xFF,
}

#[derive(Debug, Clone, Copy)]
pub enum LedEffect {
    Off,
    Static { r: u8, g: u8, b: u8 },
    Breathing { r: u8, g: u8, b: u8, period_ms: u16, brightness: u8 },
    ColorCycle { period_ms: u16, brightness: u8 },
}

pub fn set_led_effect(dev: &HidppDevice, zone: LedZone, effect: LedEffect) -> Result<(), String> {
    let feat_idx = dev.get_feature_index(FEATURE_COLOR_LED_EFFECTS)?;
    let zone_idx = zone as u8;

    let mut params = [0u8; 13];

    match effect {
        LedEffect::Off => {
            // Function 3: set_zone_effect(zone_idx, effect_idx=0)
            params[0] = zone_idx;
            params[1] = 0x00; // Effect 0: Off
        }
        LedEffect::Static { r, g, b } => {
            // Function 3: set_zone_effect(zone_idx, effect_idx=1, [r, g, b])
            params[0] = zone_idx;
            params[1] = 0x01; // Effect 1: Fixed Color
            params[2] = r;
            params[3] = g;
            params[4] = b;
            params[12] = 0x01; // Volatile persistence (RAM)
        }
        LedEffect::Breathing { r, g, b, period_ms, brightness } => {
            params[0] = zone_idx;
            params[1] = 0x02; // Effect 2: Breathing
            params[2] = (period_ms >> 8) as u8;
            params[3] = (period_ms & 0xFF) as u8;
            params[4] = r;
            params[5] = g;
            params[6] = b;
            params[7] = brightness;
            params[12] = 0x01;
        }
        LedEffect::ColorCycle { period_ms, brightness } => {
            params[0] = zone_idx;
            params[1] = 0x03; // Effect 3: Color Cycle
            params[2] = (period_ms >> 8) as u8;
            params[3] = (period_ms & 0xFF) as u8;
            params[4] = brightness;
            params[12] = 0x01;
        }
    }

    let _resp = dev.call_function(feat_idx, 0x03, &params)?;
    Ok(())
}
