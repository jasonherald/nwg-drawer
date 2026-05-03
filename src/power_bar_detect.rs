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
    if !slot.is_empty() {
        return;
    }
    for cmd in &["hyprlock", "swaylock", "swaylock-effects"] {
        if command_on_path(cmd) {
            *slot = cmd.to_string();
            log::info!("  Lock: {}", cmd);
            return;
        }
    }
}

/// Tries each candidate command (checks the first word on PATH) and uses the first found.
fn detect_command(slot: &mut String, label: &str, candidates: &[&str]) {
    if !slot.is_empty() {
        return;
    }
    for cmd in candidates {
        let bin = cmd.split_whitespace().next().unwrap_or("");
        if command_on_path(bin) {
            *slot = cmd.to_string();
            log::info!("  {}: {}", label, cmd);
            return;
        }
    }
}

/// Checks if a command exists on PATH.
fn command_on_path(cmd: &str) -> bool {
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':') {
            let full = std::path::Path::new(dir).join(cmd);
            if full.is_file() {
                return true;
            }
        }
    }
    false
}

/// Checks if the system supports suspend via systemctl.
fn can_suspend() -> bool {
    std::process::Command::new("systemctl")
        .arg("can-suspend")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "yes")
        .unwrap_or(false)
}
