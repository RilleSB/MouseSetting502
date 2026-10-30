//! G502 HERO MouseButtonSpy (0x8110), based on the device trace in
//! https://github.com/libratbag/libratbag/issues/1831.
use crate::hidpp::{HidppDevice, packet::HidppPacket};

pub const FEATURE_MOUSE_BUTTON_SPY: u16 = 0x8110;

pub fn start_button_spy(dev: &HidppDevice) -> Result<u8, String> {
    let index = dev.get_feature_index(FEATURE_MOUSE_BUTTON_SPY)?;
    let count = dev.call_function(index, 0, &[])?;
    if count.params.first().copied() != Some(11) {
        return Err("Unsupported mouse button layout (expected G502 HERO, 11 buttons)".into());
    }
    dev.call_function(index, 1, &[])?;
    Ok(index)
}

pub fn button_bitmap(packet: &HidppPacket, index: u8) -> Option<u16> {
    if packet.report_id != 0x11 || packet.feature_idx != index
        || packet.function_id != 0 || packet.software_id != 0 {
        return None;
    }
    let bytes = packet.params.get(..2)?;
    Some(u16::from_be_bytes([bytes[0], bytes[1]]) & 0x07ff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinguishes_button_events_from_replies_and_other_features() {
        let mut packet = HidppPacket::new_long(0xff, 13, 0, 0, &[0x01, 0x60]).unwrap();
        assert_eq!(button_bitmap(&packet, 13), Some(0x160));
        packet.software_id = 1;
        assert_eq!(button_bitmap(&packet, 13), None);
        packet.software_id = 0;
        assert_eq!(button_bitmap(&packet, 12), None);
        packet.params = vec![1];
        assert_eq!(button_bitmap(&packet, 13), None);
    }
}
