use super::*;

use crate::document::DocumentDisplayMode;

/// Space between the formula bar and the narrow live/source bar.
pub(super) const DISPLAY_MODE_BAR_GAP: f32 = 6.0;

/// The two modes in the order the bar shows them, with their short and full names.
const MODES: [(DocumentDisplayMode, &str, &str); 2] = [
    (DocumentDisplayMode::Live, "view-live-short", "view-live"),
    (
        DocumentDisplayMode::Source,
        "view-source-short",
        "view-source",
    ),
];

impl TexcelApp {
    /// Returns the outer width of the live/source bar, or zero when no table is open.
    pub(super) fn display_mode_bar_width(&self, ui: &egui::Ui) -> f32 {
        if self
            .workspace
            .active_document()
            .and_then(|document| document.active_table())
            .is_none()
        {
            return 0.0;
        }
        let buttons = MODES
            .iter()
            .map(|(_, short, _)| chrome::command_width(ui, &context_text(ui.ctx(), short)))
            .sum::<f32>();
        buttons + f32::from(CHROME_EDGE_INSET) * 2.0 + DISPLAY_MODE_BAR_GAP
    }

    /// Draws the narrow bar right of the formula bar that switches live and source editing.
    pub(super) fn draw_display_mode_bar(&mut self, ui: &mut egui::Ui, navigation_blocked: bool) {
        let Some((document_id, table_id, current)) =
            self.workspace.active_document().and_then(|document| {
                let table = document.active_table()?;
                Some((document.id, table.id, document.display_mode))
            })
        else {
            return;
        };
        let shortcut = ui.ctx().format_shortcut(&egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND,
            egui::Key::Slash,
        ));
        let help = context_text(ui.ctx(), "view-mode-help");
        let mut chosen = None;
        egui::Frame::new()
            .fill(ui.visuals().extreme_bg_color)
            .inner_margin(egui::Margin::same(CHROME_EDGE_INSET))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add_enabled_ui(!navigation_blocked, |ui| {
                    ui.horizontal(|ui| {
                        for (mode, short, full) in MODES {
                            let visible = context_text(ui.ctx(), short);
                            let name = context_text(ui.ctx(), full);
                            let response = ui
                                .add(
                                    egui::Button::selectable(mode == current, visible.clone())
                                        .min_size(Vec2::new(
                                            chrome::command_width(ui, &visible),
                                            CHROME_CONTROL_HEIGHT,
                                        )),
                                )
                                .accessible_name(name.clone())
                                .on_hover_text(format!("{name} ({shortcut})\n{help}"));
                            if response.clicked() && mode != current {
                                chosen = Some(mode);
                            }
                        }
                    });
                });
            });
        if let Some(mode) = chosen {
            self.set_display_mode(document_id, table_id, mode, ui.ctx());
        }
    }

    /// Switches every table in a document between live and source editing.
    pub(super) fn set_display_mode(
        &mut self,
        document_id: DocumentId,
        table_id: TableId,
        mode: DocumentDisplayMode,
        ctx: &egui::Context,
    ) {
        if let Some(document) = self.workspace.documents.get_mut(&document_id) {
            document.display_mode = mode;
            self.mark_document_preference_change(
                document_id,
                table_id,
                TablePreferenceField::DisplayMode,
            );
            ctx.request_repaint();
        }
    }
}
