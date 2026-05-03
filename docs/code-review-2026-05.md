# Code review action plan — May 2026

This document captures the comprehensive code-review backlog produced from a parallel multi-agent audit of `nwg-drawer` v0.3.0 (commit `3c2a23d`). It is the source of truth for the refactor epic; each filed issue links back here for context.

## Scope

Five independent reviews ran in parallel (Opus 4.7 1M, read-only):

- **A.** Idiomatic Rust + error handling
- **B.** Architecture + module boundaries
- **C.** GTK4 + concurrency patterns
- **D.** Documentation + naming + magic numbers + dead code
- **E.** Testability + test coverage

Total findings: ~50 raw items, deduped and grouped into 22 PR-sized issues plus a parent epic. CLAUDE.md-endorsed patterns (WellContext, exmex, init_or_null, capture-phase nav) are excluded from change.

## Three load-bearing facts that fell out of the review

1. **`src/ui/pinned.rs` is dead.** Not declared in `src/ui/mod.rs`, never imported, duplicates the live pin-row logic in `well_builder.rs::build_pinned_flow`. Verified.
2. **`shorten_home` in `file_search.rs:208` has a latent bug.** `parent.starts_with(&home)` matches sibling directories — `HOME=/home/user` plus `parent=/home/userfoo/Docs` produces `~foo/Docs`. Also misbehaves when `HOME` is unset (`""` prefix-matches everything → every path gets a stray `~`).
3. **`main.rs` is 605 lines but CLAUDE.md describes it as "~185 lines (Coordinator)".** The coordinator promise no longer holds — activate-time wiring, lifecycle, and power-bar autodetect have all accumulated inline.

---

## The epic

**Title:** Epic: Code-quality refactor pass — make the code as polished as the app

**Body:**

> v0.3.0 shipped to crates.io stable. This epic catalogues the cleanup work that turns the first-cut implementation into a reference-quality Rust GTK4 codebase. Source: comprehensive multi-agent code review, May 2026 — see `docs/code-review-2026-05.md` for full findings.
>
> Goals: clean, idiomatic, conceptually sound, readable. No behavior changes that aren't fixing a bug. Each child issue is one PR.
>
> **Definition of done:** all child issues closed; `make lint` clean; CHANGELOG entry per [Keep a Changelog]; sonar dashboard shows no new issues.

Labels: `epic`, `refactor`

---

## Child issues

Issues are numbered for filing order (rough priority) but every issue is independent unless explicitly noted. Severity follows agent rubric: **high** = correctness/footgun/dead code, **med** = clarity/maintainability/perf, **low** = polish. Effort: S (<1h), M (half day), L (multi-day).

### Wave 1 — safety + dead code (do first, unblock everything)

#### Issue 1: Delete dead `src/ui/pinned.rs`

- **Severity:** high · **Effort:** S · **Labels:** `cleanup`, `dead-code`
- **Source:** A-F5, B-F1, C-F9, D-F2 (4-way confirmed)

`src/ui/pinned.rs` (135 lines) defines `build_pinned_flow_box` + `create_pinned_button` but is **not** declared in `src/ui/mod.rs:1-13` and not referenced anywhere. The live pin-row implementation is in `well_builder.rs::build_pinned_flow` + `build_pinned_button`. CLAUDE.md explicitly forbids dead code.

**Acceptance:** `git rm src/ui/pinned.rs`; `cargo build` clean; no references found via `rg pinned::`.

#### Issue 2: Fix `RefCell` re-entry fragility in category/search/pin-toggle callbacks

- **Severity:** high · **Effort:** S · **Labels:** `correctness`, `gtk4`
- **Source:** C-F1, C-F5, C-F12

Three places hold sequential `borrow_mut` calls across what could become a re-entry path. Today nothing crashes (each `borrow_mut()` is a per-statement temporary), but the pattern is one signal-emitter away from a panic:

