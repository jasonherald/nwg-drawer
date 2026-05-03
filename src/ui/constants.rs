//! UI layout constants for the drawer.
//!
//! Named so the call sites read at design-system level instead of
//! sprinkling magic numbers through every builder. CLAUDE.md mandates
//! "all UI dimensions in `ui/constants.rs`" — anything Rust passes to
//! gtk4 sizing / margin / spacing APIs lives here. CSS-only dimensions
//! (math result row sizing, etc.) live in `assets/drawer.css` instead;
//! see issue #35.

// ── Search bar ──────────────────────────────────────────────────────

/// Width of the search entry widget.
pub const SEARCH_ENTRY_WIDTH: i32 = 500;

/// Top margin above the search entry.
pub const SEARCH_TOP_MARGIN: i32 = 24;

// ── Well + content area ─────────────────────────────────────────────

/// Side margin for the main content well (left and right padding).
pub const WELL_SIDE_MARGIN: i32 = 16;

/// Top margin for the content area below the search bar.
pub const CONTENT_TOP_MARGIN: i32 = 8;

// ── App buttons (grid + pinned row) ─────────────────────────────────

/// Maximum characters shown in an app label.
pub const APP_NAME_MAX_CHARS: usize = 20;

/// Max width-chars hint for app labels (GTK ellipsize).
pub const APP_LABEL_MAX_WIDTH_CHARS: i32 = 14;

/// Vertical spacing between an app button's icon and its label.
pub const APP_BUTTON_VBOX_SPACING: i32 = 4;

/// Maximum characters shown in an app button's tooltip (description).
pub const APP_TOOLTIP_MAX_CHARS: usize = 120;

/// Diameter of the pin indicator badge in pixels.
pub const PIN_BADGE_SIZE: i32 = 8;

/// Horizontal spacing between pin badge and label.
pub const PIN_BADGE_LABEL_GAP: i32 = 3;

/// Top margin between the pinned-row container and the categories bar.
pub const PINNED_BOX_TOP_MARGIN: i32 = 4;

// ── Categories bar ──────────────────────────────────────────────────

/// Horizontal spacing between category buttons in the bar.
pub const CATEGORY_BAR_SPACING: i32 = 4;

/// Top margin for the category bar.
pub const CATEGORY_BAR_TOP_MARGIN: i32 = 8;

/// Bottom margin for the category bar.
pub const CATEGORY_BAR_BOTTOM_MARGIN: i32 = 4;

/// Horizontal spacing inside a category button (icon + label box).
pub const CATEGORY_BUTTON_INNER_SPACING: i32 = 4;

/// Pixel size for the icon shown inside each category button.
pub const CATEGORY_ICON_SIZE: i32 = 16;

// ── File search results ─────────────────────────────────────────────

/// Width of the filename column in file search results.
pub const FILE_NAME_COLUMN_WIDTH: i32 = 250;

/// Pixel size for file type icons in search results.
pub const FILE_ICON_SIZE: i32 = 20;

/// Vertical spacing between the header / separator / result rows in
/// the file-search results container.
pub const FILE_RESULTS_VBOX_SPACING: i32 = 2;

/// Bottom margin of the separator that sits between the file-results
/// header row and the first result.
pub const FILE_HEADER_SEPARATOR_BOTTOM_MARGIN: i32 = 2;

/// Horizontal spacing between [icon | filename | path] within one
/// file-result row.
pub const FILE_RESULT_ROW_SPACING: i32 = 8;

// ── Status / power bar / dividers ───────────────────────────────────

/// Top/bottom margin for the power bar and status display area.
pub const STATUS_AREA_VERTICAL_MARGIN: i32 = 12;

/// Top/bottom margin for section dividers.
pub const DIVIDER_VERTICAL_MARGIN: i32 = 8;

/// Side margin for section dividers (left/right padding).
pub const DIVIDER_SIDE_MARGIN: i32 = 16;

// ── Math result ─────────────────────────────────────────────────────
//
// Math result-row sizing (font-size, spacing, border-radius, padding) lives
// in `assets/drawer.css` under the `.math-result` / `.math-copy` rules
// rather than here — those values are only consumed by the static stylesheet,
// not by Rust-side widget construction. See issue #35.

/// Vertical spacing between math result row and "Copied!" label.
pub const MATH_VBOX_SPACING: i32 = 4;

/// Horizontal spacing between widgets within the math result row
/// (label / divider / copy button). The label sets its own right-margin
/// via the `.math-result` CSS rule and the copy button sets its own
/// left-margin via `.math-copy`, so the box-level spacing is intentionally
/// 0 — having both wouldn't compose visibly.
pub const MATH_ROW_SPACING: i32 = 0;

/// Duration in seconds before the "Copied!" confirmation label auto-hides.
pub const COPIED_LABEL_TIMEOUT_SECS: u64 = 2;

// ── GDK / input ─────────────────────────────────────────────────────

/// GDK button code for the right mouse button. Used by `GestureClick`
/// listeners that toggle pin / unpin / drawer-close on right-click.
pub const MOUSE_BUTTON_RIGHT: u32 = 3;
