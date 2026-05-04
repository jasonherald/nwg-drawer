//! UI submodules for the drawer.
//!
//! Every GTK widget the drawer constructs lives somewhere under here.
//! Submodules are organized by feature area (app grid, pinned row,
//! categories, file search, math, power bar) plus a few cross-cutting
//! helpers (constants, navigation, well_builder, well_context, widgets).
//! See `CLAUDE.md` for the WellContext convention that ties them
//! together.
//!
//! All submodules are `pub(crate)`: the drawer is a binary-only crate,
//! so plain `pub` would falsely advertise an external API. Switching
//! to a lib in the future would lift these intentionally.

pub(crate) mod app_grid;
pub(crate) mod categories;
pub(crate) mod constants;
pub(crate) mod file_search;
pub(crate) mod math;
pub(crate) mod math_widget;
pub(crate) mod navigation;
pub(crate) mod pin_ops;
pub(crate) mod power_bar;
pub(crate) mod search;
pub(crate) mod search_handler;
pub(crate) mod well_builder;
pub(crate) mod well_context;
pub(crate) mod widgets;
pub(crate) mod window;
