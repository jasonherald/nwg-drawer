# CLAUDE.md — nwg-drawer

## What is this?

A Launchpad-style application launcher + file search overlay for Hyprland, Sway, and any Wayland compositor with layer-shell support. Written in Rust, ported from [nwg-piotr/nwg-drawer](https://github.com/nwg-piotr/nwg-drawer) (Go).

Consumes [`nwg-common`](https://github.com/jasonherald/nwg-common) for compositor IPC, `.desktop` parsing, pin-file management, CSS hot-reload, and launch plumbing.

Pre-split (before v0.3.0) this lived inside the [mac-doc-hyprland](https://github.com/jasonherald/mac-doc-hyprland) monorepo at `crates/nwg-drawer/`.

## Build & test

```bash
cargo build                   # Debug build
cargo build --release         # Release build
cargo test                    # Unit tests
cargo clippy --all-targets    # Lint (should be zero warnings)
cargo fmt --all               # Format
make test                     # Unit tests + clippy
make lint                     # Full check: fmt + clippy + test + deny + audit
```

`make test-integration` exists as a scaffold (bootstraps headless Sway) but this crate has **no integration tests today**. The drawer is on-demand (spawn, show, exit on selection/focus loss), which makes integration testing awkward. Room for future additions; see [tests/integration/CLASSIFICATION.md](https://github.com/jasonherald/mac-doc-hyprland/blob/main/tests/integration/CLASSIFICATION.md) in the monorepo for classification rules.

## Install (dev workflow)

**Use the no-sudo invocation when iterating locally.** Default `make install` is system-wide:

```bash
make install PREFIX=$HOME/.local BINDIR=$HOME/.cargo/bin
```

See the README for the full install matrix (default / no-sudo / distro-parity / cargo install).

## Run locally

```bash
# Basic with auto-detected power bar
nwg-drawer --pb-auto

# Resident mode (stays in memory, toggle with signals)
nwg-drawer -r --pb-auto

# Force Sway backend (auto-detection is usually enough)
nwg-drawer --wm sway

# Use uwsm launcher mode (auto-detects Hyprland or Sway from env).
# Note: uwsm is a launch-wrapper mode, NOT a trigger for the null
# fallback — detection still falls through to
# HYPRLAND_INSTANCE_SIGNATURE / SWAYSOCK. The null backend kicks in
# automatically when neither env var is set (Niri, river, Openbox).
nwg-drawer --wm uwsm
```

## What lives where

```text
src/
├── main.rs            # Coordinator (~185 lines)
├── config.rs          # clap CLI with CloseButton enum
├── state.rs           # DrawerState + AppRegistry sub-struct
├── desktop_loader.rs  # Scans .desktop files, multi-category assignment
├── listeners.rs       # Focus detector, file watcher, signal receiver
└── ui/
    ├── navigation.rs     # install_grid_nav — capture-phase arrow-key
    │                     # traversal across FlowBox grids (app grid,
    │                     # pinned row, categories). Uses Weak refs
    │                     # to avoid widget ↔ controller cycles.
    ├── well_builder.rs   # WellContext (bundles 7+ params into one struct)
    ├── well_context.rs   # The WellContext struct itself
    ├── search_handler.rs
    ├── app_grid.rs, pinned.rs, file_search.rs
    ├── widgets.rs
    ├── math.rs           # exmex-based expression evaluator (see Conventions)
    ├── categories.rs
    ├── power_bar.rs
    ├── search.rs
    └── window.rs

assets/
└── drawer.css         # Embedded default CSS via include_str!()
```

## Conventions

- **Enums over strings** — CloseButton, etc. are `clap::ValueEnum` or repr enums.
- **Named constants** — all UI dimensions in `ui/constants.rs`.
- **`WellContext`** — the well/category/search builders take one `WellContext` struct, not 7+ individual params. Expanding a builder signature is a smell; add a field to the context instead.
- **Compositor trait only** — all WM IPC goes through `nwg_common::compositor::Compositor`. The drawer uses `init_or_null` to fall back gracefully on Niri, river, Openbox, etc.
- **Math evaluation uses `exmex`** — not `meval` (migrated in #64). Safe parser, no `eval`/shell. Custom operator factory adds `pi` / `π`, `%` modulo, and base-10 `log`. Scoped to pure arithmetic (no variables) so random search queries don't accidentally evaluate.
- **Inline math results don't trap keyboard focus** — vbox/row/label stay non-focusable so arrow keys continue to navigate the grid.
- **Pin/unpin operations roll back on save failure.**
- **`launch_desktop_entry()`** — use the helper from `nwg_common::launch`, not inline strip/prepend/launch.
- **No `#[allow(dead_code)]`, no magic numbers, log errors, tests at bottom of file.**

## Key patterns

### Capture-phase keyboard nav

The drawer's arrow-key navigation uses a capture-phase event handler to bypass `FlowBox`'s default key bindings. See `ui/navigation.rs` (`install_grid_nav`). Avoids intercepting type-to-search characters; only arrows/Enter/Escape are consumed. Up/down edges can jump to adjacent FlowBox grids (app grid → pinned row → categories) via the `up_target` / `down_target` parameters.

### Launcher command

The drawer is typically spawned by the dock's launcher button (`nwg-dock -c "nwg-drawer --pb-auto"`). Resident mode (`-r`) keeps it in memory for faster subsequent opens; on-demand is the default.

### Null-compositor fallback

`init_or_null` returns a `NullCompositor` when neither Hyprland nor Sway is detected. Most compositor methods return `DockError::NoCompositorDetected`, but launches still work (direct spawn via `nwg_common::launch`). Useful for running the drawer on Niri, river, Openbox, etc.

## Shared pin file

`~/.cache/mac-dock-pinned` (contract defined in `nwg_common::pinning`). Shared with the dock; right-click an app → Pin. Changes picked up via `notify` crate for instant sync.

## CSS path

`~/.config/nwg-drawer/drawer.css` — live hot-reload via `nwg_common::config::css::watch_css`. `@import` graph walked to 32 levels, cycles detected.

## See also

- `CHANGELOG.md` — user-visible changes per release, Keep-a-Changelog format.
- `README.md` — public-facing docs + install matrix + dock integration.
- [`nwg-common`](https://github.com/jasonherald/nwg-common) — shared library.
- [`nwg-dock`](https://github.com/jasonherald/nwg-dock) — dock that typically spawns the drawer via its launcher button.
- Parent monorepo archive: [jasonherald/mac-doc-hyprland](https://github.com/jasonherald/mac-doc-hyprland).
