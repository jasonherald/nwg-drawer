use clap::{Parser, ValueEnum};

/// Known Go-style single-dash flags for the drawer binary.
/// Single-character flags (-s, -o, -g, -i, -c, -r, -d, -k, -v) work natively.
const LEGACY_FLAGS: &[&str] = &[
    "is",
    "ovl",
    "open",
    "close",
    "mt",
    "ml",
    "mr",
    "mb",
    "fscol",
    "ft",
    "fm",
    "spacing",
    "lang",
    "term",
    "wm",
    "fslen",
    "nocats",
    "nofs",
    "pbexit",
    "pblock",
    "pbpoweroff",
    "pbreboot",
    "pbsleep",
    "pbsize",
    "pbuseicontheme",
    "pbauto",
    "closebtn",
    "opacity",
    "pi",
];

/// Converts Go-style single-dash flags to clap-compatible double-dash flags.
pub fn normalize_legacy_flags(args: impl Iterator<Item = String>) -> Vec<String> {
    nwg_common::config::flags::normalize_legacy_flags(args, LEGACY_FLAGS)
}

/// A macOS-style application drawer/launcher for Hyprland/Sway.
#[derive(Parser, Debug, Clone)]
#[command(name = "nwg-drawer", version, about)]
pub struct DrawerConfig {
    /// CSS file name
    #[arg(short = 's', long, default_value = "drawer.css")]
    pub css_file: String,

    /// Name of output to display on
    #[arg(short = 'o', long, default_value = "")]
    pub output: String,

    /// Use overlay layer (otherwise top)
    #[arg(long, alias = "ovl")]
    pub overlay: bool,

    /// Window background opacity 0-100 (default: 88)
    #[arg(long, default_value_t = 88)]
    pub opacity: u8,

    /// GTK theme name
    #[arg(short = 'g', long, default_value = "")]
    pub gtk_theme: String,

    /// GTK icon theme name
    #[arg(short = 'i', long, default_value = "")]
    pub icon_theme: String,

    /// Icon size in pixels
    #[arg(long, alias = "is", default_value_t = 64)]
    pub icon_size: i32,

    /// Number of columns in the app grid
    #[arg(short = 'c', long, default_value_t = 6)]
    pub columns: u32,

    /// Icon spacing
    #[arg(long, default_value_t = 20)]
    pub spacing: u32,

    /// Force lang (e.g. "en", "pl")
    #[arg(long, default_value = "")]
    pub lang: String,

    /// File manager command
    #[arg(long, alias = "fm", default_value = "thunar")]
    pub file_manager: String,

    /// Terminal emulator
    #[arg(long, default_value = "foot")]
    pub term: String,

    /// File search name length limit
    #[arg(long, alias = "fslen", default_value_t = 80)]
    pub fs_name_limit: usize,

    /// Disable category filtering
    #[arg(long, alias = "nocats")]
    pub no_cats: bool,

    /// Disable file search
    #[arg(long, alias = "nofs")]
    pub no_fs: bool,

    /// Maximum number of file search results
    #[arg(long, default_value_t = 25)]
    pub fs_max_results: usize,

    /// Leave the program resident in memory
    #[arg(short = 'r', long)]
    pub resident: bool,

    /// File search result columns
    #[arg(long, alias = "fscol", default_value_t = 2)]
    pub fs_columns: u32,

    /// Margin top
    #[arg(long, default_value_t = 0)]
    pub mt: i32,

    /// Margin left
    #[arg(long, default_value_t = 0)]
    pub ml: i32,

    /// Margin right
    #[arg(long, default_value_t = 0)]
    pub mr: i32,

    /// Margin bottom
    #[arg(long, default_value_t = 0)]
    pub mb: i32,

    /// Auto-detect power bar buttons from system capabilities
    #[arg(long, alias = "pbauto")]
    pub pb_auto: bool,

    /// Power bar exit command
    #[arg(long, alias = "pbexit", default_value = "")]
    pub pb_exit: String,

    /// Power bar lock command
    #[arg(long, alias = "pblock", default_value = "")]
    pub pb_lock: String,

    /// Power bar poweroff command
    #[arg(long, alias = "pbpoweroff", default_value = "")]
    pub pb_poweroff: String,

    /// Power bar reboot command
    #[arg(long, alias = "pbreboot", default_value = "")]
    pub pb_reboot: String,

    /// Power bar sleep command
    #[arg(long, alias = "pbsleep", default_value = "")]
    pub pb_sleep: String,

    /// Power bar icon size
    #[arg(long, alias = "pbsize", default_value_t = 64)]
    pub pb_size: i32,

    /// Use icon theme for power bar (instead of built-in)
    #[arg(long, alias = "pbuseicontheme")]
    pub pb_use_icon_theme: bool,

    /// Turn on debug messages
    #[arg(short = 'd', long)]
    pub debug: bool,

    /// Set keyboard interactivity to on-demand
    #[arg(short = 'k', long)]
    pub keyboard_on_demand: bool,

    /// Close button position
    #[arg(long, value_enum, default_value_t = CloseButton::None)]
    pub closebtn: CloseButton,

    /// Open a running resident instance
    #[arg(long)]
    pub open: bool,

    /// Close a running resident instance
    #[arg(long)]
    pub close: bool,

    /// Force GTK theme for libadwaita apps (prepends GTK_THEME= to launch commands)
    #[arg(long, alias = "ft")]
    pub force_theme: bool,

