//! Profile changes commit files before publishing new application state.
//! Rollback handles ordinary I/O failures; this is not a crash-safe transaction
//! or a lock against other application processes editing the same files.
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};

use super::{
    profiles::AppProfile,
    storage::{atomic_write_file, get_storage_path},
};

static PROFILE_COMMIT_LOCK: Mutex<()> = Mutex::new(());
static NEXT_PROFILE_ID: AtomicU64 = AtomicU64::new(0);

pub fn new_profile_id(profiles: &[AppProfile]) -> String {
    loop {
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let serial = NEXT_PROFILE_ID.fetch_add(1, Ordering::Relaxed);
        let id = format!("prof_{time:x}_{:x}_{serial:x}", std::process::id());
        if profiles.iter().all(|profile| profile.id != id) {
            return id;
        }
    }
}

#[derive(Clone)]
pub struct ProfileStore {
    profiles: PathBuf,
    active: PathBuf,
    bindings: PathBuf,
}

impl ProfileStore {
    /// Save a macro edit and its references as one operation with I/O rollback.
    pub fn save_macro_edit(
        &self,
        macros_path: &Path,
        macros: &[super::db::LgMacro],
        profiles: &[AppProfile],
        active_id: &str,
    ) -> Result<(), String> {
        validate_ids(profiles)?;
        let active = profiles
            .iter()
            .find(|profile| profile.id == active_id)
            .ok_or("Активный профиль не найден")?;
        write_updates(&[
            (macros_path.to_path_buf(), json(macros)?),
            (self.profiles.clone(), json(profiles)?),
            (self.bindings.clone(), json(&active.keybinds)?),
        ])
    }

    pub fn user() -> Self {
        Self {
            profiles: get_storage_path("profiles.json"),
            active: get_storage_path("active_profile.txt"),
            bindings: get_storage_path("keybinds.json"),
        }
    }

    pub fn in_dir(directory: &Path) -> Self {
        Self {
            profiles: directory.join("profiles.json"),
            active: directory.join("active_profile.txt"),
            bindings: directory.join("keybinds.json"),
        }
    }

    pub fn save_list(&self, profiles: &[AppProfile]) -> Result<(), String> {
        validate_ids(profiles)?;
        write_updates(&[(self.profiles.clone(), json(profiles)?)])
    }

    pub fn activate(&self, profiles: &[AppProfile], id: &str) -> Result<AppProfile, String> {
        self.activate_inner(profiles, id, false)
    }

    pub fn replace_and_activate(
        &self,
        profiles: &[AppProfile],
        id: &str,
    ) -> Result<AppProfile, String> {
        self.activate_inner(profiles, id, true)
    }

    fn activate_inner(
        &self,
        profiles: &[AppProfile],
        id: &str,
        replace: bool,
    ) -> Result<AppProfile, String> {
        validate_ids(profiles)?;
        let selected = profiles
            .iter()
            .find(|profile| profile.id == id)
            .ok_or_else(|| format!("Профиль '{id}' не найден"))?
            .clone();
        if selected.dpi_presets.is_empty() {
            return Err("В профиле должен быть хотя бы один пресет DPI".into());
        }
        let mut updates = Vec::new();
        if replace {
            updates.push((self.profiles.clone(), json(profiles)?));
        }
        updates.push((self.bindings.clone(), json(&selected.keybinds)?));
        // Publish the active ID last, after the profile and bindings are stored.
        updates.push((self.active.clone(), selected.id.clone()));
        write_updates(&updates)?;
        Ok(selected)
    }
}

fn validate_ids(profiles: &[AppProfile]) -> Result<(), String> {
    let mut ids = std::collections::HashSet::new();
    if profiles.is_empty() {
        return Err("Нельзя удалить все профили".into());
    }
    for profile in profiles {
        if profile.id.trim().is_empty() || !ids.insert(&profile.id) {
            return Err("Профили содержат пустые или повторяющиеся идентификаторы".into());
        }
    }
    Ok(())
}

fn json(value: &(impl serde::Serialize + ?Sized)) -> Result<String, String> {
    serde_json::to_string_pretty(value)
        .map_err(|error| format!("Не удалось сериализовать настройки: {error}"))
}

pub(super) fn write_updates(updates: &[(PathBuf, String)]) -> Result<(), String> {
    write_updates_with(updates, atomic_write_file)
}