- `ui/categories.rs:80-86` — two `borrow_mut` calls then `apply_category_filter` (which immediately re-borrows)
- `ui/search_handler.rs:13-48` — every line a separate `borrow_mut` inside a `connect_search_changed` (re-entrant via `set_text("")`)
- `ui/app_grid.rs:170-194` — right-click pin/unpin holds `borrow_mut` across `pinning::save_pinned` I/O

`well_builder.rs:240-251` already has the right pattern (scope the borrow, drop, then call I/O / rebuild). Mirror that.

Also: `ui/search_handler.rs:11` uses `Rc<RefCell<bool>>` for `in_search_mode` where `Rc<Cell<bool>>` would be panic-proof (`Cell` is `Copy`-only). `focus_pending` already uses `Cell` correctly — convert the search flag to match.

**Acceptance:** every callback that mutates state coalesces into a single short borrow scope; `in_search_mode` becomes `Cell<bool>`; new comment block in `state.rs` documents the "borrow → drop → rebuild" rule.

---

### Wave 2 — high-leverage tests + the latent bug

#### Issue 3: Coverage for `desktop_loader::load_desktop_entries`

- **Severity:** high · **Effort:** M · **Labels:** `tests`
- **Source:** E-F4

Three behaviors live in 72 untested lines: (a) first-occurrence-wins dedup across multiple app_dirs, (b) sort by lowercased localized name, (c) multi-category assignment via `assign_categories`. A regression where a later directory wins, or dedup is dropped, silently corrupts user `.desktop` overrides.

**Suggested fixtures:** `tempfile::TempDir` with two app dirs, `firefox.desktop` in each with different `Exec=`. Inject `NullCompositor` from `nwg-common` to dodge IPC.

**Acceptance:** three tests minimum — first-dir-wins; sort case-insensitive; multi-category fan-out.

#### Issue 4: Coverage for `file_search::walk_directory` (cap, exclusions, hidden dirs)

- **Severity:** high · **Effort:** M · **Labels:** `tests`, `correctness`
- **Source:** E-F3, E-F2

`walk_directory` is recursive with a `max_results` cap. A bug where the cap leaks past one level becomes "drawer freezes for 4 seconds on every keystroke" — exactly the kind of regression `cargo test` should catch. Same module also needs:

- `is_excluded` substring-match semantics pinned (currently `"node"` excludes `node_modules`)
- `file_type_icon` lookup table regression test

**Acceptance:** tempfile-backed walk fixture; cap respected at depth ≥3; excluded subtree skipped; chmod-000 dir doesn't panic; `is_excluded` empty-list returns false; `file_type_icon` cases including unknown ext.

#### Issue 5: Extract + test `shorten_home` (latent bug)

- **Severity:** med · **Effort:** S · **Labels:** `correctness`, `tests`, `refactor`
- **Source:** E-F8 (verified — see top of this doc)

`file_search.rs:206-212` does prefix matching that mishandles sibling directories and unset `HOME`. Extract to a pure `fn shorten_home(parent: &str, home: &str) -> String`, fix to require trailing `/` (or use `Path::strip_prefix`), test the sibling case explicitly.

**Acceptance:** new pure fn; test for sibling-dir non-match; test for empty `home` short-circuits; existing UI behavior preserved for the common case.

---

### Wave 3 — architecture (do these before piling on idiom changes)

#### Issue 6: Split `main.rs` — extract activation, lifecycle, power-bar autodetect

- **Severity:** high · **Effort:** M · **Labels:** `refactor`, `architecture`
- **Source:** B-F2

`main.rs` is 605 lines vs CLAUDE.md's "~185 line Coordinator." Three coherent concerns mash together:

1. **Lifecycle/singleton** (≈65 lines): `handle_open_close`, `handle_existing_instance`, `acquire_singleton_lock` → new `src/lifecycle.rs`
2. **GTK activate-time wiring** (≈290 lines, including the inline 71-line backdrop block and a 9-arg `activate_drawer` with `#[allow(too_many_arguments)]`) → either `src/activate.rs` or fold body into `ui::window`. Backdrop block becomes `setup_click_catcher_backdrops`.
3. **Power-bar autodetect** (≈70 lines): `auto_detect_power_bar`, `detect_lock`, `detect_command`, `command_on_path`, `can_suspend` → new `src/power_bar_detect.rs`

