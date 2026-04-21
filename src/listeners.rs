use crate::config::DrawerConfig;
use crate::ui::well_builder;
use crate::{desktop_loader, watcher};
use gtk4::glib;
use gtk4::prelude::*;
use nwg_common::compositor::Compositor;
use nwg_common::pinning;
use nwg_common::signals::WindowCommand;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc;

/// Sets up keyboard handler for the drawer window.
///
/// - Navigation keys (arrows, Tab, Page Up/Down, Home/End) propagate to
///   FlowBox children for keyboard navigation between app icons
/// - Escape clears search or closes the drawer
/// - Return handles `:command` execution and math evaluation (only when
///   the search entry has focus — otherwise it propagates to the focused
///   button, launching the app via GTK4's button activate)
/// - Any other key auto-focuses the search entry so typing starts a search
pub fn setup_keyboard(
    win: &gtk4::ApplicationWindow,
    search_entry: &gtk4::SearchEntry,
    config: &Rc<DrawerConfig>,
    on_launch: &Rc<dyn Fn()>,
    compositor: &Rc<dyn Compositor>,
) {
    let win_ctrl = win.clone();
    let win = win.clone();
    let config = Rc::clone(config);
    let search_entry = search_entry.clone();
    let on_launch = Rc::clone(on_launch);
    let compositor = Rc::clone(compositor);

    // SearchEntry consumes Return internally via its `activate` signal before
    // the window's capture-phase key controller sees it. Connect directly to
    // handle `:command` execution and math evaluation.
    {
        let search_entry_ref = search_entry.clone();
        let compositor = Rc::clone(&compositor);
        let on_launch = Rc::clone(&on_launch);
        search_entry.connect_activate(move |_| {
            handle_return(&search_entry_ref, &*compositor, &on_launch);
        });
    }

    // Key press handler — intercepts Escape, Return, and auto-focus-search.
    // Capture phase so it fires even when no widget has focus (e.g. fresh open).
    // Navigation keys return Proceed so GTK handles focus movement.
    let key_ctrl = gtk4::EventControllerKey::new();
    key_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
    key_ctrl.connect_key_pressed(move |_, keyval, _, _| {
        match keyval {
            gtk4::gdk::Key::Escape => {
                handle_escape(&search_entry, &win, config.resident);
                gtk4::glib::Propagation::Stop
            }

            gtk4::gdk::Key::Return | gtk4::gdk::Key::KP_Enter => {
                // Handled by SearchEntry's activate signal when search has focus.
                // This path covers Return when a grid button has focus.
                gtk4::glib::Propagation::Proceed
            }

            // Navigation keys — let GTK handle focus movement
            gtk4::gdk::Key::Up
            | gtk4::gdk::Key::Down
            | gtk4::gdk::Key::Left
            | gtk4::gdk::Key::Right
            | gtk4::gdk::Key::Tab
            | gtk4::gdk::Key::ISO_Left_Tab
            | gtk4::gdk::Key::Page_Up
            | gtk4::gdk::Key::Page_Down
            | gtk4::gdk::Key::Home
            | gtk4::gdk::Key::End => gtk4::glib::Propagation::Proceed,

            // Any other key — auto-focus search entry so typing starts a search
            _ => {
                if !search_entry.has_focus() {
                    search_entry.grab_focus();
                }
                gtk4::glib::Propagation::Proceed
            }
        }
    });
    win_ctrl.add_controller(key_ctrl);
}

/// Polls compositor active window to close drawer when another window gets focus.
///
/// Also handles reactive focus delivery: when `focus_pending` is set (by the
/// signal poller on show), waits for the compositor to deliver focus via
/// `is_active_notify` before grabbing focus on the search entry. This avoids
/// timing races with idle/timeout callbacks.
pub fn setup_focus_detector(
    win: &gtk4::ApplicationWindow,
    search_entry: &gtk4::SearchEntry,
    well_ctx: &crate::ui::well_context::WellContext,
    focus_pending: &Rc<Cell<bool>>,
    on_launch: &Rc<dyn Fn()>,
    compositor: &Rc<dyn Compositor>,
) {
    // React to compositor focus delivery via is_active_notify.
    // When focus_pending is set, the drawer was just shown — grab focus on
    // the search entry once the compositor confirms focus. Skip the close
    // logic during the transition to avoid closing during show.
    {
        let on_launch = Rc::clone(on_launch);
        let win_ref = win.clone();
        let entry = search_entry.clone();
        let ctx = well_ctx.clone();
        let pending = Rc::clone(focus_pending);
        win.connect_is_active_notify(move |_| {
            if pending.get() {
                if win_ref.is_active() {
                    // Compositor delivered focus — grab it on the search entry
                    pending.set(false);
                    complete_show(&entry, &ctx);
                }
                // Still pending but not active yet — skip close logic
                return;
            }
            if !win_ref.is_active() {
                on_launch();
            }
        });
    }

    let win = win.clone();
    let on_launch = Rc::clone(on_launch);
    let compositor = Rc::clone(compositor);
    let baseline: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

    glib::timeout_add_local(std::time::Duration::from_millis(300), move || {
        if !win.is_visible() {
            *baseline.borrow_mut() = None;
            return glib::ControlFlow::Continue;
        }
        poll_active_window(&compositor, &baseline, &on_launch);
        glib::ControlFlow::Continue
    });
}

