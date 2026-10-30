//! Per-user Windows startup. The entry is changed only on an explicit click.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupState { Disabled, Current, OtherCopy, Occupied }

const VALUE_NAME: &str = "MouseSettings502";
const STARTUP_ARGUMENT: &str = "--autostart";

fn command_for(path: &std::path::Path) -> Result<String, String> {
    let path = path.to_str().ok_or("Путь к приложению содержит неподдерживаемые символы")?;
    if path.contains('"') { return Err("Некорректный путь к приложению".into()); }
    Ok(format!("\"{path}\" {STARTUP_ARGUMENT}"))
}

fn classify(value: Option<&str>, current: &std::path::Path) -> Result<StartupState, String> {
    let Some(value) = value else { return Ok(StartupState::Disabled); };
    if value.trim().eq_ignore_ascii_case(&command_for(current)?) { return Ok(StartupState::Current); }
    let Some((executable, args)) = value.trim().strip_prefix('"').and_then(|v| v.split_once('"')) else {
        return Ok(StartupState::Occupied);
    };
    let name = std::path::Path::new(executable).file_name().and_then(|s| s.to_str()).unwrap_or("");
    if args.trim().eq_ignore_ascii_case(STARTUP_ARGUMENT)
        && (name.eq_ignore_ascii_case("ghub_rust.exe") || name.eq_ignore_ascii_case("Mouse Settings 502.exe")) {
        Ok(StartupState::OtherCopy)
    } else { Ok(StartupState::Occupied) }
}

