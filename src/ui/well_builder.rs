use crate::ui;
use crate::ui::navigation;
use crate::ui::well_context::WellContext;
use gtk4::prelude::*;
use nwg_common::pinning;
use std::rc::Rc;

/// Builds the normal (non-search) well content.
///
/// Pinned items go into `pinned_box` (above the ScrolledWindow, fixed).
/// App grid goes into `well` (inside the ScrolledWindow, scrollable).
pub fn build_normal_well(ctx: &WellContext) {
    // Drop any in-flight file-search worker so its results don't land
    // on the now-normal-view well.
    ctx.file_search.invalidate();

    clear_box(&ctx.well);
    clear_box(&ctx.pinned_box);

    // Read just the count out of the borrow; cloning the whole `pinned`
    // Vec was wasted work since we only need is_empty() / len() for
    // layout decisions.
    let pinned_len = ctx.state.borrow().pinned.len();

    // Rebuild callback shared by pinned unpin + app grid pin
    let on_rebuild = build_rebuild_callback(ctx);

    // Pinned items (above scroll)
    if pinned_len > 0 {
        let pf = build_pinned_flow(ctx, &on_rebuild);
        pf.set_halign(gtk4::Align::Center);
        ctx.pinned_box.append(&pf);
    }

    // App grid (scrollable)
    let flow = ui::app_grid::build_app_flow_box(ctx, None, "", Some(&on_rebuild));
    flow.set_halign(gtk4::Align::Center);
    ctx.well.append(&flow);

    // Install grid navigation on both FlowBoxes.
    let pinned_flow_opt = if pinned_len > 0 {
        ctx.pinned_box
            .first_child()
            .and_then(|w| w.downcast::<gtk4::FlowBox>().ok())
    } else {
        None
    };

    // App grid gets capture-phase arrow handler + cross-section up
    navigation::install_grid_nav(
        &flow,
        ctx.config.columns,
        pinned_flow_opt.as_ref(), // up target
        None,                     // no down target (bottom of layout)
    );

    // Pinned grid gets capture-phase arrow handler + cross-section down
    if let Some(ref pf) = pinned_flow_opt {
        navigation::install_grid_nav(
            pf,
            ctx.config.columns.min(pinned_len as u32).max(1),
            None,        // no up target (top of layout)
            Some(&flow), // down target
        );
    }
}

/// Builds search results — hides pinned, shows matching apps + files.
pub fn build_search_results(ctx: &WellContext, phrase: &str) {
    clear_box(&ctx.well);
    // Hide pinned during search
    ctx.pinned_box.set_visible(false);

    // Inline math result (e.g. "1+1 = 2") shown above app results
    if let Some(math_widget) = ui::math_widget::build_math_result(phrase) {
        ctx.well.append(&math_widget);
    }

    // Rebuild callback — rebuild_preserving_category checks active_search
    // and will re-run the search instead of restoring normal view.
    let on_rebuild = build_rebuild_callback(ctx);
    let app_flow = ui::app_grid::build_app_flow_box(ctx, None, phrase, Some(&on_rebuild));
    app_flow.set_halign(gtk4::Align::Center);
    ctx.well.append(&app_flow);

    // Search results get navigation too (no cross-section targets)
    navigation::install_grid_nav(&app_flow, ctx.config.columns, None, None);

    // File results — dispatched asynchronously. The walk runs on a worker
    // thread; results appear in `ctx.well` via the consumer future spawned
    // by `FileSearchDispatcher::new`. Stale results from prior keystrokes
    // are dropped via the dispatcher's generation counter.
    if !ctx.config.no_fs && phrase.len() > 2 {
        ctx.file_search.dispatch(phrase, &ctx.state);
    } else {
        // Sub-three-character phrases or `--no-fs` mode: no file search.
        // Bump the generation so any in-flight worker's results don't
        // sneak into the well.
        ctx.file_search.invalidate();
    }
}

