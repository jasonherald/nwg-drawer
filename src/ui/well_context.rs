use crate::config::DrawerConfig;
use crate::state::DrawerState;
use std::cell::RefCell;
use std::path::PathBuf;
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
    pub pinned_file: Rc<PathBuf>,
    pub on_launch: Rc<dyn Fn()>,
    pub status_label: gtk4::Label,
    pub search_entry: gtk4::SearchEntry,
}
