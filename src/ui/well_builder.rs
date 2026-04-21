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
    clear_box(&ctx.well);
    clear_box(&ctx.pinned_box);

    let pinned = ctx.state.borrow().pinned.clone();

    // Rebuild callback shared by pinned unpin + app grid pin
    let on_rebuild = build_rebuild_callback(ctx);

    // Pinned items (above scroll)
    if !pinned.is_empty() {
        let pf = build_pinned_flow(ctx, &on_rebuild);
        pf.set_halign(gtk4::Align::Center);
        ctx.pinned_box.append(&pf);
    }

    // App grid (scrollable)
    let flow = ui::app_grid::build_app_flow_box(
        &ctx.config,
        &ctx.state,
        None,
        "",
        &ctx.pinned_file,
        Rc::clone(&ctx.on_launch),
        &ctx.status_label,
        Some(&on_rebuild),
    );
    flow.set_halign(gtk4::Align::Center);
    ctx.well.append(&flow);

    // Install grid navigation on both FlowBoxes.
    let has_pinned = !pinned.is_empty();
    let pinned_flow_opt = if has_pinned {
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
            ctx.config.columns.min(pinned.len() as u32).max(1),
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
    if let Some(math_widget) = ui::math::build_math_result(phrase) {
        ctx.well.append(&math_widget);
    }

    // Rebuild callback — rebuild_preserving_category checks active_search
    // and will re-run the search instead of restoring normal view.
    let on_rebuild = build_rebuild_callback(ctx);
    let app_flow = ui::app_grid::build_app_flow_box(
        &ctx.config,
        &ctx.state,
        None,
        phrase,
        &ctx.pinned_file,
        Rc::clone(&ctx.on_launch),
        &ctx.status_label,
        Some(&on_rebuild),
    );
    app_flow.set_halign(gtk4::Align::Center);
    ctx.well.append(&app_flow);

    // Search results get navigation too (no cross-section targets)
    navigation::install_grid_nav(&app_flow, ctx.config.columns, None, None);

    // File results
    if !ctx.config.no_fs && phrase.len() > 2 {
        let file_results = ui::file_search::search_files(
            phrase,
            &ctx.config,
            &ctx.state,
            Rc::clone(&ctx.on_launch),
        );
        // file_search::search_files adds a header + separator before result rows
        let total_children = count_children(&file_results);
        let file_count = total_children.saturating_sub(2);
        if file_count > 0 {
            ctx.well.append(&divider());
            ctx.status_label.set_text(&format!(
                "{} file results | LMB: open | RMB: file manager",
                file_count
            ));
            file_results.set_halign(gtk4::Align::Center);
            ctx.well.append(&file_results);

            // Up from first file result → back to app search results
            navigation::install_file_results_nav(&file_results);
        }
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
    let pinned = ctx.state.borrow().pinned.clone();
    let cols = ctx.config.columns.min(pinned.len() as u32).max(1);
    flow_box.set_min_children_per_line(cols);
    flow_box.set_max_children_per_line(cols);
    flow_box.set_column_spacing(ctx.config.spacing);
    flow_box.set_row_spacing(ctx.config.spacing);
    flow_box.set_homogeneous(true);
    flow_box.set_selection_mode(gtk4::SelectionMode::None);

    let id2entry = ctx.state.borrow().apps.id2entry.clone();
    let app_dirs = ctx.state.borrow().app_dirs.clone();

    for desktop_id in &pinned {
        let entry = match id2entry.get(desktop_id) {
            Some(e) if !e.desktop_id.is_empty() && !e.no_display => e,
            _ => continue,
        };
        let button = build_pinned_button(entry, ctx, &app_dirs, on_rebuild, desktop_id);
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
    let path = ctx.pinned_file.as_ref().clone();
    let rebuild = Rc::clone(on_rebuild);
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(3);
    gesture.connect_released(move |gesture, _, _, _| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        let mut s = state_ref.borrow_mut();
        if pinning::unpin_item(&mut s.pinned, &id) {
            if let Err(e) = pinning::save_pinned(&s.pinned, &path) {
                log::error!("Failed to save pinned state: {}", e);
                // Restore the pin so UI stays in sync with disk
                s.pinned.push(id.clone());
                return;
            }
            log::info!("Unpinned {}", id);
            drop(s);
            rebuild();
        }
    });
    button.add_controller(gesture);

    button
}

/// Creates a callback that rebuilds the entire well + pinned_box.
/// Public so category filter can create rebuild callbacks for pin/unpin.
pub fn build_rebuild_callback(ctx: &WellContext) -> Rc<dyn Fn()> {
    let ctx = ctx.clone();
    Rc::new(move || {
        let ctx = ctx.clone();
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

fn divider() -> gtk4::Separator {
    let sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
    sep.set_margin_top(super::constants::DIVIDER_VERTICAL_MARGIN);
    sep.set_margin_bottom(super::constants::DIVIDER_VERTICAL_MARGIN);
    sep.set_margin_start(super::constants::DIVIDER_SIDE_MARGIN);
    sep.set_margin_end(super::constants::DIVIDER_SIDE_MARGIN);
    sep
}

fn count_children(widget: &impl IsA<gtk4::Widget>) -> i32 {
    let mut count = 0;
    let mut child = widget.first_child();
    while let Some(c) = child {
        count += 1;
        child = c.next_sibling();
    }
    count
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
