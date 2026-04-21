mod config;
mod desktop_loader;
mod listeners;
mod state;
mod ui;
mod watcher;

use crate::config::DrawerConfig;
use crate::state::DrawerState;
use clap::Parser;
use gtk4::prelude::*;
use nwg_common::config::paths;
use nwg_common::desktop::dirs::get_app_dirs;
use nwg_common::pinning;
use nwg_common::signals;
use nwg_common::singleton;
use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

/// Mac-style drawer CSS, embedded at compile time.
const DRAWER_CSS: &str = include_str!("assets/drawer.css");

fn main() {
    nwg_common::process::handle_dump_args();
    let mut config = DrawerConfig::parse_from(config::normalize_legacy_flags(std::env::args()));

    if config.debug {
        env_logger::Builder::from_default_env()
            .filter_level(log::LevelFilter::Debug)
            .init();
    } else {
        env_logger::init();
    }

    if config.pb_auto {
        auto_detect_power_bar(&mut config);
    }

    handle_open_close(&config);
    handle_existing_instance(&config);
    let _lock = acquire_singleton_lock();
    let compositor: Rc<dyn nwg_common::compositor::Compositor> =
        Rc::from(nwg_common::compositor::init_or_null(config.wm));

    let sig_rx = Rc::new(signals::setup_signal_handlers(config.resident));
    let config_dir = paths::config_dir("nwg-drawer");
    if let Err(e) = paths::ensure_dir(&config_dir) {
        log::warn!("Failed to create config dir: {}", e);
    }

    let cache_dir = paths::cache_dir().expect("Couldn't determine cache directory");
    let pinned_file = cache_dir.join("mac-dock-pinned");
    let css_path = if config.css_file.starts_with('/') {
        std::path::PathBuf::from(&config.css_file)
    } else {
        config_dir.join(&config.css_file)
    };

    if !css_path.exists()
        && let Some(data_dir) = paths::find_data_home("nwg-drawer")
    {
        let src = data_dir.join("nwg-drawer/drawer.css");
        if let Err(e) = paths::copy_file(&src, &css_path) {
            log::warn!("Failed copying default CSS: {}", e);
        }
    }

    let app_dirs = get_app_dirs();
    let exclusions = paths::load_text_lines(&config_dir.join("excluded-dirs")).unwrap_or_default();
    let data_home = paths::find_data_home("nwg-drawer");

    let app = gtk4::Application::builder()
        .application_id("com.mac-drawer.hyprland")
        .build();

    let config = Rc::new(config);
    let pinned_file = Rc::new(pinned_file);
    let css_path = Rc::new(css_path);
    let data_home = Rc::new(data_home);

    app.connect_activate(move |app| {
        activate_drawer(
            app,
            &css_path,
            &config,
            &app_dirs,
            &compositor,
            &pinned_file,
            &exclusions,
            &data_home,
            &sig_rx,
        );
    });

    app.run_with_args::<String>(&[]);
}

