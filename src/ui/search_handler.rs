use crate::ui::well_builder;
use crate::ui::well_context::WellContext;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Connects the search entry to the well, handling search/clear/command modes.
pub fn connect_search(ctx: &WellContext) {
    let search_entry = ctx.search_entry.clone();
    let ctx = ctx.clone();
    let in_search_mode = Rc::new(RefCell::new(false));

    search_entry.connect_search_changed(move |entry| {
        let phrase = entry.text().to_string();

        if phrase.is_empty() {
            if *in_search_mode.borrow() {
                *in_search_mode.borrow_mut() = false;
                ctx.state.borrow_mut().active_search.clear();
                well_builder::restore_normal_well(&ctx);
            }
            ctx.status_label.set_text("");
            return;
        }

        *in_search_mode.borrow_mut() = true;

        // Command mode (: prefix) — clear search state so rebuilds don't restore stale results
        if phrase.starts_with(':') {
            ctx.state.borrow_mut().active_search.clear();
            while let Some(child) = ctx.well.first_child() {
                ctx.well.remove(&child);
            }
            ctx.pinned_box.set_visible(false);
            if phrase.len() > 1 {
                let cmd_text = phrase.strip_prefix(':').unwrap_or(&phrase);
                ctx.status_label
                    .set_text(&format!("Execute \"{}\"", cmd_text));
            } else {
                ctx.status_label.set_text("Execute a command");
            }
            return;
        }

        // Search mode — track in state and show matching apps + files
        ctx.state.borrow_mut().active_search = phrase.clone();
        well_builder::build_search_results(&ctx, &phrase);
    });
}
