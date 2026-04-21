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

/// Font size for inline math result text and copy button.
pub const MATH_FONT_SIZE: i32 = 20;

/// Horizontal margin between math result/button and the divider.
pub const MATH_SPACING: i32 = 12;

/// Border radius for the math copy button.
pub const MATH_BORDER_RADIUS: i32 = 6;

/// Vertical/horizontal padding inside the math copy button.
pub const MATH_BUTTON_PADDING_V: i32 = 4;
pub const MATH_BUTTON_PADDING_H: i32 = 16;

/// Vertical spacing between math result row and "Copied!" label.
pub const MATH_VBOX_SPACING: i32 = 4;

/// Duration in seconds before the "Copied!" confirmation label auto-hides.
pub const COPIED_LABEL_TIMEOUT_SECS: u64 = 2;
