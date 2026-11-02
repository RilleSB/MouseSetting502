use crate::hidpp::device::HidppDevice;

pub const FEATURE_ONBOARD_PROFILES: u16 = 0x8100;

/// Small transport boundary lets protocol tests run without USB hardware.
pub trait ProfileTransport {
    fn profile_call(&self, function: u8, params: &[u8]) -> Result<Vec<u8>, String>;
}
impl ProfileTransport for HidppDevice {
    fn profile_call(&self, function: u8, params: &[u8]) -> Result<Vec<u8>, String> {
        let index = self.get_feature_index(FEATURE_ONBOARD_PROFILES)?;
        Ok(self.call_function(index, function, params)?.params)
    }
}

pub fn get_onboard_mode(dev: &impl ProfileTransport) -> Result<bool, String> {
    let response = dev.profile_call(2, &[])?;
    match response.first() {
        Some(1) => Ok(true),
        Some(2) => Ok(false),
        mode => Err(format!("Unknown onboard mode: {mode:?}")),
    }
}

pub fn set_onboard_mode(dev: &impl ProfileTransport, onboard: bool) -> Result<(), String> {
    dev.profile_call(1, &[if onboard { 1 } else { 2 }])?;
    if get_onboard_mode(dev)? != onboard {
        return Err("Mouse did not confirm the requested memory mode".into());
    }
    Ok(())
}

/// Returns the index of the currently active onboard profile (1-indexed).
pub fn get_current_onboard_profile(dev: &impl ProfileTransport) -> Result<u8, String> {
    // 0x40 on the wire: profile address is a big-endian u16.
    let resp = dev.profile_call(4, &[])?;
    match resp.get(..2) {
        Some([0, slot]) if *slot > 0 => Ok(*slot),
        _ => Err("Invalid current onboard profile address".into()),
    }
}

/// Switches the active onboard profile using its 1-based memory address.
pub fn set_current_onboard_profile(
    dev: &impl ProfileTransport,
    profile_idx: u8,
) -> Result<(), String> {
    if profile_idx == 0 {
        return Err("Profile slots start at 1".into());
    }
    dev.profile_call(3, &[0, profile_idx])?;
    Ok(())
}

/// Explicitly unsupported until profile-sector persistence is implemented.
pub fn sync_all_onboard_profiles_dpi(dev: &HidppDevice, target_dpi: u16) -> Result<(), String> {
    let _ = (dev, target_dpi);
    // Active DPI writes do not persist profile flash. Never report a fake sync.
    Err("Запись DPI во все слоты памяти пока не реализована. Измените DPI активного режима.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    type ExpectedCall = (u8, Vec<u8>, Vec<u8>);
    struct Script(std::sync::Mutex<std::collections::VecDeque<ExpectedCall>>);
    impl ProfileTransport for Script {
        fn profile_call(&self, function: u8, params: &[u8]) -> Result<Vec<u8>, String> {
            let (expected, payload, reply) = self
                .0
                .lock()
                .unwrap()
                .pop_front()
                .expect("Unexpected command");
            assert_eq!((function, params), (expected, payload.as_slice()));
            Ok(reply)
        }
    }
    fn script(commands: Vec<ExpectedCall>) -> Script {
        Script(std::sync::Mutex::new(commands.into()))
    }

    #[test]
    fn reads_profile_address_instead_of_mode() {
        let device = script(vec![(4, vec![], vec![0, 3])]);
        assert_eq!(get_current_onboard_profile(&device).unwrap(), 3);
    }

    #[test]
    fn profile_switch_must_not_change_mode() {
        let device = script(vec![(3, vec![0, 3], vec![])]);
        set_current_onboard_profile(&device, 3).unwrap();
        assert!(set_current_onboard_profile(&device, 0).is_err());
    }

    #[test]
    fn host_mode_is_confirmed_by_readback() {
        let device = script(vec![(1, vec![2], vec![]), (2, vec![], vec![2])]);
        set_onboard_mode(&device, false).unwrap();
        assert!(device.0.lock().unwrap().is_empty());
        let refused = script(vec![(1, vec![2], vec![]), (2, vec![], vec![1])]);
        assert!(set_onboard_mode(&refused, false).is_err());
        assert!(get_onboard_mode(&script(vec![(2, vec![], vec![0])])).is_err());
    }
}
