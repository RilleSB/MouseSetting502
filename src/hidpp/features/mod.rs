pub mod buttons;
pub mod dpi;
pub mod name;
pub mod profiles;
pub mod report_rate;
pub mod rgb;

pub use dpi::{DpiRange, get_current_dpi, get_dpi_range, set_dpi};
pub use name::get_device_name;
pub use profiles::{
    FEATURE_ONBOARD_PROFILES, get_current_onboard_profile, set_current_onboard_profile,
    sync_all_onboard_profiles_dpi,
};
pub use rgb::{
    LedEffect, LedZone, enable_host_mode, get_zone_count, set_led_effect, turn_off_all_leds,
};
