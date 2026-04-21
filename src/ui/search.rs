/// Creates and configures the search entry widget.
pub fn setup_search_entry() -> gtk4::SearchEntry {
    let entry = gtk4::SearchEntry::new();
    entry.set_placeholder_text(Some("Type to search"));
    entry
}

/// Subsequence matching: checks if all chars of needle appear in order in haystack.
pub fn subsequence_match(needle: &str, haystack: &str) -> bool {
    let needle = needle.to_lowercase();
    let haystack = haystack.to_lowercase();

    let mut needle_chars = needle.chars();
    let mut current = needle_chars.next();

    for h in haystack.chars() {
        if let Some(n) = current {
            if n == h {
                current = needle_chars.next();
            }
        } else {
            return true;
        }
    }
    current.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subsequence_match() {
        assert!(subsequence_match("ff", "firefox"));
        assert!(subsequence_match("frfx", "firefox"));
        assert!(subsequence_match("firefox", "firefox"));
        assert!(!subsequence_match("fz", "firefox"));
        assert!(subsequence_match("", "anything"));
        assert!(subsequence_match("FI", "firefox")); // case insensitive
    }
}
