# nwg-drawer

[![crates.io](https://img.shields.io/crates/v/nwg-drawer.svg)](https://crates.io/crates/nwg-drawer)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

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

- **Rust 1.97** or later (pinned in `rust-toolchain.toml`; rustup picks it up automatically)
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

### From crates.io (recommended for end users)

```bash
cargo install nwg-drawer
```

Installs only the binary; the drawer-installed data assets (`drawer.css`, category icons) are not copied. The drawer falls back to its embedded defaults when the filesystem assets are missing, so `cargo install` alone gets you a working drawer; `make install` adds user-customizable CSS and icons at the system location.

### `make install` — for source builds and distro packagers

The Makefile install path drops the binary and the bundled CSS + category icons. Three invocations depending on where the binary should land:

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

## Usage

```bash
# Basic with auto-detected power bar
nwg-drawer --pb-auto

# Fully configured
nwg-drawer --opacity 88 --pb-auto --columns 8

# Resident mode (stays in memory, toggle with signals)
nwg-drawer -r --pb-auto
```

### Compositor backend (`--wm`)

The drawer auto-detects Hyprland (via `HYPRLAND_INSTANCE_SIGNATURE`) or Sway (via `SWAYSOCK`) at startup. **Most users don't need `--wm` at all.**

When you do want to override:

| Value | Effect |
|---|---|
| _omitted_ (default) | Auto-detect from env vars; falls back to a null backend on Niri/river/Openbox/etc. |
| `--wm hyprland` | Force the Hyprland IPC backend regardless of environment. |
| `--wm sway` | Force the Sway IPC backend regardless of environment. |
| `--wm uwsm` | Launch wrapper for [uwsm](https://github.com/Vladimir-csp/uwsm) (Universal Wayland Session Manager) sessions. **Compositor detection still falls through to the env vars** — this flag controls how launched apps are spawned, not which backend serves drawer IPC. |

#### Note for Slackware, Void, Alpine, and other non-systemd distros

`--wm uwsm` is for users running under a uwsm-managed Wayland session, which currently requires systemd. **If you're not on systemd, just don't pass `--wm uwsm` and the drawer works normally** — app launches go directly through the compositor's exec mechanism (Hyprland's `dispatch exec`, Sway's `exec`) or via `sh -c` for the null backend. No `uwsm` binary is ever invoked.

The drawer has no `which uwsm` probe and no PATH check — the only place it would shell out to `uwsm` is when the user explicitly opts in with `--wm uwsm`. Even there, if the binary turns out to be missing the launcher logs a warning and falls back to direct launch (`sh -c`), so a misconfigured launcher still launches apps.

## Integration with the dock

The drawer is typically launched by the dock's launcher button. Configure the dock with `-c "nwg-drawer --pb-auto"` to wire it up:

```ini
# ~/.config/hypr/autostart.conf (Hyprland example)
exec-once = nwg-dock -d -i 48 --mb 10 --hide-timeout 400 --launch-animation -c "nwg-drawer --opacity 88 --pb-auto"
```

Pins work bidirectionally — pin an app from the drawer (right-click → Pin) and it shows up in the dock instantly, and vice versa.

## Hyprland Lua config (Omarchy 4.0 "Quattro" and other Lua setups)

Hyprland 0.55+ Lua configurations don't read `autostart.conf` or
`bindings.conf`. On Omarchy 4.0 the equivalents live in
`~/.config/hypr/autostart.lua` and `~/.config/hypr/bindings.lua`:

```lua
-- ~/.config/hypr/autostart.lua — resident mode (optional, faster opens)
o.launch_on_start([[env GDK_WAYLAND_DISABLE=zwp_linux_dmabuf_v1 nwg-drawer -r --opacity 88 --pb-auto]])

-- ~/.config/hypr/bindings.lua — open the drawer (toggles the resident
-- instance if one is running, spawns on-demand otherwise)
o.bind("SUPER + D", "App drawer", "nwg-drawer --opacity 88 --pb-auto")
```

(On plain Lua setups without Omarchy's helpers, register the autostart
command on the start hook instead:
`hl.on("hyprland.start", function() hl.exec_cmd([[nwg-drawer -r …]]) end)`.)

**Migrating to Omarchy 4.0:** the Quattro migration generates the new
`.lua` config files but does **not** carry custom `exec-once` or `bind`
lines across from the `.conf` files — after the upgrade a resident
drawer (and your drawer keybinding) silently stops working until you
re-add them in the `.lua` files as above. App launches from the drawer
are fully functional on Lua sessions as of nwg-common 0.7, which
retries dispatches in the session's Lua syntax automatically.

### Known issue: GTK4 crash on DPMS cycles (Hyprland ≥ 0.56)

GTK ≤ 4.22 has a bug in its Wayland dmabuf-feedback handler
(`gdk/wayland/gdkdmabuf-wayland.c` munmaps the wrong pointer when the
compositor re-sends `zwp_linux_dmabuf_feedback_v1`). Hyprland 0.56
started re-sending that feedback every time an output is
disabled/re-enabled, so any GTK4 client — the drawer included — can
segfault after a DPMS off/on cycle. Resident mode is the exposed case:
the process sits through every screen blank. Whether a given cycle
crashes depends on heap-allocation alignment, so it strikes
intermittently.

Until a fixed GTK ships, launch the drawer with the dmabuf protocol
disabled (as in the Lua snippet above, or the classic equivalent):

```ini
# ~/.config/hypr/autostart.conf
exec-once = env GDK_WAYLAND_DISABLE=zwp_linux_dmabuf_v1 nwg-drawer -r --opacity 88 --pb-auto
```

The drawer doesn't use dmabuf texture import or graphics offload, so
the only effect of the switch is dodging the crash.

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