/// Rebuilds the well, preserving the current view mode (search, category, or normal).
pub fn rebuild_preserving_category(ctx: &WellContext) {
    let active_search = ctx.state.borrow().active_search.clone();
    let active_cat = ctx.state.borrow().active_category.clone();

    match determine_rebuild_mode(&active_search, &active_cat) {
        RebuildMode::Search => {
            build_search_results(ctx, &active_search);
        }
        RebuildMode::Category => {
            build_normal_well(ctx);
            crate::ui::categories::apply_category_filter(ctx, &active_cat);
        }
        RebuildMode::Normal => {
            build_normal_well(ctx);
        }
    }
}

/// Restores the normal well (used when clearing search).
pub fn restore_normal_well(ctx: &WellContext) {
    ctx.pinned_box.set_visible(true);
    build_normal_well(ctx);
}

/// Builds the pinned items FlowBox with right-click unpin + immediate rebuild.
fn build_pinned_flow(ctx: &WellContext, on_rebuild: &Rc<dyn Fn()>) -> gtk4::FlowBox {
    let flow_box = gtk4::FlowBox::new();

    // Hold one immutable borrow across the iteration, mirroring
    // `build_app_flow_box`. Saves cloning `pinned` (Vec<String>),
    // `id2entry` (HashMap, often the largest registry by far), and
    // `app_dirs` per build. RefCell allows multiple immutable borrows;
    // build_pinned_button's own `.borrow()`s coexist fine.
    let s = ctx.state.borrow();

    let cols = ctx.config.columns.min(s.pinned.len() as u32).max(1);
    flow_box.set_min_children_per_line(cols);
    flow_box.set_max_children_per_line(cols);
    flow_box.set_column_spacing(ctx.config.spacing);
    flow_box.set_row_spacing(ctx.config.spacing);
    flow_box.set_homogeneous(true);
    flow_box.set_selection_mode(gtk4::SelectionMode::None);

    for desktop_id in &s.pinned {
        let entry = match s.apps.id2entry.get(desktop_id) {
            Some(e) if !e.desktop_id.is_empty() && !e.no_display => e,
            _ => continue,
        };
        let button = build_pinned_button(entry, ctx, &s.app_dirs, on_rebuild, desktop_id);
        if ctx.config.pin_indicator {
            crate::ui::widgets::apply_pin_badge(&button);
        }
        flow_box.insert(&button, -1);
        // Keep FlowBoxChild non-focusable — we handle navigation ourselves
        if let Some(child) = flow_box.last_child() {
            child.set_focusable(false);
        }
    }

    flow_box
}

/// Builds a single pinned icon button with click-to-launch and right-click-to-unpin.
fn build_pinned_button(
    entry: &nwg_common::desktop::entry::DesktopEntry,
    ctx: &WellContext,
    app_dirs: &[std::path::PathBuf],
    on_rebuild: &Rc<dyn Fn()>,
    desktop_id: &str,
) -> gtk4::Button {
    let name = if !entry.name_loc.is_empty() {
        &entry.name_loc
    } else {
        &entry.name
    };
    let desc = if !entry.comment_loc.is_empty() {
        &entry.comment_loc
    } else {
        &entry.comment
    };
    let button = crate::ui::widgets::app_icon_button(
        &entry.icon,
        name,
        ctx.config.icon_size,
        app_dirs,
        &ctx.status_label,
        desc,
    );

    // Click → launch
    let exec = entry.exec.clone();
    let terminal = entry.terminal;
    let term = ctx.config.term.clone();
    let on_launch_ref = Rc::clone(&ctx.on_launch);
    let compositor = Rc::clone(&ctx.state.borrow().compositor);
    let theme_prefix = ctx.state.borrow().gtk_theme_prefix.clone();
    button.connect_clicked(move |_| {
        nwg_common::launch::launch_desktop_entry(
            &exec,
            terminal,
            &term,
            &theme_prefix,
            &*compositor,
        );
        on_launch_ref();
    });

    // Right-click → unpin + immediate rebuild
    let id = desktop_id.to_string();
    let state_ref = Rc::clone(&ctx.state);
    // Cheap Rc clone of the path; the closure consumes it via deref to
    // `&Path` at the save_pinned call sites — no PathBuf allocation.
    let path = Rc::clone(&ctx.pinned_file);
    let rebuild = Rc::clone(on_rebuild);
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(super::constants::MOUSE_BUTTON_RIGHT);
    gesture.connect_released(move |gesture, _, _, _| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);

        // Phase 1: capture original position, unpin in-memory under a tight
        // borrow, snapshot the new pin list, then release the borrow before I/O.
        let (snapshot, original_pos) = {
            let mut s = state_ref.borrow_mut();
            let original_pos = s.pinned.iter().position(|p| p == &id);
            if !pinning::unpin_item(&mut s.pinned, &id) {
                return;
            }
            (s.pinned.clone(), original_pos)
        };

        // Phase 2: I/O outside any borrow.
        if let Err(e) = pinning::save_pinned(&snapshot, &path) {
            log::error!("Failed to save pinned state: {}", e);
            // Restore the pin at its original position so UI ordering survives
            // a save failure (push would put it at the end of the row).
            let mut s = state_ref.borrow_mut();
            if let Some(pos) = original_pos {
                s.pinned.insert(pos, id.clone());
            } else {
                s.pinned.push(id.clone());
            }
            return;
        }
        log::info!("Unpinned {}", id);
        rebuild();
    });
    button.add_controller(gesture);

    button
}

