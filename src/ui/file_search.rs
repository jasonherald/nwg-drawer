use super::constants;
use crate::config::DrawerConfig;
use crate::state::DrawerState;
use crate::xdg_dirs::XdgDirBucket;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Debounce window after the last keystroke before kicking off the
/// file-system walk. Long enough that a fast typist's bursts produce
/// one walk per phrase rather than per character; short enough that
/// the results feel prompt for a finished phrase.
const FILE_SEARCH_DEBOUNCE_MS: u64 = 150;

/// Async file-search dispatcher, owned by `WellContext`.
///
/// Each `dispatch` call:
/// - Increments the generation counter so any in-flight worker's
///   results can be detected as stale at the consumer.
/// - Cancels any pending debounce timeout.
/// - Schedules a `FILE_SEARCH_DEBOUNCE_MS` timeout that, on fire,
///   spawns an OS thread to run `walk_for_search` and ship results
///   back via `async_channel`.
///
/// A single consumer future on the GTK main loop receives
/// `(generation, rows)` results, drops them if the generation has
/// moved (user typed since), and otherwise appends the divider +
/// results widget to the well.
pub struct FileSearchDispatcher {
    generation: Rc<Cell<u64>>,
    /// `Rc<Cell<…>>` so the timeout closure can share the slot and
    /// clear it when it fires — once a `glib::SourceId` has fired,
    /// glib has already auto-removed the source and a later
    /// `id.remove()` call panics with "Source ID … was not found"
    /// (glib 0.21's `SourceId::remove` unwraps the result).
    pending_debounce: Rc<Cell<Option<glib::SourceId>>>,
    result_tx: async_channel::Sender<(u64, Vec<FileSearchRow>)>,
    config: Rc<DrawerConfig>,
}

impl FileSearchDispatcher {
    /// Constructs the dispatcher and spawns the long-running result
    /// consumer future. The consumer captures the well + supporting
    /// widgets it needs to render results; `state` and `on_launch` are
    /// only used at result-render time, never moved off the main thread.
    pub fn new(
        config: Rc<DrawerConfig>,
        well: gtk4::Box,
        status_label: gtk4::Label,
        state: Rc<RefCell<DrawerState>>,
        on_launch: Rc<dyn Fn()>,
    ) -> Self {
        let (result_tx, result_rx) = async_channel::unbounded::<(u64, Vec<FileSearchRow>)>();
        let generation = Rc::new(Cell::new(0u64));

        // Result consumer — runs forever on the main loop, applying
        // results that match the current generation.
        let gen_consumer = Rc::clone(&generation);
        glib::spawn_future_local(async move {
            loop {
                let (gen_id, rows) = match result_rx.recv().await {
                    Ok(v) => v,
                    Err(e) => {
                        log::error!("file-search result channel closed: {e}");
                        break;
                    }
                };
                if gen_id != gen_consumer.get() {
                    // User typed more after this worker started; results
                    // are for a phrase the user has moved past.
                    continue;
                }
                if rows.is_empty() {
                    continue;
                }
                let count = rows.len();
                // Borrow `preferred_apps` for the build call — `build_results_widget`
                // takes a reference, so cloning the whole HashMap on the hot path
                // is wasted work.
                let state_ref = state.borrow();
                let widget = build_results_widget(&rows, &state_ref.preferred_apps, &on_launch);
                widget.set_halign(gtk4::Align::Center);
                well.append(&super::well_builder::divider());
                well.append(&widget);
                status_label.set_text(&format!(
                    "{} file results | LMB: open | RMB: file manager",
                    count
                ));
                // Up from first file result → back to app search results
                super::navigation::install_file_results_nav(&widget);
            }
        });

        Self {
            generation,
            pending_debounce: Rc::new(Cell::new(None)),
            result_tx,
            config,
        }
    }

    /// Schedules a debounced file-system walk for `phrase`. The walk
    /// runs on a worker OS thread; results land on the consumer future.
    pub fn dispatch(&self, phrase: &str, state: &Rc<RefCell<DrawerState>>) {
        let gen_id = self.generation.get().wrapping_add(1);
        self.generation.set(gen_id);
        // Cancel any still-pending debounce. If `take()` returns `Some`,
        // the source has not fired yet (the closure clears the slot when
        // it does), so `remove()` is safe to call.
        if let Some(id) = self.pending_debounce.take() {
            id.remove();
        }

        // Snapshot Send-safe state under a tight borrow.
        let phrase_owned = phrase.to_string();
        let (user_dirs, exclusions) = {
            let s = state.borrow();
            (s.user_dirs.clone(), s.exclusions.clone())
        };
        let max = self.config.fs_max_results;
        let tx = self.result_tx.clone();
        let pending_slot = Rc::clone(&self.pending_debounce);

        let id = glib::timeout_add_local_once(
            std::time::Duration::from_millis(FILE_SEARCH_DEBOUNCE_MS),
            move || {
                // Clear our slot — glib auto-removes the source after
                // the closure returns, so a later `dispatch` /
                // `invalidate` calling `id.remove()` would panic with
                // "Source ID not found" (issue #39 follow-up).
                pending_slot.set(None);
                std::thread::spawn(move || {
                    let rows = walk_for_search(&phrase_owned, &user_dirs, &exclusions, max);
                    let _ = tx.send_blocking((gen_id, rows));
                });
            },
        );
        self.pending_debounce.set(Some(id));
    }