    /// Show pin indicator dot on pinned apps in the grid
    #[arg(long, alias = "pi")]
    pub pin_indicator: bool,

    /// Window manager override (auto-detected from environment if not specified)
    #[arg(long, value_enum)]
    pub wm: Option<nwg_common::compositor::WmOverride>,
}

/// Close button position in the drawer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CloseButton {
    Left,
    Right,
    None,
}

impl DrawerConfig {
    /// Whether any power bar button is configured.
    pub fn has_power_bar(&self) -> bool {
        !self.pb_exit.is_empty()
            || !self.pb_lock.is_empty()
            || !self.pb_poweroff.is_empty()
            || !self.pb_reboot.is_empty()
            || !self.pb_sleep.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_ICON_SIZE: i32 = 48;
    const TEST_ICON_SIZE_STR: &str = "48";
    const TEST_FS_COLS: u32 = 3;
    const TEST_FS_COLS_STR: &str = "3";

    #[test]
    fn legacy_single_dash_flags() {
        let args = vec![
            "nwg-drawer",
            "-is",
            TEST_ICON_SIZE_STR,
            "-term",
            "foot",
            "-fscol",
            TEST_FS_COLS_STR,
        ]
        .into_iter()
        .map(String::from);
        let normalized = normalize_legacy_flags(args);
        assert_eq!(
            normalized,
            vec![
                "nwg-drawer",
                "--is",
                TEST_ICON_SIZE_STR,
                "--term",
                "foot",
                "--fscol",
                TEST_FS_COLS_STR,
            ]
        );
    }

    #[test]
    fn legacy_bool_flags() {
        let args = vec!["nwg-drawer", "-ft", "-nocats", "-nofs"]
            .into_iter()
            .map(String::from);
        let normalized = normalize_legacy_flags(args);
        assert_eq!(normalized, vec!["nwg-drawer", "--ft", "--nocats", "--nofs"]);
    }

    #[test]
    fn native_flags_unchanged() {
        let args = vec![
            "nwg-drawer",
            "-d",
            "-r",
            "-s",
            "custom.css",
            "--icon-size",
            TEST_ICON_SIZE_STR,
        ]
        .into_iter()
        .map(String::from);
        let normalized = normalize_legacy_flags(args);
        assert_eq!(
            normalized,
            vec![
                "nwg-drawer",
                "-d",
                "-r",
                "-s",
                "custom.css",
                "--icon-size",
                TEST_ICON_SIZE_STR,
            ]
        );
    }

    #[test]
    fn legacy_equals_form() {
        let args = vec!["nwg-drawer", "-is=48", "-term=foot", "-fscol=3"]
            .into_iter()
            .map(String::from);
        let normalized = normalize_legacy_flags(args);
        assert_eq!(
            normalized,
            vec!["nwg-drawer", "--is=48", "--term=foot", "--fscol=3"]
        );
    }

    #[test]
    fn unknown_flags_unchanged() {
        let args = vec!["nwg-drawer", "-unknown=value", "-x"]
            .into_iter()
            .map(String::from);
        let normalized = normalize_legacy_flags(args);
        assert_eq!(normalized, vec!["nwg-drawer", "-unknown=value", "-x"]);
    }

    #[test]
    fn legacy_flags_parse_correctly() {
        let config = DrawerConfig::parse_from(normalize_legacy_flags(
            vec![
                "nwg-drawer",
                "-is",
                TEST_ICON_SIZE_STR,
                "-term",
                "alacritty",
                "-fscol",
                TEST_FS_COLS_STR,
                "-ft",
            ]
            .into_iter()
            .map(String::from),
        ));
        assert_eq!(config.icon_size, TEST_ICON_SIZE);
        assert_eq!(config.term, "alacritty");
        assert_eq!(config.fs_columns, TEST_FS_COLS);
        assert!(config.force_theme);
    }

    #[test]
    fn wm_flag_long_form() {
        let config = DrawerConfig::parse_from(normalize_legacy_flags(
            vec!["nwg-drawer", "--wm", "uwsm"]
                .into_iter()
                .map(String::from),
        ));
        assert_eq!(config.wm, Some(nwg_common::compositor::WmOverride::Uwsm));
    }

    #[test]
    fn wm_flag_legacy_single_dash() {
        let config = DrawerConfig::parse_from(normalize_legacy_flags(
            vec!["nwg-drawer", "-wm", "uwsm"]
                .into_iter()
                .map(String::from),
        ));
        assert_eq!(config.wm, Some(nwg_common::compositor::WmOverride::Uwsm));
    }

    #[test]
    fn wm_flag_default_none() {
        let config = DrawerConfig::parse_from(normalize_legacy_flags(
            vec!["nwg-drawer"].into_iter().map(String::from),
        ));
        assert_eq!(config.wm, None);
    }

    #[test]
    fn pin_indicator_flag() {
        let config = DrawerConfig::parse_from(["nwg-drawer", "--pin-indicator"]);
        assert!(config.pin_indicator);
    }

    #[test]
    fn pin_indicator_legacy_alias() {
        let config = DrawerConfig::parse_from(normalize_legacy_flags(
            vec!["nwg-drawer", "-pi"].into_iter().map(String::from),
        ));
        assert!(config.pin_indicator);
    }

    #[test]
    fn pin_indicator_default_off() {
        let config = DrawerConfig::parse_from(["nwg-drawer"]);
        assert!(!config.pin_indicator);
    }
}
