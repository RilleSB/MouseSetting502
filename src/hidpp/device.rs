use std::collections::HashMap;
use std::sync::Mutex;
use hidapi::HidDevice;
use super::packet::HidppPacket;

pub struct HidppDevice {
    device: Mutex<HidDevice>,
    feature_cache: Mutex<HashMap<u16, u8>>,
    device_idx: u8,
}

impl HidppDevice {
    pub fn new(device: HidDevice, device_idx: u8) -> Self {
        Self {
            device: Mutex::new(device),
            feature_cache: Mutex::new(HashMap::new()),
            device_idx,
        }
    }

    pub fn send_request(&self, packet: &HidppPacket) -> Result<HidppPacket, String> {
        let dev = self.device.lock().map_err(|e| e.to_string())?;
        let bytes = packet.to_bytes();

        dev.write(&bytes).map_err(|e| format!("HID write error: {e}"))?;

        let mut buf = [0u8; 64];
        let n = dev.read_timeout(&mut buf, 1000).map_err(|e| format!("HID read error: {e}"))?;
        if n == 0 {
            return Err("HID response timeout".into());
        }

        // HID++ 2.0 error packet format: [0x11, dev_idx, 0xFF, orig_feature, orig_func, error_code, ...]
        if buf[2] == 0xFF {
            let orig_feature = buf[3];
            let orig_func = buf[4];
            let err_code = buf[5];
            return Err(format!(
                "HID++ Error: Feature 0x{:02X}, Func 0x{:02X}, ErrorCode 0x{:02X}",
                orig_feature, orig_func, err_code
            ));
        }

        HidppPacket::from_bytes(&buf[..n])
    }

    pub fn call_function(&self, feature_idx: u8, function_id: u8, params: &[u8]) -> Result<HidppPacket, String> {
        let req = HidppPacket::new_long(self.device_idx, feature_idx, function_id, 0x01, params);
        self.send_request(&req)
    }

    pub fn get_feature_index(&self, feature_id: u16) -> Result<u8, String> {
        let mut cache = self.feature_cache.lock().map_err(|e| e.to_string())?;
        if let Some(&idx) = cache.get(&feature_id) {
            return Ok(idx);
        }

        // Root feature (0x0000) is always index 0x00
        // Function 0: getFeature(feature_id)
        let params = [(feature_id >> 8) as u8, (feature_id & 0xFF) as u8];
        let req = HidppPacket::new_long(self.device_idx, 0x00, 0x00, 0x01, &params);
        let resp = self.send_request(&req)?;

        let idx = resp.params[0];
        if idx == 0 {
            return Err(format!("Feature 0x{:04X} not supported by device", feature_id));
        }

        cache.insert(feature_id, idx);
        Ok(idx)
    }
}