    /// Bumps the generation without dispatching. Call when the user
    /// clears the search bar or switches to a non-search view —
    /// any in-flight worker's results will be discarded.
    pub fn invalidate(&self) {
        self.generation.set(self.generation.get().wrapping_add(1));
        // Same staleness guarantee as `dispatch`: a `Some` here means
        // the source hasn't fired yet, so removing is safe.
        if let Some(id) = self.pending_debounce.take() {
            id.remove();
        }
    }
}

/// One result row from the file-system walk.
///
/// `Send + 'static` so the walker can run on a worker thread (issue #39)
/// and ship the results back to the GTK main thread for widget
/// construction.
#[derive(Debug, Clone)]
pub struct FileSearchRow {
    /// Display path (relative to the XDG dir root).
    pub display: String,
    /// Full path, used for launching.
    pub path: PathBuf,
    /// Whether this is a directory (affects the icon).
    pub is_dir: bool,
}

/// Walks all configured XDG user directories matching `phrase`, returning
/// at most `max_results` rows sorted by display path.
///
/// Pure / Send-safe — operates on owned data only, no GTK references.
/// Designed to run on a worker thread; the result vec is shipped back to
/// the main thread via `async_channel` and rendered by [`build_results_widget`].
pub fn walk_for_search(
    phrase: &str,
    user_dirs: &HashMap<XdgDirBucket, PathBuf>,
    exclusions: &[String],
    max_results: usize,
) -> Vec<FileSearchRow> {
    let mut all_results: Vec<FileSearchRow> = Vec::new();

    for (bucket, dir_path) in user_dirs {
        // Skip $HOME itself — walking the entire home tree is what every
        // other XDG dir is a more-targeted slice of.
        if *bucket == XdgDirBucket::Home || !dir_path.exists() {
            continue;
        }
        let remaining = max_results.saturating_sub(all_results.len());
        if remaining == 0 {
            break;
        }
        for result in walk_directory(dir_path, phrase, exclusions, remaining) {
            let display = result
                .path
                .strip_prefix(dir_path)
                .unwrap_or(&result.path)
                .to_string_lossy()
                .to_string();
            all_results.push(FileSearchRow {
                display,
                path: result.path,
                is_dir: result.is_dir,
            });
        }
    }

    // Sort alphabetically by display name
    all_results.sort_by_key(|r| r.display.to_lowercase());
    all_results
}

/// Builds the GTK results widget from already-walked rows.
///
/// Stays on the main thread — uses `Rc<dyn Fn()>` for `on_launch` and
/// constructs `gtk4` widgets. Caller is responsible for appending the
/// returned `Box` to the well at the right position (after the divider).
pub fn build_results_widget(
    rows: &[FileSearchRow],
    preferred_apps: &HashMap<String, String>,
    on_launch: &Rc<dyn Fn()>,
) -> gtk4::Box {
    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 2);

    if !rows.is_empty() {
        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        header.add_css_class("file-list-header");

        let name_col = gtk4::Label::new(Some("Name"));
        name_col.set_halign(gtk4::Align::Start);
        name_col.set_hexpand(true);
        name_col.set_width_request(constants::FILE_NAME_COLUMN_WIDTH);
        header.append(&name_col);

        let path_col = gtk4::Label::new(Some("Location"));
        path_col.set_halign(gtk4::Align::Start);
        path_col.set_hexpand(true);
        header.append(&path_col);

        container.append(&header);

        let sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
        sep.set_margin_bottom(2);
        container.append(&sep);
    }

    for row in rows {
        let widget = file_result_row(
            &row.display,
            &row.path,
            row.is_dir,
            preferred_apps,
            on_launch,
        );
        container.append(&widget);
    }

    container
}

struct FileResult {
    path: std::path::PathBuf,
    is_dir: bool,
}

