//! Coordinator. Parses args, resolves dirs, hands off to lifecycle and
//! activate-time wiring, and runs the GTK main loop.

mod activate;
mod config;
mod desktop_loader;
mod lifecycle;
mod listeners;
mod power_bar_detect;
mod state;
mod ui;
mod watcher;
mod xdg_dirs;

use crate::config::DrawerConfig;
use clap::Parser;
use gtk4::prelude::*;
use nwg_common::config::paths;
use nwg_common::desktop::dirs::get_app_dirs;
use nwg_common::signals;
use std::cell::Cell;
use std::rc::Rc;

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

    // Lifecycle gates first — `--open` / `--close` and finding an existing
    // instance both exit before reaching pb_auto, so doing autodetect first
    // would waste PATH probes and a `systemctl can-suspend` syscall on the
    // hot exit path.
    lifecycle::handle_open_close(&config);
    lifecycle::handle_existing_instance(&config);
    let _lock = lifecycle::acquire_singleton_lock();

    if config.pb_auto {
        power_bar_detect::auto_detect_power_bar(&mut config);
    }

    let compositor: Rc<dyn nwg_common::compositor::Compositor> =
        Rc::from(nwg_common::compositor::init_or_null(config.wm));

    // RT-signal handler returns a sync `mpsc::Receiver`. Bridge it to an
    // `async_channel` so the listener can `recv().await` on the glib main
    // loop instead of polling. The forwarding thread exits when the
    // glib-side receiver drops at process shutdown.
    let signal_mpsc = signals::setup_signal_handlers(config.resident);
    let (sig_tx, sig_rx) = async_channel::unbounded::<nwg_common::signals::WindowCommand>();
    std::thread::spawn(move || {
        while let Ok(cmd) = signal_mpsc.recv() {
            if sig_tx.send_blocking(cmd).is_err() {
                break;
            }
        }
    });
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

    let init = Rc::new(activate::DrawerInit {
        config: Rc::new(config),
        css_path: Rc::new(css_path),
        // `Rc::from(PathBuf)` produces an `Rc<Path>` directly — no
        // double indirection through `Rc<PathBuf>`.
        pinned_file: Rc::from(pinned_file),
        // No `Rc` wrap: `data_home` is read once per activate via
        // `as_deref()`, never cloned into closures.
        data_home,
        app_dirs,
        exclusions,
        compositor,
        sig_rx,
    });

    // Guard against repeated activation. Our singleton lock plus GTK's D-Bus
    // primary-instance registration normally hold this to one fire per
    // process, but a race where the lock file is lost between acquire and
    // gtk init could land a remote-activate from a second process. Building
    // a second window stack here would duplicate listeners, double-consume
    // `sig_rx`, and leave the original drawer in a broken state. Easier to
    // refuse the second activate.
    let activated = Cell::new(false);
    app.connect_activate(move |app| {
        if activated.replace(true) {
            log::warn!("connect_activate fired twice; ignoring duplicate");
            return;
        }
        activate::activate_drawer(app, &init);
    });

    app.run_with_args::<String>(&[]);
}
