//! App-grid `FlowBox` builder.
//!
//! Builds the main scrollable grid of application buttons, optionally
//! filtered by category or search phrase. Each button wires up
//! left-click to launch and right-click to toggle pin state, plus a
//! pin-indicator dot when the entry is pinned.
//!
//! Owns [`subsequence_match`], the lightweight fuzzy matcher used by
//! the search-mode filter — kept here because the app grid is the only
//! consumer (the file-search and math paths use different mechanisms).

use crate::config::DrawerConfig;
use crate::state::DrawerState;
use crate::ui::well_context::WellContext;
use crate::ui::widgets;
use gtk4::prelude::*;
use nwg_common::desktop::entry::DesktopEntry;
use nwg_common::pinning;
use std::cell::RefCell;
use std::rc::Rc;

/// Subsequence matching: checks if all chars of `needle` appear in
/// order (but not necessarily contiguously) in `haystack`. Both inputs
/// are lowercased per call; callers that match against many haystacks
/// with a fixed needle should pre-lowercase the needle.
///
/// Used by the search-mode app filter — `"ff"` matches `"firefox"`,
/// `"frfx"` does too, but `"fz"` does not.
///
/// # Examples
///
/// ```rust,ignore
/// // Subsequence — chars in order, gaps allowed:
/// assert!(subsequence_match("ff", "firefox"));
/// assert!(subsequence_match("frfx", "firefox"));
///
/// // Case-insensitive (both sides lowercased internally):
/// assert!(subsequence_match("FI", "firefox"));
///
/// // Out-of-order or missing chars don't match:
/// assert!(!subsequence_match("xf", "firefox"));
/// assert!(!subsequence_match("fz", "firefox"));
///
/// // Empty needle matches anything:
/// assert!(subsequence_match("", "firefox"));
/// ```
///
/// (Doctests aren't compiled — this is a binary-only crate. The
/// runtime regression suite in `#[cfg(test)] mod tests` covers the
/// same cases.)
pub(crate) fn subsequence_match(needle: &str, haystack: &str) -> bool {
    let needle = needle.to_lowercase();
    let haystack = haystack.to_lowercase();

    let mut needle_chars = needle.chars();
    let mut current = needle_chars.next();

    for h in haystack.chars() {
        if let Some(n) = current {
            if n == h {
                current = needle_chars.next();
            }
        } else {
            return true;
        }
    }
    current.is_none()
}

/// Creates the app FlowBox with optional category filter and search.
///
/// Reads `config`, `state`, `pinned_file`, `on_launch`, and `status_label`
/// from `ctx`. Per-call inputs (filter, phrase, rebuild callback) stay as
/// explicit parameters because callers vary on each.
pub fn build_app_flow_box(
    ctx: &WellContext,
    category_filter: Option<&[String]>,
    search_phrase: &str,
    on_rebuild: Option<&Rc<dyn Fn()>>,
) -> gtk4::FlowBox {
    let flow_box = create_flow_box(&ctx.config);
    let needle = search_phrase.to_lowercase();

    // Hold one immutable borrow across the iteration. Iterating
    // `&s.apps.entries` directly avoids cloning the entire
    // `Vec<DesktopEntry>` per build (used to be a multi-KB allocation
    // per keystroke under heavy `.desktop` registries). RefCell allows
    // multiple immutable borrows, so build_button's own `.borrow()`s
    // for inner closures still work; only borrow_mut would conflict,
    // and those run at click time when this Ref is long dropped.
    let s = ctx.state.borrow();

    for entry in &s.apps.entries {
        if entry.no_display {
            continue;
        }

        let show = if search_phrase.is_empty() {
            match category_filter {
                Some(ids) => ids.iter().any(|id| id == &entry.desktop_id),
                None => true,
            }
        } else {
            subsequence_match(&needle, &entry.name_loc)
                || entry.comment_loc.to_lowercase().contains(&needle)
                || entry.comment.to_lowercase().contains(&needle)
                || entry.exec.to_lowercase().contains(&needle)
        };

        if show {
            let button = build_button(entry, ctx, &s.app_dirs, on_rebuild);
            insert_into_flow(&flow_box, &button);
        }
    }

    flow_box
}

/// Creates a standard FlowBox with the shared configuration.
fn create_flow_box(config: &DrawerConfig) -> gtk4::FlowBox {
    let flow_box = gtk4::FlowBox::new();
    flow_box.set_min_children_per_line(config.columns);
    flow_box.set_max_children_per_line(config.columns);
    flow_box.set_column_spacing(config.spacing);
    flow_box.set_row_spacing(config.spacing);
    flow_box.set_homogeneous(true);
    flow_box.set_selection_mode(gtk4::SelectionMode::None);
    flow_box
}

