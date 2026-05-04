//! GTK widget for inline math results in the file-search well.
//!
//! Builds a `Box` containing the rendered `expr = result` line plus a
//! "Copy" button that shells out to `wl-copy` on click. The pure
//! evaluator + `format_result` lives in [`super::math`]; this module
//! is the GTK / clipboard / keyboard-navigation half.
//!
//! Styling (font size, padding, border radius, colors) is in
//! `assets/drawer.css` under the `.math-result` / `.math-copy` rules.
//! No runtime CSS injection — the embedded stylesheet is loaded once at
//! drawer activation.
use super::constants;
use super::math::{MathResult, eval_expression};
use gtk4::prelude::*;

/// Builds an inline math result widget for the search well.
/// Returns `None` if the phrase isn't a math expression.
pub fn build_math_result(phrase: &str) -> Option<gtk4::Box> {
    let (label_text, result_str) = match eval_expression(phrase) {
        MathResult::Value(val) => {
            let r = super::math::format_result(val);
            (format!("{} = {}", phrase, r), Some(r))
        }
        MathResult::Error(msg) => (format!("{} — {}", phrase, msg), None),
        MathResult::NotMath => return None,
    };

    let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, constants::MATH_VBOX_SPACING);
    vbox.set_halign(gtk4::Align::Center);
    vbox.set_margin_top(constants::STATUS_AREA_VERTICAL_MARGIN);
    vbox.set_margin_bottom(constants::STATUS_AREA_VERTICAL_MARGIN);
    // Don't set focusable(false) — the capture-phase key controller needs
    // the container to participate in event dispatch for arrow key navigation.

    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, constants::MATH_ROW_SPACING);
    row.set_halign(gtk4::Align::Center);

    let label = gtk4::Label::new(Some(&label_text));
    label.add_css_class("math-result");
    label.set_halign(gtk4::Align::End);
    label.set_focusable(false);
    row.append(&label);

    if let Some(result_copy) = result_str {
        append_copy_button(&row, &vbox, result_copy);
    } else {
        vbox.append(&row);
    }

    install_keyboard_nav(&vbox);
    Some(vbox)
}

/// Capture-phase key controller on the math vbox — fires before GTK's
/// focus machinery processes arrow keys on the child Copy button. Same
/// pattern as `install_grid_nav` on FlowBoxes (see `super::navigation`).
fn install_keyboard_nav(vbox: &gtk4::Box) {
    let vbox_weak = vbox.downgrade();
    let nav_ctrl = gtk4::EventControllerKey::new();
    nav_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
    nav_ctrl.connect_key_pressed(move |_, keyval, _, _| {
        let Some(vbox_ref) = vbox_weak.upgrade() else {
            return gtk4::glib::Propagation::Proceed;
        };
        match keyval {
            gtk4::gdk::Key::Down | gtk4::gdk::Key::Tab => {
                if super::navigation::focus_next_sibling(&vbox_ref) {
                    gtk4::glib::Propagation::Stop
                } else {
                    gtk4::glib::Propagation::Proceed
                }
            }
            gtk4::gdk::Key::Up | gtk4::gdk::Key::ISO_Left_Tab => {
                if super::navigation::focus_prev_sibling(&vbox_ref) {
                    gtk4::glib::Propagation::Stop
                } else {
                    gtk4::glib::Propagation::Proceed
                }
            }
            _ => gtk4::glib::Propagation::Proceed,
        }
    });
    vbox.add_controller(nav_ctrl);
}

/// Appends a copy button and "Copied!" label to the math result row.
/// Copies the result to clipboard via wl-copy on click, with a 2-second
/// confirmation label that resets on repeated clicks.
fn append_copy_button(row: &gtk4::Box, vbox: &gtk4::Box, result_copy: String) {
    let sep = gtk4::Separator::new(gtk4::Orientation::Vertical);
    sep.add_css_class("math-divider");
    row.append(&sep);

    let copy_btn = gtk4::Button::with_label("Copy");
    copy_btn.add_css_class("math-copy");
    copy_btn.set_focusable(true);
    copy_btn.set_halign(gtk4::Align::Start);

    let copied_label = gtk4::Label::new(Some("Copied!"));
    copied_label.add_css_class("math-copied");
    copied_label.set_visible(false);

    let copied_ref = copied_label.clone();
    let pending_timer: std::rc::Rc<std::cell::Cell<Option<gtk4::glib::SourceId>>> =
        std::rc::Rc::new(std::cell::Cell::new(None));
    let timer_ref = std::rc::Rc::clone(&pending_timer);
    copy_btn.connect_clicked(move |_| {
        let mut cmd = std::process::Command::new("wl-copy");
        // `--` ends wl-copy's option parsing so negative results (e.g. `-5`,
        // `-3.14`) aren't mistaken for unknown flags.
        cmd.arg("--").arg(&result_copy);
        match cmd.spawn() {
            Ok(child) => nwg_common::launch::reap_child(child, "wl-copy".to_string()),
            Err(e) => {
                log::warn!(
                    "Failed to spawn wl-copy: {} (clipboard copy unavailable)",
                    e
                );
                return;
            }
        }
        // Cancel previous hide timer so repeated clicks reset the 2s window
        if let Some(id) = timer_ref.take() {
            id.remove();
        }
        copied_ref.set_visible(true);
        // WeakRef so the still-pending timer doesn't keep the label
        // alive after the math result row is rebuilt.
        let hide_weak = copied_ref.downgrade();
        let timer_reset = std::rc::Rc::clone(&timer_ref);
        let id = gtk4::glib::timeout_add_local_once(
            std::time::Duration::from_secs(constants::COPIED_LABEL_TIMEOUT_SECS),
            move || {
                if let Some(label) = hide_weak.upgrade() {
                    label.set_visible(false);
                }
                timer_reset.set(None);
            },
        );
        timer_ref.set(Some(id));
    });
    row.append(&copy_btn);
    vbox.append(row);
    vbox.append(&copied_label);
}
