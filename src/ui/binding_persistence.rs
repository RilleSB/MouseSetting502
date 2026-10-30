use std::sync::{Arc, Mutex};
use crate::lghub::{AppProfile, ButtonBinding, KeybindListener, get_active_profile_id, save_profiles, save_bindings};

pub fn store_assignment(button: &str, action: &str, bindings: &Arc<Mutex<Vec<ButtonBinding>>>,
    profiles: &Arc<Mutex<Vec<AppProfile>>>, listener: &KeybindListener) -> Result<(), String> {
    let mut bindings = bindings.lock().map_err(|_| "Настройки кнопок заняты")?;
    let mut profiles = profiles.lock().map_err(|_| "Настройки профилей заняты")?;
    let mut next = bindings.clone();
    let binding = next.iter_mut().find(|binding| binding.button_id == button)
        .ok_or_else(|| format!("Кнопка {button} не найдена"))?;
    binding.assigned_macro = action.into();
    let mut next_profiles = profiles.clone();
    let active_id = get_active_profile_id();
    let profile = next_profiles.iter_mut().find(|profile| profile.id == active_id)
        .ok_or("Активный профиль не найден")?;
    profile.keybinds = next.clone();
    save_bindings(&next)?;
    if let Err(error) = save_profiles(&next_profiles) {
        return match save_bindings(&bindings) {
            Ok(()) => Err(format!("Не удалось сохранить профиль: {error}")),
            Err(rollback) => Err(format!("Не удалось сохранить профиль: {error}; откат настроек: {rollback}")),
        };
    }
    *bindings = next;
    *profiles = next_profiles;
    listener.update_bindings(bindings.clone());
    Ok(())
}
