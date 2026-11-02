//! Keep custom macros, every profile, and the active binding cache consistent.
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::lghub::{AppProfile, ButtonBinding, KeybindListener, LgHubSettings, LgMacro};
use crate::lghub::{macro_engine::MacroPlayer, profile_store::ProfileStore};

#[derive(Clone)]
pub(super) struct MacroEditState {
    pub custom: Arc<Mutex<Vec<LgMacro>>>,
    pub profiles: Arc<Mutex<Vec<AppProfile>>>,
    pub bindings: Arc<Mutex<Vec<ButtonBinding>>>,
    pub listener: Arc<KeybindListener>,
    pub player: Arc<MacroPlayer>,
    pub imported: Arc<Option<LgHubSettings>>,
    pub store: ProfileStore,
    pub macros_path: PathBuf,
}

enum Edit {
    Save { original: String, data: LgMacro },
    Delete(String),
}

struct EditPlan {
    custom: Vec<LgMacro>,
    profiles: Vec<AppProfile>,
    bindings: Vec<ButtonBinding>,
    replaced_name: Option<String>,
}

impl MacroEditState {
    pub fn save(&self, original: &str, data: LgMacro, active_id: &str) -> Result<bool, String> {
        self.apply(
            Edit::Save {
                original: original.trim().into(),
                data,
            },
            active_id,
        )
    }

    pub fn delete(&self, name: &str, active_id: &str) -> Result<bool, String> {
        self.apply(Edit::Delete(name.trim().into()), active_id)
    }

    fn apply(&self, edit: Edit, active_id: &str) -> Result<bool, String> {
        // Match the binding and profile controllers' shared lock order.
        let mut bindings = self.bindings.lock().map_err(|_| "Назначения недоступны")?;
        let mut profiles = self.profiles.lock().map_err(|_| "Профили недоступны")?;
        let mut custom = self.custom.lock().map_err(|_| "Макросы недоступны")?;
        let plan = prepare(&custom, &profiles, self.imported.as_ref(), edit, active_id)?;
        self.store
            .save_macro_edit(&self.macros_path, &plan.custom, &plan.profiles, active_id)?;

        let stopped = plan
            .replaced_name
            .as_deref()
            .is_some_and(|name| self.player.stop_if_name(name));
        *bindings = plan.bindings;
        *profiles = plan.profiles;
        *custom = plan.custom;
        // This also invalidates queued button events carrying the old names.
        self.listener.update_bindings(bindings.clone());
        Ok(stopped)
    }
}

fn is_builtin(name: &str) -> bool {
    matches!(name, "__DPI_UP__" | "__DPI_DOWN__" | "__PROFILE_NEXT__")
        || name.starts_with(crate::lghub::button_actions::KEY_PREFIX)
}

fn prepare(
    custom: &[LgMacro],
    profiles: &[AppProfile],
    imported: &Option<LgHubSettings>,
    edit: Edit,
    active_id: &str,
) -> Result<EditPlan, String> {
    let mut next_custom = custom.to_vec();
    let mut next_profiles = profiles.to_vec();
    let (original, replacement) = match edit {
        Edit::Save { original, mut data } => {
            data.name = data.name.trim().to_string();
            if data.name.is_empty() {
                return Err("Укажите название макроса".into());
            }
            if is_builtin(&data.name) {
                return Err("Это имя зарезервировано для системного назначения".into());
            }
            let original_index = if original.is_empty() {
                None
            } else {
                Some(find_original(custom, &original)?)
            };
            if custom.iter().enumerate().any(|(index, existing)| {
                Some(index) != original_index && existing.name.trim() == data.name
            }) {
                return Err("Макрос с таким именем уже существует".into());
            }
            if original != data.name
                && imported.as_ref().is_some_and(|settings| {
                    settings.cards.iter().any(|card| {
                        card.macro_data.as_ref().is_some_and(|m| {
                            m.name.trim() == data.name
                                || card
                                    .name
                                    .as_deref()
                                    .is_some_and(|name| name.trim() == data.name)
                        })
                    })
                })
            {
                return Err("Это имя уже используется макросом G HUB".into());
            }
            let new_name = data.name.clone();
            match original_index {
                Some(index) => next_custom[index] = data,
                None => next_custom.push(data),
            }
            (
                if original.is_empty() {
                    None
                } else {
                    Some(original)
                },
                new_name,
            )
        }
        Edit::Delete(name) => {
            let index = find_original(custom, &name)?;
            next_custom.remove(index);
            (Some(name), String::new())
        }
    };

    if let Some(old) = original.as_deref().filter(|name| !is_builtin(name)) {
        for profile in &mut next_profiles {
            for binding in &mut profile.keybinds {
                if binding.assigned_macro.trim() == old {
                    binding.assigned_macro = replacement.clone();
                }
            }
        }
    }
    let bindings = next_profiles
        .iter()
        .find(|profile| profile.id == active_id)
        .ok_or("Активный профиль не найден")?
        .keybinds
        .clone();
    Ok(EditPlan {
        custom: next_custom,
        profiles: next_profiles,
        bindings,
        replaced_name: original,
    })
}