After: `main.rs` ≈ 150 lines doing `arg parsing → dirs → app.connect_activate → run`.

**Acceptance:** `main.rs` ≤ 200 lines; CLAUDE.md "What lives where" updated; the `#[allow(clippy::too_many_arguments)]` on `activate_drawer` gone.

#### Issue 7: Trim `ui/math.rs` — separate evaluator from widget + clipboard

- **Severity:** med · **Effort:** S · **Labels:** `refactor`, `architecture`
- **Source:** B-F4

`ui/math.rs` (493 lines) holds three separable concerns:

1. Pure evaluator (`DrawerOpsFactory` + `eval_expression` + `format_result` + 30 unit tests, ~150 lines) — load-bearing
2. GTK widget builder (`build_math_result`, `append_copy_button`, dynamic CSS via `static Once`, ~100 lines)
3. Clipboard subprocess (wl-copy spawn + 2-second timeout label, ~30 lines)

The dynamic `Once`-driven CSS injection is also out of step with the rest of the codebase (everything else uses `assets/drawer.css` + `include_str!`).

**Acceptance:** `ui/math.rs` ≤ 250 lines (evaluator + tests); new `ui/math_widget.rs` for builder + clipboard; math styles moved into `assets/drawer.css`; `Once` block removed.

#### Issue 8: Make `build_app_flow_box` take `&WellContext`

- **Severity:** med · **Effort:** S · **Labels:** `refactor`
- **Source:** B-F5

`WellContext` is honored at 13 call sites — but `build_app_flow_box` takes 8 individual params (with `#[allow(clippy::too_many_arguments)]`) that the three callers reassemble manually from `WellContext` fields. This is the exact smell `WellContext` was created to eliminate.

**Acceptance:** signature is `fn build_app_flow_box(ctx: &WellContext, category_filter: Option<&[String]>, search_phrase: &str) -> FlowBox` (or similar); three call sites collapse; `#[allow(too_many_arguments)]` removed.

#### Issue 9: Extract `WindowOp` command handling into `listeners/commands.rs`

- **Severity:** low · **Effort:** S · **Labels:** `refactor`
- **Source:** B-F3

`listeners.rs:241-368` already has 7 unit tests against `WindowOp` + `resolve_window_op` + `handle_window_command` — pure state-machine code with zero GTK deps. Lifting it into its own file separates "wire up GTK timeouts" (top half of `listeners.rs`) from "decide what to do with a window command" (bottom half).

**Acceptance:** new `src/listeners/commands.rs` (or `src/window_ops.rs`); `listeners.rs` shrinks to wiring; existing tests follow the code.

#### Issue 10: Move XDG user-dir helpers out of `state.rs`

- **Severity:** low · **Effort:** M · **Labels:** `refactor`
- **Source:** B-F10, D-F10, E-F1

40% of `state.rs` is one-shot XDG `user-dirs.dirs` parsing (`map_xdg_user_dirs` + `parse_user_dir_line`). One-shot config load is not state. First check `nwg_common::config::paths` for an upstream-able home; otherwise move to `src/xdg_dirs.rs`. Also: `parse_user_dir_line` discards parse-failure context (no log even at debug) — fix while moving.

**Acceptance:** `state.rs` is data-only; helpers either upstreamed or in their own module; `parse_user_dir_line` logs malformed lines at `debug`; new pure-function tests against the parser (combines E-F1 here so we get coverage in the same PR).

---

### Wave 4 — performance / hot paths

#### Issue 11: Move file-system walk off the GTK main thread (with debounce)

- **Severity:** med · **Effort:** M · **Labels:** `performance`, `gtk4`
- **Source:** C-F2, C-F10