/// Sets up inotify-based file watcher for pin and desktop file changes.
pub fn setup_file_watcher(
    app_dirs: &[std::path::PathBuf],
    ctx: &crate::ui::well_context::WellContext,
) {
    let watch_rx = watcher::start_watcher(app_dirs, &ctx.pinned_file);
    let ctx = ctx.clone();

    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        while let Ok(event) = watch_rx.try_recv() {
            match event {
                watcher::WatchEvent::DesktopFilesChanged => {
                    log::info!("Desktop files changed, reloading...");
                    desktop_loader::load_desktop_entries(&mut ctx.state.borrow_mut());
                    well_builder::rebuild_preserving_category(&ctx);
                }
                watcher::WatchEvent::PinnedChanged => {
                    log::info!("Pinned file changed, rebuilding...");
                    ctx.state.borrow_mut().pinned = pinning::load_pinned(&ctx.pinned_file);
                    well_builder::rebuild_preserving_category(&ctx);
                }
            }
        }
        glib::ControlFlow::Continue
    });
}

/// Sets up signal handler polling for SIGRTMIN+1/2/3.
pub fn setup_signal_poller(
    win: &gtk4::ApplicationWindow,
    search_entry: &gtk4::SearchEntry,
    well_ctx: &crate::ui::well_context::WellContext,
    focus_pending: &Rc<Cell<bool>>,
    sig_rx: &Rc<mpsc::Receiver<WindowCommand>>,
    resident: bool,
) {
    let win = win.clone();
    let entry = search_entry.clone();
    let ctx = well_ctx.clone();
    let pending = Rc::clone(focus_pending);
    let rx = Rc::clone(sig_rx);

    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        while let Ok(cmd) = rx.try_recv() {
            handle_window_command(&win, &entry, &ctx, &pending, cmd, resident);
        }
        glib::ControlFlow::Continue
    });
}

/// Checks if the active window changed and closes the drawer if so.
fn poll_active_window(
    compositor: &Rc<dyn Compositor>,
    baseline: &Rc<RefCell<Option<String>>>,
    on_launch: &Rc<dyn Fn()>,
) {
    let active = match compositor.get_active_window() {
        Ok(a) => a,
        Err(_) => {
            // Compositor error (e.g. workspace with no windows) — close
            close_if_baseline_set(baseline, on_launch);
            return;
        }
    };

    // Empty id+class means no window focused (e.g. switched workspace) — close
    if active.id.is_empty() && active.class.is_empty() {
        close_if_baseline_set(baseline, on_launch);
        return;
    }

    // Skip partial responses (e.g. layer-shell surfaces)
    if active.id.is_empty() || active.class.is_empty() {
        return;
    }

    let mut b = baseline.borrow_mut();
    if b.is_none() {
        *b = Some(active.id);
    } else if b.as_deref() != Some(&active.id) {
        *b = None;
        drop(b);
        on_launch();
    }
}

/// Clears baseline and fires on_launch if a baseline was set.
fn close_if_baseline_set(baseline: &Rc<RefCell<Option<String>>>, on_launch: &Rc<dyn Fn()>) {
    let mut b = baseline.borrow_mut();
    if b.is_some() {
        *b = None;
        drop(b);
        on_launch();
    }
}

/// What to do with the window for a given command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowOp {
    Show,
    Hide,
    Close,
}

/// Pure decision function: determines the window operation for a command.
/// Testable without GTK objects.
fn resolve_window_op(cmd: &WindowCommand, visible: bool, resident: bool) -> WindowOp {
    match cmd {
        WindowCommand::Show => WindowOp::Show,
        WindowCommand::Hide => {
            if resident {
                WindowOp::Hide
            } else {
                WindowOp::Close
            }
        }
        WindowCommand::Toggle => {
            if visible {
                if resident {
                    WindowOp::Hide
                } else {
                    WindowOp::Close
                }
            } else {
                WindowOp::Show
            }
        }
        WindowCommand::Quit => WindowOp::Close,
    }
}