/// Inserts a button into a FlowBox with a non-focusable FlowBoxChild wrapper.
/// Navigation is handled by our capture-phase controller, not FlowBox internals.
fn insert_into_flow(flow_box: &gtk4::FlowBox, button: &gtk4::Button) {
    flow_box.insert(button, -1);
    if let Some(child) = flow_box.last_child() {
        child.set_focusable(false);
    }
}

/// Builds an app button with click-to-launch and right-click-to-pin.
///
/// `app_dirs` is borrowed from the caller's already-held `DrawerState`
/// Ref (see `build_app_flow_box`) to avoid the per-entry `Vec<PathBuf>`
/// clone that the previous implementation incurred. The slice is only
/// used during the call to `app_icon_button`; nothing inside the
/// per-button closures captures it.
fn build_button(
    entry: &DesktopEntry,
    ctx: &WellContext,
    app_dirs: &[std::path::PathBuf],
    on_rebuild: Option<&Rc<dyn Fn()>>,
) -> gtk4::Button {
    let name = widgets::display_name(entry);
    let desc = widgets::display_desc(entry);

    let button = widgets::app_icon_button(
        &entry.icon,
        name,
        ctx.config.icon_size,
        app_dirs,
        &ctx.status_label,
        desc,
    );

    connect_launch(&button, entry, &ctx.config, &ctx.state, &ctx.on_launch);

    if let Some(rebuild) = on_rebuild {
        connect_pin(&button, entry, &ctx.state, &ctx.pinned_file, rebuild);
    }

    // Pin indicator dot (only in grid, not in pinned section)
    if ctx.config.pin_indicator && pinning::is_pinned(&ctx.state.borrow().pinned, &entry.desktop_id)
    {
        widgets::apply_pin_badge(&button);
    }

    let tooltip = widgets::truncate(desc, super::constants::APP_TOOLTIP_MAX_CHARS);
    if !tooltip.is_empty() {
        button.set_tooltip_text(Some(&tooltip));
    }

    button
}

/// Connects left-click to launch the app.
fn connect_launch(
    button: &gtk4::Button,
    entry: &DesktopEntry,
    config: &DrawerConfig,
    state: &Rc<RefCell<DrawerState>>,
    on_launch: &Rc<dyn Fn()>,
) {
    let exec = entry.exec.clone();
    let terminal = entry.terminal;
    let term_cmd = config.term.clone();
    let on_launch_click = Rc::clone(on_launch);
    let compositor = Rc::clone(&state.borrow().compositor);
    let theme_prefix = state.borrow().gtk_theme_prefix.clone();
    button.connect_clicked(move |_| {
        nwg_common::launch::launch_desktop_entry(
            &exec,
            terminal,
            &term_cmd,
            &theme_prefix,
            &*compositor,
        );
        on_launch_click();
    });
}

/// Connects right-click to toggle pin state and trigger a rebuild.
fn connect_pin(
    button: &gtk4::Button,
    entry: &DesktopEntry,
    state: &Rc<RefCell<DrawerState>>,
    pinned_file: &Rc<std::path::Path>,
    rebuild: &Rc<dyn Fn()>,
) {
    let id = entry.desktop_id.clone();
    let state_ref = Rc::clone(state);
    // Cheap Rc clone — closure derefs to `&Path` for save_pinned, no
    // PathBuf allocation per pin/unpin click.
    let path = Rc::clone(pinned_file);
    let rebuild = Rc::clone(rebuild);
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(super::constants::MOUSE_BUTTON_RIGHT);
    gesture.connect_released(move |gesture, _, _, _| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        match super::pin_ops::toggle_pin_with_save(&state_ref, &id, &path) {
            Ok(was_pinned) => {
                log::info!("{} {}", if was_pinned { "Unpinned" } else { "Pinned" }, id);
                rebuild();
            }
            Err(e) => log::error!("Failed to save pinned state: {}", e),
        }
    });
    button.add_controller(gesture);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subsequence_match() {
        assert!(subsequence_match("ff", "firefox"));
        assert!(subsequence_match("frfx", "firefox"));
        assert!(subsequence_match("firefox", "firefox"));
        assert!(!subsequence_match("fz", "firefox"));
        assert!(subsequence_match("", "anything"));
        assert!(subsequence_match("FI", "firefox")); // case insensitive
    }
}
