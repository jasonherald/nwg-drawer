//! UI layout constants for the drawer.
//! Named to make the intent clear and tuning easy.

/// Width of the search entry widget.
pub const SEARCH_ENTRY_WIDTH: i32 = 500;

/// Top margin above the search entry.
pub const SEARCH_TOP_MARGIN: i32 = 24;

/// Side margin for the main content well (left and right padding).
pub const WELL_SIDE_MARGIN: i32 = 16;

/// Maximum characters shown in an app label.
pub const APP_NAME_MAX_CHARS: usize = 20;

/// Max width-chars hint for app labels (GTK ellipsize).
pub const APP_LABEL_MAX_WIDTH_CHARS: i32 = 14;

/// Width of the filename column in file search results.
pub const FILE_NAME_COLUMN_WIDTH: i32 = 250;

/// Pixel size for file type icons in search results.
pub const FILE_ICON_SIZE: i32 = 20;

/// Top margin for the content area below the search bar.
pub const CONTENT_TOP_MARGIN: i32 = 8;

/// Diameter of the pin indicator badge in pixels.
pub const PIN_BADGE_SIZE: i32 = 8;

/// Horizontal spacing between pin badge and label.
pub const PIN_BADGE_LABEL_GAP: i32 = 3;

/// Top margin for the category bar.
pub const CATEGORY_BAR_TOP_MARGIN: i32 = 8;

/// Bottom margin for the category bar.
pub const CATEGORY_BAR_BOTTOM_MARGIN: i32 = 4;

/// Top/bottom margin for the power bar and status display area.
pub const STATUS_AREA_VERTICAL_MARGIN: i32 = 12;

/// Top/bottom margin for section dividers.
pub const DIVIDER_VERTICAL_MARGIN: i32 = 8;

/// Side margin for section dividers (left/right padding).
pub const DIVIDER_SIDE_MARGIN: i32 = 16;

// Math result-row sizing (font-size, spacing, border-radius, padding) lives
// in `assets/drawer.css` under the `.math-result` / `.math-copy` rules
// rather than here — those values are only consumed by the static stylesheet,
// not by Rust-side widget construction. See issue #35.

/// Vertical spacing between math result row and "Copied!" label.
pub const MATH_VBOX_SPACING: i32 = 4;

/// Duration in seconds before the "Copied!" confirmation label auto-hides.
pub const COPIED_LABEL_TIMEOUT_SECS: u64 = 2;
