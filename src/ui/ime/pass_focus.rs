//! Keyboard focus as each window last left it.
//!
//! Raw input is prepared before egui starts the window's pass, while egui's memory still
//! answers for whichever window ran last. With several windows (a hidden root that keeps
//! drawing, the Help window) that is often another window, whose focus would make the IME
//! code think the editor lost focus and cancel the preedit.

use super::super::*;
use std::collections::HashMap;

/// The window that ran last and the focus each window had at the end of its pass.
#[derive(Clone, Default)]
struct PassFocus {
    last_viewport: Option<egui::ViewportId>,
    focus: HashMap<egui::ViewportId, Option<Id>>,
}

/// Where the record lives in egui's shared data.
fn pass_focus_id() -> Id {
    Id::unique("texcel_pass_focus")
}

/// Records each window's focus at the end of every pass, once per egui context.
pub(in crate::ui) fn track_pass_focus(ctx: &egui::Context) {
    ctx.on_end_pass(
        "texcel_pass_focus",
        Arc::new(|ui: &mut egui::Ui| {
            let ctx = ui.ctx();
            let viewport = ctx.viewport_id();
            let focused = ctx.memory(|memory| memory.focused());
            ctx.data_mut(|data| {
                let record = data.get_temp_mut_or_default::<PassFocus>(pass_focus_id());
                record.last_viewport = Some(viewport);
                record.focus.insert(viewport, focused);
            });
        }),
    );
}

/// Tells whether egui's memory still answers for another window than `viewport`.
pub(in crate::ui) fn memory_is_for_another_window(
    ctx: &egui::Context,
    viewport: egui::ViewportId,
) -> bool {
    ctx.data(|data| data.get_temp::<PassFocus>(pass_focus_id()))
        .and_then(|record| record.last_viewport)
        .is_some_and(|last| last != viewport)
}

/// Returns the focus of `viewport` while its raw input is prepared.
pub(in crate::ui) fn focus_before_pass(
    ctx: &egui::Context,
    viewport: egui::ViewportId,
) -> Option<Id> {
    if memory_is_for_another_window(ctx, viewport) {
        ctx.data(|data| data.get_temp::<PassFocus>(pass_focus_id()))
            .and_then(|record| record.focus.get(&viewport).copied())
            .flatten()
    } else {
        ctx.memory(|memory| memory.focused())
    }
}