/// Sets up the drawer UI: CSS, state, window, layout, search, and listeners.
#[allow(clippy::too_many_arguments)]
fn activate_drawer(
    app: &gtk4::Application,
    css_path: &Rc<std::path::PathBuf>,
    config: &Rc<DrawerConfig>,
    app_dirs: &[std::path::PathBuf],
    compositor: &Rc<dyn nwg_common::compositor::Compositor>,
    pinned_file: &Rc<std::path::PathBuf>,
    exclusions: &[String],
    data_home: &Rc<Option<std::path::PathBuf>>,
    sig_rx: &Rc<std::sync::mpsc::Receiver<nwg_common::signals::WindowCommand>>,
) {
    let config = Rc::clone(config);
    let pinned_file = Rc::clone(pinned_file);

    // CSS (with hot-reload for user CSS file)
    let user_provider = nwg_common::config::css::load_css(css_path);
    nwg_common::config::css::watch_css(css_path, &user_provider);
    nwg_common::config::css::load_css_from_data(DRAWER_CSS);

    // Apply user-configurable opacity (overrides the default in embedded CSS)
    let opacity = config.opacity.min(100) as f64 / 100.0;
    let opacity_css = format!(
        "window {{ background-color: rgba(22, 22, 30, {:.2}); }}",
        opacity
    );
    nwg_common::config::css::load_css_override(&opacity_css);

    apply_theme_settings(&config);

    // State
    let state = Rc::new(RefCell::new(DrawerState::new(
        app_dirs.to_vec(),
        Rc::clone(compositor),
    )));
    state.borrow_mut().exclusions = exclusions.to_vec();
    desktop_loader::load_desktop_entries(&mut state.borrow_mut());
    load_preferred_apps(&mut state.borrow_mut());
    state.borrow_mut().pinned = pinning::load_pinned(&pinned_file);

    apply_force_theme(&config, &state);

    // Window
    let win = gtk4::ApplicationWindow::new(app);
    let target_monitor = resolve_target_monitor(&config, compositor);
    ui::window::setup_drawer_window(&win, &config, target_monitor.as_ref());

    // Layout
    let main_vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    win.set_child(Some(&main_vbox));

    setup_close_button(&main_vbox, &win, &config);

    let search_entry = ui::search::setup_search_entry();
    search_entry.add_css_class("drawer-search");
    search_entry.set_hexpand(false);
    search_entry.set_halign(gtk4::Align::Center);
    search_entry.set_width_request(ui::constants::SEARCH_ENTRY_WIDTH);
    search_entry.set_margin_top(ui::constants::SEARCH_TOP_MARGIN);
    main_vbox.append(&search_entry);

    let status_label = gtk4::Label::new(None);
    status_label.add_css_class("status-label");

    // On-launch callback
    let on_launch: Rc<dyn Fn()> = {
        let win = win.clone();
        let config = Rc::clone(&config);
        let search_entry = search_entry.clone();
        Rc::new(move || {
            if config.resident {
                search_entry.set_text("");
            }
            listeners::quit_or_hide(&win, config.resident);
        })
    };

    // Multi-monitor click-catcher backdrops. The drawer is a layer-shell
    // surface pinned to a single output, so clicks on other monitors
    // never reach it and the existing focus-loss + active-window-poll
    // close paths sometimes miss them (issue #55). One backdrop per
    // *other* monitor — the drawer's own monitor is excluded so the
    // backdrop doesn't race the drawer for click delivery there.
    //
    // Only create backdrops in OnDemand keyboard mode. In Exclusive
    // mode (the default) Hyprland drops pointer events to other
    // layer-shell surfaces regardless of opacity — visible backdrops
    // that never receive clicks would be worse UX than no backdrops
    // at all (a dimmed desktop with no way to dismiss it).
    if config.keyboard_on_demand {
        let drawer_monitor_name = drawer_monitor_connector(&config, compositor, &target_monitor);
        let backdrops = nwg_common::layer_shell::create_fullscreen_backdrops(
            app,
            "nwg-drawer-backdrop",
            "drawer-backdrop",
            drawer_monitor_name.as_deref(),
        );
        for backdrop in &backdrops {
            let click = gtk4::GestureClick::new();
            let on_launch_bd = Rc::clone(&on_launch);
            click.connect_released(move |gesture, _, _, _| {
                gesture.set_state(gtk4::EventSequenceState::Claimed);
                on_launch_bd();
            });
            backdrop.add_controller(click);
            backdrop.present();
            backdrop.set_visible(false);
        }
        // Sync backdrop visibility with the drawer window. Using
        // `connect_visible_notify` means every code path that toggles
        // the drawer's visibility (signal poller, focus-loss, escape,
        // close button, right-click on background) automatically
        // toggles the backdrops too — no need to thread the Vec
        // through every site.
        win.connect_visible_notify(move |w| {
            let visible = w.is_visible();
            for b in &backdrops {
                b.set_visible(visible);
            }
        });
    }

    // Pinned items (above scroll, fixed — never scrolls out of view)
    let pinned_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    pinned_box.set_halign(gtk4::Align::Center);
    pinned_box.set_margin_top(4);

    // Scrolled window (only app grid scrolls)
    let scrolled = gtk4::ScrolledWindow::new();
    scrolled.set_vexpand(true);
    scrolled.set_hexpand(true);

    // Right-click on scrolled area → close drawer
    let right_click = gtk4::GestureClick::new();
    right_click.set_button(3);
    // Bubble phase so child button gestures (pin toggle) fire first
    right_click.set_propagation_phase(gtk4::PropagationPhase::Bubble);
    let win_rc = win.clone();
    let config_rc = Rc::clone(&config);
    right_click.connect_released(move |gesture, _, _, _| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        listeners::quit_or_hide(&win_rc, config_rc.resident);
    });
    scrolled.add_controller(right_click);
    // Allow focus to pass through scrolled window to app grid buttons
    scrolled.set_focus_on_click(false);

    // App grid well (inside scrolled window)
    let well = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    well.add_css_class("section-well");
    well.set_hexpand(true);
    well.set_margin_start(ui::constants::WELL_SIDE_MARGIN);
    well.set_margin_end(ui::constants::WELL_SIDE_MARGIN);
    well.set_margin_top(ui::constants::CONTENT_TOP_MARGIN);
    scrolled.set_child(Some(&well));

    // Shared context for well/category/search builders
    let well_ctx = ui::well_context::WellContext {
        well: well.clone(),
        pinned_box: pinned_box.clone(),
        config: Rc::clone(&config),
        state: Rc::clone(&state),
        pinned_file: Rc::clone(&pinned_file),
        on_launch: Rc::clone(&on_launch),
        status_label: status_label.clone(),
        search_entry: search_entry.clone(),
    };

    // Categories (above scroll, fixed)
    if !config.no_cats {
        let cat_bar = ui::categories::build_category_bar(&well_ctx);
        main_vbox.append(&cat_bar);
    }

    // Add pinned + scrolled to layout (after categories)
    main_vbox.append(&pinned_box);
    main_vbox.append(&scrolled);

    // Build initial content
    ui::well_builder::build_normal_well(&well_ctx);

    // Search
    ui::search_handler::connect_search(&well_ctx);

    // Power bar + status
    if config.has_power_bar() {
        main_vbox.append(&ui::power_bar::build_power_bar(
            &config,
            Rc::clone(&on_launch),
            data_home.as_deref(),
            &status_label,
        ));
    }
    main_vbox.append(&status_label);

    // Shared flag: set when the drawer is shown, cleared when focus is confirmed
    let focus_pending = Rc::new(Cell::new(false));

    // Listeners
    listeners::setup_keyboard(&win, &search_entry, &config, &on_launch, compositor);
    listeners::setup_focus_detector(
        &win,
        &search_entry,
        &well_ctx,
        &focus_pending,
        &on_launch,
        compositor,
    );
    listeners::setup_file_watcher(app_dirs, &well_ctx);
    listeners::setup_signal_poller(
        &win,
        &search_entry,
        &well_ctx,
        &focus_pending,
        sig_rx,
        config.resident,
    );

    // In resident mode, start hidden — the signal poller will show the window
    // when a SIGRTMIN+1 (toggle) or SIGRTMIN+2 (show) signal is received.
    if config.resident {
        win.present();
        win.set_visible(false);
    } else {
        win.present();
    }
}

