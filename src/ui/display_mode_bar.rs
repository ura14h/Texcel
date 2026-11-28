use super::*;

use crate::document::DocumentDisplayMode;

#[cfg(test)]
#[path = "display_mode_bar_test.rs"]
mod tests;

/// Width of the faint line between the formula bar and the live/source switch.
pub(super) const DISPLAY_MODE_BAR_GAP: f32 = 1.0;

/// The button label for each mode, and the tooltip naming the mode a click switches to.
fn mode_keys(mode: DocumentDisplayMode) -> (&'static str, &'static str) {
    match mode {
        DocumentDisplayMode::Live => ("view-live", "view-switch-to-source"),
        DocumentDisplayMode::Source => ("view-source", "view-switch-to-live"),
    }
}

/// Width of the switch: the wider of its two labels, so it does not jump when clicked.
fn switch_width(ui: &egui::Ui) -> f32 {
    [DocumentDisplayMode::Live, DocumentDisplayMode::Source]
        .into_iter()
        .map(|mode| chrome::command_width(ui, &context_text(ui.ctx(), mode_keys(mode).0)))
        .fold(0.0, f32::max)
}

impl TexcelApp {
    /// Returns the outer width of the live/source switch, or zero when no table is open.
    pub(super) fn display_mode_bar_width(&self, ui: &egui::Ui) -> f32 {
        if self
            .workspace
            .active_document()
            .and_then(|document| document.active_table())
            .is_none()
        {
            return 0.0;
        }
        switch_width(ui) + f32::from(CHROME_EDGE_INSET) * 2.0 + DISPLAY_MODE_BAR_GAP
    }

    /// Draws the one button right of the formula bar that names the current mode and
    /// switches to the other one.
    pub(super) fn draw_display_mode_bar(&mut self, ui: &mut egui::Ui, navigation_blocked: bool) {
        let Some((document_id, current)) = self.workspace.active_document().and_then(|document| {
            document
                .active_table()
                .map(|_| (document.id, document.display_mode))
        }) else {
            return;
        };
        let shortcut = ui.ctx().format_shortcut(&egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND,
            egui::Key::Slash,
        ));
        let (label_key, switch_key) = mode_keys(current);
        let label = context_text(ui.ctx(), label_key);
        let hover = format!(
            "{} ({shortcut})\n{}",
            context_text(ui.ctx(), switch_key),
            context_text(ui.ctx(), "view-mode-help")
        );
        let mut clicked = false;
        egui::Frame::new()
            .fill(ui.visuals().extreme_bg_color)
            .inner_margin(egui::Margin::same(CHROME_EDGE_INSET))
            .show(ui, |ui| {
                ui.add_enabled_ui(!navigation_blocked, |ui| {
                    clicked = ui
                        .add(
                            egui::Button::new(label)
                                .min_size(Vec2::new(switch_width(ui), CHROME_CONTROL_HEIGHT)),
                        )
                        .on_hover_text(hover)
                        .clicked();
                });
            });
        if clicked {
            let next = match current {
                DocumentDisplayMode::Live => DocumentDisplayMode::Source,
                DocumentDisplayMode::Source => DocumentDisplayMode::Live,
            };
            self.set_display_mode(document_id, next, ui.ctx());
        }
    }

    /// Switches every table in a document between live and source editing.
    pub(super) fn set_display_mode(
        &mut self,
        document_id: DocumentId,
        mode: DocumentDisplayMode,
        ctx: &egui::Context,
    ) {
        if let Some(document) = self.workspace.documents.get_mut(&document_id) {
            document.display_mode = mode;
            ctx.request_repaint();
        }
    }
}