/// Creates a callback that rebuilds the entire well + pinned_box.
///
/// Defers via `idle_add_local_once` so any caller mid-mutation
/// (pin/unpin handlers holding a `borrow_mut`) drops their borrow
/// before `rebuild_preserving_category` re-borrows the state — the
/// "borrow → drop → rebuild" rule documented on `DrawerState`.
///
/// Wraps the captured context in `Rc<WellContext>` so each invocation
/// re-clones one refcount bump rather than the full 9-field shallow
/// clone of `WellContext`. (Rebuild fires on every pin/unpin click;
/// it's not perf-critical, but cleaner this way.)
pub fn build_rebuild_callback(ctx: &WellContext) -> Rc<dyn Fn()> {
    let ctx = Rc::new(ctx.clone());
    Rc::new(move || {
        let ctx = Rc::clone(&ctx);
        gtk4::glib::idle_add_local_once(move || {
            rebuild_preserving_category(&ctx);
        });
    })
}

// ---------------------------------------------------------------------------
// Grid navigation — capture-phase controller that handles all arrow keys.
//
// GTK4's FlowBox internal `move_cursor` keybinding is unreliable with
// SelectionMode::None and non-focusable FlowBoxChildren (it consumes events
// without actually moving focus). We bypass it entirely by intercepting
// arrow keys in the Capture phase — before the FlowBox sees them.
// ---------------------------------------------------------------------------

fn clear_box(container: &gtk4::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

pub(super) fn divider() -> gtk4::Separator {
    let sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
    sep.set_margin_top(super::constants::DIVIDER_VERTICAL_MARGIN);
    sep.set_margin_bottom(super::constants::DIVIDER_VERTICAL_MARGIN);
    sep.set_margin_start(super::constants::DIVIDER_SIDE_MARGIN);
    sep.set_margin_end(super::constants::DIVIDER_SIDE_MARGIN);
    sep
}

/// Which rebuild path to take when refreshing the well.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RebuildMode {
    /// Re-run the active search query.
    Search,
    /// Rebuild normal well then re-apply category filter.
    Category,
    /// Rebuild normal well (show all apps).
    Normal,
}

/// Pure decision function: determines the rebuild mode from current state.
/// Search takes precedence over category (you can search within a category view).
fn determine_rebuild_mode(active_search: &str, active_category: &[String]) -> RebuildMode {
    if !active_search.is_empty() {
        RebuildMode::Search
    } else if !active_category.is_empty() {
        RebuildMode::Category
    } else {
        RebuildMode::Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuild_mode_search_takes_precedence() {
        assert_eq!(
            determine_rebuild_mode("firefox", &["Network".to_string()]),
            RebuildMode::Search
        );
    }

    #[test]
    fn rebuild_mode_category_when_no_search() {
        assert_eq!(
            determine_rebuild_mode("", &["Network".to_string()]),
            RebuildMode::Category
        );
    }

    #[test]
    fn rebuild_mode_normal_when_both_empty() {
        assert_eq!(determine_rebuild_mode("", &[]), RebuildMode::Normal);
    }
}
