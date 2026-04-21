use crate::config::DrawerConfig;
use crate::state::DrawerState;
use crate::ui::widgets;
use gtk4::prelude::*;
use nwg_common::desktop::entry::DesktopEntry;
use nwg_common::pinning;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Builds the pinned items FlowBox.
pub fn build_pinned_flow_box(
    config: &DrawerConfig,
    state: &Rc<RefCell<DrawerState>>,
    pinned_file: &Path,
    on_launch: Rc<dyn Fn()>,
    status_label: &gtk4::Label,
) -> gtk4::FlowBox {
    let flow_box = gtk4::FlowBox::new();

    let pinned = state.borrow().pinned.clone();
    // Use fewer columns if there are fewer pinned items (matches Go behavior)
    let cols = config.columns.min(pinned.len() as u32).max(1);
    flow_box.set_min_children_per_line(cols);
    flow_box.set_max_children_per_line(cols);
    flow_box.set_column_spacing(config.spacing);
    flow_box.set_row_spacing(config.spacing);
    flow_box.set_homogeneous(true);
    flow_box.set_widget_name("pinned-box");
    flow_box.set_selection_mode(gtk4::SelectionMode::Single);

    let id2entry = state.borrow().apps.id2entry.clone();
    let app_dirs = state.borrow().app_dirs.clone();

    for desktop_id in &pinned {
        let entry = match id2entry.get(desktop_id) {
            Some(e) if !e.desktop_id.is_empty() => e,
            _ => {
                log::debug!("Pinned item doesn't seem to exist: {}", desktop_id);
                continue;
            }
        };

        let button = create_pinned_button(
            entry,
            config,
            state,
            pinned_file,
            desktop_id,
            &app_dirs,
            &on_launch,
            status_label,
        );
        flow_box.insert(&button, -1);
    }

    // Enter on a focused FlowBoxChild activates the button inside (launches app)
    flow_box.connect_child_activated(|_, child| {
        if let Some(button) = child.child() {
            if let Ok(btn) = button.downcast::<gtk4::Button>() {
                btn.emit_clicked();
            }
        }
    });

    flow_box
}

/// Creates a single pinned app button with launch (left-click) and unpin (right-click) actions.
#[allow(clippy::too_many_arguments)]
fn create_pinned_button(
    entry: &DesktopEntry,
    config: &DrawerConfig,
    state: &Rc<RefCell<DrawerState>>,
    pinned_file: &Path,
    desktop_id: &str,
    app_dirs: &[PathBuf],
    on_launch: &Rc<dyn Fn()>,
    status_label: &gtk4::Label,
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
    let button = widgets::app_icon_button(
        &entry.icon,
        name,
        config.icon_size,
        app_dirs,
        status_label,
        desc,
    );

    // Left click → launch
    let exec = entry.exec.clone();
    let terminal = entry.terminal;
    let term = config.term.clone();
    let on_launch_ref = Rc::clone(on_launch);
    let compositor = Rc::clone(&state.borrow().compositor);
    let theme_prefix = state.borrow().gtk_theme_prefix.clone();
    button.connect_clicked(move |_| {
        nwg_common::launch::launch_desktop_entry(
            &exec, terminal, &term, &theme_prefix, &*compositor,
        );
        on_launch_ref();
    });

    // Right-click → unpin
    let id = desktop_id.to_string();
    let state_ref = Rc::clone(state);
    let pinned_path = pinned_file.to_path_buf();
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(3);
    gesture.connect_released(move |gesture, _, _, _| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        let mut s = state_ref.borrow_mut();
        if pinning::unpin_item(&mut s.pinned, &id) {
            if let Err(e) = pinning::save_pinned(&s.pinned, &pinned_path) {
                log::error!("Failed to save pinned state: {}", e);
                pinning::pin_item(&mut s.pinned, &id);
                return;
            }
            log::info!("Unpinned {}", id);
        }
    });
    button.add_controller(gesture);

    button
}
