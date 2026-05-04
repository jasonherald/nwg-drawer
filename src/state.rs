//! Shared mutable state for the drawer.
//!
//! [`DrawerState`] is held as `Rc<RefCell<…>>` and threaded through
//! every UI builder and event closure via [`crate::ui::well_context`].
//!
//! ## RefCell discipline
//!
//! Mutating callbacks (search, pin/unpin, file-search consumer) follow
//! a tight pattern: take `borrow_mut`, snapshot what's needed, drop the
//! borrow before any I/O, GTK rebuild, or callback dispatch. This
//! avoids "already borrowed" panics from re-entrant signals while a
//! `borrow_mut` is live.
//!
//! ## `active_search` precedence
//!
//! When `active_search` is non-empty, the well shows search results
//! and the category bar's selected category is *visually* preserved
//! but does not filter. Builders consult both fields; see
//! [`crate::ui::well_builder`] for the rebuild matrix.

use crate::xdg_dirs::{self, XdgDirBucket};
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
///
/// Shared via `Rc<RefCell<DrawerState>>` across every callback in the UI.
///
/// **Borrow discipline.** Every callback that mutates `DrawerState` must:
///
/// 1. Take `borrow_mut` in a tight block scope
///    (`{ let mut s = state.borrow_mut(); … }`), coalescing any related
///    mutations into that single scope.
/// 2. Drop the borrow **before** calling any function that can re-enter the
///    GTK main loop — most notably the rebuild callback created by
///    [`crate::ui::well_builder::build_rebuild_callback`], which schedules a
///    re-borrow via `glib::idle_add_local_once`. A borrow held across the
///    rebuild path panics on the second `borrow_mut`.
/// 3. Drop the borrow **before** file I/O (e.g. `nwg_common::pinning::save_pinned`)
///    where feasible. Snapshot whatever the I/O needs, release, then save.
///    A panic during I/O while the borrow is held leaves the cell poisoned
///    for the rest of the process; doing the I/O outside the borrow keeps
///    the cell recoverable and protects against any future re-entrant signal
///    delivered while the syscall is in flight.
///
/// Canonical examples:
/// - [`crate::ui::app_grid::connect_pin`] — pin/unpin toggle: borrow, mutate,
///   snapshot, release; save outside; rollback under a fresh borrow on error.
/// - [`crate::ui::well_builder::build_pinned_button`] — unpin from the pinned
///   row: same shape.
///
/// Booleans owned by callbacks (e.g. `in_search_mode`, `focus_pending`) should
/// use `Rc<Cell<bool>>` rather than `Rc<RefCell<bool>>` — `Cell` is `Copy`-only
/// and can't panic on overlapping borrows.
pub struct DrawerState {
    /// Application registry (desktop entries, categories).
    pub apps: AppRegistry,
    /// Pinned item desktop IDs.
    pub pinned: Vec<String>,
    /// App directories for icon/exec resolution.
    pub app_dirs: Vec<PathBuf>,
    /// XDG user directory map (e.g. "documents" → "/home/user/Documents").
    pub user_dirs: HashMap<XdgDirBucket, PathBuf>,
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
    /// Construct a fresh state.
    ///
    /// `compositor` is stored as the same `Rc<dyn Compositor>` passed
    /// in by `main`, which also keeps a clone for the launch path
    /// (`launch_desktop_entry` → compositor IPC). Sharing the instance
    /// avoids reopening the Hyprland/Sway socket per click and keeps
    /// the null-fallback decision (`init_or_null`) made once at
    /// startup applied uniformly to everything that talks to the WM.
    pub fn new(app_dirs: Vec<PathBuf>, compositor: Rc<dyn Compositor>) -> Self {
        Self {
            apps: AppRegistry::new(),
            pinned: Vec::new(),
            app_dirs,
            user_dirs: xdg_dirs::map_xdg_user_dirs(),
            exclusions: Vec::new(),
            preferred_apps: HashMap::new(),
            gtk_theme_prefix: String::new(),
            compositor,
            active_category: Vec::new(),
            active_search: String::new(),
        }
    }
}