`connect_search_changed` fires per keystroke. For each fire with `phrase.len() > 2`, `build_search_results` calls `file_search::search_files` synchronously, which recursively walks every XDG user dir. On a heavy `~/Downloads` this is multi-second blocking I/O on the GTK main thread — every keystroke. No cancellation: `"foo"` then `"foob"` queues a second walk before the first returns.

**Approach:** debounce 150 ms after last keystroke; run walk on a worker thread; generation counter so stale results from a prior phrase get dropped; deliver results via the channel pattern in Issue 12.

**Acceptance:** typing in a heavy dir tree no longer blocks the UI; new test confirms generation-counter drops stale results.

#### Issue 12: Replace `mpsc` + 100 ms-poll with `glib::MainContext::channel`

- **Severity:** med · **Effort:** S · **Labels:** `gtk4`, `performance`
- **Source:** C-F3

Both file watcher and signal poller use `std::sync::mpsc::Receiver` polled every 100 ms via `glib::timeout_add_local`. This adds up to 100 ms latency on every signal/file event, burns wakeups when idle, and is the exact use case `glib::MainContext::channel` solves. SIGRTMIN+1 toggle currently has up to 100 ms latency.

**Acceptance:** `watcher::start_watcher` and `signals::setup_signal_handlers` return a `glib`-attached receiver; `attach_local` callbacks fire as events arrive; the two `timeout_add_local` pollers go away; signal latency under 10 ms.

#### Issue 13: Reduce gratuitous clones in builder hot paths

- **Severity:** med · **Effort:** M · **Labels:** `performance`, `cleanup`
- **Source:** A-F1, A-F2, A-F9, A-F10, A-F13, A-F14, A-F16, A-F18, C-F11

Every well rebuild does `state.borrow().apps.entries.clone()`, `id2entry.clone()`, `pinned.clone()`, `app_dirs.clone()`, `gtk_theme_prefix.clone()`, `exclusions.clone()`, `preferred_apps.clone()`, `user_dirs.clone()`. Plus per-entry `app_dirs.clone()` and `gtk_theme_prefix.clone()` inside the loop. With hundreds of `.desktop` entries this is several KB-MB allocated per keystroke.

Bundle of related clean-ups:

