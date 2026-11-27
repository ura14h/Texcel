use super::*;

impl TexcelApp {
    /// Collects special paste options for one grid paste request.
    pub(super) fn draw_paste_special_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut options) = self.paste_special_dialog.take() else {
            return;
        };
        let mut paste = false;
        let mut close = false;
        let modal_id = Id::unique("paste_special_dialog");
        let paste_key =
            self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Enter, self.edit.composing);
        let close_key =
            self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Escape, self.edit.composing);
        egui::Modal::new(modal_id).show(ctx, |ui| {
            // Wide enough for the label-and-control rows shared with Save As.
            let width = (ctx.content_rect().width() - 40.0).clamp(300.0, 560.0);
            ui.set_min_width(width.min(480.0));
            ui.set_max_width(width);
            visual::dialog_surface(ui, |ui| {
                visual::prepare_dialog_body(ui);
                visual::dialog_title(ui, &text(self.language, "dialog-paste-special-title"));
                visual::group_gap(ui);
                let language = self.language;
                // Same rows and words as Save As and Copy with Options.
                appearance_dialog::settings_row(ui, &text(language, "option-format-label"), |ui| {
                    ui.vertical(|ui| {
                        appearance_dialog::compact_choices(ui);
                        ui.radio_value(
                            &mut options.text_only,
                            false,
                            text(language, "option-paste-source-auto"),
                        );
                        ui.radio_value(
                            &mut options.text_only,
                            true,
                            text(language, "option-paste-source-text-only"),
                        );
                    });
                });
                appearance_dialog::settings_row(
                    ui,
                    &text(language, "option-content-label"),
                    |ui| {
                        ui.vertical(|ui| {
                            appearance_dialog::compact_choices(ui);
                            ui.horizontal_wrapped(|ui| {
                                ui.radio_value(
                                    &mut options.values_only,
                                    false,
                                    text(language, "option-content-formulas"),
                                );
                                ui.radio_value(
                                    &mut options.values_only,
                                    true,
                                    text(language, "option-content-values"),
                                );
                            });
                            // Plain text has no formatting to keep or remove.
                            ui.add_enabled_ui(!options.text_only, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.radio_value(
                                        &mut options.strip_formatting,
                                        false,
                                        text(language, "option-formatting-keep"),
                                    );
                                    ui.radio_value(
                                        &mut options.strip_formatting,
                                        true,
                                        text(language, "option-formatting-strip"),
                                    );
                                });
                            });
                            if options.text_only && options.values_only {
                                ui.label(
                                    egui::RichText::new(text(
                                        language,
                                        "option-paste-as-string-hint",
                                    ))
                                    .small()
                                    .weak(),
                                );
                            }
                        });
                    },
                );
                appearance_dialog::settings_row(ui, &text(language, "option-layout-label"), |ui| {
                    ui.vertical(|ui| {
                        ui.checkbox(
                            &mut options.transpose,
                            text(language, "top-paste-transpose"),
                        );
                        ui.checkbox(
                            &mut options.skip_blank,
                            text(language, "top-paste-skip-blanks"),
                        );
                    });
                });
                ui.separator();
                visual::dialog_footer(ui, |ui| {
                    #[cfg(target_os = "macos")]
                    {
                        let paste_button = egui::Button::new(text(self.language, "action-paste"))
                            .fill(ui.visuals().selection.bg_fill);
                        if ui
                            .add_sized([132.0, visual::BUTTON_HEIGHT], paste_button)
                            .clicked()
                        {
                            paste = true;
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
                        let paste_button = egui::Button::new(text(self.language, "action-paste"))
                            .fill(ui.visuals().selection.bg_fill);
                        if ui
                            .add_sized([132.0, visual::BUTTON_HEIGHT], paste_button)
                            .clicked()
                        {
                            paste = true;
                        }
                    }
                });
            });
        });
        if paste_key {
            paste = true;
        } else if close_key {
            close = true;
        }
        if !close && !paste {
            self.paste_special_dialog = Some(options);
        } else if paste {
            self.deferred_commands.push(commands::DeferredAppCommand {
                command: commands::AppCommand::PasteSystem(options),
                owner: commands::InputOwner::Grid,
                blocked: false,
                focus: None,
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                native_target: None,
            });
        }
    }
}
