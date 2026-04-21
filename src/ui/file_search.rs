use super::constants;
use crate::config::DrawerConfig;
use crate::state::DrawerState;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

/// Performs file search across XDG user directories.
/// Returns a Box with a clean columnar list: icon | filename | path
pub fn search_files(
    phrase: &str,
    config: &DrawerConfig,
    state: &Rc<RefCell<DrawerState>>,
    on_launch: Rc<dyn Fn()>,
) -> gtk4::Box {
    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 2);

    let user_dirs = state.borrow().user_dirs.clone();
    let exclusions = state.borrow().exclusions.clone();
    let preferred_apps = state.borrow().preferred_apps.clone();

    let mut all_results: Vec<(String, std::path::PathBuf, bool)> = Vec::new();

    for (dir_name, dir_path) in &user_dirs {
        if dir_name == "home" || !dir_path.exists() {
            continue;
        }
        let remaining = config.fs_max_results.saturating_sub(all_results.len());
        if remaining == 0 {
            break;
        }
        for result in walk_directory(dir_path, phrase, &exclusions, remaining) {
            let display = result
                .path
                .strip_prefix(dir_path)
                .unwrap_or(&result.path)
                .to_string_lossy()
                .to_string();
            all_results.push((display, result.path, result.is_dir));
        }
    }

    // Sort alphabetically by display name
    all_results.sort_by_key(|a| a.0.to_lowercase());

    // Column header
    if !all_results.is_empty() {
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

    // Result rows
    for (display, file_path, is_dir) in &all_results {
        let row = file_result_row(
            display,
            file_path,
            *is_dir,
            &preferred_apps,
            Rc::clone(&on_launch),
        );
        container.append(&row);
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
    on_launch: Rc<dyn Fn()>,
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
    let short_path = if parent.starts_with(&home) {
        format!("~{}", &parent[home.len()..])
    } else {
        parent
    };
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
