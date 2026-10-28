pub mod name;
pub mod dpi;
pub mod rgb;
pub mod profiles;

pub use name::get_device_name;
pub use dpi::{get_current_dpi, set_dpi, get_dpi_range, DpiRange};
pub use rgb::{set_led_effect, turn_off_all_leds, enable_host_mode, get_zone_count, LedZone, LedEffect};
pub use profiles::{
    FEATURE_ONBOARD_PROFILES,
    get_current_onboard_profile,
    set_current_onboard_profile,
    sync_all_onboard_profiles_dpi,
};
