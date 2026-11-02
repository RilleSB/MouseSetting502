//! Tag device writes with their owner and sequence. Persist confirmations before
//! the next restore reads a profile, and keep stale replies out of the UI.
use std::collections::HashMap;
use std::sync::Mutex;

use crate::lghub::{AppProfile, dpi_presets::normalize_dpi, profile_store::ProfileStore};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DpiTarget {
    Value(u16),
    Restore,
}

#[derive(Clone, Debug)]
pub(super) struct DpiRequest {
    pub serial: u64,
    pub profile_id: String,
    pub profile_name: String,
    pub target: DpiTarget,
}

struct Latest {
    request: DpiRequest,
    pending: bool,
    baseline: Option<u16>,
}

#[derive(Default)]
struct RequestState {
    next: u64,
    latest: Option<Latest>,
    confirmed: HashMap<String, u64>,
}

#[derive(Default)]
pub(super) struct DpiRequests(Mutex<RequestState>);

pub(super) enum DpiOutcome {
    Confirmed {
        value: u16,
        saved: Result<(), String>,
    },
    Failed(String),
    Superseded,
}

pub(super) struct DpiCompletion {
    pub request: DpiRequest,
    pub outcome: DpiOutcome,
}

impl DpiCompletion {
    pub fn confirmed(&self) -> Option<u16> {
        match self.outcome {
            DpiOutcome::Confirmed { value, .. } => Some(value),
            _ => None,
        }
    }

    pub fn status(&self) -> String {
        let name = &self.request.profile_name;
        match &self.outcome {
            DpiOutcome::Confirmed {
                value,
                saved: Ok(()),
            } => format!("DPI {value} подтверждён и сохранён в профиле '{name}'"),
            DpiOutcome::Confirmed {
                value,
                saved: Err(error),
            } => format!("DPI {value} установлен, но не сохранён в профиле '{name}': {error}"),
            DpiOutcome::Failed(error) => {
                format!("Профиль '{name}' выбран; DPI не применён: {error}")
            }
            DpiOutcome::Superseded => String::new(),
        }
    }
}

impl DpiRequests {
    pub fn issue(
        &self,
        profile_id: String,
        profile_name: String,
        target: DpiTarget,
    ) -> Result<DpiRequest, String> {
        if profile_id.is_empty() {
            return Err("Активный профиль не выбран".into());
        }
        let target = match target {
            DpiTarget::Value(value) => DpiTarget::Value(normalize_dpi(i32::from(value))),
            DpiTarget::Restore => DpiTarget::Restore,
        };
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        state.next = state
            .next
            .checked_add(1)
            .ok_or("Исчерпаны номера запросов DPI")?;
        let request = DpiRequest {
            serial: state.next,
            profile_id,
            profile_name,
            target,
        };
        let baseline = match target {
            DpiTarget::Value(value) => Some(value),
            DpiTarget::Restore => None,
        };
        state.latest = Some(Latest {
            request: request.clone(),
            pending: true,
            baseline,
        });
        Ok(request)
    }

    pub fn observation(&self) -> Option<u64> {
        let state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        // A read begun while a command is pending must never become valid just
        // because that command completed before its UI callback was delivered.
        if state.latest.as_ref().is_some_and(|latest| latest.pending) {
            None
        } else {
            Some(state.next)
        }
    }

    pub fn can_observe(&self, serial: Option<u64>) -> bool {
        let state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        Some(state.next) == serial && state.latest.as_ref().is_none_or(|latest| !latest.pending)
    }

    pub fn can_publish(&self, request: &DpiRequest, active_id: &str) -> bool {
        let state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        request.profile_id == active_id && state.next == request.serial
    }

    pub fn acknowledge(&self, request: &DpiRequest) {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(latest) = state
            .latest
            .as_mut()
            .filter(|latest| latest.request.serial == request.serial)
        {
            latest.baseline = None;
        }
    }

