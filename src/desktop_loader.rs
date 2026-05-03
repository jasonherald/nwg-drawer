use crate::state::{AppRegistry, DrawerState};
use nwg_common::desktop::categories::assign_categories;
use nwg_common::desktop::dirs;
use nwg_common::desktop::entry;
use std::path::{Path, PathBuf};

/// Scans all app directories, parses .desktop files, and populates state.
pub fn load_desktop_entries(state: &mut DrawerState) {
    load_into(&mut state.apps, &state.app_dirs);
}

/// Inner helper that does the actual loading work against a registry +
/// a list of directories. Extracted from `load_desktop_entries` so unit
/// tests can drive it directly without constructing a `DrawerState`
/// (and thus without needing a `Compositor` instance).
fn load_into(apps: &mut AppRegistry, app_dirs: &[PathBuf]) {
    apps.id2entry.clear();
    apps.entries.clear();
    apps.category_lists.clear();

    let mut seen_ids = std::collections::HashSet::new();

    for dir in app_dirs {
        let files = dirs::list_desktop_files(dir);
        for path in files {
            let id = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            // First *successfully parsed* occurrence wins. Marking the id
            // seen only after a successful parse means an unreadable user
            // override (permission denied, broken symlink, etc.) falls
            // through to the system-wide copy instead of erasing the app
            // from the launcher.
            if seen_ids.contains(&id) {
                continue;
            }
            if process_desktop_file(&id, &path, apps) {
                seen_ids.insert(id);
            }
        }
    }

    // Sort by localized name. `sort_by_cached_key` extracts each key once
    // (vs `sort_by_key` which re-extracts on every comparison), saving N
    // String allocations from `to_lowercase()` per startup.
    apps.entries
        .sort_by_cached_key(|a| a.name_loc.to_lowercase());

    log::info!(
        "Loaded {} desktop entries from {} directories",
        apps.entries.len(),
        app_dirs.len()
    );
}

