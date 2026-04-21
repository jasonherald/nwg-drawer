use crate::config::DrawerConfig;
use crate::state::DrawerState;
use crate::ui::search::subsequence_match;
use crate::ui::widgets;
use gtk4::prelude::*;
use nwg_common::desktop::entry::DesktopEntry;
use nwg_common::pinning;
use std::cell::RefCell;
use std::rc::Rc;

/// Creates the app FlowBox with optional category filter and search.
#[allow(clippy::too_many_arguments)]
pub fn build_app_flow_box(
    config: &DrawerConfig,
    state: &Rc<RefCell<DrawerState>>,
    category_filter: Option<&[String]>,
    search_phrase: &str,
    pinned_file: &std::path::Path,
    on_launch: Rc<dyn Fn()>,
    status_label: &gtk4::Label,
    on_rebuild: Option<&Rc<dyn Fn()>>,
) -> gtk4::FlowBox {
    let flow_box = create_flow_box(config);
    let entries = state.borrow().apps.entries.clone();
    let needle = search_phrase.to_lowercase();

    for entry in &entries {
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
            let button = build_button(
                entry,
                config,
                state,
                pinned_file,
                &on_launch,
                status_label,
                on_rebuild,
            );
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
fn build_button(
    entry: &DesktopEntry,
    config: &DrawerConfig,
    state: &Rc<RefCell<DrawerState>>,
    pinned_file: &std::path::Path,
    on_launch: &Rc<dyn Fn()>,
    status_label: &gtk4::Label,
    on_rebuild: Option<&Rc<dyn Fn()>>,
) -> gtk4::Button {
    let app_dirs = state.borrow().app_dirs.clone();
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

    let button = widgets::app_icon_button(
        &entry.icon,
        name,
        config.icon_size,
        &app_dirs,
        status_label,
        desc,
    );

    connect_launch(&button, entry, config, state, on_launch);

    if let Some(rebuild) = on_rebuild {
        connect_pin(&button, entry, state, pinned_file, rebuild);
    }

    // Pin indicator dot (only in grid, not in pinned section)
    if config.pin_indicator && pinning::is_pinned(&state.borrow().pinned, &entry.desktop_id) {
        widgets::apply_pin_badge(&button);
    }

    let tooltip = widgets::truncate(desc, 120);
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
    pinned_file: &std::path::Path,
    rebuild: &Rc<dyn Fn()>,
) {
    let id = entry.desktop_id.clone();
    let state_ref = Rc::clone(state);
    let path = pinned_file.to_path_buf();
    let rebuild = Rc::clone(rebuild);
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(3);
    gesture.connect_released(move |gesture, _, _, _| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        let mut s = state_ref.borrow_mut();
        let was_pinned = pinning::is_pinned(&s.pinned, &id);
        if was_pinned {
            pinning::unpin_item(&mut s.pinned, &id);
        } else {
            pinning::pin_item(&mut s.pinned, &id);
        }
        if let Err(e) = pinning::save_pinned(&s.pinned, &path) {
            log::error!("Failed to save pinned state: {}", e);
            // Rollback in-memory state to stay in sync with disk
            if was_pinned {
                pinning::pin_item(&mut s.pinned, &id);
            } else {
                pinning::unpin_item(&mut s.pinned, &id);
            }
            return;
        }
        log::info!("{} {}", if was_pinned { "Unpinned" } else { "Pinned" }, id);
        drop(s);
        rebuild();
    });
    button.add_controller(gesture);
}
