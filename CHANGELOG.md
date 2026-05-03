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

### Fixed

- Pin-toggle save failure no longer silently reorders the pinned row — the
  rollback path now restores the unpinned item at its original position
  instead of appending it. Pre-existing latent bug surfaced during #30.
- File-search path display now correctly handles sibling directories of
  `$HOME` and the case where `$HOME` is unset. Previously `/home/userfoo/Docs`
  would render as `~foo/Docs` (sibling of `/home/user`), and an unset `$HOME`
  caused every path to gain a leading `~`. Resolves #33.
- Locked the registry invariant for `NoDisplay=true` `.desktop` entries:
  they are no longer added to `apps.entries` / `apps.category_lists` at all,
  only to `id2entry` (so pinned ids still resolve). The downstream display
  paths already filtered them defensively, so no user-visible regression
  was reachable today, but the invariant is now enforced at the source.

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
