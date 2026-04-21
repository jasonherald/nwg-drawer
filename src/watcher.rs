use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

/// Events from the file watcher.
#[derive(Debug)]
pub enum WatchEvent {
    /// Desktop files changed (app added/removed).
    DesktopFilesChanged,
    /// Pinned file changed.
    PinnedChanged,
}

/// Starts watching app directories and the pin cache file for changes.
/// Returns a receiver that emits WatchEvent when relevant files change.
pub fn start_watcher(
    app_dirs: &[std::path::PathBuf],
    pin_file: &Path,
) -> mpsc::Receiver<WatchEvent> {
    let (tx, rx) = mpsc::channel();

    let pin_path = pin_file.to_path_buf();
    let app_dir_list: Vec<_> = app_dirs.to_vec();

    std::thread::spawn(move || {
        let (notify_tx, notify_rx) = mpsc::channel();

        let mut watcher = match notify::recommended_watcher(move |res: Result<Event, _>| {
            if let Ok(event) = res {
                let _ = notify_tx.send(event); // Non-critical: receiver may have dropped
            }
        }) {
            Ok(w) => w,
            Err(e) => {
                log::error!("Failed to create file watcher: {}", e);
                return;
            }
        };

        register_watch_paths(&mut watcher, &app_dir_list, &pin_path);

        for event in notify_rx {
            if let Some(watch_event) = classify_watch_event(&event, &pin_path) {
                let _ = tx.send(watch_event); // Non-critical: receiver may have dropped
            }
        }
    });

    rx
}

/// Registers all app directories and the pin file's parent with the watcher.
fn register_watch_paths(watcher: &mut impl Watcher, app_dirs: &[PathBuf], pin_path: &Path) {
    for dir in app_dirs {
        if dir.exists()
            && let Err(e) = watcher.watch(dir, RecursiveMode::Recursive)
        {
            log::warn!("Failed to watch {}: {}", dir.display(), e);
        }
    }

    if let Some(parent) = pin_path.parent()
        && parent.exists()
        && let Err(e) = watcher.watch(parent, RecursiveMode::NonRecursive)
    {
        log::warn!("Failed to watch {}: {}", parent.display(), e);
    }
}

/// Determines if a file-system event corresponds to a desktop file change or pin file change.
/// Returns `None` for irrelevant events (e.g. access-only or unrelated file types).
fn classify_watch_event(event: &Event, pin_path: &PathBuf) -> Option<WatchEvent> {
    match event.kind {
        EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_) => {
            let is_pin = event.paths.iter().any(|p| p == pin_path);
            let is_desktop = event
                .paths
                .iter()
                .any(|p| p.extension().is_some_and(|ext| ext == "desktop"));

            if is_pin {
                Some(WatchEvent::PinnedChanged)
            } else if is_desktop {
                Some(WatchEvent::DesktopFilesChanged)
            } else {
                None
            }
        }
        _ => None,
    }
}