fn walk_directory(
    root: &Path,
    phrase: &str,
    exclusions: &[String],
    max_results: usize,
) -> Vec<FileResult> {
    let mut results = Vec::new();
    let phrase_lower = phrase.to_lowercase();
    walk_inner(
        root,
        root,
        &phrase_lower,
        exclusions,
        &mut results,
        max_results,
    );
    results
}

fn walk_inner(
    dir: &Path,
    root: &Path,
    phrase: &str,
    exclusions: &[String],
    results: &mut Vec<FileResult>,
    max_results: usize,
) {
    if results.len() >= max_results {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.filter_map(|e| e.ok()) {
        if results.len() >= max_results {
            return;
        }
        process_dir_entry(&entry, root, phrase, exclusions, results, max_results);
    }
}

fn process_dir_entry(
    entry: &std::fs::DirEntry,
    root: &Path,
    phrase: &str,
    exclusions: &[String],
    results: &mut Vec<FileResult>,
    max_results: usize,
) {
    let path = entry.path();
    let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy();
    if is_excluded(&relative, exclusions) {
        return;
    }
    if relative.to_lowercase().contains(phrase) {
        results.push(FileResult {
            is_dir: path.is_dir(),
            path: path.clone(),
        });
    }
    if path.is_dir() {
        walk_inner(&path, root, phrase, exclusions, results, max_results);
    }
}

/// Checks whether a relative path matches any exclusion pattern.
fn is_excluded(relative: &str, exclusions: &[String]) -> bool {
    exclusions.iter().any(|ex| relative.contains(ex))
}

/// Creates a single file result row: [icon] [filename] [path]
fn file_result_row(
    display: &str,
    file_path: &Path,
    is_dir: bool,
    preferred_apps: &std::collections::HashMap<String, String>,
    on_launch: &Rc<dyn Fn()>,
) -> gtk4::Button {
    let button = gtk4::Button::new();
    button.add_css_class("file-result-row");
    button.set_has_frame(false);

    let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);

    // Icon — system theme based on file type
    let icon_name = if is_dir {
        "folder"
    } else {
        file_type_icon(file_path)
    };
    let icon = gtk4::Image::from_icon_name(icon_name);
    icon.set_pixel_size(constants::FILE_ICON_SIZE);
    icon.set_valign(gtk4::Align::Center);
    hbox.append(&icon);

    // Filename column
    let filename = file_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let name_label = gtk4::Label::new(Some(&filename));
    name_label.set_halign(gtk4::Align::Start);
    name_label.set_hexpand(true);
    name_label.set_width_request(constants::FILE_NAME_COLUMN_WIDTH);
    name_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    name_label.add_css_class("file-result-name");
    hbox.append(&name_label);

    // Path/location column
    let parent = file_path
        .parent()
        .unwrap_or(file_path)
        .to_string_lossy()
        .to_string();
    // Shorten home prefix
    let home = std::env::var("HOME").unwrap_or_default();
    let short_path = shorten_home(&parent, &home);
    let path_label = gtk4::Label::new(Some(&short_path));
    path_label.set_halign(gtk4::Align::Start);
    path_label.set_hexpand(true);
    path_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    path_label.add_css_class("file-result-path");
    hbox.append(&path_label);

    button.set_child(Some(&hbox));
    button.set_tooltip_text(Some(display));

    // Click → open
    let path = file_path.to_path_buf();
    let path_str = file_path.to_string_lossy().to_string();
    let preferred_cmd =
        nwg_common::desktop::preferred_apps::find_preferred_app(&path_str, preferred_apps);
    let on_launch = Rc::clone(on_launch);
    button.connect_clicked(move |_| {
        let cmd = if let Some(ref app) = preferred_cmd {
            let mut c = std::process::Command::new(app);
            c.arg(&path);
            c
        } else {
            let mut c = std::process::Command::new("xdg-open");
            c.arg(&path);
            c
        };
        nwg_common::launch::spawn_and_forget(cmd, &path.to_string_lossy());
        on_launch();
    });

    button
}

/// Replaces a leading `home` directory in `parent` with `~`.
///
/// Component-aware via `Path::strip_prefix`, so `/home/user` does not
/// prefix-match `/home/userfoo` (a sibling, not a child). Returns `parent`
/// unchanged when `home` is empty (e.g. `$HOME` unset) or when `parent` is
/// not under `home`.
fn shorten_home(parent: &str, home: &str) -> String {
    if home.is_empty() {
        return parent.to_string();
    }
    match std::path::Path::new(parent).strip_prefix(std::path::Path::new(home)) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.to_string_lossy()),
        Err(_) => parent.to_string(),
    }
}

