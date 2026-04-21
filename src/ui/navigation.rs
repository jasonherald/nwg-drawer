use gtk4::prelude::*;

/// Installs a capture-phase key controller on a FlowBox that handles
/// Up/Down/Left/Right arrow navigation within the grid and across sections.
///
/// `up_target` / `down_target`: optional FlowBox to jump to when reaching
/// the top/bottom edge of this grid.
pub fn install_grid_nav(
    flow: &gtk4::FlowBox,
    columns: u32,
    up_target: Option<&gtk4::FlowBox>,
    down_target: Option<&gtk4::FlowBox>,
) {
    // Use weak references to avoid cycles: widget owns controller → closure → widget
    let flow_weak = flow.downgrade();
    let up_weak = up_target.map(|t| t.downgrade());
    let down_weak = down_target.map(|t| t.downgrade());
    let cols = columns.max(1);

    // Remove any previous grid-nav controller to avoid stacking
    remove_named_controller(flow, "grid-nav");

    let ctrl = gtk4::EventControllerKey::new();
    ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
    ctrl.set_name(Some("grid-nav"));

    ctrl.connect_key_pressed(move |_, keyval, _, _| {
        let Some(flow_ref) = flow_weak.upgrade() else {
            return gtk4::glib::Propagation::Proceed;
        };
        let total = count_flow_children(&flow_ref);
        if total == 0 {
            return gtk4::glib::Propagation::Proceed;
        }

        let up_ref = up_weak.as_ref().and_then(|w| w.upgrade());
        let down_ref = down_weak.as_ref().and_then(|w| w.upgrade());
        let (idx, col) = focused_position(&flow_ref, cols);

        match keyval {
            gtk4::gdk::Key::Right => nav_horizontal(&flow_ref, idx, col, 1, cols, total),
            gtk4::gdk::Key::Left => nav_horizontal(&flow_ref, idx, col, -1, cols, total),
            gtk4::gdk::Key::Down => nav_down(&flow_ref, idx, col, cols, total, &down_ref),
            gtk4::gdk::Key::Up => nav_up(&flow_ref, idx, col, cols, &up_ref),
            _ => gtk4::glib::Propagation::Proceed,
        }
    });

    flow.add_controller(ctrl);
}

/// Installs Up/Down navigation on file search results (vertical button list).
/// GTK handles Down between buttons natively. Up from the first button
/// needs to reach the app search FlowBox above.
pub(super) fn install_file_results_nav(container: &gtk4::Box) {
    // Remove any previous controller to avoid stacking on rebuild
    remove_named_controller(container, "file-results-nav");

    let container_weak = container.downgrade();
    let ctrl = gtk4::EventControllerKey::new();
    ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
    ctrl.set_name(Some("file-results-nav"));
    ctrl.connect_key_pressed(move |_, keyval, _, _| {
        let Some(container_ref) = container_weak.upgrade() else {
            return gtk4::glib::Propagation::Proceed;
        };
        match keyval {
            gtk4::gdk::Key::Up => {
                // Check if focus is on the first button
                if let Some(first) = first_focusable_child(&container_ref)
                    && (first.has_focus() || first.is_focus())
                    && focus_prev_sibling(&container_ref)
                {
                    return gtk4::glib::Propagation::Stop;
                }
                gtk4::glib::Propagation::Proceed
            }
            _ => gtk4::glib::Propagation::Proceed,
        }
    });
    container.add_controller(ctrl);
}

/// Handles Left/Right navigation within a grid row.
/// Clamps to row boundaries so Right on the last column doesn't wrap to the next row.
fn nav_horizontal(
    flow: &gtk4::FlowBox,
    idx: i32,
    col: i32,
    delta: i32,
    cols: u32,
    total: i32,
) -> gtk4::glib::Propagation {
    let new_col = col + delta;
    if new_col < 0 || new_col >= cols as i32 {
        return gtk4::glib::Propagation::Stop; // At row edge — don't wrap
    }
    let next = idx + delta;
    if next >= 0 && next < total {
        focus_child_button(flow, next);
    }
    gtk4::glib::Propagation::Stop
}

