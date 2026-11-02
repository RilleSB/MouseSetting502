pub mod custom_macros;
pub mod db;
pub mod dpi_presets;
pub mod hid_to_vk;
pub mod keybinds;
pub mod macro_engine;
pub mod profile_store;
pub mod profiles;
pub mod storage;

pub use custom_macros::{
    default_preset_macros, delete_macro, format_component, load_custom_macros, save_custom_macros,
    save_or_update_macro, update_custom_macro,
};
pub use db::{LgCard, LgHubSettings, LgMacro, LgMacroComponent, LgProfile};
pub use dpi_presets::{
    add_dpi_preset, get_next_preset, get_prev_preset, load_dpi_presets, remove_dpi_preset,
    save_dpi_presets,
};
pub use hid_to_vk::hid_usage_to_vk;
pub use keybinds::{
    ButtonBinding, KeybindListener, assign_button_macro, clear_button_macro, default_bindings,
    load_bindings, rename_assigned_macro, save_bindings,
};
pub use macro_engine::{ActiveMacro, MacroPlayer, play_macro_once, start_macro, start_macro_loop};
pub use profiles::{
    AppProfile, delete_profile_by_id, get_active_profile, get_active_profile_id, get_g9_lock,
    load_profiles, save_or_update_profile, save_profiles, set_active_profile_id, set_g9_lock,
};
pub mod button_actions;
