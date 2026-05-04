//! Small, reusable widget factories shared across builders.
//!
//! `app_icon_button` is the canonical "icon over label" launcher
//! button used by both the app grid and the pinned row, so the two
//! always share styling and hover/focus behavior. `apply_pin_badge`
//! adds the small overlay dot for pinned items, and `truncate`
//! shortens long names/tooltips at grapheme boundaries.

use super::constants;
use gtk4::prelude::*;
use nwg_common::desktop::entry::DesktopEntry;
use nwg_common::desktop::icons;

/// Returns the display name for a desktop entry: the localized
/// `Name[xx]` if non-empty, else the unlocalized `Name`. Used by the
/// app-grid and pinned-row button factories so both routes pick the
/// same string for any given entry.
pub fn display_name(entry: &DesktopEntry) -> &str {
    if !entry.name_loc.is_empty() {
        &entry.name_loc
    } else {
        &entry.name
    }
}

/// Returns the display description for a desktop entry: the localized
/// `Comment[xx]` if non-empty, else the unlocalized `Comment`. Same
/// rationale as [`display_name`].
pub fn display_desc(entry: &DesktopEntry) -> &str {
    if !entry.comment_loc.is_empty() {
        &entry.comment_loc
    } else {
        &entry.comment
    }
}

/// Creates and configures the search entry widget at the top of the
/// drawer. Style + sizing live in CSS / Rust margins on the call site.
pub fn build_search_entry() -> gtk4::SearchEntry {
    let entry = gtk4::SearchEntry::new();
    entry.set_placeholder_text(Some("Type to search"));
    entry
}

/// Creates a GTK4 button with icon above label, matching macOS Launchpad style.
///
/// Shared between the app-grid and pinned-flow builders to eliminate duplication.
/// If `status_label` and `description` are provided, the button updates the
/// status bar on hover/focus with the app description (matching Go behavior).
pub fn app_icon_button(
    icon_name: &str,
    display_name: &str,
    icon_size: i32,
    app_dirs: &[std::path::PathBuf],
    status_label: &gtk4::Label,
    description: &str,
) -> gtk4::Button {
    let button = gtk4::Button::new();
    button.set_has_frame(false);
    button.add_css_class("app-button");

    let vbox = gtk4::Box::new(
        gtk4::Orientation::Vertical,
        constants::APP_BUTTON_VBOX_SPACING,
    );
    vbox.set_halign(gtk4::Align::Center);

    // Icon — try theme/file, fall back to generic "application-x-executable"
    let image = if !icon_name.is_empty() {
        icons::create_image(icon_name, icon_size, app_dirs)
    } else {
        None
    };
    let image = image.unwrap_or_else(|| {
        let fallback = gtk4::Image::from_icon_name("application-x-executable");
        fallback.set_pixel_size(icon_size);
        fallback
    });
    image.set_pixel_size(icon_size);
    image.set_halign(gtk4::Align::Center);
    vbox.append(&image);

    // Label
    let label = gtk4::Label::new(Some(&truncate(display_name, constants::APP_NAME_MAX_CHARS)));
    label.set_halign(gtk4::Align::Center);
    label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    label.set_max_width_chars(constants::APP_LABEL_MAX_WIDTH_CHARS);
    vbox.append(&label);

    button.set_child(Some(&vbox));

    // Status label: show description on hover/focus, clear on leave
    if !description.is_empty() {
        let desc = description.to_string();
        let label_enter = status_label.clone();
        let motion = gtk4::EventControllerMotion::new();
        let desc_enter = desc.clone();
        motion.connect_enter(move |_, _, _| {
            label_enter.set_text(&desc_enter);
        });
        let label_leave = status_label.clone();
        motion.connect_leave(move |_| {
            label_leave.set_text("");
        });
        button.add_controller(motion);

        // Also update on keyboard focus
        let label_focus = status_label.clone();
        let focus_ctrl = gtk4::EventControllerFocus::new();
        focus_ctrl.connect_enter(move |_| {
            label_focus.set_text(&desc);
        });
        let label_unfocus = status_label.clone();
        focus_ctrl.connect_leave(move |_| {
            label_unfocus.set_text("");
        });
        button.add_controller(focus_ctrl);
    }

    button
}

/// Adds a pin indicator dot to the left of the app label.
///
/// Finds the Label inside the button's VBox, removes it, wraps it in a
/// horizontal Box with a small dot + label, and re-appends it to the VBox.
pub fn apply_pin_badge(button: &gtk4::Button) {
    let Some(vbox_widget) = button.child() else {
        return;
    };
    let Ok(vbox) = vbox_widget.downcast::<gtk4::Box>() else {
        return;
    };

    // Find the label (second child after Image)
    let Some(image) = vbox.first_child() else {
        return;
    };
    let Some(label_widget) = image.next_sibling() else {
        return;
    };

    // Remove label from vbox
    vbox.remove(&label_widget);

    // Create horizontal box: [dot] [label]
    let hbox = gtk4::Box::new(
        gtk4::Orientation::Horizontal,
        constants::PIN_BADGE_LABEL_GAP,
    );
    hbox.set_halign(gtk4::Align::Center);

    let badge = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    badge.add_css_class("pin-badge");
    badge.set_size_request(constants::PIN_BADGE_SIZE, constants::PIN_BADGE_SIZE);
    badge.set_valign(gtk4::Align::Center);

    hbox.append(&badge);
    hbox.append(&label_widget);

    vbox.append(&hbox);
}

/// Truncates a string to `max` *characters*, appending an ellipsis
/// (`…`) if truncation occurred. The ellipsis itself counts toward
/// `max`, so the returned string never exceeds `max` chars.
///
/// Counts are **char-based**, not byte-based: a `max` of 5 returns 5
/// graphemes-ish even for multi-byte input like CJK or emoji. This is
/// the gotcha — if you want byte-based truncation (e.g. for fixed-width
/// buffers), use `s.bytes().take(n)` and re-validate UTF-8 instead.
///
/// # Examples
///
/// ```rust,ignore
/// // Short input — passes through unchanged:
/// assert_eq!(truncate("Hi", 20), "Hi");
///
/// // Long ASCII — last char becomes the ellipsis (max counts the `…`):
/// assert_eq!(truncate("Application", 5), "Appl…");
///
/// // CJK input — char-based, not byte-based:
/// // 日本語 is 3 chars but 9 bytes. With max=2 we keep 1 char + ellipsis,
/// // not 2 bytes' worth.
/// let result = truncate("日本語", 2);
/// assert_eq!(result.chars().count(), 2);
/// assert!(result.ends_with('…'));
/// ```
///
/// (Doctests aren't compiled — this is a binary-only crate. The
/// runtime regression suite in `#[cfg(test)] mod tests` covers the
/// same cases.)
pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let truncated: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", truncated)
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate("Hi", 20), "Hi");
    }

    #[test]
    fn truncate_long_string() {
        let result = truncate("Very Long Application Name Here", 10);
        assert!(result.ends_with('…'));
        assert!(result.chars().count() <= 10);
    }

    #[test]
    fn truncate_exact_length() {
        assert_eq!(truncate("12345", 5), "12345");
    }

    #[test]
    fn truncate_unicode() {
        // Ensure char-based truncation, not byte-based
        let result = truncate("日本語のアプリケーション名前テスト", 5);
        assert!(result.ends_with('…'));
        assert_eq!(result.chars().count(), 5);
    }
}
