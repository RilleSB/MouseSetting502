pub mod db;
pub mod hid_to_vk;
pub mod macro_engine;
pub mod keybinds;
pub mod custom_macros;
pub mod storage;
pub mod dpi_presets;

pub use db::{LgHubSettings, LgCard, LgProfile, LgMacro, LgMacroComponent};
pub use hid_to_vk::hid_usage_to_vk;
pub use macro_engine::{play_macro_once, start_macro_loop, ActiveMacro};
pub use custom_macros::{load_custom_macros, save_custom_macros, save_or_update_macro, update_custom_macro, delete_macro, format_component, default_preset_macros};
pub use keybinds::{ButtonBinding, KeybindListener, load_bindings, save_bindings, assign_button_macro, clear_button_macro, rename_assigned_macro, default_bindings};
pub use dpi_presets::{load_dpi_presets, save_dpi_presets, add_dpi_preset, remove_dpi_preset, get_next_preset, get_prev_preset};
