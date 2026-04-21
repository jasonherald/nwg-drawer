use crate::config::DrawerConfig;
use gtk4_layer_shell::LayerShell;

/// Configures the drawer as a full-screen layer-shell overlay.
pub fn setup_drawer_window(
    win: &gtk4::ApplicationWindow,
    config: &DrawerConfig,
    monitor: Option<&gtk4::gdk::Monitor>,
) {
    win.init_layer_shell();
    win.set_namespace(Some("nwg-drawer"));

    if let Some(mon) = monitor {
        win.set_monitor(Some(mon));
    }

    // Full-screen anchoring
    win.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
    win.set_anchor(gtk4_layer_shell::Edge::Top, true);
    win.set_anchor(gtk4_layer_shell::Edge::Left, true);
    win.set_anchor(gtk4_layer_shell::Edge::Right, true);

    // Layer
    if config.overlay {
        win.set_layer(gtk4_layer_shell::Layer::Overlay);
        win.set_exclusive_zone(-1);
    } else {
        win.set_layer(gtk4_layer_shell::Layer::Top);
    }

    // Margins
    win.set_margin(gtk4_layer_shell::Edge::Top, config.mt);
    win.set_margin(gtk4_layer_shell::Edge::Left, config.ml);
    win.set_margin(gtk4_layer_shell::Edge::Right, config.mr);
    win.set_margin(gtk4_layer_shell::Edge::Bottom, config.mb);

    // Keyboard interactivity
    if config.keyboard_on_demand {
        log::info!("Setting keyboard mode to: on-demand");
        win.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::OnDemand);
    } else {
        win.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::Exclusive);
    }
}
