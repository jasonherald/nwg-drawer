# Changelog

All notable changes to `nwg-drawer` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> **Pre-split note:** Prior to v0.3.0, this crate lived inside the
> [`mac-doc-hyprland`](https://github.com/jasonherald/mac-doc-hyprland) monorepo
> at `crates/nwg-drawer/`. v0.3.0 is the first release in its own repo. The
> full pre-split history is preserved in the monorepo's git log; this file
> only documents changes from v0.3.0 onward.

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