- `app_grid.rs:24,92,144-145`, `well_builder.rs:155,164-165,218-219`, `pinned.rs:32-33,105-106` — hold a single immutable `Ref` across the iteration; hoist `app_dirs`/`gtk_theme_prefix` out of per-entry loops
- `well_context.rs:17` — `Rc<PathBuf>` → `Rc<Path>` (drops the `.as_ref().clone()` in `well_builder.rs:234`)
- `main.rs:78,80,103,107,109` — `Rc<Option<PathBuf>>` for `data_home` simplifies to `Option<&Path>` plumbed through
- `app_grid.rs:19, file_search.rs:15, pinned.rs:16, power_bar.rs:43` — `on_launch: Rc<dyn Fn()>` taken by value should be `&Rc<dyn Fn()>` (matches `build_button`'s pattern)
- `listeners.rs:362, search_handler.rs:14` — `entry.text().to_string()` materializes when `&str` would do (`GString` derefs to `&str`)
- `math.rs:245-260` — `format_result` allocates twice; bind `format!`, slice trim, single `String::from`
- `well_builder.rs:260-268` — outer + inner clone of `WellContext` inside `build_rebuild_callback`; outer is redundant

**Acceptance:** flame-graph or `cargo bench` shows fewer allocs per rebuild; behavior unchanged.

---

### Wave 5 — magic numbers + docs

#### Issue 14: Promote magic numbers to named constants

- **Severity:** med · **Effort:** S · **Labels:** `cleanup`, `docs`
- **Source:** D-F3, D-F4, D-F9

CLAUDE.md mandates "all UI dimensions in `ui/constants.rs`" and "no magic numbers." Stragglers found in:

UI dimensions:
- `categories.rs:10,69,71` — bar spacing, button spacing, icon size
- `file_search.rs:17,49,66,173` — vbox spacing, header separator margin, row spacing
- `widgets.rs:22` — button vbox spacing
- `app_grid.rs:124` — `APP_TOOLTIP_MAX_CHARS` (caller-side magic)
- `main.rs:225` — pinned-box top margin

Window-level:
- `main.rs:121-123` — `WINDOW_BG_RGB: (u8, u8, u8) = (22, 22, 30)` and `OPACITY_MAX_PERCENT: u8 = 100` (rgb triple is the most "magic" and currently only documented inside a `format!` string)

Mouse buttons:
- `app_grid.rs:171, well_builder.rs:237, main.rs:234` — three places use literal `3` for GDK right-button. Add `MOUSE_BUTTON_RIGHT: u32 = 3` (probably in `ui/constants.rs` or a tiny `gdk_buttons.rs`).

Durations (load-bearing — keep in `listeners.rs` so the tuning context stays close):
- `listeners.rs:135,153,187,297` — `ACTIVE_WINDOW_POLL_MS = 300`, `FILE_WATCHER_PUMP_MS = 100` (goes away with Issue 12), `SIGNAL_POLL_MS = 100` (also goes away with Issue 12), `FOCUS_FALLBACK_TIMEOUT_MS = 200` (with the existing inline rationale moved to a doc comment)

Hygiene:
- `ui/constants.rs` itself: add `// --- Search bar ---`-style section banners; split the dual-doc'd `MATH_BUTTON_PADDING_V`/`_H` so each has its own `///` line.

**Acceptance:** zero remaining numeric literals in `src/ui/*.rs` that name UI dimensions; clippy `unwarranted_dim_literals` (informally, manual review) clean.

#### Issue 15: Add `//!` module-level docs to every source file

- **Severity:** med · **Effort:** M · **Labels:** `docs`
- **Source:** D-F1, D-F5

Only `ui/constants.rs` has a `//!` block today. `cargo doc` and grep-the-source readers land on bare `use` statements. Priority files (highest payoff first):

- `main.rs` — entry point + activation lifecycle
- `listeners.rs` — the four glib loops + `focus_pending` handshake
- `well_builder.rs` — rebuild matrix (search/category/normal), and **delete the misplaced "Grid navigation" banner at lines 270-277** (it documents code that lives in `navigation.rs`)
- `navigation.rs` — capture-phase rationale (relocate that banner here)
- `state.rs` — `RefCell` discipline + `active_search` precedence rule
- `ui/math.rs` — pure-evaluator-vs-widget contract
- everything else: 2-4 line summaries

**Acceptance:** every `src/*.rs` and `src/ui/*.rs` opens with a `//!` block; `cargo doc --no-deps --document-private-items` produces useful module pages.

#### Issue 16: Add missing `///` docs and WHY-comments at three subtle spots

- **Severity:** med · **Effort:** S · **Labels:** `docs`
- **Source:** D-F6, D-F8, D-F11, A-F8, A-F15

Targeted, not bulk. Specifically:

- `state.rs::DrawerState::new` — note that `compositor` is shared with the launch path
- `well_builder.rs::build_rebuild_callback` — explain the `idle_add_local_once` deferral and why (avoid re-entrant `borrow_mut` panics — ties to Issue 2)
- `main.rs:25` (`nwg_common::process::handle_dump_args()`) — comment that this is the singleton-handshake hook, must run before clap parses
- `well_builder.rs:111` — `saturating_sub(2)` → "minus header label + separator added by `search_files`"
- `math.rs:236-243` — explain the `1e15` and `1e-4` thresholds (the `0.5e-6` one is already well-commented)
- `listeners.rs:285-302` — link the show-then-grab-focus dance to the `is_active_notify` handler at `:114`
- `ui/math.rs:69-72,77-82` (`MathResult::NotMath`) — `log::trace!` parse and eval errors so they're recoverable behind `RUST_LOG=trace`
- `state.rs:85-103` (`parse_user_dir_line`) — `log::debug!` on skipped lines (covered in Issue 10)

**Acceptance:** the seven cited lines have one-line WHY comments; the trace logging fires under `RUST_LOG=trace`.

---

### Wave 6 — refactor consolidation

#### Issue 17: Consolidate three duplicated patterns into helpers

- **Severity:** med · **Effort:** S · **Labels:** `refactor`, `cleanup`
- **Source:** A-F3, A-F4, A-F6, A-F7

Three near-identical patterns sit in three files each:

1. **Pin/unpin with rollback** (`app_grid.rs:172-194`, `well_builder.rs:238-252`, `pinned.rs:120-131` — last one goes away with Issue 1) — extract `fn toggle_pin_with_save(state: &mut DrawerState, id: &str, path: &Path) -> io::Result<bool>` that rolls back internally
2. **Localized name/desc fallback** (`app_grid.rs:93-102`, `well_builder.rs:194-203`, `pinned.rs:81-90`) — extract `fn display_name(e: &DesktopEntry) -> &str` (consider upstreaming to `nwg-common`)
3. **`while let Some(child) = container.first_child() { container.remove(...) }`** (three places) — `well_builder.rs` already has private `clear_box`; promote to `pub(super)` or `ui::dom::clear_box`
4. **Manual sibling-walk for child counting** (`navigation.rs:305-313`, `well_builder.rs:294-302`) — replace both with `widget.observe_children().n_items()`

**Acceptance:** four helpers extracted; net LOC reduction; no behavior change.

#### Issue 18: Idiomatic-Rust polish bundle

- **Severity:** low · **Effort:** S · **Labels:** `cleanup`
- **Source:** A-F11, A-F17, A-F19, A-F20, B-F6, C-F4, C-F6, C-F7, C-F8

Bundle of small wins. Each is a one-liner-ish; if a maintainer prefers smaller PRs, this can split:

- `main.rs:586-596` — `command_on_path` should use `std::env::split_paths` (handles empty PATH segments and `OsString` losslessly)
- `main.rs:121` — `f64::from(config.opacity.min(100)) / 100.0` reads cleaner than the `as f64` cast
- `watcher.rs:30,44` — `let _ = tx.send(...)` should `return` on `SendError` (dropped receiver = exit watcher thread, don't keep running)
- `main.rs:26` — `parse_from(args: impl Iterator<Item = String>)` allocates a `Vec<String>` only to re-iterate; thread the iterator if `nwg_common::config::flags::normalize_legacy_flags` exposes one
- **`pub` → `pub(crate)`/`pub(super)` sweep** across the binary-only crate. Reserve plain `pub` for things you'd export if this becomes a lib. (B-F6)
- `main.rs:189-220` — backdrops `connect_visible_notify` over-captures strong refs; downgrade to `Vec<glib::WeakRef<ApplicationWindow>>` or move into a single `SignalHandlerId` cell
- `categories.rs:16,128-132` — drop the `Rc<RefCell<Vec<Button>>>`; `select_button(active)` walks `active.parent()`'s children
- `math.rs:201-227` — `pending_timer` closure should capture `WeakRef` not strong refs (cosmetic; benign with widget clones)
- `listeners.rs:53-86` — capture-phase keyboard handler grabs focus on modifier-only key presses; ignore `keyval.is_modifier_key()`

**Acceptance:** `make lint` clean; nothing perf-regresses.

#### Issue 19: Naming + minor renames

- **Severity:** low · **Effort:** S · **Labels:** `cleanup`
- **Source:** D-F7, B-F7

- `ui::search::setup_search_entry` → `build_search_entry` (matches `build_app_flow_box`, `build_category_bar`, `build_pinned_flow`, `build_power_bar`)
- `listeners.rs::clear_drawer_state` (5-line fn, 1-line body, 1 caller) — inline at the call site or rename to `clear_search_text`
- Fold `ui/search.rs` (1-line widget builder + `subsequence_match` algorithm) into either `ui/app_grid.rs` (the only consumer of `subsequence_match`) or rename to `ui/matching.rs`. Rationale: today's `search.rs` and `search_handler.rs` invite "which one do I use?" every time.

**Acceptance:** every renamed symbol unique-grep-replaced; no dangling references.

---

### Wave 7 — coverage tests (after architecture lands)

#### Issue 20: Refactor + test `auto_detect_power_bar` priority rules

- **Severity:** med · **Effort:** M · **Labels:** `refactor`, `tests`
- **Source:** E-F6

`detect_lock` tries `hyprlock`, then `swaylock`, then `swaylock-effects`; `detect_command` tries `loginctl <action>` then `systemctl <action>`. The "explicit flag wins, slot only fills if empty" rule is the README contract. A regression where `detect_command` overwrites a pre-set slot silently overrides user config.

**Approach:** extract `fn pick_first_present(slot: &mut Option<String>, candidates: &[&str], probe: impl Fn(&str) -> bool)`. Tests inject the probe. The existing `command_on_path` becomes the default probe; `pick_first_present` is pure and testable. Combines well with Issue 6 (which moves these into `power_bar_detect.rs` anyway).

**Acceptance:** four pure-function tests (pre-filled slot preserved; first match wins; no match → empty slot; empty candidate list).

#### Issue 21: Doctests for `subsequence_match` and `truncate`

- **Severity:** low · **Effort:** S · **Labels:** `tests`, `docs`
- **Source:** E-F9

Both are pure stringly-typed helpers that already have unit tests. A `///` doctest doubles as documentation for callers. `truncate` has a unicode-vs-byte gotcha that doctests can demonstrate clearly.

**Acceptance:** `cargo test` exercises the doctests; both functions render usefully in `cargo doc`.

#### Issue 22 (optional): `--dump-args` integration test

- **Severity:** low · **Effort:** M · **Labels:** `tests`
- **Source:** E-F5

The drawer has one CLI side-effect that exits before GTK init: `--dump-args <pid>` (handled at `main.rs:25`). A drawer-level integration test that builds the binary and runs `nwg-drawer --dump-args $$` would catch the regression class where someone accidentally calls into GTK before the dump path is processed (real concern — the ordering at `main.rs:25` is load-bearing). nwg-common already has unit-level coverage; this is the binary-level smoke.

**Acceptance:** `tests/dump_args.rs` exists; runs in CI via the existing `cargo test` workflow; uses `env!("CARGO_BIN_EXE_nwg-drawer")`.

---

## Suggested execution order

The waves above are in priority order. Suggested iteration:

1. **Wave 1** clears safety issues and dead code → unblocks everything else (Issues 1–2)
2. **Wave 2** lands the high-value tests + the latent `shorten_home` bug fix while we have the test fixture mindset (Issues 3–5)
3. **Wave 3** is the architectural reshape — `main.rs` split, math.rs trim, `WellContext` consistency, listener split, XDG move (Issues 6–10). Do this before Wave 4-6 because subsequent issues touch files this wave moves around.
4. **Wave 4** is hot-path performance (Issues 11–13). Issue 13 is the biggest in this wave; it can split into 3 PRs if a maintainer wants.
5. **Wave 5** is magic-number sweeps + module docs (Issues 14–16) — small, low-risk, high readability payoff
6. **Wave 6** is consolidation + polish (Issues 17–19)
7. **Wave 7** is coverage backfill + the optional integration test (Issues 20–22)

Estimated total effort:
- 13 × S (~13h) — Issues 1, 2, 5, 7, 8, 9, 12, 14, 16, 17, 18, 19, 21
- 9 × M (~36h) — Issues 3, 4, 6, 10, 11, 13, 15, 20, 22
- 0 × L

Total: ~49h, or 5–10 working days of focused refactoring.

## How to file

After review, each issue can be filed via:

```bash
gh issue create \
  --title "<issue title>" \
  --label "<labels>" \
  --body "$(sed -n '/^#### Issue N:/,/^---$/p' docs/code-review-2026-05.md)"
```

Or driven by a small script that walks this doc. Each filed issue should link back to this document (`docs/code-review-2026-05.md`) for full context.
