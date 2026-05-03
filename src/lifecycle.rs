//! Drawer process lifecycle: open/close-flag dispatch, existing-instance
//! handling, and singleton-lock acquisition.
//!
//! Runs before GTK init in `main()`. Each function may exit the process
//! before any UI work happens — `--open` / `--close` always exit, and
//! finding a running resident instance also exits without starting a
//! second one.

use crate::config::DrawerConfig;
use nwg_common::signals;
use nwg_common::singleton;

/// Handles `--open` / `--close` flags by sending a signal to the running
/// instance, then exits. No-op if neither flag is set.
pub(crate) fn handle_open_close(config: &DrawerConfig) {
    if !config.open && !config.close {
        return;
    }
    if let Some(pid) = singleton::find_running_pid("mac-drawer") {
        let sig = if config.open {
            signals::sig_show()
        } else {
            signals::sig_hide()
        };
        let action = if config.open { "show" } else { "hide" };
        if signals::send_signal_to_pid(pid, sig) {
            log::info!("Sent {} signal to running instance (pid {})", action, pid);
        } else {
            log::error!(
                "Failed to send {} signal to running instance (pid {}) — signal call returned false (stale PID, permissions, etc.)",
                action,
                pid
            );
        }
    } else {
        log::warn!("No running drawer instance found");
    }
    std::process::exit(0);
}

/// Checks for an existing running instance and handles it BEFORE acquiring the lock.
/// This avoids the race where the lock is released by the dying instance before
/// we check it, causing us to start a full new instance unintentionally.
pub(crate) fn handle_existing_instance(config: &DrawerConfig) {
    let Some(pid) = singleton::find_running_pid("mac-drawer") else {
        return; // No existing instance — proceed to start
    };

    if config.resident {
        // Resident invocation finding existing instance → warn and exit
        // Use eprintln so it's always visible (not gated by RUST_LOG)
        eprintln!("Resident instance already running (pid {})", pid);
        std::process::exit(0);
    }

    // Non-resident invocation finding existing instance → toggle and exit
    if signals::send_signal_to_pid(pid, signals::sig_toggle()) {
        log::info!("Sent toggle signal to existing instance (pid {})", pid);
        std::process::exit(0);
    }
    // Signal failed (stale PID) — fall through to start a fresh instance
    log::warn!(
        "Failed to signal existing instance (pid {}), starting fresh",
        pid
    );
}

/// Acquires the singleton lock. If another instance holds it, exit.
/// Instance signaling is handled by `handle_existing_instance` before this.
pub(crate) fn acquire_singleton_lock() -> singleton::LockFile {
    match singleton::acquire_lock("mac-drawer") {
        Ok(lock) => lock,
        Err(Some(pid)) => {
            log::warn!("Another instance is running (pid {})", pid);
            std::process::exit(0);
        }
        Err(None) => {
            log::error!("Failed to acquire singleton lock");
            std::process::exit(1);
        }
    }
}
