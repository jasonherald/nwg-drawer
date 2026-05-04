//! Auto-detection of power-bar command slots from system capabilities.
//!
//! Driven by the `--pb-auto` CLI flag. Each detector only fills its slot
//! when the corresponding `--pb-*` flag was *not* explicitly set, so user
//! overrides always win. See `auto_detect_power_bar` for the full ordering
//! contract pinned by issue #20 of the action plan.

use crate::config::DrawerConfig;

/// Auto-detects power bar commands from system capabilities.
///
/// Only fills in empty slots — explicit `--pb-*` flags take priority.
pub(crate) fn auto_detect_power_bar(config: &mut DrawerConfig) {
    log::info!("Auto-detecting power bar buttons...");

    detect_lock(&mut config.pb_lock);
    detect_command(&mut config.pb_exit, "Exit", &["loginctl terminate-session"]);
    detect_command(
        &mut config.pb_poweroff,
        "Poweroff",
        &["loginctl poweroff", "systemctl poweroff"],
    );
    detect_command(
        &mut config.pb_reboot,
        "Reboot",
        &["loginctl reboot", "systemctl reboot"],
    );

    // Suspend — only if the system actually supports it
    if config.pb_sleep.is_empty() && can_suspend() {
        detect_command(&mut config.pb_sleep, "Suspend", &["systemctl suspend"]);
    }
}

/// Tries each candidate lock command and uses the first found on PATH.
fn detect_lock(slot: &mut String) {
    let was_empty = slot.is_empty();
    pick_first_present(slot, &["hyprlock", "swaylock", "swaylock-effects"], |cmd| {
        command_on_path(cmd)
    });
    if was_empty && !slot.is_empty() {
        log::info!("  Lock: {}", slot);
    }
}

/// Tries each candidate command (checks the first word on PATH) and uses the first found.
fn detect_command(slot: &mut String, label: &str, candidates: &[&str]) {
    let was_empty = slot.is_empty();
    pick_first_present(slot, candidates, |cmd| {
        let bin = cmd.split_whitespace().next().unwrap_or("");
        command_on_path(bin)
    });
    if was_empty && !slot.is_empty() {
        log::info!("  {}: {}", label, slot);
    }
}

/// Fills `slot` with the first `candidate` whose `probe` returns
/// `true`. If `slot` is already non-empty, it is left untouched —
/// this is the "explicit user value always wins" rule pinned by the
/// `--pb-auto` README contract.
///
/// Pure except for the probe; tests inject a synthetic probe to
/// exercise the priority matrix without depending on real `PATH`
/// state. The production probe is [`command_on_path`].
fn pick_first_present(slot: &mut String, candidates: &[&str], mut probe: impl FnMut(&str) -> bool) {
    if !slot.is_empty() {
        return;
    }
    for &candidate in candidates {
        if probe(candidate) {
            *slot = candidate.to_string();
            return;
        }
    }
}

/// Checks if a command exists on PATH.
///
/// Uses `std::env::split_paths` so empty segments (`PATH="…::…"`) and
/// non-UTF-8 path entries are handled losslessly per platform conventions.
fn command_on_path(cmd: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(cmd).is_file())
}

/// Checks if the system supports suspend via systemctl.
fn can_suspend() -> bool {
    std::process::Command::new("systemctl")
        .arg("can-suspend")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "yes")
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_first_present_preserves_prefilled_slot() {
        // Explicit user value must never be clobbered. Pass a probe
        // that panics if called — the test fails not just on a
        // changed slot but on any unnecessary probe call (e.g. a
        // future short-circuit regression that walks the candidates
        // before checking is_empty).
        let mut slot = "user-set-cmd".to_string();
        pick_first_present(&mut slot, &["a", "b"], |_| {
            panic!("probe must not be called when slot is prefilled")
        });
        assert_eq!(slot, "user-set-cmd");
    }

    #[test]
    fn pick_first_present_first_match_wins() {
        // Probe rejects `a`, accepts `b` and `c` — the first acceptable
        // candidate (`b`) lands in the slot.
        let mut slot = String::new();
        pick_first_present(&mut slot, &["a", "b", "c"], |cmd| cmd != "a");
        assert_eq!(slot, "b");
    }

    #[test]
    fn pick_first_present_no_match_leaves_slot_empty() {
        // Probe rejects everything → slot stays empty (caller's `if
        // slot.is_empty()` guard then skips logging).
        let mut slot = String::new();
        pick_first_present(&mut slot, &["a", "b"], |_| false);
        assert!(slot.is_empty());
    }

    #[test]
    fn pick_first_present_empty_candidates_leaves_slot_empty() {
        // Defensive: empty slate (e.g. all candidates filtered out
        // upstream) must not panic and must not fill the slot.
        let mut slot = String::new();
        pick_first_present(&mut slot, &[], |_| true);
        assert!(slot.is_empty());
    }
}
