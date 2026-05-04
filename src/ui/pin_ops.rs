//! Pin-file mutations with rollback semantics.
//!
//! Both pin and unpin paths in the drawer follow the same shape:
//! mutate the in-memory pinned list under a tight `borrow_mut`,
//! snapshot the result, drop the borrow, persist outside any borrow,
//! and roll back the in-memory mutation if persistence fails. This
//! module centralizes that pattern so the call sites can stay narrow.

use crate::state::DrawerState;
use nwg_common::pinning;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

/// Toggles `id`'s pin state in `state`, persists the new pinned list
/// to `path`, and rolls back the in-memory mutation if persistence
/// fails.
///
/// On `Ok`, returns the *previous* pin state (`true` = `id` was pinned
/// before this call, so this call unpinned it). Callers use it for
/// log-line wording — "Pinned X" vs "Unpinned X".
///
/// On `Err`, the in-memory pinned list is restored to its pre-call
/// order — including the original position when re-pinning a removed
/// item, so a save failure doesn't silently reorder the user's pinned
/// row. Callers don't need to undo anything.
pub(super) fn toggle_pin_with_save(
    state: &Rc<RefCell<DrawerState>>,
    id: &str,
    path: &Path,
) -> std::io::Result<bool> {
    // Phase 1: capture rollback context, mutate, snapshot, release.
    let (was_pinned, original_pos, snapshot) = {
        let mut s = state.borrow_mut();
        let was_pinned = pinning::is_pinned(&s.pinned, id);
        // Position is only meaningful for the unpin path — re-pinning
        // never needs a row-position to restore to.
        let original_pos = if was_pinned {
            s.pinned.iter().position(|p| p == id)
        } else {
            None
        };
        if was_pinned {
            pinning::unpin_item(&mut s.pinned, id);
        } else {
            pinning::pin_item(&mut s.pinned, id);
        }
        (was_pinned, original_pos, s.pinned.clone())
    };

    // Phase 2: I/O outside any borrow — a re-entrant signal during
    // save can't deadlock against state.
    if let Err(e) = pinning::save_pinned(&snapshot, path) {
        let mut s = state.borrow_mut();
        if was_pinned {
            // Re-insert at original position so a save failure doesn't
            // silently reorder the user's pinned row.
            if let Some(pos) = original_pos {
                s.pinned.insert(pos, id.to_string());
            } else {
                s.pinned.push(id.to_string());
            }
        } else {
            // Wasn't pinned before; remove what we just appended.
            pinning::unpin_item(&mut s.pinned, id);
        }
        return Err(e);
    }
    Ok(was_pinned)
}
