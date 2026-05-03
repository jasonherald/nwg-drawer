use crate::config::DrawerConfig;
use crate::state::DrawerState;
use crate::ui::file_search::FileSearchDispatcher;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

/// Shared context for well/category/search UI builders.
///
/// Bundles commonly-needed references so functions don't need 7+ parameters.
/// Follows the DockContext pattern from nwg-dock/src/context.rs.
#[derive(Clone)]
pub struct WellContext {
    pub well: gtk4::Box,
    pub pinned_box: gtk4::Box,
    pub config: Rc<DrawerConfig>,
    pub state: Rc<RefCell<DrawerState>>,
    pub pinned_file: Rc<Path>,
    pub on_launch: Rc<dyn Fn()>,
    pub status_label: gtk4::Label,
    pub search_entry: gtk4::SearchEntry,
    /// Async dispatcher for file-system search. `dispatch(phrase)` runs
    /// the walk on a worker thread and appends results to `well` via
    /// the consumer future spawned at `WellContext` construction time.
    pub file_search: Rc<FileSearchDispatcher>,
}
