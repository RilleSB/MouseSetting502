use slint::{ComponentHandle, VecModel};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use super::bridge::{KeybindItem, MainWindow, ProfileItem};
use super::models::{update_ui_keybind_list, update_ui_profile_list};
use crate::lghub::macro_engine::MacroPlayer;
use crate::lghub::profile_store::{ProfileStore, new_profile_id};
use crate::lghub::{AppProfile, ButtonBinding, KeybindListener};

pub struct ProfileControllerContext<'a> {
    pub app: &'a MainWindow,
    pub profile_model: &'a Rc<VecModel<ProfileItem>>,
    pub dpi_presets_model: &'a Rc<VecModel<i32>>,
    pub keybind_model: &'a Rc<VecModel<KeybindItem>>,
    pub profiles: &'a Arc<Mutex<Vec<AppProfile>>>,
    pub dpi_presets: &'a Arc<Mutex<Vec<u16>>>,
    pub keybinds: &'a Arc<Mutex<Vec<ButtonBinding>>>,
    pub listener: &'a Arc<KeybindListener>,
    pub player: &'a Arc<MacroPlayer>,
}

#[derive(Clone)]
struct ProfileState {
    profiles: Arc<Mutex<Vec<AppProfile>>>,
    dpi_presets: Arc<Mutex<Vec<u16>>>,
    keybinds: Arc<Mutex<Vec<ButtonBinding>>>,
    listener: Arc<KeybindListener>,
    player: Arc<MacroPlayer>,
}

impl ProfileState {
    fn activate(
        &self,
        store: &ProfileStore,
        id: &str,
        replacement: Option<Vec<AppProfile>>,
    ) -> Result<AppProfile, String> {
        // Match the preset/binding editors' lock order. Acquire every fallible
        // state guard before committing, so publication cannot partially fail.
        let mut presets = self
            .dpi_presets
            .lock()
            .map_err(|_| "Настройки DPI недоступны")?;
        let mut bindings = self.keybinds.lock().map_err(|_| "Назначения недоступны")?;
        let mut profiles = self.profiles.lock().map_err(|_| "Профили недоступны")?;
        let selected = match &replacement {
            Some(next) => store.replace_and_activate(next, id)?,
            None => store.activate(&profiles, id)?,
        };
        self.player.stop();
        self.listener.release_buttons();
        if let Some(next) = replacement {
            *profiles = next;
        }
        *presets = selected.dpi_presets.clone();
        *bindings = selected.keybinds.clone();
        self.listener.update_bindings(bindings.clone());
        Ok(selected)
    }
}

#[derive(Clone)]
struct ProfileController {
    weak: slint::Weak<MainWindow>,
    profile_model: Rc<VecModel<ProfileItem>>,
    dpi_model: Rc<VecModel<i32>>,
    keybind_model: Rc<VecModel<KeybindItem>>,
    state: ProfileState,
    store: ProfileStore,
}

impl ProfileController {
    fn report(&self, result: Result<(), String>) {
        if let Err(error) = result
            && let Some(app) = self.weak.upgrade()
        {
            app.set_status_msg(format!("Не удалось изменить профиль: {error}").into());
        }
    }

    // Switching and deleting the active profile share this complete transition.
    fn activate(&self, id: &str, replacement: Option<Vec<AppProfile>>) -> Result<(), String> {
        let Some(app) = self.weak.upgrade() else {
            return Ok(());
        };
        let selected = self.state.activate(&self.store, id, replacement)?;
        self.dpi_model.set_vec(
            selected
                .dpi_presets
                .iter()
                .map(|&dpi| i32::from(dpi))
                .collect::<Vec<_>>(),
        );
        let profiles = self
            .state
            .profiles
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        update_ui_profile_list(&self.profile_model, &profiles, &selected.id);
        drop(profiles);
        update_ui_keybind_list(&self.keybind_model, &self.state.keybinds);
        app.set_active_macro_name("".into());
        app.set_active_profile_id(selected.id.clone().into());
        app.set_active_profile_name(selected.name.clone().into());
        app.set_status_msg(format!("Профиль '{}' выбран. Применяется DPI…", selected.name).into());
        app.invoke_apply_profile_dpi(
            selected.id.into(),
            selected.name.into(),
            i32::from(selected.current_dpi),
        );
        Ok(())
    }

    fn insert(&self, profile: AppProfile) -> Result<(), String> {
        let mut profiles = self
            .state
            .profiles
            .lock()
            .map_err(|_| "Профили недоступны")?;
        let mut next = profiles.clone();
        next.push(profile);
        self.store.save_list(&next)?;
        *profiles = next;
        if let Some(app) = self.weak.upgrade() {
            update_ui_profile_list(
                &self.profile_model,
                &profiles,
                app.get_active_profile_id().as_str(),
            );
        }
        Ok(())
    }