/// Processes a single window command from the signal handler.
fn handle_window_command(
    win: &gtk4::ApplicationWindow,
    search_entry: &gtk4::SearchEntry,
    well_ctx: &crate::ui::well_context::WellContext,
    focus_pending: &Rc<Cell<bool>>,
    cmd: WindowCommand,
    resident: bool,
) {
    match resolve_window_op(&cmd, win.is_visible(), resident) {
        WindowOp::Show => {
            // Clear search/category state so the drawer opens fresh.
            // Don't rebuild yet — that happens in complete_show after focus.
            clear_drawer_state(search_entry);
            focus_pending.set(true);
            win.set_visible(true);
            // Fallback: if is_active_notify doesn't fire within 200ms
            // (e.g. --keyboard-on-demand mode), grab focus anyway.
            let entry = search_entry.clone();
            let ctx = well_ctx.clone();
            let pending = Rc::clone(focus_pending);
            glib::timeout_add_local_once(std::time::Duration::from_millis(200), move || {
                if pending.get() {
                    pending.set(false);
                    complete_show(&entry, &ctx);
                }
            });
        }
        WindowOp::Hide => win.set_visible(false),
        WindowOp::Close => quit_or_hide(win, false),
    }
}

/// Called when focus is confirmed (via is_active_notify or timeout fallback).
/// Grabs focus on the search entry and rebuilds the well if needed.
fn complete_show(
    search_entry: &gtk4::SearchEntry,
    well_ctx: &crate::ui::well_context::WellContext,
) {
    search_entry.grab_focus();
    // Rebuild after focus so the widget tree is stable
    let had_category = !well_ctx.state.borrow().active_category.is_empty();
    if had_category {
        well_ctx.state.borrow_mut().active_category.clear();
        well_builder::rebuild_preserving_category(well_ctx);
    }
}

/// Clears search text before showing. Category clearing and rebuild are
/// deferred to `complete_show` after focus is confirmed.
fn clear_drawer_state(search_entry: &gtk4::SearchEntry) {
    search_entry.set_text("");
}

/// Quits the application (non-resident) or hides the window (resident).
/// Public so main.rs close paths can use the same logic.
pub fn quit_or_hide(win: &gtk4::ApplicationWindow, resident: bool) {
    if resident {
        win.set_visible(false);
    } else if let Some(app) = win.application() {
        app.quit();
    } else {
        win.close();
    }
}

/// Handles Escape key: clear search, or close/hide drawer.
fn handle_escape(search_entry: &gtk4::SearchEntry, win: &gtk4::ApplicationWindow, resident: bool) {
    let text = search_entry.text();
    if !text.is_empty() {
        search_entry.set_text("");
    } else {
        quit_or_hide(win, resident);
    }
}

/// Handles Return key: execute `:command` (only when search has focus).
/// Math evaluation is handled inline by build_search_results.
fn handle_return(
    search_entry: &gtk4::SearchEntry,
    compositor: &dyn Compositor,
    on_launch: &Rc<dyn Fn()>,
) {
    if !search_entry.has_focus() {
        return;
    }
    let text = search_entry.text().to_string();
    if text.starts_with(':') && text.len() > 1 {
        let cmd = &text[1..];
        nwg_common::launch::launch_via_compositor(cmd, compositor);
        on_launch();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resident_toggle_hides() {
        assert_eq!(
            resolve_window_op(&WindowCommand::Toggle, true, true),
            WindowOp::Hide
        );
    }

    #[test]
    fn resident_toggle_shows() {
        assert_eq!(
            resolve_window_op(&WindowCommand::Toggle, false, true),
            WindowOp::Show
        );
    }

    #[test]
    fn non_resident_toggle_closes() {
        assert_eq!(
            resolve_window_op(&WindowCommand::Toggle, true, false),
            WindowOp::Close
        );
    }

    #[test]
    fn non_resident_hide_closes() {
        assert_eq!(
            resolve_window_op(&WindowCommand::Hide, true, false),
            WindowOp::Close
        );
    }

    #[test]
    fn resident_hide_hides() {
        assert_eq!(
            resolve_window_op(&WindowCommand::Hide, true, true),
            WindowOp::Hide
        );
    }

    #[test]
    fn show_always_shows() {
        assert_eq!(
            resolve_window_op(&WindowCommand::Show, false, false),
            WindowOp::Show
        );
        assert_eq!(
            resolve_window_op(&WindowCommand::Show, false, true),
            WindowOp::Show
        );
    }

    #[test]
    fn quit_always_closes() {
        assert_eq!(
            resolve_window_op(&WindowCommand::Quit, true, true),
            WindowOp::Close
        );
        assert_eq!(
            resolve_window_op(&WindowCommand::Quit, true, false),
            WindowOp::Close
        );
    }
}