/// Applies force-theme for libadwaita apps (ignored under uwsm).
fn apply_force_theme(config: &DrawerConfig, state: &Rc<RefCell<state::DrawerState>>) {
    if !config.force_theme {
        return;
    }
    if config.wm == Some(nwg_common::compositor::WmOverride::Uwsm) {
        log::warn!("--force-theme ignored when running through uwsm");
        return;
    }
    if let Some(settings) = gtk4::Settings::default() {
        let theme = settings
            .gtk_theme_name()
            .map(|s| s.to_string())
            .unwrap_or_default();
        if !theme.is_empty() {
            state.borrow_mut().gtk_theme_prefix = format!("GTK_THEME={}", theme);
            log::info!("Force theme enabled: GTK_THEME={}", theme);
        }
    }
}

/// Applies GTK theme and icon theme settings from the config.
fn apply_theme_settings(config: &DrawerConfig) {
    if let Some(settings) = gtk4::Settings::default() {
        if !config.gtk_theme.is_empty() {
            settings.set_gtk_theme_name(Some(&config.gtk_theme));
            log::info!("Using theme: {}", config.gtk_theme);
        } else {
            settings.set_property("gtk-application-prefer-dark-theme", true);
        }
        if !config.icon_theme.is_empty() {
            settings.set_gtk_icon_theme_name(Some(&config.icon_theme));
            log::info!("Using icon theme: {}", config.icon_theme);
        }
    }
}

fn load_preferred_apps(state: &mut DrawerState) {
    let pa_file = paths::config_dir("nwg-drawer").join("preferred-apps.json");
    if pa_file.exists()
        && let Some(pa) = nwg_common::desktop::preferred_apps::load_preferred_apps(&pa_file)
    {
        log::info!("Found {} custom file associations", pa.len());
        state.preferred_apps = pa;
    }
}