    fn create(&self, name: &str, description: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Укажите название профиля".into());
        }
        let presets = self
            .state
            .dpi_presets
            .lock()
            .map_err(|_| "Настройки DPI недоступны")?
            .clone();
        let keybinds = self
            .state
            .keybinds
            .lock()
            .map_err(|_| "Назначения недоступны")?
            .clone();
        let id = new_profile_id(
            &self
                .state
                .profiles
                .lock()
                .map_err(|_| "Профили недоступны")?,
        );
        let Some(app) = self.weak.upgrade() else {
            return Ok(());
        };
        let current_dpi = u16::try_from(app.get_current_dpi())
            .ok()
            .filter(|dpi| (100..=25600).contains(dpi))
            .unwrap_or_else(|| presets.first().copied().unwrap_or(800));
        self.insert(AppProfile {
            id,
            name: name.into(),
            description: if description.trim().is_empty() {
                "Пользовательский профиль".into()
            } else {
                description.trim().into()
            },
            dpi_presets: presets,
            current_dpi,
            keybinds,
            rgb_mode: "static".into(),
            rgb_color: (0, 229, 255),
        })?;
        app.set_status_msg(format!("Создан профиль '{name}'").into());
        Ok(())
    }

    fn duplicate(&self, id: &str) -> Result<(), String> {
        let mut copy = {
            let profiles = self
                .state
                .profiles
                .lock()
                .map_err(|_| "Профили недоступны")?;
            let mut copy = profiles
                .iter()
                .find(|profile| profile.id == id)
                .ok_or("Исходный профиль не найден")?
                .clone();
            copy.id = new_profile_id(&profiles);
            copy
        };
        copy.name = format!("{} (Копия)", copy.name);
        let name = copy.name.clone();
        self.insert(copy)?;
        if let Some(app) = self.weak.upgrade() {
            app.set_status_msg(format!("Создан профиль '{name}'").into());
        }
        Ok(())
    }

    fn delete(&self, id: &str) -> Result<(), String> {
        let Some(app) = self.weak.upgrade() else {
            return Ok(());
        };
        let mut next = self
            .state
            .profiles
            .lock()
            .map_err(|_| "Профили недоступны")?
            .clone();
        let name = next
            .iter()
            .find(|profile| profile.id == id)
            .ok_or("Профиль не найден")?
            .name
            .clone();
        if next.len() <= 1 {
            return Err("Невозможно удалить единственный профиль".into());
        }
        next.retain(|profile| profile.id != id);
        if app.get_active_profile_id().as_str() == id {
            let next_id = next[0].id.clone();
            self.activate(&next_id, Some(next))?;
            // Activation's status also reports the pending hardware operation.
        } else {
            let mut profiles = self
                .state
                .profiles
                .lock()
                .map_err(|_| "Профили недоступны")?;
            self.store.save_list(&next)?;
            *profiles = next;
            update_ui_profile_list(
                &self.profile_model,
                &profiles,
                app.get_active_profile_id().as_str(),
            );
            app.set_status_msg(format!("Профиль '{name}' удалён").into());
        }
        Ok(())
    }

    fn next(&self) -> Result<(), String> {
        let Some(app) = self.weak.upgrade() else {
            return Ok(());
        };
        let id = {
            let profiles = self
                .state
                .profiles
                .lock()
                .map_err(|_| "Профили недоступны")?;
            let index = profiles
                .iter()
                .position(|profile| profile.id == app.get_active_profile_id().as_str())
                .ok_or("Активный профиль не найден")?;
            profiles[(index + 1) % profiles.len()].id.clone()
        };
        self.activate(&id, None)
    }
}