fn file_type_icon(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    match ext.as_str() {
        "pdf" => "application-pdf",
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "bmp" => "image-x-generic",
        "mp3" | "flac" | "ogg" | "wav" | "m4a" | "aac" => "audio-x-generic",
        "mp4" | "mkv" | "avi" | "webm" | "mov" | "wmv" => "video-x-generic",
        "zip" | "tar" | "gz" | "xz" | "bz2" | "7z" | "rar" | "zst" => "package-x-generic",
        "txt" | "md" | "log" | "conf" | "cfg" | "ini" => "text-x-generic",
        "rs" | "py" | "js" | "ts" | "go" | "c" | "cpp" | "h" | "sh" | "lua" => "text-x-script",
        "html" | "htm" | "css" | "xml" | "json" | "yaml" | "toml" => "text-html",
        "doc" | "docx" | "odt" | "rtf" => "x-office-document",
        "xls" | "xlsx" | "ods" | "csv" => "x-office-spreadsheet",
        "ppt" | "pptx" | "odp" => "x-office-presentation",
        "3mf" | "stl" | "obj" | "step" | "stp" => "application-x-blender",
        _ => "text-x-generic",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    // ── shorten_home ────────────────────────────────────────────────────

    #[test]
    fn shortens_path_under_home() {
        assert_eq!(shorten_home("/home/user/Docs", "/home/user"), "~/Docs");
    }

    #[test]
    fn handles_exact_home_match() {
        assert_eq!(shorten_home("/home/user", "/home/user"), "~");
    }

    #[test]
    fn handles_nested_path() {
        assert_eq!(shorten_home("/home/user/a/b/c", "/home/user"), "~/a/b/c");
    }

    #[test]
    fn handles_trailing_slash_on_home() {
        assert_eq!(shorten_home("/home/user/Docs", "/home/user/"), "~/Docs");
    }

    #[test]
    fn returns_parent_unchanged_when_outside_home() {
        assert_eq!(shorten_home("/etc/foo", "/home/user"), "/etc/foo");
    }

    #[test]
    fn does_not_match_sibling_directory() {
        // /home/userfoo is a sibling of /home/user, not a child. The
        // String::starts_with check incorrectly matches here, producing
        // "~foo/Docs" instead of leaving the path alone.
        assert_eq!(
            shorten_home("/home/userfoo/Docs", "/home/user"),
            "/home/userfoo/Docs"
        );
    }

    #[test]
    fn returns_parent_unchanged_when_home_unset() {
        // When $HOME is unset we get an empty string, and "".starts_with("")
        // is always true — so every path used to gain a leading "~".
        assert_eq!(shorten_home("/etc/something", ""), "/etc/something");
    }

    // ── is_excluded ─────────────────────────────────────────────────────

    #[test]
    fn is_excluded_returns_false_for_empty_list() {
        assert!(!is_excluded("any/path/here", &[]));
    }

    #[test]
    fn is_excluded_matches_substring() {
        // The check is `relative.contains(ex)` — a literal substring match,
        // not a glob or path-component match. Pin that behavior so future
        // refactors don't silently change semantics across user setups.
        assert!(is_excluded(
            "myproject/target/build.rs",
            &["target".to_string()]
        ));
    }

    #[test]
    fn is_excluded_substring_match_catches_node_modules_via_node() {
        // Documented gotcha: the substring rule means an exclusion of
        // `"node"` also excludes anything containing `node` — including
        // `node_modules`, `nodemcu/`, etc.
        assert!(is_excluded(
            "frontend/node_modules/x",
            &["node".to_string()]
        ));
    }

    // ── file_type_icon ──────────────────────────────────────────────────

    #[test]
    fn file_type_icon_recognized_extensions() {
        assert_eq!(file_type_icon(Path::new("doc.pdf")), "application-pdf");
        assert_eq!(file_type_icon(Path::new("clip.mp4")), "video-x-generic");
        assert_eq!(file_type_icon(Path::new("song.mp3")), "audio-x-generic");
        assert_eq!(
            file_type_icon(Path::new("archive.zst")),
            "package-x-generic"
        );
        assert_eq!(file_type_icon(Path::new("main.rs")), "text-x-script");
        assert_eq!(file_type_icon(Path::new("page.html")), "text-html");
    }

    #[test]
    fn file_type_icon_unknown_extension_falls_back() {
        assert_eq!(file_type_icon(Path::new("blob.xyz")), "text-x-generic");
    }

    #[test]
    fn file_type_icon_no_extension_falls_back() {
        assert_eq!(file_type_icon(Path::new("README")), "text-x-generic");
    }

    #[test]
    fn file_type_icon_is_case_insensitive() {
        // Pinned: extension lookup lowercases the input. Otherwise common
        // shouty filenames like `.PDF` or `.JPG` wouldn't match.
        assert_eq!(file_type_icon(Path::new("scan.PDF")), "application-pdf");
        assert_eq!(file_type_icon(Path::new("photo.JPG")), "image-x-generic");
    }

    // ── walk_directory ──────────────────────────────────────────────────
    //
    // Fixture helper: lays down a directory tree under `root` from a list of
    // (relative-path, kind) tuples. Files get touched empty; "dir" entries
    // create directories.
    fn build_tree(root: &Path, entries: &[(&str, &str)]) {
        for (rel, kind) in entries {
            let full = root.join(rel);
            match *kind {
                "dir" => fs::create_dir_all(&full).expect("create_dir_all"),
                _ => {
                    if let Some(parent) = full.parent() {
                        fs::create_dir_all(parent).expect("create_dir_all parent");
                    }
                    fs::File::create(&full).expect("touch file");
                }
            }
        }
    }

    #[test]
    fn walk_directory_caps_results_at_max() {
        let root = tempfile::tempdir().expect("tempdir");
        // 30 matching files spread across 3 nesting levels.
        let mut entries = Vec::new();
        for i in 0..10 {
            entries.push((format!("match-top-{}.txt", i), "file".to_string()));
        }
        for i in 0..10 {
            entries.push((format!("a/match-mid-{}.txt", i), "file".to_string()));
        }
        for i in 0..10 {
            entries.push((format!("a/b/match-deep-{}.txt", i), "file".to_string()));
        }
        let entries_ref: Vec<(&str, &str)> = entries
            .iter()
            .map(|(p, k)| (p.as_str(), k.as_str()))
            .collect();
        build_tree(root.path(), &entries_ref);

        let results = walk_directory(root.path(), "match", &[], 7);
        assert!(
            results.len() <= 7,
            "expected ≤7 results, got {}",
            results.len()
        );
    }

    #[test]
    fn walk_directory_skips_excluded_subtree() {
        let root = tempfile::tempdir().expect("tempdir");
        build_tree(
            root.path(),
            &[
                ("src/note.txt", "file"),
                ("target/note.txt", "file"),
                ("docs/note.txt", "file"),
            ],
        );

        let results = walk_directory(root.path(), "note", &["target".to_string()], 100);
        let paths: Vec<String> = results
            .iter()
            .map(|r| r.path.to_string_lossy().to_string())
            .collect();
        assert!(
            paths.iter().any(|p| p.contains("src/note.txt")),
            "missing src result; got {:?}",
            paths
        );
        assert!(
            paths.iter().any(|p| p.contains("docs/note.txt")),
            "missing docs result; got {:?}",
            paths
        );
        assert!(
            !paths.iter().any(|p| p.contains("target/note.txt")),
            "target should be excluded; got {:?}",
            paths
        );
    }

    #[test]
    fn walk_directory_does_not_panic_on_unreadable_subdir() {
        // A directory we can't read (e.g. permissions failure) must not
        // panic the walk — `read_dir` errors are swallowed via `match … Err
        // => return`. Pin that behavior so a future refactor that introduces
        // an `unwrap` here doesn't crash the drawer's file search on a user's
        // tree.
        let root = tempfile::tempdir().expect("tempdir");
        build_tree(
            root.path(),
            &[("readable/match.txt", "file"), ("locked", "dir")],
        );

        // Drop read permission on the locked dir. If chmod fails (e.g. ACLs
        // or unusual filesystem), skip rather than fail the test — we're
        // pinning behavior, not testing chmod itself.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let locked = root.path().join("locked");
            fs::create_dir_all(locked.join("inner")).ok();
            fs::File::create(locked.join("inner/match-locked.txt")).ok();
            let perms = fs::Permissions::from_mode(0o000);
            if fs::set_permissions(&locked, perms).is_err() {
                return;
            }

            // Should return without panicking. The unreadable subtree is
            // silently skipped; the readable one is still walked.
            let results = walk_directory(root.path(), "match", &[], 100);
            let paths: Vec<String> = results
                .iter()
                .map(|r| r.path.to_string_lossy().to_string())
                .collect();
            assert!(
                paths.iter().any(|p| p.contains("readable/match.txt")),
                "readable subtree should still be walked; got {:?}",
                paths
            );

            // Restore permissions so tempfile cleanup can recurse into the
            // dir to remove it.
            let _ = fs::set_permissions(&locked, fs::Permissions::from_mode(0o755));
        }
    }
}
