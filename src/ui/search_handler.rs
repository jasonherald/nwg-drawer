use crate::ui::well_builder;
use crate::ui::well_context::WellContext;
use gtk4::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

/// Connects the search entry to the well, handling search/clear/command modes.
pub fn connect_search(ctx: &WellContext) {
    let search_entry = ctx.search_entry.clone();
    let ctx = ctx.clone();
    // `Cell` (not `RefCell`) — the flag is `Copy`, so reads/writes can never
    // panic on overlapping borrows (matches `focus_pending` in listeners.rs).
    let in_search_mode = Rc::new(Cell::new(false));

    search_entry.connect_search_changed(move |entry| {
        // `GString` derefs to `&str`, so the body can pattern-match and
        // strip prefixes on a borrowed slice — we only allocate a
        // `String` at the one site that needs owned data
        // (`active_search` in DrawerState).
        let phrase_gs = entry.text();
        let phrase: &str = &phrase_gs;

        if phrase.is_empty() {
            if in_search_mode.get() {
                in_search_mode.set(false);
                ctx.state.borrow_mut().active_search.clear();
                well_builder::restore_normal_well(&ctx);
            }
            ctx.status_label.set_text("");
            return;
        }

        in_search_mode.set(true);

        // Command mode (: prefix) — clear search state so rebuilds don't restore stale results
        if let Some(cmd_text) = phrase.strip_prefix(':') {
            ctx.state.borrow_mut().active_search.clear();
            while let Some(child) = ctx.well.first_child() {
                ctx.well.remove(&child);
            }
            ctx.pinned_box.set_visible(false);
            if cmd_text.is_empty() {
                ctx.status_label.set_text("Execute a command");
            } else {
                ctx.status_label
                    .set_text(&format!("Execute \"{}\"", cmd_text));
            }
            return;
        }

        // Search mode — track in state and show matching apps + files
        ctx.state.borrow_mut().active_search = phrase.to_string();
        well_builder::build_search_results(&ctx, phrase);
    });
}
