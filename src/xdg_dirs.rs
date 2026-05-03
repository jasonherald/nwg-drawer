//! XDG user-directory resolution for file search.
//!
//! Reads `~/.config/user-dirs.dirs` (or `$XDG_CONFIG_HOME/user-dirs.dirs`)
//! and returns a `name → PathBuf` map covering `home`, `documents`,
//! `downloads`, `music`, `pictures`, `videos`. Used by `file_search` to
//! enumerate the search roots.
//!
//! Defaults are seeded from `$HOME/<DefaultName>` so the drawer still
//! works on systems where the user-dirs.dirs file is missing or
//! malformed; entries from the file override the defaults.
//!
//! Extracted from `state.rs` (issue #38) so the parser is unit-testable
//! without constructing a `DrawerState` and so `state.rs` can stay
//! data-only.

use std::collections::HashMap;
use std::path::PathBuf;

/// Maps XDG user directory names to paths. Defaults to `$HOME/<Name>`
/// for each well-known directory, then overrides from the entries
/// found in `~/.config/user-dirs.dirs`.
pub(crate) fn map_xdg_user_dirs() -> HashMap<String, PathBuf> {
    let mut result = HashMap::new();
    let home = std::env::var("HOME").unwrap_or_default();

    result.insert("home".into(), PathBuf::from(&home));
    result.insert("documents".into(), PathBuf::from(&home).join("Documents"));
    result.insert("downloads".into(), PathBuf::from(&home).join("Downloads"));
    result.insert("music".into(), PathBuf::from(&home).join("Music"));
    result.insert("pictures".into(), PathBuf::from(&home).join("Pictures"));
    result.insert("videos".into(), PathBuf::from(&home).join("Videos"));

    let config_home =
        std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| format!("{}/.config", home));
    let user_dirs_file = PathBuf::from(&config_home).join("user-dirs.dirs");

    if let Ok(content) = std::fs::read_to_string(&user_dirs_file) {
        for line in content.lines() {
            let line = line.trim();
            // Skip blanks and comments without logging — those are normal.
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let key = match line.split_once('=').map(|(k, _)| k) {
                Some(k) => k,
                None => {
                    log::debug!("Skipping malformed user-dirs.dirs line (no '='): {}", line);
                    continue;
                }
            };
            // The XDG keys we care about. Anything else (e.g.
            // XDG_TEMPLATES_DIR, XDG_PUBLICSHARE_DIR) is silently
            // ignored — the drawer doesn't search those by default.
            let bucket = match key {
                "XDG_DOCUMENTS_DIR" => "documents",
                "XDG_DOWNLOAD_DIR" => "downloads",
                "XDG_MUSIC_DIR" => "music",
                "XDG_PICTURES_DIR" => "pictures",
                "XDG_VIDEOS_DIR" => "videos",
                _ => continue,
            };
            match parse_user_dir_line(line, &home) {
                Some(val) => {
                    result.insert(bucket.into(), val);
                }
                None => {
                    log::debug!(
                        "Skipping unparseable user-dirs.dirs line for {}: {}",
                        key,
                        line
                    );
                }
            }
        }
    }

    result
}

/// Parses a single `XDG_FOO_DIR="$HOME/Bar"` style line. Returns the
/// resolved `PathBuf` on success, or `None` if the line lacks `=` or
/// the value is empty after trimming quotes.
fn parse_user_dir_line(line: &str, home: &str) -> Option<PathBuf> {
    let (_, value) = line.split_once('=')?;
    let value = value.trim().trim_matches('"');
    if value.is_empty() {
        return None;
    }
    let expanded = value.replace("$HOME", home);
    Some(PathBuf::from(expanded))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quoted_value_with_home_expansion() {
        assert_eq!(
            parse_user_dir_line("XDG_DOCUMENTS_DIR=\"$HOME/Docs\"", "/home/u"),
            Some(PathBuf::from("/home/u/Docs"))
        );
    }

    #[test]
    fn parses_unquoted_value() {
        assert_eq!(
            parse_user_dir_line("XDG_DOWNLOAD_DIR=$HOME/Downloads", "/home/u"),
            Some(PathBuf::from("/home/u/Downloads"))
        );
    }

    #[test]
    fn line_without_equals_returns_none() {
        assert!(parse_user_dir_line("XDG_DOCUMENTS_DIR", "/home/u").is_none());
    }

    #[test]
    fn empty_value_returns_none() {
        assert!(parse_user_dir_line("XDG_DOCUMENTS_DIR=\"\"", "/home/u").is_none());
        assert!(parse_user_dir_line("XDG_DOCUMENTS_DIR=", "/home/u").is_none());
    }

    #[test]
    fn home_substitution_works_anywhere_in_path() {
        // Documented behavior: `$HOME` is replaced wherever it appears,
        // not just as a prefix. Pin this so a future "only-prefix"
        // optimization doesn't silently change semantics.
        assert_eq!(
            parse_user_dir_line("XDG_DOCUMENTS_DIR=\"/mnt/$HOME/Docs\"", "/home/u"),
            Some(PathBuf::from("/mnt//home/u/Docs"))
        );
    }

    #[test]
    fn handles_path_without_home_var() {
        // Absolute path with no $HOME reference passes through.
        assert_eq!(
            parse_user_dir_line("XDG_DOCUMENTS_DIR=\"/srv/shared/docs\"", "/home/u"),
            Some(PathBuf::from("/srv/shared/docs"))
        );
    }

    #[test]
    fn whitespace_around_value_is_trimmed() {
        assert_eq!(
            parse_user_dir_line("XDG_PICTURES_DIR=  \"$HOME/Pics\"  ", "/home/u"),
            Some(PathBuf::from("/home/u/Pics"))
        );
    }
}
