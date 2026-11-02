use super::packet::HidppPacket;
use hidapi::HidDevice;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

pub struct HidppDevice {
    device: Mutex<HidDevice>,
    feature_cache: Mutex<HashMap<u16, u8>>,
    device_idx: u8,
    sw_id_counter: AtomicU8,
    notifications: Mutex<VecDeque<HidppPacket>>,
}

impl HidppDevice {
    pub fn new(device: HidDevice, device_idx: u8) -> Self {
        Self {
            device: Mutex::new(device),
            feature_cache: Mutex::new(HashMap::new()),
            device_idx,
            sw_id_counter: AtomicU8::new(1),
            notifications: Mutex::new(VecDeque::new()),
        }
    }

    fn next_sw_id(&self) -> u8 {
        let prev = self.sw_id_counter.fetch_add(1, Ordering::SeqCst);
        (prev % 14) + 1 // IDs 1..=14 (avoiding 0 and 15 which G HUB often uses)
    }

    /// Sends a request and waits for the specific matching response (discarding notifications / alien packets).
    pub fn send_request(
        &self,
        packet: &HidppPacket,
        timeout: Duration,
    ) -> Result<HidppPacket, String> {
        let dev = self.device.lock().unwrap_or_else(|e| e.into_inner());
        let bytes = packet.to_bytes();

        dev.write(&bytes)
            .map_err(|e| format!("HID write error: {e}"))?;

        let deadline = Instant::now() + timeout;
        let mut buf = [0u8; 64];

        while Instant::now() < deadline {
            let remaining_ms = deadline
                .saturating_duration_since(Instant::now())
                .as_millis() as i32;
            let read_timeout = remaining_ms.clamp(10, 200);

            let n = match dev.read_timeout(&mut buf, read_timeout) {
                Ok(n) if n > 0 => n,
                Ok(_) => continue, // No packet within poll slice, continue until deadline
                Err(e) => return Err(format!("HID read error: {e}")),
            };

            let incoming = match HidppPacket::from_bytes(&buf[..n]) {
                Ok(pkt) => pkt,
                Err(_) => continue, // Skip malformed reports
            };

            // Check if this packet is the response to our request
            if incoming.is_response_to(packet) {
                incoming.check_error()?;
                return Ok(incoming);
            } else if incoming.device_idx == self.device_idx
                && incoming.software_id == 0
                && incoming.feature_idx != 0xff
            {
                self.notifications
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push_back(incoming);
            }
        }

        Err(format!(
            "HID response timeout waiting for Feature=0x{:02X}, Func=0x{:X}",
            packet.feature_idx, packet.function_id
        ))
    }

    pub fn call_function(
        &self,
        feature_idx: u8,
        function_id: u8,
        params: &[u8],
    ) -> Result<HidppPacket, String> {
        let sw_id = self.next_sw_id();
        let req = HidppPacket::new_long(self.device_idx, feature_idx, function_id, sw_id, params)?;
        self.send_request(&req, Duration::from_millis(800))
    }

    /// Requests and event reads use the same lock, preserving notification order.
    pub fn read_notification(&self, timeout_ms: i32) -> Result<Option<HidppPacket>, String> {
        let dev = self.device.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(packet) = self
            .notifications
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pop_front()
        {
            return Ok(Some(packet));
        }
        let mut buf = [0u8; 64];
        let n = dev
            .read_timeout(&mut buf, timeout_ms.clamp(0, 50))
            .map_err(|e| format!("HID read error: {e}"))?;
        if n == 0 {
            return Ok(None);
        }
        Ok(HidppPacket::from_bytes(&buf[..n]).ok().filter(|packet| {
            packet.device_idx == self.device_idx
                && packet.software_id == 0
                && packet.feature_idx != 0xff
        }))
    }

    pub fn get_feature_index(&self, feature_id: u16) -> Result<u8, String> {
        // 1. Check cache first without holding lock during I/O
        {
            let cache = self.feature_cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(&idx) = cache.get(&feature_id) {
                return Ok(idx);
            }
        }

        // 2. Query Root feature (0x0000, always index 0x00) -> Function 0: getFeature(feature_id)
        let params = [(feature_id >> 8) as u8, (feature_id & 0xFF) as u8];
        let resp = self.call_function(0x00, 0x00, &params)?;

        let idx = resp
            .params
            .first()
            .copied()
            .ok_or("Malformed Root feature response: empty params")?;
        if idx == 0 {
            return Err(format!(
                "Feature 0x{:04X} not supported by device",
                feature_id
            ));
        }

        // 3. Store in cache
        {
            let mut cache = self.feature_cache.lock().unwrap_or_else(|e| e.into_inner());
            cache.insert(feature_id, idx);
        }

        Ok(idx)
    }
}
