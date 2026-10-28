use std::fs;
use std::path::{Path, PathBuf};

/// Returns the configuration directory in %APPDATA%\ghub_rust (or . on non-Windows/fallback).
pub fn get_appdata_dir() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        let dir = PathBuf::from(appdata).join("ghub_rust");
        let _ = fs::create_dir_all(&dir);
        dir
    } else {
        PathBuf::from(".")
    }
}

/// Returns the full path to a file inside the %APPDATA%\ghub_rust directory.
/// Automatically migrates any existing legacy file from the current working directory if found.
pub fn get_storage_path(filename: &str) -> PathBuf {
    let target = get_appdata_dir().join(filename);
    let local = PathBuf::from(filename);
    if !target.exists() && local.exists()
        && let Ok(content) = fs::read_to_string(&local) {
            let _ = fs::write(&target, content);
        }
    target
}

/// Atomically writes content to a file using a temporary file and rename.
pub fn atomic_write_file(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let tmp_path = path.with_extension(format!("tmp.{}", std::process::id()));
    fs::write(&tmp_path, content)
        .map_err(|e| format!("Failed to write temporary file {}: {e}", tmp_path.display()))?;

    if path.exists() {
        let _ = fs::remove_file(path);
    }
    fs::rename(&tmp_path, path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        format!("Failed to rename temporary file to {}: {e}", path.display())
    })?;
    Ok(())
}