    pub fn baseline(&self, profile_id: &str, displayed: u16, stored: u16) -> u16 {
        let state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(latest) = state
            .latest
            .as_ref()
            .filter(|latest| latest.request.profile_id == profile_id)
        {
            if latest.pending && latest.request.target == DpiTarget::Restore {
                return stored;
            }
            if let Some(value) = latest.baseline {
                return value;
            }
        }
        displayed
    }

    pub fn finish(&self, request: &DpiRequest, confirmed: Option<u16>) {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(latest) = state
            .latest
            .as_mut()
            .filter(|latest| latest.request.serial == request.serial)
        {
            latest.pending = false;
            latest.baseline = confirmed;
        }
    }

    pub fn execute(
        &self,
        request: DpiRequest,
        profiles: &Mutex<Vec<AppProfile>>,
        store: &ProfileStore,
        send: impl FnOnce(u16) -> Result<u16, String>,
    ) -> Option<DpiCompletion> {
        {
            let state = self.0.lock().unwrap_or_else(|error| error.into_inner());
            if !state
                .latest
                .as_ref()
                .is_some_and(|latest| latest.pending && latest.request.serial == request.serial)
            {
                return None;
            }
        }
        let result = match request.target {
            DpiTarget::Value(value) => Ok(value),
            // Resolve at execution time: an in-flight request for this profile
            // may have confirmed a new value since the UI queued the restore.
            DpiTarget::Restore => profiles
                .lock()
                .map_err(|_| "Профили недоступны".to_string())
                .and_then(|profiles| {
                    profiles
                        .iter()
                        .find(|profile| profile.id == request.profile_id)
                        .map(|profile| normalize_dpi(i32::from(profile.current_dpi)))
                        .ok_or("Профиль был удалён".into())
                }),
        }
        .and_then(send);
        let outcome = match result {
            Ok(value) if (100..=25600).contains(&value) => {
                match self.persist(&request, value, profiles, store) {
                    Ok(false) => DpiOutcome::Superseded,
                    result => DpiOutcome::Confirmed {
                        value,
                        saved: result.map(|_| ()),
                    },
                }
            }
            Ok(value) => DpiOutcome::Failed(format!("Мышь вернула некорректный DPI: {value}")),
            Err(error) => DpiOutcome::Failed(error),
        };
        let completion = DpiCompletion { request, outcome };
        self.finish(&completion.request, completion.confirmed());
        Some(completion)
    }

    fn persist(
        &self,
        request: &DpiRequest,
        confirmed: u16,
        profiles: &Mutex<Vec<AppProfile>>,
        store: &ProfileStore,
    ) -> Result<bool, String> {
        // All profile mutations hold this lock across save and publication.
        // Never hold the request lock while acquiring the profile lock.
        let mut profiles = profiles.lock().map_err(|_| "Профили недоступны")?;
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        let previous = state
            .confirmed
            .entry(request.profile_id.clone())
            .or_default();
        if *previous >= request.serial {
            return Ok(false);
        }
        *previous = request.serial;
        let mut next = profiles.clone();
        let profile = next
            .iter_mut()
            .find(|profile| profile.id == request.profile_id)
            .ok_or("Профиль удалён до подтверждения DPI")?;
        if profile.current_dpi == confirmed {
            return Ok(true);
        }
        profile.current_dpi = confirmed;
        store.save_list(&next)?;
        *profiles = next;
        Ok(true)
    }
}

