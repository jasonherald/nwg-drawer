//! Pure-decision window-command state machine, separated from listener wiring.
//!
//! The signal poller in `super` receives `WindowCommand`s from the
//! RT-signal channel and dispatches them via [`handle_window_command`].
//! The decision of "show vs hide vs close" is factored into the pure
//! [`resolve_window_op`] function so the matrix can be unit-tested
//! without GTK objects — the existing test suite covers all
//! WindowCommand × visible × resident combinations.

use crate::ui::well_context::WellContext;
use gtk4::glib;
use gtk4::prelude::*;
use nwg_common::signals::WindowCommand;
use std::cell::Cell;
use std::rc::Rc;

/// Fallback delay before forcing focus on the search entry after a `Show`
/// command, used only when the compositor never delivers `is_active_notify`
/// (e.g. `--keyboard-on-demand` mode where the drawer never receives keyboard
/// focus from the compositor).
const FOCUS_FALLBACK_DELAY_MS: u64 = 200;

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
pub(super) fn handle_window_command(
    win: &gtk4::ApplicationWindow,
    search_entry: &gtk4::SearchEntry,
    well_ctx: &WellContext,
    focus_pending: &Rc<Cell<bool>>,
    cmd: WindowCommand,
    resident: bool,
) {
    match resolve_window_op(&cmd, win.is_visible(), resident) {
        WindowOp::Show => {
            // Clear search text so the drawer opens fresh; category
            // clearing and rebuild are deferred to `complete_show`
            // after focus is confirmed.
            search_entry.set_text("");
            focus_pending.set(true);
            win.set_visible(true);
            // Fallback: if is_active_notify doesn't fire in time
            // (e.g. --keyboard-on-demand mode), grab focus anyway.
            let entry = search_entry.clone();
            let ctx = well_ctx.clone();
            let pending = Rc::clone(focus_pending);
            glib::timeout_add_local_once(
                std::time::Duration::from_millis(FOCUS_FALLBACK_DELAY_MS),
                move || {
                    if pending.get() {
                        pending.set(false);
                        super::complete_show(&entry, &ctx);
                    }
                },
            );
        }
        // A Hide / Close arriving inside the fallback window must disarm
        // the pending flag, otherwise the still-scheduled timeout will
        // run `complete_show` against a now-hidden window.
        WindowOp::Hide => {
            focus_pending.set(false);
            win.set_visible(false);
        }
        WindowOp::Close => {
            focus_pending.set(false);
            super::quit_or_hide(win, false);
        }
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
