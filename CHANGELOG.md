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

- Tightened `RefCell` borrow discipline across the category-button, search,
  and pin-toggle callbacks. Sequential `borrow_mut` calls now coalesce into a
  single block scope; pin/unpin handlers snapshot the pin list and release
  the borrow before `save_pinned` I/O so a hypothetical re-entrant signal
  during save can't deadlock. `in_search_mode` switched from
  `Rc<RefCell<bool>>` to `Rc<Cell<bool>>` to match `focus_pending` and remove
  the borrow-panic surface for a `Copy`-only flag. New doc-comment on
  `DrawerState` documents the borrow → drop → rebuild rule. Resolves #30.

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
