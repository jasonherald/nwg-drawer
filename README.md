# nwg-drawer

A Launchpad-style application launcher and file search overlay for [Hyprland](https://hyprland.org/), [Sway](https://swaywm.org/), and any Wayland compositor with layer-shell support. Written in Rust.

Ported from [nwg-piotr/nwg-drawer](https://github.com/nwg-piotr/nwg-drawer) (Go) with enhancements: compositor-neutral IPC via the Compositor trait, shared pin state with [`nwg-dock`](https://github.com/jasonherald/nwg-dock), and graceful fallback on unsupported compositors (Niri, river, Openbox).

## Features

- **Full-screen overlay** — dark transparent Launchpad-style launcher
- **Keyboard navigation** — arrow keys between icons, Enter to launch, type to search
- **Category filtering** — filter bar with per-category buttons
- **Description line** — status bar shows app description on hover/focus
- **Power bar** — lock/exit/reboot/sleep/poweroff with `--pb-auto` auto-detection
- **Configurable opacity** — `--opacity 0-100` for background transparency
- **Subsequence search** — type to filter apps by name, description, or command
- **File search** — columnar results with system theme icons, sorted alphabetically
- **Math evaluation** — type expressions like `2+2` and get results with clipboard copy (via `exmex`)
- **Command execution** — prefix with `:` to run arbitrary commands
- **Pin sync** — shared pin file with [`nwg-dock`](https://github.com/jasonherald/nwg-dock); changes reflect instantly on both via inotify
- **Compositor-neutral** — runs on Hyprland, Sway, and any layer-shell-capable compositor. Graceful feature degradation on the null backend (Niri, river, Openbox, etc.)
- **Go flag compatibility** — accepts original Go nwg-drawer flag names (`--pbexit`, `--nocats`, etc.)

## Install

### Requirements

- **Rust 1.95** or later (pinned in `rust-toolchain.toml`; rustup picks it up automatically)
- **GTK4** and **gtk4-layer-shell** system libraries
- A Wayland compositor with `wlr-layer-shell` support (Hyprland, Sway, Niri, river, etc.)

### Install system dependencies

```bash
# Arch Linux
sudo pacman -S gtk4 gtk4-layer-shell

# Ubuntu/Debian
sudo apt install libgtk-4-dev libgtk4-layer-shell-dev

# Fedora
sudo dnf install gtk4-devel gtk4-layer-shell-devel
```

### `make install` — three invocations

**Default — system-wide (needs sudo):**

```bash
sudo make install
```

Writes:
- `nwg-drawer` → `/usr/local/bin/nwg-drawer`
- Data files (drawer.css, `img/`) → `/usr/local/share/nwg-drawer/`

**No-sudo, dev workflow (useful when working from a clone):**

```bash
make install PREFIX=$HOME/.local BINDIR=$HOME/.cargo/bin
```

**Distro-parity (matches Go upstream's `/usr/bin` exactly):**

```bash
sudo make install PREFIX=/usr
```

### From crates.io

```bash
cargo install nwg-drawer
```

Installs only the binary; the drawer-installed data assets (`drawer.css`, category icons) are not copied. The drawer falls back to its embedded defaults when the filesystem assets are missing, so `cargo install` alone gets you a working drawer; `make install` adds user-customizable CSS and icons at the system location.

## Usage

```bash
# Basic with auto-detected power bar
nwg-drawer --pb-auto

# Fully configured
nwg-drawer --opacity 88 --pb-auto --columns 8

# Resident mode (stays in memory, toggle with signals)
nwg-drawer -r --pb-auto

# Force Sway backend (usually auto-detected)
nwg-drawer --wm sway
```

## Integration with the dock

The drawer is typically launched by the dock's launcher button. Configure the dock with `-c "nwg-drawer --pb-auto"` to wire it up:

```ini
# ~/.config/hypr/autostart.conf (Hyprland example)
exec-once = nwg-dock -d -i 48 --mb 10 --hide-timeout 400 --launch-animation -c "nwg-drawer --opacity 88 --pb-auto"
```

Pins work bidirectionally — pin an app from the drawer (right-click → Pin) and it shows up in the dock instantly, and vice versa.

## Signal control (resident mode only)

```bash
# Toggle visibility
pkill -f -35 nwg-drawer     # SIGRTMIN+1

# Show
pkill -f -36 nwg-drawer     # SIGRTMIN+2

# Hide
pkill -f -37 nwg-drawer     # SIGRTMIN+3
```

## Theming

The drawer loads CSS from `~/.config/nwg-drawer/drawer.css`. Changes are picked up instantly via live file-change detection — no restart or signal needed. Hot-reload follows the full `@import` graph, so [tinty](https://github.com/tinted-theming/tinty) and similar theme managers work out of the box.

Override the path with `-s /path/to/custom.css`.

### base16 themes via tinty

Use the [tinted-nwg-dock](https://github.com/tinted-theming/tinted-nwg-dock) templates with the drawer hook:

```toml
[[items]]
name = "base16-nwg-drawer"
path = "https://github.com/tinted-theming/tinted-nwg-dock"
themes-dir = "themes"
hook = "cp '%f' ~/.config/nwg-drawer/drawer.css"
supported-systems = ["base16"]
```

Applying a theme reloads the drawer live.

## Shared pin file

Pin state lives at `~/.cache/mac-dock-pinned`, shared with [`nwg-dock`](https://github.com/jasonherald/nwg-dock). Pin/unpin from either side; the other picks up the change via inotify within milliseconds.

## Contributing

PRs welcome. `main` is protected — open from a feature branch. Run `make lint` (fmt + clippy + test + deny + audit) locally before requesting review.

User-visible PRs add a CHANGELOG bullet under `## [x.y.z] — Unreleased` in `CHANGELOG.md`, following [Keep a Changelog](https://keepachangelog.com).

## Deviations from Go `nwg-drawer`

- **Multi-compositor** — Go version is Sway/Hyprland via separate branches; Rust port auto-detects and degrades gracefully on unsupported compositors.
- **Shared pin file** — Go drawer uses `~/.cache/nwg-pin-cache`; Rust port shares `~/.cache/mac-dock-pinned` with the dock.
- **Math evaluation** — Go uses the `expr` library for arbitrary expression evaluation; Rust port uses the [`exmex`](https://crates.io/crates/exmex) crate (safe parser, no `eval`/shell) with a small custom operator factory that adds `pi` / `π`, `%` modulo, and base-10 `log`. Scoped to pure arithmetic (no variables) so random search queries don't accidentally evaluate.
- **CLI flag naming** — multi-word flags standardized to kebab-case (`--nocats` → `--no-cats`, `--pbsize` → `--pb-size`). Multi-char Go short forms not available; use long forms.
- **Launcher auto-detection** — unrelated but worth noting: if the configured launcher command is missing, the launcher button in the dock hides automatically; the drawer itself has no such dependency.

## Credits

Ported from [nwg-piotr/nwg-drawer](https://github.com/nwg-piotr/nwg-drawer) (MIT).

## License

MIT. See `LICENSE`.
