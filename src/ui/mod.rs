//! UI submodules for the drawer.
//!
//! Every GTK widget the drawer constructs lives somewhere under here.
//! Submodules are organized by feature area (app grid, pinned row,
//! categories, file search, math, power bar) plus a few cross-cutting
//! helpers (constants, navigation, well_builder, well_context, widgets).
//! See `CLAUDE.md` for the WellContext convention that ties them
//! together.

pub mod app_grid;
pub mod categories;
pub mod constants;
pub mod file_search;
pub mod math;
pub mod math_widget;
pub mod navigation;
pub mod power_bar;
pub mod search;
pub mod search_handler;
pub mod well_builder;
pub mod well_context;
pub mod widgets;
pub mod window;