fn find_original(custom: &[LgMacro], name: &str) -> Result<usize, String> {
    if name.is_empty() {
        return Err("Укажите название макроса".into());
    }
    let mut matches = custom
        .iter()
        .enumerate()
        .filter(|(_, data)| data.name.trim() == name);
    let index = matches
        .next()
        .ok_or("Исходный пользовательский макрос не найден")?
        .0;
    if matches.next().is_some() {
        return Err("Найдено несколько макросов с этим именем".into());
    }
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lghub::{
        LgCard,
        custom_macros::{load_custom_macros_from_path, save_custom_macros_to_path},
        keybinds::load_bindings_from_path,
        profiles::{default_profiles, load_profiles_from_path},
    };

    fn data(name: &str) -> LgMacro {
        LgMacro {
            name: name.into(),
            action_name: Some(name.into()),
            macro_type: "toggle".into(),
            is_toggle: true,
            sequence: Vec::new(),
        }
    }

    fn fixture() -> (tempfile::TempDir, MacroEditState, String) {
        let dir = tempfile::tempdir().unwrap();
        let mut profiles = default_profiles();
        for profile in &mut profiles {
            profile.keybinds[3].assigned_macro = "Old".into();
            profile.keybinds[4].assigned_macro = "OldSuffix".into();
            profile.keybinds[5].assigned_macro = "__KEY__:17,65".into();
        }
        let active_id = profiles[0].id.clone();
        let custom = vec![data("Old"), data("OldSuffix")];
        let store = ProfileStore::in_dir(dir.path());
        let macros_path = dir.path().join("user_macros.json");
        store.replace_and_activate(&profiles, &active_id).unwrap();
        save_custom_macros_to_path(&custom, &macros_path).unwrap();
        let bindings = profiles[0].keybinds.clone();
        let state = MacroEditState {
            custom: Arc::new(Mutex::new(custom)),
            profiles: Arc::new(Mutex::new(profiles)),
            bindings: Arc::new(Mutex::new(bindings.clone())),
            listener: Arc::new(KeybindListener::new(bindings)),
            player: Arc::new(MacroPlayer::default()),
            imported: Arc::new(None),
            store,
            macros_path,
        };
        (dir, state, active_id)
    }

    #[derive(Debug, PartialEq)]
    struct Snapshot {
        custom: serde_json::Value,
        profiles: Vec<AppProfile>,
        bindings: Vec<ButtonBinding>,
        files: Vec<Vec<u8>>,
        player_generation: u64,
        listener_generation: u64,
    }
    fn snapshot(state: &MacroEditState) -> Snapshot {
        let root = state.macros_path.parent().unwrap();
        Snapshot {
            custom: serde_json::to_value(&*state.custom.lock().unwrap()).unwrap(),
            profiles: state.profiles.lock().unwrap().clone(),
            bindings: state.bindings.lock().unwrap().clone(),
            files: [
                "user_macros.json",
                "profiles.json",
                "keybinds.json",
                "active_profile.txt",
            ]
            .iter()
            .map(|name| std::fs::read(root.join(name)).unwrap())
            .collect(),
            player_generation: state.player.generation(),
            listener_generation: state.listener.event_generation(),
        }
    }

    #[test]
    fn rename_updates_all_profiles_and_survives_switching_and_reload() {
        let (dir, state, active_id) = fixture();
        let events = state.listener.event_generation();
        assert!(!state.save("Old", data("New"), &active_id).unwrap());
        let profiles = load_profiles_from_path(&dir.path().join("profiles.json"));
        assert_eq!(*state.profiles.lock().unwrap(), profiles);
        for profile in &profiles {
            assert_eq!(profile.keybinds[3].assigned_macro, "New");
            assert_eq!(profile.keybinds[4].assigned_macro, "OldSuffix");
            let selected = state.store.activate(&profiles, &profile.id).unwrap();
            assert_eq!(
                load_bindings_from_path(&dir.path().join("keybinds.json")),
                selected.keybinds
            );
        }
        assert_eq!(state.bindings.lock().unwrap()[3].assigned_macro, "New");
        let reloaded = load_custom_macros_from_path(&state.macros_path);
        assert_eq!(reloaded[0].name, "New");
        assert!(!reloaded.iter().any(|data| data.name == "Old"));
        assert_ne!(state.listener.event_generation(), events);
    }

    #[test]
    fn delete_clears_all_profiles_but_preserves_other_macros_keys_and_builtin_actions() {
        let (dir, state, active_id) = fixture();
        state.delete("Old", &active_id).unwrap();
        let reloaded = load_custom_macros_from_path(&state.macros_path);
        assert!(!reloaded.iter().any(|data| data.name == "Old"));
        let profiles = load_profiles_from_path(&dir.path().join("profiles.json"));
        for profile in &profiles {
            assert_eq!(profile.keybinds[3].assigned_macro, "");
            assert_eq!(profile.keybinds[4].assigned_macro, "OldSuffix");
            assert_eq!(profile.keybinds[5].assigned_macro, "__KEY__:17,65");
            assert_eq!(profile.keybinds[6].assigned_macro, "__DPI_DOWN__");
            state.store.activate(&profiles, &profile.id).unwrap();
            assert_eq!(
                load_bindings_from_path(&dir.path().join("keybinds.json"))[3].assigned_macro,
                ""
            );
        }
        assert_eq!(state.bindings.lock().unwrap()[3].assigned_macro, "");
    }

    #[test]
    fn saving_without_rename_preserves_references_and_updates_sequence() {
        let (_dir, state, active_id) = fixture();
        let mut edited = data("Old");
        edited.sequence.push(crate::lghub::LgMacroComponent {
            mouse_button: None,
            mouse_down: None,
            vk_code: None,
            key_down: None,
            delay_ms: Some(123),
        });
        state.save("Old", edited, &active_id).unwrap();
        assert_eq!(
            load_custom_macros_from_path(&state.macros_path)[0].sequence[0].delay_ms,
            Some(123)
        );
        for profile in &*state.profiles.lock().unwrap() {
            assert_eq!(profile.keybinds[3].assigned_macro, "Old");
        }
    }

    #[test]
    fn create_and_delete_last_macro_leave_an_explicitly_empty_list_after_reload() {
        let (_dir, state, active_id) = fixture();
        state.delete("Old", &active_id).unwrap();
        state.delete("OldSuffix", &active_id).unwrap();
        assert!(load_custom_macros_from_path(&state.macros_path).is_empty());
        state.save("", data("New"), &active_id).unwrap();
        assert_eq!(load_custom_macros_from_path(&state.macros_path).len(), 1);
        state.delete("New", &active_id).unwrap();
        assert!(load_custom_macros_from_path(&state.macros_path).is_empty());
        assert!(state.custom.lock().unwrap().is_empty());
    }

    #[test]
    fn invalid_names_collisions_and_missing_originals_leave_all_state_unchanged() {
        let (_dir, state, active_id) = fixture();
        let before = snapshot(&state);
        for name in [
            "",
            "   ",
            "__DPI_UP__",
            "__DPI_DOWN__",
            "__PROFILE_NEXT__",
            "__KEY__:17,65",
            "OldSuffix",
        ] {
            assert!(state.save("Old", data(name), &active_id).is_err());
            assert_eq!(snapshot(&state), before);
        }
        assert!(state.save("", data(" Old "), &active_id).is_err());
        assert!(state.save("missing", data("New"), &active_id).is_err());
        assert!(state.delete("missing", &active_id).is_err());
        assert_eq!(snapshot(&state), before);
    }

    #[test]
    fn imported_macro_names_and_aliases_cannot_be_shadowed_by_a_rename() {
        let (_dir, mut state, active_id) = fixture();
        state.imported = Arc::new(Some(LgHubSettings {
            cards: vec![LgCard {
                id: "card".into(),
                name: Some("ImportedAlias".into()),
                attribute: None,
                macro_data: Some(data("ImportedName")),
            }],
            profiles: Vec::new(),
        }));
        let before = snapshot(&state);
        for name in ["ImportedName", "ImportedAlias"] {
            assert!(state.save("Old", data(name), &active_id).is_err());
            assert_eq!(snapshot(&state), before);
        }
    }

    #[test]
    fn removing_a_legacy_reserved_macro_does_not_clear_builtin_assignments() {
        let (_dir, state, active_id) = fixture();
        state.custom.lock().unwrap().push(data("__DPI_DOWN__"));
        state.delete("__DPI_DOWN__", &active_id).unwrap();
        for profile in &*state.profiles.lock().unwrap() {
            assert_eq!(profile.keybinds[6].assigned_macro, "__DPI_DOWN__");
        }
    }

    #[test]
    fn editing_stops_the_affected_run_but_keeps_an_unrelated_run_active() {
        let (_dir, state, active_id) = fixture();
        // Empty toggle loops produce no real input, even with the system sink.
        state
            .player
            .trigger(data("OldSuffix"), state.player.generation());
        assert!(!state.save("Old", data("New"), &active_id).unwrap());
        assert!(!state.player.is_idle_at(state.player.generation()));
        assert!(state.delete("OldSuffix", &active_id).unwrap());
        assert!(state.player.is_idle_at(state.player.generation()));
        state.player.trigger(data("New"), state.player.generation());
        assert!(state.save("New", data("Renamed"), &active_id).unwrap());
        assert!(state.player.is_idle_at(state.player.generation()));
    }

    #[test]
    fn unknown_active_profile_aborts_before_updating_files_or_runtime() {
        let (_dir, state, _) = fixture();
        let before = snapshot(&state);
        assert!(state.save("Old", data("New"), "missing").is_err());
        assert!(state.delete("Old", "missing").is_err());
        assert_eq!(snapshot(&state), before);
    }

    #[cfg(windows)]
    #[test]
    fn locked_binding_cache_rolls_back_rename_delete_and_creation() {
        use std::os::windows::fs::OpenOptionsExt;
        let (dir, state, active_id) = fixture();
        let before = snapshot(&state);
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(dir.path().join("keybinds.json"))
            .unwrap();
        assert!(state.save("Old", data("New"), &active_id).is_err());
        assert_eq!(snapshot(&state), before);
        assert!(state.delete("Old", &active_id).is_err());
        assert_eq!(snapshot(&state), before);
        assert!(state.save("", data("Created"), &active_id).is_err());
        assert_eq!(snapshot(&state), before);
        drop(lock);
        state.save("Old", data("New"), &active_id).unwrap();
        assert_eq!(
            load_custom_macros_from_path(&state.macros_path)[0].name,
            "New"
        );
    }

    #[cfg(windows)]
    #[test]
    fn locked_profiles_roll_back_macro_file_and_do_not_stop_the_old_run() {
        use std::os::windows::fs::OpenOptionsExt;
        let (dir, state, active_id) = fixture();
        state.player.trigger(data("Old"), state.player.generation());
        let before = snapshot(&state);
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(dir.path().join("profiles.json"))
            .unwrap();
        assert!(state.save("Old", data("New"), &active_id).is_err());
        assert_eq!(snapshot(&state), before);
        assert!(!state.player.is_idle_at(state.player.generation()));
        drop(lock);
        assert!(state.save("Old", data("New"), &active_id).unwrap());
        assert!(state.player.is_idle_at(state.player.generation()));
    }
}
