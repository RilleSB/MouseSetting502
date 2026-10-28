use std::fs;
use std::path::{Path, PathBuf};
use super::storage::{get_storage_path as get_appdata_storage_path, atomic_write_file};

const DPI_PRESETS_FILE: &str = "dpi_presets.json";

fn get_storage_path() -> PathBuf {
    get_appdata_storage_path(DPI_PRESETS_FILE)
}

pub fn default_dpi_presets() -> Vec<u16> {
    vec![400, 800, 1200, 1600, 2400, 3200, 6400]
}

pub fn load_dpi_presets() -> Vec<u16> {
    load_dpi_presets_from_path(&get_storage_path())
}

pub fn load_dpi_presets_from_path(path: &Path) -> Vec<u16> {
    if !path.exists() {
        let def = default_dpi_presets();
        let _ = save_dpi_presets_to_path(&def, path);
        return def;
    }

    if let Ok(json_str) = fs::read_to_string(path)
        && let Ok(list) = serde_json::from_str::<Vec<u16>>(&json_str)
        && !list.is_empty() {
            let mut sorted = list;
            sorted.sort_unstable();
            sorted.dedup();
            return sorted;
        }

    let def = default_dpi_presets();
    let _ = save_dpi_presets_to_path(&def, path);
    def
}

pub fn save_dpi_presets(presets: &[u16]) -> Result<(), String> {
    save_dpi_presets_to_path(presets, &get_storage_path())
}

pub fn save_dpi_presets_to_path(presets: &[u16], path: &Path) -> Result<(), String> {
    let mut sorted = presets.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let json_str = serde_json::to_string_pretty(&sorted)
        .map_err(|e| format!("JSON serialization error: {e}"))?;
    atomic_write_file(path, &json_str)
}

pub fn add_dpi_preset(presets: &mut Vec<u16>, dpi: u16) -> Result<(), String> {
    let clamped = ((dpi + 25) / 50 * 50).clamp(100, 25600);
    if !presets.contains(&clamped) {
        presets.push(clamped);
        presets.sort_unstable();
        presets.dedup();
        save_dpi_presets(presets)?;
    }
    Ok(())
}

pub fn remove_dpi_preset(presets: &mut Vec<u16>, dpi: u16) -> Result<(), String> {
    if presets.len() > 1 {
        presets.retain(|&x| x != dpi);
        save_dpi_presets(presets)?;
    }
    Ok(())
}

pub fn get_next_preset(presets: &[u16], current: u16) -> u16 {
    if presets.is_empty() {
        return current;
    }
    for &p in presets {
        if p > current {
            return p;
        }
    }
    // If at or above max, cycle to first
    presets[0]
}

pub fn get_prev_preset(presets: &[u16], current: u16) -> u16 {
    if presets.is_empty() {
        return current;
    }
    for &p in presets.iter().rev() {
        if p < current {
            return p;
        }
    }
    // If at or below min, cycle to last
    *presets.last().unwrap_or(&current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_presets_cycle() {
        let presets = vec![400, 800, 1600];
        assert_eq!(get_next_preset(&presets, 400), 800);
        assert_eq!(get_next_preset(&presets, 800), 1600);
        assert_eq!(get_next_preset(&presets, 1600), 400); // cycle

        assert_eq!(get_prev_preset(&presets, 1600), 800);
        assert_eq!(get_prev_preset(&presets, 800), 400);
        assert_eq!(get_prev_preset(&presets, 400), 1600); // cycle
    }

    #[test]
    fn test_add_remove_preset() {
        let mut presets = vec![400, 800];
        add_dpi_preset(&mut presets, 1200).unwrap();
        assert_eq!(presets, vec![400, 800, 1200]);

        remove_dpi_preset(&mut presets, 800).unwrap();
        assert_eq!(presets, vec![400, 1200]);

        // Don't remove last one
        remove_dpi_preset(&mut presets, 400).unwrap();
        assert_eq!(presets, vec![1200]);
        remove_dpi_preset(&mut presets, 1200).unwrap();
        assert_eq!(presets, vec![1200]);
    }
}