fn write_updates_with(
    updates: &[(PathBuf, String)],
    mut write: impl FnMut(&Path, &str) -> Result<(), String>,
) -> Result<(), String> {
    let _guard = PROFILE_COMMIT_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let snapshots: Vec<_> = updates
        .iter()
        .map(|(path, _)| match fs::read_to_string(path) {
            Ok(contents) => Ok(Some(contents)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!(
                "Не удалось прочитать настройки {}: {error}",
                path.display()
            )),
        })
        .collect::<Result<_, _>>()?;

    for (index, (path, contents)) in updates.iter().enumerate() {
        if let Err(error) = write(path, contents) {
            let mut rollback_errors = Vec::new();
            for previous in (0..index).rev() {
                let path = &updates[previous].0;
                let result = match &snapshots[previous] {
                    Some(contents) => write(path, contents),
                    None => fs::remove_file(path)
                        .or_else(|error| {
                            if error.kind() == std::io::ErrorKind::NotFound {
                                Ok(())
                            } else {
                                Err(error)
                            }
                        })
                        .map_err(|error| error.to_string()),
                };
                if let Err(error) = result {
                    rollback_errors.push(format!("{}: {error}", path.display()));
                }
            }
            return if rollback_errors.is_empty() {
                Err(format!("Сохранение отменено: {error}"))
            } else {
                Err(format!(
                    "{error}; не удалось восстановить файлы: {}",
                    rollback_errors.join("; ")
                ))
            };
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lghub::{
        keybinds::load_bindings_from_path,
        profiles::{default_profiles, get_active_profile_id_from_path, load_profiles_from_path},
    };

    fn contents(store: &ProfileStore) -> Vec<Vec<u8>> {
        [&store.profiles, &store.bindings, &store.active]
            .iter()
            .map(|path| fs::read(path).unwrap())
            .collect()
    }

    #[test]
    fn activation_keeps_selected_profile_id_and_bindings_consistent() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProfileStore::in_dir(dir.path());
        let mut profiles = default_profiles();
        profiles[1].keybinds[0].assigned_macro = "different assignment".into();
        store.save_list(&profiles).unwrap();
        store.activate(&profiles, &profiles[0].id).unwrap();
        let selected = store.activate(&profiles, &profiles[1].id).unwrap();
        assert_eq!(selected, profiles[1]);
        assert_eq!(get_active_profile_id_from_path(&store.active), selected.id);
        assert_eq!(load_bindings_from_path(&store.bindings), selected.keybinds);
        assert_eq!(load_profiles_from_path(&store.profiles), profiles);
    }

    #[test]
    fn deleting_active_profile_commits_remaining_list_and_new_selection_together() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProfileStore::in_dir(dir.path());
        let mut profiles = default_profiles();
        store
            .replace_and_activate(&profiles, &profiles[0].id)
            .unwrap();
        profiles.remove(0);
        let selected = store
            .replace_and_activate(&profiles, &profiles[0].id)
            .unwrap();
        assert_eq!(load_profiles_from_path(&store.profiles), profiles);
        assert_eq!(get_active_profile_id_from_path(&store.active), selected.id);
        assert_eq!(load_bindings_from_path(&store.bindings), selected.keybinds);
    }

    #[test]
    fn deleting_inactive_profile_preserves_selection_and_cached_bindings() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProfileStore::in_dir(dir.path());
        let mut profiles = default_profiles();
        store
            .replace_and_activate(&profiles, &profiles[0].id)
            .unwrap();
        let active = fs::read(&store.active).unwrap();
        let bindings = fs::read(&store.bindings).unwrap();
        profiles.pop();
        store.save_list(&profiles).unwrap();
        assert_eq!(fs::read(&store.active).unwrap(), active);
        assert_eq!(fs::read(&store.bindings).unwrap(), bindings);
        assert_eq!(load_profiles_from_path(&store.profiles), profiles);
    }

    #[test]
    fn invalid_selection_and_duplicate_ids_do_not_modify_files() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProfileStore::in_dir(dir.path());
        let mut profiles = default_profiles();
        store
            .replace_and_activate(&profiles, &profiles[0].id)
            .unwrap();
        let before = contents(&store);
        assert!(store.activate(&profiles, "missing").is_err());
        profiles[1].id = profiles[0].id.clone();
        assert!(store.save_list(&profiles).is_err());
        assert!(store.activate(&profiles, &profiles[0].id).is_err());
        assert_eq!(contents(&store), before);
    }

    #[test]
    fn failed_final_write_rolls_back_all_preceding_files_byte_for_byte() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProfileStore::in_dir(dir.path());
        let profiles = default_profiles();
        store
            .replace_and_activate(&profiles, &profiles[0].id)
            .unwrap();
        let before = contents(&store);
        let updates = vec![
            (store.profiles.clone(), "new profiles".into()),
            (store.bindings.clone(), "new bindings".into()),
            (store.active.clone(), "new active id".into()),
        ];
        let error = write_updates_with(&updates, |path, contents| {
            if path == store.active {
                Err("simulated active-ID failure".into())
            } else {
                atomic_write_file(path, contents)
            }
        })
        .unwrap_err();
        assert!(error.contains("Сохранение отменено"));
        assert_eq!(contents(&store), before);
    }

    #[test]
    fn rollback_removes_newly_created_files() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProfileStore::in_dir(dir.path());
        fs::write(&store.active, "old").unwrap();
        let error = write_updates_with(
            &[
                (store.bindings.clone(), "new bindings".into()),
                (store.active.clone(), "new id".into()),
            ],
            |path, contents| {
                if path == store.active {
                    Err("simulated failure".into())
                } else {
                    atomic_write_file(path, contents)
                }
            },
        )
        .unwrap_err();
        assert!(error.contains("Сохранение отменено"));
        assert!(!store.bindings.exists());
        assert_eq!(fs::read_to_string(&store.active).unwrap(), "old");
    }

    #[test]
    fn rollback_failure_explicitly_reports_files_that_could_not_be_restored() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProfileStore::in_dir(dir.path());
        let profiles = default_profiles();
        store
            .replace_and_activate(&profiles, &profiles[0].id)
            .unwrap();
        let mut writes = 0;
        let error = write_updates_with(
            &[
                (store.bindings.clone(), "new bindings".into()),
                (store.active.clone(), "new active id".into()),
            ],
            |path, contents| {
                writes += 1;
                if writes >= 2 {
                    Err("disk unavailable".into())
                } else {
                    atomic_write_file(path, contents)
                }
            },
        )
        .unwrap_err();
        assert!(error.contains("не удалось восстановить файлы"));
        assert!(error.contains("keybinds.json"));
        assert_eq!(
            get_active_profile_id_from_path(&store.active),
            profiles[0].id
        );
    }

    #[test]
    fn unreadable_snapshot_aborts_before_writing_any_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProfileStore::in_dir(dir.path());
        let profiles = default_profiles();
        store.save_list(&profiles).unwrap();
        fs::create_dir(&store.active).unwrap();
        let before = fs::read(&store.profiles).unwrap();
        assert!(
            store
                .replace_and_activate(&profiles, &profiles[1].id)
                .is_err()
        );
        assert_eq!(fs::read(&store.profiles).unwrap(), before);
        assert!(!store.bindings.exists());
        assert!(store.active.is_dir());
    }

    #[test]
    fn rapid_creations_and_copies_have_unique_persisted_ids() {
        let dir = tempfile::tempdir().unwrap();
        let store = ProfileStore::in_dir(dir.path());
        let mut profiles = default_profiles();
        for _ in 0..1000 {
            let mut copy = profiles[0].clone();
            copy.id = new_profile_id(&profiles);
            profiles.push(copy);
        }
        store.save_list(&profiles).unwrap();
        let loaded = load_profiles_from_path(&store.profiles);
        let ids: std::collections::HashSet<_> = loaded.iter().map(|profile| &profile.id).collect();
        assert_eq!(loaded.len(), 1003);
        assert_eq!(ids.len(), loaded.len());
    }

    #[cfg(windows)]
    #[test]
    fn locked_active_id_preserves_old_selection_and_restores_cached_bindings() {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 1;
        let dir = tempfile::tempdir().unwrap();
        let store = ProfileStore::in_dir(dir.path());
        let profiles = default_profiles();
        store
            .replace_and_activate(&profiles, &profiles[0].id)
            .unwrap();
        let before = contents(&store);
        let lock = fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(&store.active)
            .unwrap();
        assert!(store.activate(&profiles, &profiles[1].id).is_err());
        drop(lock);
        assert_eq!(contents(&store), before);
        store.activate(&profiles, &profiles[1].id).unwrap();
        assert_eq!(
            get_active_profile_id_from_path(&store.active),
            profiles[1].id
        );
    }
}