#[cfg(windows)]
mod platform {
    use super::{StartupState, VALUE_NAME, classify, command_for};
    use std::{ffi::OsStr, os::windows::ffi::OsStrExt, path::Path, ptr};
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE,
        REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW,
        RegQueryValueExW, RegSetValueExW,
    };

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    fn wide(text: impl AsRef<OsStr>) -> Vec<u16> { text.as_ref().encode_wide().chain(Some(0)).collect() }
    fn error(code: u32) -> String { std::io::Error::from_raw_os_error(code as i32).to_string() }
    struct Key(HKEY);
    impl Drop for Key { fn drop(&mut self) { unsafe { RegCloseKey(self.0); } } }

    fn open(path: &str, write: bool, create: bool) -> Result<Option<Key>, String> {
        let mut handle: HKEY = ptr::null_mut();
        let key = wide(path);
        let code = unsafe {
            if create {
                RegCreateKeyExW(HKEY_CURRENT_USER, key.as_ptr(), 0, ptr::null(), REG_OPTION_NON_VOLATILE,
                    KEY_QUERY_VALUE | KEY_SET_VALUE, ptr::null(), &mut handle, ptr::null_mut())
            } else {
                let access = if write { KEY_QUERY_VALUE | KEY_SET_VALUE } else { KEY_QUERY_VALUE };
                RegOpenKeyExW(HKEY_CURRENT_USER, key.as_ptr(), 0, access, &mut handle)
            }
        };
        if code == ERROR_FILE_NOT_FOUND && !create { Ok(None) }
        else if code == ERROR_SUCCESS { Ok(Some(Key(handle))) }
        else { Err(error(code)) }
    }

    fn read(key: &Key) -> Result<Option<String>, String> {
        let name = wide(VALUE_NAME);
        let mut kind = 0;
        let mut size = 0;
        let code = unsafe { RegQueryValueExW(key.0, name.as_ptr(), ptr::null(), &mut kind, ptr::null_mut(), &mut size) };
        if code == ERROR_FILE_NOT_FOUND { return Ok(None); }
        if code != ERROR_SUCCESS { return Err(error(code)); }
        if kind != REG_SZ || size > 65536 || size % 2 != 0 { return Err("Некорректная запись автозагрузки".into()); }
        let mut bytes = vec![0u8; size as usize];
        let code = unsafe { RegQueryValueExW(key.0, name.as_ptr(), ptr::null(), &mut kind, bytes.as_mut_ptr(), &mut size) };
        if code != ERROR_SUCCESS { return Err(error(code)); }
        if kind != REG_SZ || size as usize > bytes.len() || size % 2 != 0 {
            return Err("Запись автозагрузки изменилась во время чтения".into());
        }
        let words: Vec<u16> = bytes[..size as usize].chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
        let length = words.iter().position(|w| *w == 0).unwrap_or(words.len());
        String::from_utf16(&words[..length]).map(Some).map_err(|_| "Некорректный текст автозагрузки".into())
    }

    fn state_at(path: &str, executable: &Path) -> Result<StartupState, String> {
        match open(path, false, false)? {
            Some(key) => classify(read(&key)?.as_deref(), executable),
            None => Ok(StartupState::Disabled),
        }
    }

    fn set_at(path: &str, executable: &Path, enabled: bool) -> Result<StartupState, String> {
        let Some(key) = open(path, true, enabled)? else { return Ok(StartupState::Disabled); };
        let state = classify(read(&key)?.as_deref(), executable)?;
        if state == StartupState::Occupied { return Err("Имя записи автозагрузки занято другой программой".into()); }
        let name = wide(VALUE_NAME);
        let code = if enabled {
            let value = wide(command_for(executable)?);
            unsafe { RegSetValueExW(key.0, name.as_ptr(), 0, REG_SZ, value.as_ptr().cast(), (value.len() * 2) as u32) }
        } else if state == StartupState::Disabled { ERROR_SUCCESS }
        else { unsafe { RegDeleteValueW(key.0, name.as_ptr()) } };
        if code != ERROR_SUCCESS { return Err(error(code)); }
        drop(key);
        let confirmed = state_at(path, executable)?;
        if confirmed != if enabled { StartupState::Current } else { StartupState::Disabled } {
            return Err("Windows не подтвердила изменение автозагрузки".into());
        }
        Ok(confirmed)
    }

    pub fn state() -> Result<StartupState, String> {
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        state_at(RUN_KEY, &executable)
    }
    pub fn set(enabled: bool) -> Result<StartupState, String> {
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        set_at(RUN_KEY, &executable, enabled)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use windows_sys::Win32::System::Registry::RegDeleteKeyW;
        #[test]
        fn isolated_registry_roundtrip_preserves_foreign_value() {
            let path = format!(r"Software\MouseSettings502-test-{}-{}", std::process::id(),
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
            struct Cleanup(String);
            impl Drop for Cleanup { fn drop(&mut self) { unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, wide(&self.0).as_ptr()); } } }
            let _cleanup = Cleanup(path.clone());
            let exe = Path::new(r"C:\Test Folder\ghub_rust.exe");
            assert_eq!(state_at(&path, exe).unwrap(), StartupState::Disabled);
            assert_eq!(set_at(&path, exe, true).unwrap(), StartupState::Current);
            let other = Path::new(r"D:\Other\ghub_rust.exe");
            assert_eq!(state_at(&path, other).unwrap(), StartupState::OtherCopy);
            assert_eq!(set_at(&path, other, false).unwrap(), StartupState::Disabled);
            let key = open(&path, true, true).unwrap().unwrap();
            let foreign = wide(r#""C:\Other\not-ours.exe" --autostart"#);
            assert_eq!(unsafe { RegSetValueExW(key.0, wide(VALUE_NAME).as_ptr(), 0, REG_SZ,
                foreign.as_ptr().cast(), (foreign.len() * 2) as u32) }, ERROR_SUCCESS);
            drop(key);
            assert_eq!(state_at(&path, exe).unwrap(), StartupState::Occupied);
            assert!(set_at(&path, exe, true).is_err());
            assert!(set_at(&path, exe, false).is_err());
        }
    }
}

#[cfg(windows)]
pub use platform::{set, state};

#[cfg(not(windows))]
pub fn state() -> Result<StartupState, String> { Err("Автозагрузка доступна только в Windows".into()) }
#[cfg(not(windows))]
pub fn set(_enabled: bool) -> Result<StartupState, String> { Err("Автозагрузка доступна только в Windows".into()) }

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    #[test]
    fn commands_quote_paths_and_do_not_claim_unrelated_entries() {
        let path = Path::new(r"C:\Program Files\ghub_rust.exe");
        let command = command_for(path).unwrap();
        assert_eq!(command, r#""C:\Program Files\ghub_rust.exe" --autostart"#);
        assert_eq!(classify(Some(&command), path).unwrap(), StartupState::Current);
        assert_eq!(classify(Some(r#""C:\Other\random.exe" --autostart"#), path).unwrap(), StartupState::Occupied);
    }
}