pub fn setup_profile_controller(ctx: &ProfileControllerContext) {
    let controller = ProfileController {
        weak: ctx.app.as_weak(),
        profile_model: ctx.profile_model.clone(),
        dpi_model: ctx.dpi_presets_model.clone(),
        keybind_model: ctx.keybind_model.clone(),
        state: ProfileState {
            profiles: ctx.profiles.clone(),
            dpi_presets: ctx.dpi_presets.clone(),
            keybinds: ctx.keybinds.clone(),
            listener: ctx.listener.clone(),
            player: ctx.player.clone(),
        },
        store: ProfileStore::user(),
    };
    let state = controller.clone();
    ctx.app
        .on_switch_profile(move |id| state.report(state.activate(id.trim(), None)));
    let state = controller.clone();
    ctx.app
        .on_next_app_profile(move || state.report(state.next()));
    let state = controller.clone();
    ctx.app.on_refresh_profile_models(move || {
        if let Some(app) = state.weak.upgrade()
            && let Ok(profiles) = state.state.profiles.lock()
        {
            update_ui_profile_list(
                &state.profile_model,
                &profiles,
                app.get_active_profile_id().as_str(),
            );
        }
    });
    let state = controller.clone();
    ctx.app.on_create_profile(move |name, description| {
        state.report(state.create(&name, &description))
    });
    let state = controller.clone();
    ctx.app
        .on_duplicate_profile(move |id| state.report(state.duplicate(id.trim())));
    let state = controller.clone();
    ctx.app
        .on_delete_profile(move |id| state.report(state.delete(id.trim())));
    // Restore the selected profile through the same path used by later switches.
    controller.report(controller.activate(ctx.app.get_active_profile_id().as_str(), None));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lghub::{
        keybinds::load_bindings_from_path,
        profiles::{default_profiles, get_active_profile_id_from_path, load_profiles_from_path},
    };

    fn state(profiles: Vec<AppProfile>) -> ProfileState {
        ProfileState {
            dpi_presets: Arc::new(Mutex::new(profiles[0].dpi_presets.clone())),
            keybinds: Arc::new(Mutex::new(profiles[0].keybinds.clone())),
            listener: Arc::new(KeybindListener::new(profiles[0].keybinds.clone())),
            player: Arc::new(MacroPlayer::default()),
            profiles: Arc::new(Mutex::new(profiles)),
        }
    }

    #[test]
    fn activation_publishes_the_same_bindings_and_presets_as_the_saved_selection() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProfileStore::in_dir(dir.path());
        let profiles = default_profiles();
        store
            .replace_and_activate(&profiles, &profiles[0].id)
            .unwrap();
        let current = state(profiles.clone());
        let generation = current.player.generation();
        let events = current.listener.event_generation();
        let selected = current.activate(&store, &profiles[1].id, None).unwrap();
        assert_eq!(selected, profiles[1]);
        assert_eq!(*current.dpi_presets.lock().unwrap(), selected.dpi_presets);
        assert_eq!(*current.keybinds.lock().unwrap(), selected.keybinds);
        assert_eq!(*current.profiles.lock().unwrap(), profiles);
        assert_eq!(
            get_active_profile_id_from_path(&dir.path().join("active_profile.txt")),
            selected.id
        );
        assert_eq!(
            load_bindings_from_path(&dir.path().join("keybinds.json")),
            selected.keybinds
        );
        assert_ne!(current.player.generation(), generation);
        assert_ne!(current.listener.event_generation(), events);
    }

    #[test]
    fn deleting_active_profile_uses_the_same_transition_for_all_runtime_state() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProfileStore::in_dir(dir.path());
        let profiles = default_profiles();
        store
            .replace_and_activate(&profiles, &profiles[0].id)
            .unwrap();
        let current = state(profiles.clone());
        let remaining = profiles[1..].to_vec();
        let id = remaining[0].id.clone();
        let selected = current
            .activate(&store, &id, Some(remaining.clone()))
            .unwrap();
        assert_eq!(*current.profiles.lock().unwrap(), remaining);
        assert_eq!(*current.dpi_presets.lock().unwrap(), selected.dpi_presets);
        assert_eq!(*current.keybinds.lock().unwrap(), selected.keybinds);
        assert_eq!(
            load_profiles_from_path(&dir.path().join("profiles.json")),
            remaining
        );
        assert_eq!(
            get_active_profile_id_from_path(&dir.path().join("active_profile.txt")),
            id
        );
    }

    #[test]
    fn invalid_activation_preserves_runtime_state_and_does_not_cancel_existing_context() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProfileStore::in_dir(dir.path());
        let profiles = default_profiles();
        store
            .replace_and_activate(&profiles, &profiles[0].id)
            .unwrap();
        let current = state(profiles.clone());
        let generation = current.player.generation();
        let events = current.listener.event_generation();
        assert!(current.activate(&store, "missing", None).is_err());
        assert_eq!(*current.profiles.lock().unwrap(), profiles);
        assert_eq!(
            *current.dpi_presets.lock().unwrap(),
            profiles[0].dpi_presets
        );
        assert_eq!(*current.keybinds.lock().unwrap(), profiles[0].keybinds);
        assert_eq!(current.player.generation(), generation);
        assert_eq!(current.listener.event_generation(), events);
    }

    #[cfg(windows)]
    #[test]
    fn failed_active_deletion_restores_files_and_preserves_all_runtime_state() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempfile::tempdir().unwrap();
        let store = ProfileStore::in_dir(dir.path());
        let profiles = default_profiles();
        store
            .replace_and_activate(&profiles, &profiles[0].id)
            .unwrap();
        let current = state(profiles.clone());
        let before = ["profiles.json", "keybinds.json", "active_profile.txt"]
            .map(|name| std::fs::read(dir.path().join(name)).unwrap());
        let generation = current.player.generation();
        let events = current.listener.event_generation();
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(dir.path().join("active_profile.txt"))
            .unwrap();
        assert!(
            current
                .activate(&store, &profiles[1].id, Some(profiles[1..].to_vec()))
                .is_err()
        );
        drop(lock);
        assert_eq!(*current.profiles.lock().unwrap(), profiles);
        assert_eq!(
            *current.dpi_presets.lock().unwrap(),
            profiles[0].dpi_presets
        );
        assert_eq!(*current.keybinds.lock().unwrap(), profiles[0].keybinds);
        assert_eq!(current.player.generation(), generation);
        assert_eq!(current.listener.event_generation(), events);
        let after = ["profiles.json", "keybinds.json", "active_profile.txt"]
            .map(|name| std::fs::read(dir.path().join(name)).unwrap());
        assert_eq!(before, after);
    }
}
