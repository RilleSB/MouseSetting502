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
    Static {
        r: u8,
        g: u8,
        b: u8,
    },
    Breathing {
        r: u8,
        g: u8,
        b: u8,
        period_ms: u16,
        brightness: u8,
    },
    ColorCycle {
        period_ms: u16,
        brightness: u8,
    },
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
    let mut params = encode_effect(zone as u8, effect, persistent);
    let result = (|| {
        let info = dev.call_function(feat_idx, 1, &[zone as u8])?;
        if info.params.len() < 4 || info.params[0] != zone as u8 {
            return Err("Malformed lighting zone response".into());
        }
        params[1] = find_effect_index(zone as u8, u16::from(params[1]), info.params[3], |index| {
            dev.call_function(feat_idx, 2, &[zone as u8, index])
                .map(|reply| reply.params)
        })?;
        dev.call_function(feat_idx, 3, &params).map(|_| ())
    })();
    match result {
        Ok(()) => Ok(()),
        // Some zones do not support Off, but accept fixed black.
        Err(_) if effect == LedEffect::Off => set_led_effect(
            dev,
            zone,
            LedEffect::Static { r: 0, g: 0, b: 0 },
            persistent,
        ),
        Err(error) => Err(error),
    }
}

// Build the semantic effect payload; set_led_effect replaces its ID with the
// zone-local index reported by GetZoneEffectInfo before sending SetZoneEffect.
// HID++ 0x8070: zone, effect index, effect-specific payload,
// persistence at byte 12. Periods are big-endian milliseconds; intensity is
// percent (0 means full brightness), not an RGB channel value.
fn encode_effect(zone: u8, effect: LedEffect, persistent: bool) -> [u8; 13] {
    let mut params = [0u8; 13];
    params[0] = zone;
    params[12] = u8::from(persistent);
    match effect {
        LedEffect::Off => {}
        LedEffect::Static { r, g, b } => {
            params[1] = 0x01;
            params[2..5].copy_from_slice(&[r, g, b]);
            params[5] = 0x02;
        }
        LedEffect::Breathing {
            r,
            g,
            b,
            period_ms,
            brightness,
        } => {
            params[1] = 0x0a;
            params[2..5].copy_from_slice(&[r, g, b]);
            params[5..7].copy_from_slice(&period_ms.to_be_bytes());
            params[7] = 0x01; // Sine waveform.
            params[8] = intensity(brightness);
        }
        LedEffect::ColorCycle {
            period_ms,
            brightness,
        } => {
            params[1] = 0x03;
            params[7..9].copy_from_slice(&period_ms.to_be_bytes());
            params[9] = intensity(brightness);
        }
    }
    params
}

fn find_effect_index(
    zone: u8,
    effect_id: u16,
    count: u8,
    mut read: impl FnMut(u8) -> Result<Vec<u8>, String>,
) -> Result<u8, String> {
    for index in 0..count {
        let info = read(index)?;
        if info.len() < 4 || info[0] != zone || info[1] != index {
            return Err("Malformed lighting effect response".into());
        }
        if u16::from_be_bytes([info[2], info[3]]) == effect_id {
            return Ok(index);
        }
    }
    Err(format!(
        "Zone {zone} does not support lighting effect 0x{effect_id:04x}"
    ))
}

fn intensity(percent: u8) -> u8 {
    match percent {
        100..=255 => 0,
        value => value.max(1),
    }
}

/// Attempts every zone even if a preceding zone rejects the effect.
pub fn turn_off_all_leds(dev: &HidppDevice, persistent: bool) -> Result<(), String> {
    let count = get_zone_count(dev)?.min(2);
    let mut errors = Vec::new();
    for zone in [LedZone::DpiLeds, LedZone::Logo]
        .into_iter()
        .take(count as usize)
    {
        if let Err(error) = set_led_effect(dev, zone, LedEffect::Off, persistent) {
            errors.push(format!("{zone:?}: {error}"));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_id_is_translated_using_the_zones_own_order() {
        // Firmware exposes breathing ID 0x0a at index 2, cycle ID 3 at index 3.
        let ids = [0u16, 1, 0x0a, 3];
        let read = |index: u8| {
            let [hi, lo] = ids[index as usize].to_be_bytes();
            Ok(vec![1, index, hi, lo])
        };
        assert_eq!(find_effect_index(1, 0x0a, 4, read).unwrap(), 2);
        assert_eq!(find_effect_index(1, 3, 4, read).unwrap(), 3);
        assert!(find_effect_index(1, 4, 4, read).is_err());
        assert!(find_effect_index(1, 0x0a, 4, |_| Ok(vec![0, 0])).is_err());
    }

    #[test]
    fn breathing_uses_effect_0a_and_big_endian_milliseconds() {
        assert_eq!(
            encode_effect(
                1,
                LedEffect::Breathing {
                    r: 12,
                    g: 34,
                    b: 56,
                    period_ms: 3000,
                    brightness: 75
                },
                false
            ),
            [1, 0x0a, 12, 34, 56, 0x0b, 0xb8, 1, 75, 0, 0, 0, 0]
        );
    }

    #[test]
    fn cycle_intensity_is_percent_and_full_brightness_is_zero() {
        assert_eq!(
            encode_effect(
                1,
                LedEffect::ColorCycle {
                    period_ms: 10000,
                    brightness: 100
                },
                true
            ),
            [1, 3, 0, 0, 0, 0, 0, 0x27, 0x10, 0, 0, 0, 1]
        );
        assert_eq!(
            encode_effect(
                0,
                LedEffect::ColorCycle {
                    period_ms: 20000,
                    brightness: 25
                },
                false
            )[9],
            25
        );
    }

    #[test]
    fn off_does_not_leave_color_or_effect_data_in_packet() {
        assert_eq!(
            encode_effect(1, LedEffect::Off, false),
            [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        );
    }
}
