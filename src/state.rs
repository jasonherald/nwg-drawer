use nwg_common::compositor::Compositor;
use nwg_common::desktop::entry::DesktopEntry;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

/// Desktop application registry — loaded from .desktop files.
pub struct AppRegistry {
    /// All parsed desktop entries (id → entry).
    pub id2entry: HashMap<String, DesktopEntry>,
    /// Desktop entries sorted by name for display.
    pub entries: Vec<DesktopEntry>,
    /// Category lists: category_name → vec of desktop IDs.
    pub category_lists: HashMap<String, Vec<String>>,
}

impl AppRegistry {
    pub fn new() -> Self {
        Self {
            id2entry: HashMap::new(),
            entries: Vec::new(),
            category_lists: HashMap::new(),
        }
    }
}

/// Mutable state for the drawer application.
pub struct DrawerState {
    /// Application registry (desktop entries, categories).
    pub apps: AppRegistry,
    /// Pinned item desktop IDs.
    pub pinned: Vec<String>,
    /// App directories for icon/exec resolution.
    pub app_dirs: Vec<PathBuf>,
    /// XDG user directory map (e.g. "documents" → "/home/user/Documents").
    pub user_dirs: HashMap<String, PathBuf>,
    /// Directories excluded from file search.
    pub exclusions: Vec<String>,
    /// Custom file associations (pattern → command).
    pub preferred_apps: HashMap<String, String>,
    /// GTK_THEME prefix for launched commands (from --force-theme flag).
    pub gtk_theme_prefix: String,
    /// Compositor backend for launching apps.
    pub compositor: Rc<dyn Compositor>,
    /// Active category filter (empty = show all apps).
    pub active_category: Vec<String>,
    /// Active search phrase (empty = not searching).
    /// Used by rebuild paths to preserve search mode across pin/unpin.
    pub active_search: String,
}

impl DrawerState {
    pub fn new(app_dirs: Vec<PathBuf>, compositor: Rc<dyn Compositor>) -> Self {
        Self {
            apps: AppRegistry::new(),
            pinned: Vec::new(),
            app_dirs,
            user_dirs: map_xdg_user_dirs(),
            exclusions: Vec::new(),
            preferred_apps: HashMap::new(),
            gtk_theme_prefix: String::new(),
            compositor,
            active_category: Vec::new(),
            active_search: String::new(),
        }
    }
}

/// Maps XDG user directory names to paths.
fn map_xdg_user_dirs() -> HashMap<String, PathBuf> {
    let mut result = HashMap::new();
    let home = std::env::var("HOME").unwrap_or_default();

    result.insert("home".into(), PathBuf::from(&home));
    result.insert("documents".into(), PathBuf::from(&home).join("Documents"));
    result.insert("downloads".into(), PathBuf::from(&home).join("Downloads"));
    result.insert("music".into(), PathBuf::from(&home).join("Music"));
    result.insert("pictures".into(), PathBuf::from(&home).join("Pictures"));
    result.insert("videos".into(), PathBuf::from(&home).join("Videos"));

    let config_home =
        std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| format!("{}/.config", home));
    let user_dirs_file = PathBuf::from(&config_home).join("user-dirs.dirs");

    if let Ok(content) = std::fs::read_to_string(&user_dirs_file) {
        for line in content.lines() {
            let line = line.trim();
            if let Some(val) = parse_user_dir_line(line, &home) {
                if line.starts_with("XDG_DOCUMENTS_DIR") {
                    result.insert("documents".into(), val);
                } else if line.starts_with("XDG_DOWNLOAD_DIR") {
                    result.insert("downloads".into(), val);
                } else if line.starts_with("XDG_MUSIC_DIR") {
                    result.insert("music".into(), val);
                } else if line.starts_with("XDG_PICTURES_DIR") {
                    result.insert("pictures".into(), val);
                } else if line.starts_with("XDG_VIDEOS_DIR") {
                    result.insert("videos".into(), val);
                }
            }
        }
    }

    result
}

fn parse_user_dir_line(line: &str, home: &str) -> Option<PathBuf> {
    let (_, value) = line.split_once('=')?;
    let value = value.trim().trim_matches('"');
    let expanded = value.replace("$HOME", home);
    Some(PathBuf::from(expanded))
}