/// Parses a single .desktop file and adds it to the registry if valid.
///
/// Returns `true` when the file parsed successfully (even if `NoDisplay`
/// suppressed the visible-registry insertion); `false` when `parse_desktop_file`
/// failed (e.g. permission denied) and the caller should leave the id
/// unclaimed so a lower-priority copy gets a chance.
fn process_desktop_file(id: &str, path: &Path, apps: &mut AppRegistry) -> bool {
    match entry::parse_desktop_file(id, path) {
        Ok(de) => {
            // `id2entry` retains every parsed entry — pinning may reference
            // a NoDisplay desktop_id, and lookups should still resolve.
            apps.id2entry.insert(de.desktop_id.clone(), de.clone());
            // `entries` and `category_lists` are display-eligible only.
            // Skipping NoDisplay entries here is the canonical filter; the
            // downstream guards in `app_grid` and `well_builder` remain as
            // defense-in-depth.
            if !de.no_display {
                for cat in assign_categories(&de.category) {
                    apps.category_lists
                        .entry(cat.to_string())
                        .or_default()
                        .push(de.desktop_id.clone());
                }
                apps.entries.push(de);
            }
            true
        }
        Err(e) => {
            log::warn!("Failed to parse {}: {}", path.display(), e);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Writes a minimal valid .desktop file at `dir/<stem>.desktop`.
    fn write_desktop(dir: &Path, stem: &str, name: &str, exec: &str, categories: &str) {
        let content = format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name={name}\n\
             Exec={exec}\n\
             Categories={categories}\n"
        );
        fs::write(dir.join(format!("{stem}.desktop")), content).expect("write desktop file");
    }

    #[test]
    fn first_occurrence_wins_across_app_dirs() {
        // User dir comes first; system dir second. Same desktop id appears
        // in both with different Exec= lines. The user override should win
        // — matches Go behavior and protects user customization.
        let user_dir = tempfile::tempdir().expect("user tempdir");
        let system_dir = tempfile::tempdir().expect("system tempdir");
        write_desktop(
            user_dir.path(),
            "firefox",
            "Firefox",
            "firefox-user --custom",
            "Network;",
        );
        write_desktop(
            system_dir.path(),
            "firefox",
            "Firefox",
            "firefox-system",
            "Network;",
        );

        let mut apps = AppRegistry::new();
        load_into(
            &mut apps,
            &[
                user_dir.path().to_path_buf(),
                system_dir.path().to_path_buf(),
            ],
        );

        assert_eq!(apps.entries.len(), 1, "expected dedup to leave one entry");
        let entry = &apps.entries[0];
        assert_eq!(entry.desktop_id, "firefox");
        assert_eq!(
            entry.exec, "firefox-user --custom",
            "user-dir Exec= should win over system-dir Exec="
        );
    }

    #[test]
    fn entries_are_sorted_case_insensitive_by_localized_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_desktop(dir.path(), "zed", "Zed", "zed", "Development;");
        write_desktop(dir.path(), "alacritty", "alacritty", "alacritty", "System;");
        write_desktop(dir.path(), "firefox", "Firefox", "firefox", "Network;");

        let mut apps = AppRegistry::new();
        load_into(&mut apps, &[dir.path().to_path_buf()]);

        let names: Vec<&str> = apps.entries.iter().map(|e| e.name_loc.as_str()).collect();
        assert_eq!(
            names,
            vec!["alacritty", "Firefox", "Zed"],
            "entries should sort by lowercased name_loc"
        );
    }

    #[test]
    fn multi_category_assignment_fans_out() {
        // `Audio;Video;Graphics;` should land the entry in BOTH AudioVideo
        // (mapped from Audio + Video, dedup'd to one bucket) and Graphics
        // (primary category).
        let dir = tempfile::tempdir().expect("tempdir");
        write_desktop(
            dir.path(),
            "kdenlive",
            "Kdenlive",
            "kdenlive",
            "Audio;Video;Graphics;",
        );

        let mut apps = AppRegistry::new();
        load_into(&mut apps, &[dir.path().to_path_buf()]);

        let av = apps
            .category_lists
            .get("AudioVideo")
            .expect("AudioVideo bucket should exist");
        let gfx = apps
            .category_lists
            .get("Graphics")
            .expect("Graphics bucket should exist");

        assert!(
            av.contains(&"kdenlive".to_string()),
            "kdenlive should be in AudioVideo (mapped from Audio + Video)"
        );
        assert!(
            gfx.contains(&"kdenlive".to_string()),
            "kdenlive should also be in Graphics"
        );
        // Dedup check: AudioVideo should appear exactly once even though
        // both Audio and Video map to it.
        assert_eq!(
            av.iter().filter(|id| *id == "kdenlive").count(),
            1,
            "kdenlive should be in AudioVideo exactly once"
        );
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_higher_priority_override_falls_through_to_lower_priority_copy() {
        // If the user-dir override is unreadable (permissions, broken
        // symlink, etc.), a naive "first seen wins" dedup would erase the
        // app from the launcher entirely. The fix marks an id as seen only
        // after a *successful* parse, so the system-dir copy still wins.
        use std::os::unix::fs::PermissionsExt;

        let user_dir = tempfile::tempdir().expect("user tempdir");
        let system_dir = tempfile::tempdir().expect("system tempdir");
        write_desktop(
            user_dir.path(),
            "firefox",
            "Firefox",
            "firefox-user",
            "Network;",
        );
        write_desktop(
            system_dir.path(),
            "firefox",
            "Firefox",
            "firefox-system",
            "Network;",
        );

        // Strip read permission from the user-dir copy so File::open fails.
        let user_path = user_dir.path().join("firefox.desktop");
        if fs::set_permissions(&user_path, fs::Permissions::from_mode(0o000)).is_err() {
            // Some filesystems / CI sandboxes ignore chmod; treat as skip.
            return;
        }

        let mut apps = AppRegistry::new();
        load_into(
            &mut apps,
            &[
                user_dir.path().to_path_buf(),
                system_dir.path().to_path_buf(),
            ],
        );

        // Restore permissions so tempfile cleanup can remove the file.
        let _ = fs::set_permissions(&user_path, fs::Permissions::from_mode(0o644));

        assert_eq!(
            apps.entries.len(),
            1,
            "system-dir copy should still load when user-dir override is unreadable"
        );
        assert_eq!(apps.entries[0].exec, "firefox-system");
    }

    #[test]
    fn no_display_entries_are_excluded_from_visible_registry_but_kept_in_id2entry() {
        // NoDisplay=true desktop files (settings panels, GTK helper modules,
        // service entries, etc.) should not appear in the launcher grid or
        // category lists, but `id2entry` retains them so a pinned
        // `desktop_id` can still resolve to its DesktopEntry.
        let dir = tempfile::tempdir().expect("tempdir");
        let content = "[Desktop Entry]\n\
                       Type=Application\n\
                       Name=Hidden Helper\n\
                       Exec=hidden-helper\n\
                       Categories=Settings;\n\
                       NoDisplay=true\n";
        fs::write(dir.path().join("hidden.desktop"), content).expect("write desktop");

        let mut apps = AppRegistry::new();
        load_into(&mut apps, &[dir.path().to_path_buf()]);

        assert!(
            apps.entries.is_empty(),
            "NoDisplay entry should not appear in apps.entries (visible registry)"
        );
        assert!(
            apps.category_lists.is_empty(),
            "NoDisplay entry should not appear in any category bucket"
        );
        assert!(
            apps.id2entry.contains_key("hidden"),
            "NoDisplay entry should still be in id2entry for pin/lookup paths"
        );
    }

    #[test]
    fn second_call_replaces_state_rather_than_appending() {
        // load_into clears state before reloading — pin that behavior so a
        // future "incremental update" optimization doesn't accidentally
        // double the entry list (or category memberships, or id lookups)
        // on every reload.
        let dir = tempfile::tempdir().expect("tempdir");
        write_desktop(dir.path(), "foo", "Foo", "foo", "Utility;");

        let mut apps = AppRegistry::new();
        load_into(&mut apps, &[dir.path().to_path_buf()]);
        load_into(&mut apps, &[dir.path().to_path_buf()]);

        assert_eq!(apps.entries.len(), 1, "second load should not double up");
        assert_eq!(apps.id2entry.len(), 1);
        assert_eq!(
            apps.category_lists.values().map(Vec::len).sum::<usize>(),
            1,
            "second load should not duplicate category memberships"
        );
    }
}
