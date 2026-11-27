use super::*;

impl TexcelApp {
    /// Collects the content and output format for one clipboard copy.
    pub(super) fn draw_copy_options_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut options) = self.copy_options_dialog.take() else {
            return;
        };
        let mut copy = false;
        let mut close = false;
        let modal_id = Id::unique("copy_options_dialog");
        let copy_key =
            self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Enter, self.edit.composing);
        let close_key =
            self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Escape, self.edit.composing);
        egui::Modal::new(modal_id).show(ctx, |ui| {
            // Wide enough for the label-and-control rows shared with Save As.
            let width = (ctx.content_rect().width() - 40.0).clamp(320.0, 560.0);
            ui.set_min_width(width.min(480.0));
            ui.set_max_width(width);
            ui.set_max_height((ctx.content_rect().height() - 40.0).max(200.0));
            visual::dialog_surface(ui, |ui| {
                visual::prepare_dialog_body(ui);
                visual::dialog_title(ui, &text(self.language, "dialog-copy-options-title"));
                let footer_height = visual::BUTTON_HEIGHT + 3.0 * visual::SPACE_8;
                let body_height = (ui.available_height() - footer_height).max(80.0);
                ui.spacing_mut().scroll.floating = false;
                ui.spacing_mut().scroll.bar_width = 6.0;
                egui::ScrollArea::vertical()
                    .id_salt("copy_options_body")
                    .max_height(body_height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        visual::group_gap(ui);
                        let language = self.language;
                        // Same rows and words as Save As and Paste Special.
                        appearance_dialog::settings_row(
                            ui,
                            &text(language, "option-format-label"),
                            |ui| {
                                appearance_dialog::compact_choices(ui);
                                // Same order as the Save As formats, with HTML last.
                                for (format, key) in [
                                    (CopyFormatChoice::Markdown, "dialog-copy-format-markdown"),
                                    (CopyFormatChoice::Csv, "dialog-copy-format-csv"),
                                    (CopyFormatChoice::Tsv, "dialog-copy-format-tsv"),
                                    (CopyFormatChoice::Html, "dialog-copy-format-html"),
                                ] {
                                    ui.radio_value(
                                        &mut options.format,
                                        format,
                                        text(language, key),
                                    );
                                }
                            },
                        );
                        appearance_dialog::settings_row(
                            ui,
                            &text(language, "option-content-label"),
                            |ui| {
                                appearance_dialog::compact_choices(ui);
                                ui.radio_value(
                                    &mut options.content,
                                    CopyContentChoice::Source,
                                    text(language, "option-content-source"),
                                );
                                ui.radio_value(
                                    &mut options.content,
                                    CopyContentChoice::Display,
                                    text(language, "option-content-display"),
                                );
                            },
                        );
                    });
                ui.separator();
                visual::dialog_footer(ui, |ui| {
                    #[cfg(target_os = "macos")]
                    {
                        let copy_button = egui::Button::new(text(self.language, "action-copy"))
                            .fill(ui.visuals().selection.bg_fill);
                        if ui
                            .add_sized([132.0, visual::BUTTON_HEIGHT], copy_button)
                            .clicked()
                        {
                            copy = true;
                        }
                        if ui.button(text(self.language, "action-cancel")).clicked() {
                            close = true;
                        }
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        if ui.button(text(self.language, "action-cancel")).clicked() {
                            close = true;
                        }
                        let copy_button = egui::Button::new(text(self.language, "action-copy"))
                            .fill(ui.visuals().selection.bg_fill);
                        if ui
                            .add_sized([132.0, visual::BUTTON_HEIGHT], copy_button)
                            .clicked()
                        {
                            copy = true;
                        }
                    }
                });
            });
        });
        if copy_key {
            copy = true;
        } else if close_key {
            close = true;
        }
        if copy {
            self.copy_selected_grid_with_content(
                ctx,
                options.format.clipboard_format(),
                options.content,
            );
        } else if !close {
            self.copy_options_dialog = Some(options);
        }
    }
}
