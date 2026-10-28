#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportType {
    Short = 0x10,
    Long = 0x11,
    VeryLong = 0x12,
}

#[derive(Debug, Clone)]
pub struct HidppPacket {
    pub report_id: u8,
    pub device_idx: u8,
    pub feature_idx: u8,
    pub function_id: u8,
    pub software_id: u8,
    pub params: Vec<u8>,
}

impl HidppPacket {
    pub fn new_long(device_idx: u8, feature_idx: u8, function_id: u8, software_id: u8, params: &[u8]) -> Self {
        Self {
            report_id: ReportType::Long as u8,
            device_idx,
            feature_idx,
            function_id,
            software_id,
            params: params.to_vec(),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = vec![0u8; 20];
        buf[0] = self.report_id;
        buf[1] = self.device_idx;
        buf[2] = self.feature_idx;
        buf[3] = (self.function_id << 4) | (self.software_id & 0x0F);
        for (i, &b) in self.params.iter().take(16).enumerate() {
            buf[4 + i] = b;
        }
        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 4 {
            return Err("Packet too short".into());
        }
        let report_id = bytes[0];
        let device_idx = bytes[1];
        let feature_idx = bytes[2];
        let function_id = bytes[3] >> 4;
        let software_id = bytes[3] & 0x0F;
        let params = bytes[4..].to_vec();

        Ok(Self {
            report_id,
            device_idx,
            feature_idx,
            function_id,
            software_id,
            params,
        })
    }
}
