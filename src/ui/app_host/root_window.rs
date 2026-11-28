//! Keeps the root window showing a document while any document is open.
//!
//! eframe draws child windows from the root window's pass, and the root cannot close without
//! ending the app. A root that hid while other documents stayed open stopped drawing, so new
//! document windows were never created. Like other multi-window egui apps (WordCraft, for
//! one), the root instead takes over another open document whenever its own goes away, and
//! hides only when no document is left (macOS keeps running with its menu bar).

use super::*;

impl HostRuntime {
    /// Brings the window assignment up to date and, when the root has lost its document while
    /// others are open, moves one of them into the root window.
    pub(super) fn settle_root_window(&mut self, ctx: &egui::Context) {
        self.sync_document_viewports();
        if self.root_document.is_some() {
            return;
        }
        let Some(document_id) = self.root_candidate() else {
            return;
        };
        let child = self.document_viewports[&document_id];
        self.adopt_window_state(child, document_id);
        self.document_viewports
            .insert(document_id, self.root_viewport);
        self.root_document = Some(document_id);
        self.retired_viewports.insert(child);
        // The root takes the place of the window it replaces, so the document stays put.
        let child_info = ctx.input(|input| input.raw.viewports.get(&child).cloned());
        if let Some(info) = child_info {
            if let Some(outer) = info.outer_rect {
                ctx.send_viewport_cmd_to(
                    self.root_viewport,
                    egui::ViewportCommand::OuterPosition(outer.min),
                );
            }
            if let Some(inner) = info.inner_rect {
                ctx.send_viewport_cmd_to(
                    self.root_viewport,
                    egui::ViewportCommand::InnerSize(inner.size()),
                );
            }
        }
        ctx.send_viewport_cmd_to(self.root_viewport, egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd_to(self.root_viewport, egui::ViewportCommand::Focus);
        ctx.request_repaint_of(self.root_viewport);
    }

    /// Picks the document the root takes over: the one about to come forward, or else the
    /// most recently opened. A document still having its columns fitted on opening waits, so
    /// the root never shows it before it is ready.
    fn root_candidate(&self) -> Option<DocumentId> {
        let has_window = |document_id: &DocumentId| {
            self.document_viewports
                .get(document_id)
                .is_some_and(|viewport| *viewport != self.root_viewport)
                && !self.app.opening_fits.contains_key(document_id)
        };
        self.app
            .pending_document_focus
            .filter(has_window)
            .or_else(|| {
                self.app
                    .workspace
                    .order
                    .iter()
                    .rev()
                    .copied()
                    .find(has_window)
            })
    }

    /// Moves a child window's own state (selection, edit, prompts) into the root slot,
    /// keeping app-wide dialogs the root had open while it held no document.
    fn adopt_window_state(&mut self, child: egui::ViewportId, document_id: DocumentId) {
        let mut state = self.app.window_states.remove(&child).unwrap_or_default();
        if let Some(old) = self.app.window_states.remove(&self.root_viewport) {
            state.settings_window_open |= old.settings_window_open;
            state.about_dialog_open |= old.about_dialog_open;
            state.app_close_dialog_open |= old.app_close_dialog_open;
        }
        state.document_id = Some(document_id);
        self.app.window_states.insert(self.root_viewport, state);
        for owner in self.app.load_request_viewports.values_mut() {
            if *owner == child {
                *owner = self.root_viewport;
            }
        }
    }

    /// Moves the opening of loaded documents forward: their column fits do not depend on any
    /// window, so they run here whether or not the root is shown.
    pub(super) fn advance_opening(&mut self, ctx: &egui::Context) {
        self.app.advance_opening_fits(ctx);
        self.app.refresh_opening_progress();
        if self.app.has_opening_documents() {
            // Check again while documents open: for a free worker slot, a finished
            // calculation, and when the progress window is due.
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }
}
