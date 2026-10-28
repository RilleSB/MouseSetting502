use crate::hidpp::device::HidppDevice;
use super::dpi::set_dpi;

pub const FEATURE_ONBOARD_PROFILES: u16 = 0x8100;

/// Returns the index of the currently active onboard profile (1-indexed).
pub fn get_current_onboard_profile(dev: &HidppDevice) -> Result<u8, String> {
    let feat_idx = dev.get_feature_index(FEATURE_ONBOARD_PROFILES)?;
    // Function 0x02: getCurrentProfile() -> params[0] is active profile index
    let resp = dev.call_function(feat_idx, 0x02, &[])?;
    resp.params.first().copied().ok_or_else(|| "Empty response from getCurrentProfile".into())
}

/// Switches the active onboard profile to the specified slot (1 or 2).
pub fn set_current_onboard_profile(dev: &HidppDevice, profile_idx: u8) -> Result<(), String> {
    let feat_idx = dev.get_feature_index(FEATURE_ONBOARD_PROFILES)?;
    // Function 0x01: setCurrentProfile(profile_idx)
    let _ = dev.call_function(feat_idx, 0x01, &[profile_idx])?;
    Ok(())
}

/// Synchronizes target DPI to all onboard profiles so switching profiles never causes DPI shifts.
pub fn sync_all_onboard_profiles_dpi(dev: &HidppDevice, target_dpi: u16) -> Result<(), String> {
    let feat_idx = dev.get_feature_index(FEATURE_ONBOARD_PROFILES)?;
    let orig = get_current_onboard_profile(dev).unwrap_or(1);

    // Slots 1 and 2 are active on G502 HERO
    for slot in [1u8, 2u8] {
        let _ = dev.call_function(feat_idx, 0x01, &[slot]);
        let _ = set_dpi(dev, 0, target_dpi);
    }

    // Restore original active profile
    let _ = dev.call_function(feat_idx, 0x01, &[orig]);
    let _ = set_dpi(dev, 0, target_dpi);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_constant() {
        assert_eq!(FEATURE_ONBOARD_PROFILES, 0x8100);
    }
}