/// Returns the connector name (e.g. "DP-1") of the monitor the drawer
/// will appear on, used to exclude that monitor from the click-catcher
/// backdrop set.
///
/// Resolution order:
/// 1. The GDK monitor returned by `resolve_target_monitor` (set when
///    the user passed `--output`).
/// 2. The compositor's currently-focused monitor — this is where
///    Hyprland places a layer-shell surface that didn't pin an output.
///
/// Returns `None` if neither resolves; callers cover all monitors and
/// accept that the drawer's monitor will get a no-op backdrop racing
/// with its own surface (the existing right-click-to-close fallback
/// still works on the drawer itself).
fn drawer_monitor_connector(
    config: &DrawerConfig,
    compositor: &Rc<dyn nwg_common::compositor::Compositor>,
    target_monitor: &Option<gtk4::gdk::Monitor>,
) -> Option<String> {
    // Let-guard so the target_monitor branch only returns when a connector
    // name is actually available. On backends where GDK omits connector
    // metadata we fall through to the `--output` flag and then to the
    // compositor's focused-monitor answer, rather than giving up and
    // creating a backdrop that races the drawer for clicks on the same
    // output (CodeRabbit catch on #71).
    if let Some(mon) = target_monitor
        && let Some(connector) = mon.connector().map(|c| c.to_string())
    {
        return Some(connector);
    }
    if !config.output.is_empty() {
        return Some(config.output.clone());
    }
    compositor
        .list_monitors()
        .map_err(|e| log::debug!("list_monitors failed while resolving drawer monitor: {e}"))
        .ok()?
        .into_iter()
        .find(|m| m.focused)
        .map(|m| m.name)
}

fn resolve_target_monitor(
    config: &DrawerConfig,
    compositor: &Rc<dyn nwg_common::compositor::Compositor>,
) -> Option<gtk4::gdk::Monitor> {
    if config.output.is_empty() {
        return None;
    }
    let display = gtk4::gdk::Display::default()?;
    let monitors = display.monitors();
    let wm_monitors = compositor.list_monitors().ok()?;

    for (i, wm) in wm_monitors.iter().enumerate() {
        if wm.name == config.output
            && let Some(item) = monitors.item(i as u32)
            && let Ok(mon) = item.downcast::<gtk4::gdk::Monitor>()
        {
            return Some(mon);
        }
    }
    log::warn!("Target output '{}' not found", config.output);
    None
}

fn setup_close_button(main_vbox: &gtk4::Box, win: &gtk4::ApplicationWindow, config: &DrawerConfig) {
    use crate::config::CloseButton;

    let align = match config.closebtn {
        CloseButton::None => return,
        CloseButton::Left => gtk4::Align::Start,
        CloseButton::Right => gtk4::Align::End,
    };

    let close_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    let close_btn = gtk4::Button::from_icon_name("window-close-symbolic");
    close_btn.add_css_class("flat");
    close_btn.set_widget_name("close-button");

    let win = win.clone();
    let resident = config.resident;
    close_btn.connect_clicked(move |_| {
        listeners::quit_or_hide(&win, resident);
    });

    close_box.set_halign(align);
    close_box.append(&close_btn);
    main_vbox.append(&close_box);
}

/// Handles --open/--close flags by sending signal to running instance.
fn handle_open_close(config: &DrawerConfig) {
    if !config.open && !config.close {
        return;
    }
    if let Some(pid) = singleton::find_running_pid("mac-drawer") {
        let sig = if config.open {
            signals::sig_show()
        } else {
            signals::sig_hide()
        };
        signals::send_signal_to_pid(pid, sig);
        log::info!(
            "Sent {} signal to running instance (pid {})",
            if config.open { "show" } else { "hide" },
            pid
        );
    } else {
        log::warn!("No running drawer instance found");
    }
    std::process::exit(0);
}

/// Checks for an existing running instance and handles it BEFORE acquiring the lock.
/// This avoids the race where the lock is released by the dying instance before
/// we check it, causing us to start a full new instance unintentionally.
fn handle_existing_instance(config: &DrawerConfig) {
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
/// Instance signaling is handled by handle_existing_instance() before this.
fn acquire_singleton_lock() -> singleton::LockFile {
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

/// Auto-detects power bar commands from system capabilities.
///
/// Only fills in empty slots — explicit --pb-* flags take priority.
fn auto_detect_power_bar(config: &mut DrawerConfig) {
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