/// Handles Down navigation: within grid, cross-section, or escape to next widget.
fn nav_down(
    flow: &gtk4::FlowBox,
    idx: i32,
    col: i32,
    cols: u32,
    total: i32,
    down_target: &Option<gtk4::FlowBox>,
) -> gtk4::glib::Propagation {
    let next = idx + cols as i32;
    if next < total {
        focus_child_button(flow, next);
        return gtk4::glib::Propagation::Stop;
    }
    // No item directly below — try cross-section FlowBox
    if let Some(target) = down_target {
        let target_total = count_flow_children(target);
        if target_total > 0 {
            // Clamp by target grid width so we land in the first row, not a later one
            let target_cols = target
                .max_children_per_line()
                .min(target_total as u32)
                .max(1) as i32;
            focus_child_button(target, col.min(target_cols - 1));
            return gtk4::glib::Propagation::Stop;
        }
        // Target exists but is empty — fall through to widget search
    }
    // No FlowBox target — try next visible widget (e.g. file results)
    if focus_next_sibling(flow) {
        return gtk4::glib::Propagation::Stop;
    }
    gtk4::glib::Propagation::Stop
}

/// Handles Up navigation: within grid, cross-section, or escape to previous widget.
fn nav_up(
    flow: &gtk4::FlowBox,
    idx: i32,
    col: i32,
    cols: u32,
    up_target: &Option<gtk4::FlowBox>,
) -> gtk4::glib::Propagation {
    let prev = idx - cols as i32;
    if prev >= 0 {
        focus_child_button(flow, prev);
        return gtk4::glib::Propagation::Stop;
    }
    // Top edge — try cross-section FlowBox
    if let Some(target) = up_target {
        let target_total = count_flow_children(target);
        if target_total > 0 {
            let target_cols = target
                .max_children_per_line()
                .min(target_total as u32)
                .max(1);
            focus_child_button(
                target,
                find_column_from_bottom(col, target_cols, target_total),
            );
            return gtk4::glib::Propagation::Stop;
        }
        // Target exists but is empty — fall through to widget search
    }
    // No FlowBox target — focus nearest widget above (categories, search)
    if focus_prev_sibling(flow) {
        return gtk4::glib::Propagation::Stop;
    }
    gtk4::glib::Propagation::Proceed
}

/// Walks up the widget tree from `start`, looking for the nearest visible
/// previous sibling that can accept focus. Handles nested containers like
/// ScrolledWindow by checking siblings at each ancestor level.
/// Uses `grab_last_focusable` to drill into containers (e.g. the math result
/// vbox) where GTK's `child_focus()` returns true but doesn't actually move
/// visible focus to inner buttons.
pub(super) fn focus_prev_sibling(start: &impl IsA<gtk4::Widget>) -> bool {
    let mut current = Some(start.as_ref().clone());
    while let Some(widget) = current {
        let mut prev = widget.prev_sibling();
        while let Some(p) = prev {
            if p.is_visible() && p.is_sensitive() && grab_last_focusable(&p) {
                return true;
            }
            prev = p.prev_sibling();
        }
        current = widget.parent();
    }
    false
}

/// Walks down the widget tree from `start`, looking for the nearest visible
/// next sibling that can accept focus. Mirror of `focus_prev_sibling`.
pub(super) fn focus_next_sibling(start: &impl IsA<gtk4::Widget>) -> bool {
    let mut current = Some(start.as_ref().clone());
    while let Some(widget) = current {
        let mut next = widget.next_sibling();
        while let Some(n) = next {
            if n.is_visible() && n.is_sensitive() && grab_first_focusable(&n) {
                return true;
            }
            next = n.next_sibling();
        }
        current = widget.parent();
    }
    false
}

/// Recursively finds and grabs focus on the first focusable widget in a tree.
/// Checks children first so we land on the deepest interactive widget (e.g. a
/// button inside a Box) rather than the container itself.
pub(super) fn grab_first_focusable(widget: &gtk4::Widget) -> bool {
    // Try children first — prefer the deepest focusable descendant
    let mut child = widget.first_child();
    while let Some(c) = child {
        if grab_first_focusable(&c) {
            return true;
        }
        child = c.next_sibling();
    }
    // No focusable children — try the widget itself
    widget.is_focusable() && widget.grab_focus()
}

/// Recursively finds and grabs focus on the last focusable widget in a tree.
/// Used when navigating upward — focuses the bottom-most interactive element.
pub(super) fn grab_last_focusable(widget: &gtk4::Widget) -> bool {
    // Check children in reverse order first (deepest last child)
    let mut child = widget.last_child();
    while let Some(c) = child {
        if grab_last_focusable(&c) {
            return true;
        }
        child = c.prev_sibling();
    }
    // Then check the widget itself
    if widget.is_focusable() && widget.grab_focus() {
        return true;
    }
    false
}

