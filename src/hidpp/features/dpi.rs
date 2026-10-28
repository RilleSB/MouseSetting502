use crate::hidpp::device::HidppDevice;

const FEATURE_ADJUSTABLE_DPI: u16 = 0x2201;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DpiRange {
    pub min_dpi: u16,
    pub max_dpi: u16,
    pub step: u16,
}

pub fn get_current_dpi(dev: &HidppDevice, sensor_idx: u8) -> Result<u16, String> {
    let feat_idx = dev.get_feature_index(FEATURE_ADJUSTABLE_DPI)?;
    // Function 2: getSensorDpi(sensor_idx)
    let resp = dev.call_function(feat_idx, 0x02, &[sensor_idx])?;
    if resp.params.len() < 3 {
        return Err("Malformed DPI response: fewer than 3 parameter bytes".into());
    }
    let dpi = u16::from_be_bytes([resp.params[1], resp.params[2]]);
    Ok(dpi)
}

pub fn set_dpi(dev: &HidppDevice, sensor_idx: u8, target_dpi: u16) -> Result<u16, String> {
    let feat_idx = dev.get_feature_index(FEATURE_ADJUSTABLE_DPI)?;

    // Clamp and align to step 50
    let valid_dpi = ((target_dpi + 25) / 50 * 50).clamp(100, 25600);

    // Function 3: setSensorDpi(sensor_idx, dpi_high, dpi_low)
    let params = [
        sensor_idx,
        (valid_dpi >> 8) as u8,
        (valid_dpi & 0xFF) as u8,
    ];
    let _resp = dev.call_function(feat_idx, 0x03, &params)?;

    // Verify confirmed DPI
    let confirmed = get_current_dpi(dev, sensor_idx)?;
    if confirmed != valid_dpi {
        return Err(format!("Device confirmed DPI {}, expected {}", confirmed, valid_dpi));
    }

    Ok(confirmed)
}

pub fn parse_dpi_list(params: &[u8]) -> Result<DpiRange, String> {
    if params.len() < 3 {
        return Err("Malformed DPI range response".into());
    }

    // Bytes starting from index 1 are pairs of big-endian 16-bit values.
    // A value with the top 3 bits set (>= 0xE000) represents the step size (v & 0x1FFF).
    // Terminator is 0x0000.
    let mut vals = Vec::new();
    let mut step = 0u16;

    let mut i = 1;
    while i + 1 < params.len() {
        let val = u16::from_be_bytes([params[i], params[i + 1]]);
        if val == 0 {
            break;
        }
        if val >= 0xE000 {
            step = val & 0x1FFF;
        } else {
            vals.push(val);
        }
        i += 2;
    }

    if vals.is_empty() {
        // Fallback for G502 HERO
        return Ok(DpiRange {
            min_dpi: 100,
            max_dpi: 25600,
            step: 50,
        });
    }

    let min_dpi = *vals.first().unwrap();
    let max_dpi = *vals.last().unwrap();
    if step == 0 {
        step = 50;
    }

    Ok(DpiRange {
        min_dpi,
        max_dpi,
        step,
    })
}

pub fn get_dpi_range(dev: &HidppDevice, sensor_idx: u8) -> Result<DpiRange, String> {
    let feat_idx = dev.get_feature_index(FEATURE_ADJUSTABLE_DPI)?;
    // Function 1: getSensorDpiList(sensor_idx)
    let resp = dev.call_function(feat_idx, 0x01, &[sensor_idx])?;
    parse_dpi_list(&resp.params)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_dpi_list_valid() {
        // sensor_idx=0, min=100 (0x0064), step=50 (0xE032), max=25600 (0x6400), term=0x0000
        let raw = [
            0x00, // sensor idx
            0x00, 0x64, // 100
            0xE0, 0x32, // step 50
            0x64, 0x00, // 25600
            0x00, 0x00, // terminator
        ];
        let range = parse_dpi_list(&raw).expect("Failed to parse valid DPI list");
        assert_eq!(range.min_dpi, 100);
        assert_eq!(range.max_dpi, 25600);
        assert_eq!(range.step, 50);
    }

    #[test]
    fn test_parse_dpi_list_too_short() {
        let raw = [0x00, 0x64];
        assert!(parse_dpi_list(&raw).is_err());
    }

    #[test]
    fn test_parse_dpi_list_fallback_when_empty() {
        let raw = [0x00, 0x00, 0x00];
        let range = parse_dpi_list(&raw).expect("Fallback range expected");
        assert_eq!(range.min_dpi, 100);
        assert_eq!(range.max_dpi, 25600);
        assert_eq!(range.step, 50);
    }
}
