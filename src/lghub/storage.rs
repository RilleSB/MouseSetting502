use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

// Configuration files are small and saved infrequently. Serialize the complete
// write so an in-flight save cannot finish after a later save in this process.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

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
    if !target.exists()
        && local.exists()
        && let Ok(content) = fs::read_to_string(&local)
    {
        let _ = fs::write(&target, content);
    }
    target
}

/// Stage and sync the contents beside the destination, then atomically replace it.
/// Failure before replacement leaves the destination untouched. In-process saves
/// are serialized; the last serialized save wins. Callers own revision ordering
/// and transactions spanning multiple files. Directory metadata is not synced,
/// so this does not guarantee persistence of the rename after power loss.
pub fn atomic_write_file(path: &Path, content: &str) -> Result<(), String> {
    atomic_write_file_with(path, |file| {
        file.write_all(content.as_bytes())?;
        file.sync_all()
    })
}

fn atomic_write_file_with(
    path: &Path,
    stage: impl FnOnce(&mut fs::File) -> io::Result<()>,
) -> Result<(), String> {
    let _guard = WRITE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|e| {
        format!(
            "Failed to create configuration directory {}: {e}",
            parent.display()
        )
    })?;

    // A random name opened exclusively avoids collisions between files, threads,
    // and processes. Staying in the same directory avoids cross-volume moves.
    let mut temporary = tempfile::Builder::new()
        .prefix(".ghub-")
        .suffix(".tmp")
        .tempfile_in(parent)
        .map_err(|e| {
            format!(
                "Failed to create temporary file in {}: {e}",
                parent.display()
            )
        })?;
    stage(temporary.as_file_mut()).map_err(|e| {
        format!(
            "Failed to write or sync temporary file for {}: {e}",
            path.display()
        )
    })?;

    // persist uses atomic replacement (MoveFileExW on Windows). Never remove the
    // old file first. On error the returned temporary file is dropped and cleaned.
    persist_with_retry(temporary, path)?;
    Ok(())
}

