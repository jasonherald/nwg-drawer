# Changelog

All notable changes to `nwg-drawer` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> **Pre-split note:** Prior to v0.3.0, this crate lived inside the
> [`mac-doc-hyprland`](https://github.com/jasonherald/mac-doc-hyprland) monorepo
> at `crates/nwg-drawer/`. v0.3.0 is the first release in its own repo. The
> full pre-split history is preserved in the monorepo's git log; this file
> only documents changes from v0.3.0 onward.

## [Unreleased]

### Changed

- Hardened category, search, and pin-toggle callbacks against re-entrant
  signal panics during pin/unpin I/O. Resolves #30.
- RT-signal toggle/show/hide and inotify-driven file-watcher events now
  reach the UI immediately instead of waiting for the old 100 ms poll
  interval. Resolves #40.
- File search no longer blocks the UI during typing. Each keystroke
  used to walk every XDG user directory synchronously on the GTK main
  thread, freezing the search bar for hundreds of milliseconds (or
  multiple seconds against a heavy `~/Downloads`). The walk now
  debounces 150 ms after the last keystroke and runs on a worker
  thread; app-name results and the math evaluator stay instant.
  Resolves #39.
- App-grid and pinned-row rebuilds avoid per-build allocations on the
  hot search path; keystroke-time overhead now scales much better on
  large `.desktop` registries.

### Fixed

- Capture-phase keyboard handler no longer grabs focus on
  modifier-only key presses. Pressing Shift before a navigation key
  (Shift+Tab, Ctrl+Backspace, Super-anything) used to yank focus to
  the search entry on the modifier-down event, before the modified
  key arrived — Shift+Tab in particular would land in the search bar
  instead of moving across the grid.
- Pin-indicator dots and other recent style additions now render on
  fresh installs. The shipped `data/nwg-drawer/drawer.css` had drifted
  from the embedded `src/assets/drawer.css` — new users got a 40-line
  stub seeded into `~/.config/nwg-drawer/drawer.css` without `.pin-badge`,
  `.drawer-search`, etc. The Makefile now installs `src/assets/drawer.css`
  directly so the shipped and embedded copies can't drift again.
- Embedded `.drawer-search` rule had `margin: 20px 25%`, which GTK4
  rejects (CSS `%` is not allowed on `margin`); the rule was being
  dropped at parse time. Removed the broken declaration — search
  centering is already handled by `set_halign(Center)` in Rust.
- File-search debounce no longer crashes the drawer when the user
  pauses long enough for the worker to fire and then keeps typing. The
  dispatcher used to retain the timeout's `SourceId` after it had
  fired; a subsequent keystroke would call `id.remove()` on the
  already-removed source, which `glib::SourceId::remove` panics on in
  glib 0.21. The slot is now cleared from inside the timeout closure
  itself.
- Pin-toggle save failure no longer silently reorders the pinned row — the
  rollback path now restores the unpinned item at its original position
  instead of appending it. Pre-existing latent bug surfaced during #30.
- File-search path display now correctly handles sibling directories of
  `$HOME` and the case where `$HOME` is unset. Previously `/home/userfoo/Docs`
  would render as `~foo/Docs` (sibling of `/home/user`), and an unset `$HOME`
  caused every path to gain a leading `~`. Resolves #33.
- Unreadable `.desktop` overrides in higher-priority app dirs (permission
  denied, broken symlink, etc.) no longer hide the app entirely — the
  launcher now falls through to the system-wide copy. Previously a single
  bad user override caused the app to disappear from the grid.
- Math result "Copy" button now copies negative results correctly. Values
  like `-5` or `-3.14` previously failed silently because `wl-copy` parsed
  the leading `-` as an unknown flag.

### Removed

- Dead `src/ui/pinned.rs` module. The file was never declared in `mod.rs`,
  not referenced anywhere, and duplicated the live pinned-row logic in
  `well_builder.rs`. Resolves #29.

## [0.3.0] — 2026-04-20

First standalone release. Extracts the drawer binary from
[`mac-doc-hyprland`](https://github.com/jasonherald/mac-doc-hyprland) as its
own repo + crates.io crate.

### Changed

- Dependency: `nwg-common` now consumed from crates.io at `"0.3"` rather than
  as a workspace path dependency.

### Added

- crates.io metadata (`description`, `readme`, `keywords`, `categories`,
  `repository`) wired up.