pub(super) fn parse_custom_dpi(text: &str) -> Result<u16, String> {
    text.trim()
        .parse::<i32>()
        .map(normalize_dpi)
        .map_err(|_| "Введите целое число DPI (100–25600)".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lghub::profiles::{default_profiles, load_profiles_from_path};

    fn fixture() -> (
        tempfile::TempDir,
        ProfileStore,
        Mutex<Vec<AppProfile>>,
        DpiRequests,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let store = ProfileStore::in_dir(dir.path());
        let profiles = default_profiles();
        store
            .replace_and_activate(&profiles, &profiles[0].id)
            .unwrap();
        (dir, store, Mutex::new(profiles), DpiRequests::default())
    }
    fn issue(requests: &DpiRequests, index: usize, target: DpiTarget) -> DpiRequest {
        let profile = &default_profiles()[index];
        requests
            .issue(profile.id.clone(), profile.name.clone(), target)
            .unwrap()
    }
    fn saved(dir: &tempfile::TempDir) -> Vec<AppProfile> {
        load_profiles_from_path(&dir.path().join("profiles.json"))
    }

    #[test]
    fn confirmed_value_is_saved_to_its_owner_after_switching_to_another_profile() {
        let (dir, store, profiles, requests) = fixture();
        let first = issue(&requests, 0, DpiTarget::Value(1250));
        let mut next = None;
        let completion = requests
            .execute(first, &profiles, &store, |target| {
                assert_eq!(target, 1250);
                store
                    .activate(&profiles.lock().unwrap(), "profile_gaming")
                    .unwrap();
                next = Some(issue(&requests, 1, DpiTarget::Restore));
                Ok(1200)
            })
            .unwrap();
        assert_eq!(completion.confirmed(), Some(1200));
        assert!(!requests.can_publish(&completion.request, "profile_gaming"));
        assert_eq!(saved(&dir)[0].current_dpi, 1200);
        assert_eq!(saved(&dir)[1].current_dpi, 400);
        let second = requests
            .execute(next.unwrap(), &profiles, &store, |target| {
                assert_eq!(target, 400);
                Ok(target)
            })
            .unwrap();
        assert!(requests.can_publish(&second.request, "profile_gaming"));
        assert_eq!(saved(&dir)[0].current_dpi, 1200);
    }

    #[test]
    fn rapid_a_b_a_restore_uses_the_confirmation_received_after_it_was_queued() {
        let (dir, store, profiles, requests) = fixture();
        let initial = issue(&requests, 0, DpiTarget::Value(1600));
        let mut b = None;
        let mut a = None;
        let old = requests
            .execute(initial, &profiles, &store, |_| {
                b = Some(issue(&requests, 1, DpiTarget::Restore));
                a = Some(issue(&requests, 0, DpiTarget::Restore));
                Ok(1600)
            })
            .unwrap();
        assert!(!requests.can_publish(&old.request, "profile_desktop"));
        assert!(
            requests
                .execute(b.unwrap(), &profiles, &store, |_| panic!(
                    "obsolete B request reached device"
                ))
                .is_none()
        );
        let completion = requests
            .execute(a.unwrap(), &profiles, &store, |target| {
                assert_eq!(target, 1600); // A's value was 800 when the restore was queued.
                Ok(target)
            })
            .unwrap();
        assert!(requests.can_publish(&completion.request, "profile_desktop"));
        assert_eq!(saved(&dir)[0].current_dpi, 1600);
    }

    #[test]
    fn superseded_queued_values_are_skipped_and_the_latest_request_wins() {
        let (dir, store, profiles, requests) = fixture();
        let old = issue(&requests, 0, DpiTarget::Value(1600));
        let new = issue(&requests, 0, DpiTarget::Value(3200));
        assert!(
            requests
                .execute(old, &profiles, &store, |_| panic!(
                    "obsolete request reached device"
                ))
                .is_none()
        );
        requests.execute(new, &profiles, &store, Ok).unwrap();
        assert_eq!(saved(&dir)[0].current_dpi, 3200);
    }

    #[test]
    fn an_out_of_order_confirmation_cannot_overwrite_a_newer_confirmed_value() {
        let (dir, store, profiles, requests) = fixture();
        let old = issue(&requests, 0, DpiTarget::Value(1600));
        let new = issue(&requests, 0, DpiTarget::Value(3200));
        assert!(requests.persist(&new, 3200, &profiles, &store).unwrap());
        assert!(!requests.persist(&old, 1600, &profiles, &store).unwrap());
        assert_eq!(saved(&dir)[0].current_dpi, 3200);
        assert_eq!(profiles.lock().unwrap()[0].current_dpi, 3200);
    }

    #[test]
    fn failed_device_command_preserves_saved_and_in_memory_dpi() {
        let (dir, store, profiles, requests) = fixture();
        let before = std::fs::read(dir.path().join("profiles.json")).unwrap();
        let request = issue(&requests, 0, DpiTarget::Value(1600));
        let completion = requests
            .execute(request, &profiles, &store, |_| Err("disconnected".into()))
            .unwrap();
        assert_eq!(completion.confirmed(), None);
        assert!(completion.status().contains("DPI не применён"));
        assert_eq!(profiles.lock().unwrap()[0].current_dpi, 800);
        assert_eq!(
            std::fs::read(dir.path().join("profiles.json")).unwrap(),
            before
        );
        assert!(requests.can_observe(requests.observation()));
    }

    #[test]
    fn removed_profile_is_not_recreated_and_other_profiles_are_not_modified() {
        let (dir, store, profiles, requests) = fixture();
        let request = issue(&requests, 0, DpiTarget::Value(1600));
        let completion = requests
            .execute(request, &profiles, &store, |_| {
                let mut profiles = profiles.lock().unwrap();
                profiles.remove(0);
                store
                    .replace_and_activate(&profiles, &profiles[0].id)
                    .unwrap();
                issue(&requests, 1, DpiTarget::Restore);
                Ok(1600)
            })
            .unwrap();
        assert!(matches!(
            completion.outcome,
            DpiOutcome::Confirmed { saved: Err(_), .. }
        ));
        assert_eq!(saved(&dir).len(), 2);
        assert_eq!(saved(&dir)[0].id, "profile_gaming");
        assert_eq!(saved(&dir)[0].current_dpi, 400);
        assert!(!requests.can_publish(&completion.request, "profile_gaming"));
    }

    #[test]
    fn deleted_profile_restore_does_not_send_any_device_command() {
        let (_dir, store, profiles, requests) = fixture();
        let request = issue(&requests, 0, DpiTarget::Restore);
        profiles.lock().unwrap().remove(0);
        let completion = requests
            .execute(request, &profiles, &store, |_| {
                panic!("deleted profile reached device")
            })
            .unwrap();
        assert_eq!(completion.confirmed(), None);
    }

    #[test]
    fn old_poll_results_are_rejected_during_and_after_a_newer_request() {
        let (_dir, store, profiles, requests) = fixture();
        let old_observation = requests.observation();
        assert!(requests.can_observe(old_observation));
        let request = issue(&requests, 0, DpiTarget::Value(1600));
        assert!(!requests.can_observe(old_observation));
        assert!(!requests.can_observe(Some(request.serial)));
        let pending_observation = requests.observation();
        let completion = requests.execute(request, &profiles, &store, Ok).unwrap();
        assert!(!requests.can_observe(pending_observation));
        assert!(!requests.can_observe(old_observation));
        assert!(requests.can_observe(Some(completion.request.serial)));
        issue(&requests, 0, DpiTarget::Value(3200));
        assert!(!requests.can_publish(&completion.request, "profile_desktop"));
        assert!(!requests.can_observe(Some(completion.request.serial)));
    }

    #[test]
    fn rapid_preset_steps_use_pending_values_and_restore_uses_the_selected_profile() {
        let (_dir, store, profiles, requests) = fixture();
        let restore = issue(&requests, 1, DpiTarget::Restore);
        assert_eq!(requests.baseline("profile_gaming", 800, 400), 400);
        let first = issue(&requests, 1, DpiTarget::Value(800));
        assert_eq!(requests.baseline("profile_gaming", 400, 400), 800);
        let presets = vec![400, 800, 1600];
        let next =
            crate::lghub::get_next_preset(&presets, requests.baseline("profile_gaming", 400, 400));
        let latest = issue(&requests, 1, DpiTarget::Value(next));
        assert_eq!(next, 1600);
        assert!(requests.execute(restore, &profiles, &store, Ok).is_none());
        assert!(requests.execute(first, &profiles, &store, Ok).is_none());
        let completion = requests
            .execute(latest, &profiles, &store, |_| Ok(1550))
            .unwrap();
        assert_eq!(requests.baseline("profile_gaming", 400, 400), 1550);
        requests.acknowledge(&completion.request);
        assert_eq!(requests.baseline("profile_gaming", 2000, 1550), 2000);
    }

    #[test]
    fn invalid_confirmation_is_not_saved() {
        let (dir, store, profiles, requests) = fixture();
        for value in [0, u16::MAX] {
            let request = issue(&requests, 0, DpiTarget::Value(1600));
            let completion = requests
                .execute(request, &profiles, &store, |_| Ok(value))
                .unwrap();
            assert_eq!(completion.confirmed(), None);
            assert_eq!(saved(&dir)[0].current_dpi, 800);
        }
    }

    #[test]
    fn confirmed_custom_dpi_outside_the_preset_list_survives_reload_and_restore() {
        let (dir, store, profiles, requests) = fixture();
        let request = issue(&requests, 0, DpiTarget::Value(950));
        requests.execute(request, &profiles, &store, Ok).unwrap();
        let reloaded = saved(&dir);
        assert_eq!(reloaded[0].current_dpi, 950);
        assert!(!reloaded[0].dpi_presets.contains(&950));
        let request = issue(&requests, 0, DpiTarget::Restore);
        requests
            .execute(request, &Mutex::new(reloaded), &store, |target| {
                assert_eq!(target, 950);
                Ok(target)
            })
            .unwrap();
    }

    #[test]
    fn extreme_input_is_clamped_before_arithmetic_and_invalid_text_is_rejected() {
        for (text, expected) in [
            ("2147483647", 25600),
            ("-2147483648", 100),
            (" 825 ", 850),
            ("824", 800),
            ("0", 100),
            ("25625", 25600),
        ] {
            assert_eq!(parse_custom_dpi(text).unwrap(), expected);
        }
        for text in ["", " ", "2147483648", "-2147483649", "1.5", "800 DPI", "🦀"] {
            assert!(parse_custom_dpi(text).is_err());
        }
        assert_eq!(normalize_dpi(i32::MAX), 25600);
        assert_eq!(normalize_dpi(i32::MIN), 100);
        assert_eq!(normalize_dpi(i32::from(u16::MAX)), 25600);
    }

    #[cfg(windows)]
    #[test]
    fn failed_save_reports_applied_hardware_value_without_changing_profile_and_can_retry() {
        use std::os::windows::fs::OpenOptionsExt;
        let (dir, store, profiles, requests) = fixture();
        let path = dir.path().join("profiles.json");
        let before = std::fs::read(&path).unwrap();
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .unwrap();
        let request = issue(&requests, 0, DpiTarget::Value(1600));
        let completion = requests.execute(request, &profiles, &store, Ok).unwrap();
        assert_eq!(completion.confirmed(), Some(1600));
        assert!(completion.status().contains("не сохранён"));
        assert_eq!(profiles.lock().unwrap()[0].current_dpi, 800);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        drop(lock);
        let request = issue(&requests, 0, DpiTarget::Value(1600));
        let completion = requests.execute(request, &profiles, &store, Ok).unwrap();
        assert!(matches!(
            completion.outcome,
            DpiOutcome::Confirmed { saved: Ok(()), .. }
        ));
        assert_eq!(saved(&dir)[0].current_dpi, 1600);
    }
}