fn persist_with_retry(mut temporary: tempfile::NamedTempFile, path: &Path) -> Result<(), String> {
    for attempt in 0..=6 {
        match temporary.persist(path) {
            Ok(_) => return Ok(()),
            Err(error) => {
                // Windows readers/scanners may briefly prevent replacement. Retry
                // the same staged file, bounded to 63ms of backoff. Permanent
                // locks/ACL failures still return an error without deleting data.
                #[cfg(windows)]
                let retryable = matches!(error.error.raw_os_error(), Some(5 | 32 | 33));
                #[cfg(not(windows))]
                let retryable = false;

                if attempt == 6 || !retryable {
                    return Err(format!(
                        "Failed to replace configuration file {}: {}",
                        path.display(),
                        error.error,
                    ));
                }
                temporary = error.file;
                std::thread::sleep(std::time::Duration::from_millis(1 << attempt));
            }
        }
    }
    unreachable!("the last persistence attempt always returns")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_nested_file_and_replaces_complete_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("настройки").join("profiles.json");
        atomic_write_file(&path, "old contents that are longer").unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "old contents that are longer"
        );
        atomic_write_file(&path, "новое").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "новое");
        atomic_write_file(&path, "").unwrap();
        assert!(fs::read(&path).unwrap().is_empty());
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    }

    #[test]
    fn partial_staging_failure_preserves_original_and_cleans_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.json");
        fs::write(&path, "original").unwrap();

        let error = atomic_write_file_with(&path, |file| {
            file.write_all(b"incomplete new contents")?;
            Err(io::Error::other("simulated write/sync failure"))
        })
        .unwrap_err();

        assert!(error.contains("write or sync"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "original");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn parent_creation_failure_is_reported_without_changing_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().join("not-a-directory");
        fs::write(&parent, "original").unwrap();
        let error = atomic_write_file(&parent.join("profiles.json"), "new").unwrap_err();
        assert!(error.contains("create configuration directory"));
        assert_eq!(fs::read_to_string(&parent).unwrap(), "original");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn replacement_failure_preserves_destination_and_cleans_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("directory-at-target");
        fs::create_dir(&path).unwrap();
        fs::write(path.join("sentinel"), "original").unwrap();
        let error = atomic_write_file(&path, "new").unwrap_err();
        assert!(error.contains("replace configuration file"));
        assert_eq!(
            fs::read_to_string(path.join("sentinel")).unwrap(),
            "original"
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn locked_destination_survives_failed_replacement_and_can_be_retried() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.json");
        fs::write(&path, "original").unwrap();
        let locked = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();
        let error = atomic_write_file(&path, "new").unwrap_err();
        assert!(error.contains("replace configuration file"));
        drop(locked);
        assert_eq!(fs::read_to_string(&path).unwrap(), "original");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        atomic_write_file(&path, "new").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    }

    #[test]
    fn parallel_writers_all_succeed_without_temporary_name_collisions() {
        use std::sync::{Arc, Barrier};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.json");
        let start = Arc::new(Barrier::new(8));
        let writers: Vec<_> = (0..8)
            .map(|n| {
                let path = path.clone();
                let start = start.clone();
                std::thread::spawn(move || {
                    start.wait();
                    for _ in 0..8 {
                        atomic_write_file(&path, &n.to_string()).unwrap();
                    }
                })
            })
            .collect();
        for writer in writers {
            writer.join().unwrap();
        }
        let saved: usize = fs::read_to_string(&path).unwrap().parse().unwrap();
        assert!(saved < 8);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn parallel_saves_never_expose_missing_or_partial_contents() {
        use std::sync::{
            Arc, Barrier,
            atomic::{AtomicBool, Ordering},
            mpsc,
        };
        use std::thread;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.json");
        let payloads: Arc<Vec<String>> = Arc::new(
            (0..8)
                .map(|n| format!("{n}:{}", "data".repeat(4096)))
                .collect(),
        );
        atomic_write_file(&path, &payloads[0]).unwrap();

        let reading = Arc::new(AtomicBool::new(true));
        let reader_path = path.clone();
        let reader_payloads = payloads.clone();
        let reader_running = reading.clone();
        let (ready_tx, ready_rx) = mpsc::channel();
        let reader = thread::spawn(move || {
            let mut first = true;
            loop {
                let observed = fs::read_to_string(&reader_path).unwrap();
                assert!(reader_payloads.contains(&observed));
                if first {
                    ready_tx.send(()).unwrap();
                    first = false;
                }
                if !reader_running.load(Ordering::SeqCst) {
                    break;
                }
                thread::yield_now();
            }
        });
        ready_rx.recv().unwrap();
        let start = Arc::new(Barrier::new(payloads.len()));
        let writers: Vec<_> = payloads
            .iter()
            .cloned()
            .map(|contents| {
                let path = path.clone();
                let start = start.clone();
                thread::spawn(move || {
                    start.wait();
                    (0..8)
                        .map(|_| atomic_write_file(&path, &contents))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let outcomes: Vec<_> = writers.into_iter().map(|writer| writer.join()).collect();
        reading.store(false, Ordering::SeqCst);
        reader.join().unwrap();
        for outcome in outcomes {
            for result in outcome.unwrap() {
                // Continuous reading can deny every retry on Windows. Failure
                // is permitted, but readers must still see an intact old/new
                // file, never a missing or partially written destination.
                #[cfg(windows)]
                if let Err(error) = result {
                    assert!(error.starts_with("Failed to replace configuration file"));
                    assert!(
                        ["(os error 5)", "(os error 32)", "(os error 33)"]
                            .iter()
                            .any(|code| error.ends_with(code))
                    );
                }
                #[cfg(not(windows))]
                result.unwrap();
            }
        }
        atomic_write_file(&path, "final save").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "final save");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