/// Finds the nearest item at `col` starting from the bottom row and walking up.
/// Handles partial last rows where the target column may not have an item.
/// Clamps `col` to the target grid width when transitioning from a wider grid.
fn find_column_from_bottom(col: i32, cols: u32, total: i32) -> i32 {
    let cols_i = cols as i32;
    let clamped_col = col.min(cols_i - 1).max(0);
    let last_row = (total - 1) / cols_i;
    for row in (0..=last_row).rev() {
        let idx = row * cols_i + clamped_col;
        if idx < total {
            return idx;
        }
    }
    total.saturating_sub(1)
}

/// Returns the first focusable child widget (skipping headers/separators).
fn first_focusable_child(container: &gtk4::Box) -> Option<gtk4::Widget> {
    let mut child = container.first_child();
    while let Some(c) = child {
        if c.is_focusable() {
            return Some(c);
        }
        child = c.next_sibling();
    }
    None
}

/// Focuses the button inside the FlowBoxChild at the given index.
fn focus_child_button(flow: &gtk4::FlowBox, index: i32) {
    if let Some(child) = flow.child_at_index(index)
        && let Some(btn) = child.first_child()
    {
        btn.grab_focus();
    }
}

/// Returns the (index, column) of the currently focused child in a FlowBox.
/// Checks both the FlowBoxChild and its inner button for focus.
fn focused_position(flow: &gtk4::FlowBox, columns: u32) -> (i32, i32) {
    let mut idx = 0;
    let mut child = flow.first_child();
    while let Some(c) = child {
        if c.has_focus() || c.is_focus() {
            return (idx, idx % columns as i32);
        }
        if let Some(inner) = c.first_child()
            && (inner.has_focus() || inner.is_focus())
        {
            return (idx, idx % columns as i32);
        }
        idx += 1;
        child = c.next_sibling();
    }
    (0, 0)
}

fn count_flow_children(flow: &gtk4::FlowBox) -> i32 {
    let mut n = 0;
    let mut child = flow.first_child();
    while let Some(c) = child {
        n += 1;
        child = c.next_sibling();
    }
    n
}

/// Removes an event controller by name from a widget.
fn remove_named_controller(widget: &impl IsA<gtk4::Widget>, name: &str) {
    let mut controllers = Vec::new();
    let list = widget.observe_controllers();
    for i in 0..list.n_items() {
        if let Some(obj) = list.item(i)
            && let Ok(ctrl) = obj.downcast::<gtk4::EventController>()
            && ctrl.name().as_deref() == Some(name)
        {
            controllers.push(ctrl);
        }
    }
    for ctrl in controllers {
        widget.remove_controller(&ctrl);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_column_from_bottom_exact() {
        // 3 columns, 6 items (2 full rows). Column 1, bottom row → index 4.
        assert_eq!(find_column_from_bottom(1, 3, 6), 4);
    }

    #[test]
    fn find_column_from_bottom_partial_last_row() {
        // 3 columns, 5 items. Last row has only 2 items (indices 3,4).
        // Column 2 doesn't exist in last row → walk up to row 0, index 2.
        assert_eq!(find_column_from_bottom(2, 3, 5), 2);
    }

    #[test]
    fn find_column_from_bottom_single_item() {
        // 3 columns, 1 item. Only index 0 exists.
        assert_eq!(find_column_from_bottom(0, 3, 1), 0);
        assert_eq!(find_column_from_bottom(1, 3, 1), 0); // col 1 not in row 0, saturates
    }

    #[test]
    fn find_column_from_bottom_full_grid() {
        // 4 columns, 8 items (2 full rows). Column 3, bottom row → index 7.
        assert_eq!(find_column_from_bottom(3, 4, 8), 7);
    }

    #[test]
    fn find_column_from_bottom_single_column() {
        // 1 column, 5 items. Column 0, bottom → index 4.
        assert_eq!(find_column_from_bottom(0, 1, 5), 4);
    }

    #[test]
    fn find_column_from_bottom_clamps_oversized_column() {
        // Transitioning from a 4-column grid to a 2-column grid.
        // col=3 exceeds target width → clamp to col 1 (last column in target).
        // 2 columns, 3 items. Row 1: [0,1], Row 2: [2]. Col 1, bottom → index 1.
        assert_eq!(find_column_from_bottom(3, 2, 3), 1);
    }
}
