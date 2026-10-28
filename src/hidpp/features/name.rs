use crate::hidpp::device::HidppDevice;

const FEATURE_DEVICE_NAME: u16 = 0x0005;

pub fn get_device_name(dev: &HidppDevice) -> Result<String, String> {
    let feat_idx = dev.get_feature_index(FEATURE_DEVICE_NAME)?;

    // Function 0: getNameLength
    let resp = dev.call_function(feat_idx, 0x00, &[])?;
    let len = resp.params[0] as usize;

    let mut name_bytes = Vec::new();
    let mut offset = 0;
    while offset < len {
        let chunk_resp = dev.call_function(feat_idx, 0x01, &[offset as u8])?;
        for &b in &chunk_resp.params {
            if b == 0 || name_bytes.len() >= len {
                break;
            }
            name_bytes.push(b);
        }
        offset += 16;
    }

    String::from_utf8(name_bytes).map_err(|e| format!("Invalid UTF-8 in device name: {e}"))
}
