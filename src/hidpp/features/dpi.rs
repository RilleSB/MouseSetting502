use crate::hidpp::device::HidppDevice;

const FEATURE_ADJUSTABLE_DPI: u16 = 0x2201;

#[derive(Debug, Clone)]
pub struct DpiRange {
    pub min_dpi: u16,
    pub max_dpi: u16,
    pub step: u16,
}

pub fn get_current_dpi(dev: &HidppDevice, sensor_idx: u8) -> Result<u16, String> {
    let feat_idx = dev.get_feature_index(FEATURE_ADJUSTABLE_DPI)?;
    // Function 2: getSensorDpi(sensor_idx)
    let resp = dev.call_function(feat_idx, 0x02, &[sensor_idx])?;
    // resp.params[0] is sensor_idx, params[1..3] is DPI in big-endian
    if resp.params.len() < 3 {
        return Err("Malformed DPI response".into());
    }
    let dpi = u16::from_be_bytes([resp.params[1], resp.params[2]]);
    Ok(dpi)
}

pub fn set_dpi(dev: &HidppDevice, sensor_idx: u8, target_dpi: u16) -> Result<(), String> {
    let feat_idx = dev.get_feature_index(FEATURE_ADJUSTABLE_DPI)?;
    // Function 3: setSensorDpi(sensor_idx, dpi_high, dpi_low)
    let params = [
        sensor_idx,
        (target_dpi >> 8) as u8,
        (target_dpi & 0xFF) as u8,
    ];
    let _resp = dev.call_function(feat_idx, 0x03, &params)?;
    Ok(())
}

pub fn get_dpi_range(dev: &HidppDevice, sensor_idx: u8) -> Result<DpiRange, String> {
    let feat_idx = dev.get_feature_index(FEATURE_ADJUSTABLE_DPI)?;
    // Function 1: getSensorDpiList(sensor_idx)
    let resp = dev.call_function(feat_idx, 0x01, &[sensor_idx])?;
    // Format: [sensor_idx, min_high, min_low, max_high, max_low, step_high, step_low, ...]
    if resp.params.len() < 7 {
        return Err("Malformed DPI range response".into());
    }
    let min_dpi = u16::from_be_bytes([resp.params[1], resp.params[2]]);
    let max_dpi = u16::from_be_bytes([resp.params[3], resp.params[4]]);
    let step = u16::from_be_bytes([resp.params[5], resp.params[6]]);

    Ok(DpiRange {
        min_dpi,
        max_dpi,
        step,
    })
}
