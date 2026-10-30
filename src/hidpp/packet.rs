#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportType {
    Short = 0x10,
    Long = 0x11,
    VeryLong = 0x12,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HidppPacket {
    pub report_id: u8,
    pub device_idx: u8,
    pub feature_idx: u8,
    pub function_id: u8,
    pub software_id: u8,
    pub params: Vec<u8>,
}

impl HidppPacket {
    pub fn new_long(device_idx: u8, feature_idx: u8, function_id: u8, software_id: u8, params: &[u8]) -> Result<Self, String> {
        if params.len() > 16 {
            return Err(format!("HID++ long report payload cannot exceed 16 bytes (got {})", params.len()));
        }
        Ok(Self {
            report_id: ReportType::Long as u8,
            device_idx,
            feature_idx,
            function_id,
            software_id: software_id & 0x0F,
            params: params.to_vec(),
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let len = match self.report_id {
            0x10 => 7,
            0x12 => 64,
            _ => 20,
        };
        let mut buf = vec![0u8; len];
        buf[0] = self.report_id;
        buf[1] = self.device_idx;
        buf[2] = self.feature_idx;
        buf[3] = (self.function_id << 4) | (self.software_id & 0x0F);
        for (i, &b) in self.params.iter().take(len - 4).enumerate() {
            buf[4 + i] = b;
        }
        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.is_empty() {
            return Err("Empty HID report".into());
        }

        let report_id = bytes[0];
        let min_len = match report_id {
            0x10 => 7,
            0x11 => 20,
            0x12 => 64,
            _ => return Err(format!("Unknown HID++ Report ID: 0x{:02X}", report_id)),
        };

        if bytes.len() < min_len {
            return Err(format!(
                "HID++ report 0x{:02X} is too short (got {} bytes, expected at least {})",
                report_id, bytes.len(), min_len
            ));
        }

        let device_idx = bytes[1];
        let feature_idx = bytes[2];
        let function_id = bytes[3] >> 4;
        let software_id = bytes[3] & 0x0F;
        let params = bytes[4..min_len].to_vec();

        Ok(Self {
            report_id,
            device_idx,
            feature_idx,
            function_id,
            software_id,
            params,
        })
    }

    /// Checks if this packet is a response to the given request.
    /// In HID++ 2.0:
    /// - Normal response matches: dev_idx, feature_idx, and function/software ID byte.
    /// - Error responses store the original feature at byte 3, then function/software ID and error.
    pub fn is_response_to(&self, req: &HidppPacket) -> bool {
        if self.device_idx != req.device_idx {
            return false;
        }

        let req_func_sw = (req.function_id << 4) | (req.software_id & 0x0F);

        // 1. Normal response
        if self.feature_idx == req.feature_idx {
            let resp_func_sw = (self.function_id << 4) | (self.software_id & 0x0F);
            return resp_func_sw == req_func_sw;
        }

        // 2. Error response: [report_id, dev_idx, 0xFF, orig_feat, orig_func_sw, error_code, ...]
        if self.feature_idx == 0xFF && self.params.len() >= 2 {
            let orig_feat = (self.function_id << 4) | self.software_id;
            let orig_func_sw = self.params[0];
            return orig_feat == req.feature_idx && orig_func_sw == req_func_sw;
        }

        false
    }

    /// If this is an error response, returns (feature_idx, func_id, software_id, error_code, error_desc)
    pub fn check_error(&self) -> Result<(), String> {
        if self.feature_idx == 0xFF && self.params.len() >= 2 {
            let orig_feat = (self.function_id << 4) | self.software_id;
            let orig_func = self.params[0] >> 4;
            let orig_sw = self.params[0] & 0x0F;
            let err_code = self.params[1];

            let desc = match err_code {
                0x01 => "Unknown",
                0x02 => "Invalid Argument",
                0x03 => "Out of Range",
                0x04 => "Hardware Error",
                0x05 => "Resource Error / Device Not Ready",
                0x06 => "Invalid Feature Index",
                0x07 => "Invalid Function ID",
                0x08 => "Busy",
                0x09 => "Unsupported",
                _ => "Unknown Error",
            };

            return Err(format!(
                "HID++ Error: FeatureIdx=0x{:02X}, Func=0x{:X}, SwId=0x{:X} -> Error 0x{:02X} ({})",
                orig_feat, orig_func, orig_sw, err_code, desc
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_long_bounds_check() {
        let ok = HidppPacket::new_long(0xFF, 0x01, 0x00, 0x01, &[0; 16]);
        assert!(ok.is_ok());

        let too_large = HidppPacket::new_long(0xFF, 0x01, 0x00, 0x01, &[0; 17]);
        assert!(too_large.is_err());
    }

    #[test]
    fn test_packet_bytes_roundtrip() {
        let req = HidppPacket::new_long(0xFF, 0x22, 0x03, 0x0A, &[0x00, 0x03, 0x20]).unwrap();
        let bytes = req.to_bytes();
        assert_eq!(bytes.len(), 20);
        assert_eq!(bytes[0], 0x11);
        assert_eq!(bytes[1], 0xFF);
        assert_eq!(bytes[2], 0x22);
        assert_eq!(bytes[3], 0x3A); // func=3, sw=0xA
        assert_eq!(bytes[4], 0x00);
        assert_eq!(bytes[5], 0x03);
        assert_eq!(bytes[6], 0x20);

        let parsed = HidppPacket::from_bytes(&bytes).expect("Failed to parse bytes");
        assert_eq!(parsed.report_id, 0x11);
        assert_eq!(parsed.device_idx, 0xFF);
        assert_eq!(parsed.feature_idx, 0x22);
        assert_eq!(parsed.function_id, 0x03);
        assert_eq!(parsed.software_id, 0x0A);
        assert_eq!(parsed.params[0], 0x00);
        assert_eq!(parsed.params[1], 0x03);
        assert_eq!(parsed.params[2], 0x20);
    }

    #[test]
    fn test_is_response_to_normal() {
        let req = HidppPacket::new_long(0xFF, 0x02, 0x01, 0x03, &[]).unwrap();
        let resp = HidppPacket::new_long(0xFF, 0x02, 0x01, 0x03, &[0xAA, 0xBB]).unwrap();
        assert!(resp.is_response_to(&req));

        // Different function ID
        let resp_diff_func = HidppPacket::new_long(0xFF, 0x02, 0x02, 0x03, &[]).unwrap();
        assert!(!resp_diff_func.is_response_to(&req));

        // Different software ID
        let resp_diff_sw = HidppPacket::new_long(0xFF, 0x02, 0x01, 0x04, &[]).unwrap();
        assert!(!resp_diff_sw.is_response_to(&req));

        // Different feature index
        let resp_diff_feat = HidppPacket::new_long(0xFF, 0x03, 0x01, 0x03, &[]).unwrap();
        assert!(!resp_diff_feat.is_response_to(&req));
    }

    #[test]
    fn test_is_response_to_error() {
        let req = HidppPacket::new_long(0xFF, 0x22, 0x03, 0x0A, &[]).unwrap();
        // Decode actual wire bytes, including the unusual byte-3 error layout.
        let err_packet = HidppPacket::from_bytes(&[0x10, 0xff, 0xff, 0x22, 0x3a, 0x06, 0]).unwrap();
        assert!(err_packet.is_response_to(&req));
        assert!(err_packet.check_error().is_err());

        // Alien error response (e.g. for feature 0x47)
        let alien_err = HidppPacket::from_bytes(&[0x10, 0xff, 0xff, 0x47, 0x11, 0x06, 0]).unwrap();
        assert!(!alien_err.is_response_to(&req));
    }
}
