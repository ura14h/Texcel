use super::*;

#[cfg(test)]
#[path = "sheet_zoom_dialog_test.rs"]
mod tests;

/// Preset sheet zooms offered before the custom value, in percent.
const SHEET_ZOOM_PRESETS: [u32; 5] = [200, 100, 75, 50, 25];

/// The zoom dialog's current choice: one preset or the custom percent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct SheetZoomDialogState {
    /// The chosen preset, or `None` when the custom value is chosen.
    preset: Option<u32>,
    /// The custom zoom in whole percent, kept even while a preset is chosen.
    custom_percent: u32,
}

impl SheetZoomDialogState {
    /// Starts on the preset that matches the current zoom, or on the custom value.
    pub(super) fn new(zoom: f32) -> Self {
        let percent = zoom_percent(zoom);
        Self {
            preset: SHEET_ZOOM_PRESETS.contains(&percent).then_some(percent),
            custom_percent: percent,
        }
    }

    /// Returns the zoom the dialog would apply.
    pub(super) fn zoom(&self) -> f32 {
        let percent = self.preset.unwrap_or(self.custom_percent);
        clamp_sheet_zoom(percent as f32 / 100.0)
    }
}

/// Converts a zoom factor into the whole percent shown to the user.
pub(super) fn zoom_percent(zoom: f32) -> u32 {
    (clamp_sheet_zoom(zoom) * 100.0).round() as u32
}

impl TexcelApp {
    /// Opens the zoom dialog for this window's sheet.
    pub(super) fn open_sheet_zoom_dialog(&mut self) {
        self.sheet_zoom_dialog = Some(SheetZoomDialogState::new(self.sheet_zoom.0));
    }

    /// Draws the sheet zoom choices and applies the chosen zoom to this window.
    pub(super) fn draw_sheet_zoom_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.sheet_zoom_dialog else {
            return;
        };
        let modal_id = Id::unique("sheet_zoom_dialog");
        let apply_key =
            self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Enter, self.edit.composing);
        let cancel_key =
            self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Escape, self.edit.composing);
        let mut apply = false;
        let mut cancel = false;
        egui::Modal::new(modal_id).show(ctx, |ui| {
            ui.set_min_width(240.0);
            ui.set_max_width(400.0);
            visual::dialog_surface(ui, |ui| {
                visual::prepare_dialog_body(ui);
                visual::dialog_title(ui, &context_text(ui.ctx(), "dialog-sheet-zoom-title"));
                for percent in SHEET_ZOOM_PRESETS {
                    ui.radio_value(&mut dialog.preset, Some(percent), format!("{percent}%"));
                }
                ui.horizontal(|ui| {
                    ui.radio_value(
                        &mut dialog.preset,
                        None,
                        context_text(ui.ctx(), "dialog-sheet-zoom-custom"),
                    );
                    let min = zoom_percent(MIN_SHEET_ZOOM);
                    let max = zoom_percent(MAX_SHEET_ZOOM);
                    let custom = ui.add(
                        egui::DragValue::new(&mut dialog.custom_percent)
                            .range(min..=max)
                            .speed(1.0)
                            .suffix("%"),
                    );
                    // Editing the custom value chooses it.
                    if custom.changed() || custom.gained_focus() {
                        dialog.preset = None;
                    }
                });
                visual::caption(
                    ui,
                    &context_message(
                        ui.ctx(),
                        &Message::new("dialog-sheet-zoom-range")
                            .with_number("min", i64::from(zoom_percent(MIN_SHEET_ZOOM)))
                            .with_number("max", i64::from(zoom_percent(MAX_SHEET_ZOOM))),
                    ),
                );
                visual::group_gap(ui);
                ui.separator();
                visual::dialog_footer(ui, |ui| {
                    let apply_label = context_text(ui.ctx(), "action-apply");
                    let cancel_label = context_text(ui.ctx(), "action-cancel");
                    // The footer is laid out right to left, so the first button is rightmost.
                    #[cfg(target_os = "macos")]
                    {
                        apply |= visual::dialog_button(ui, apply_label, 112.0, true);
                        cancel |= visual::dialog_button(ui, cancel_label, 112.0, false);
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        cancel |= visual::dialog_button(ui, cancel_label, 112.0, false);
                        apply |= visual::dialog_button(ui, apply_label, 112.0, true);
                    }
                });
            });
        });
        if cancel || cancel_key {
            self.sheet_zoom_dialog = None;
            return;
        }
        if apply || apply_key {
            self.sheet_zoom_dialog = None;
            self.set_sheet_zoom(dialog.zoom());
            self.request_grid_focus = true;
            return;
        }
        self.sheet_zoom_dialog = Some(dialog);
    }
}
