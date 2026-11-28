use super::*;

/// Shows the selected encoding using translated labels.
fn encoding_choice_label(
    ctx: &egui::Context,
    encoding: EncodingChoice,
    needs_selection: bool,
) -> String {
    if encoding == EncodingChoice::Auto && needs_selection {
        return context_text(ctx, "dialog-csv-encoding-select");
    }
    context_text(
        ctx,
        match encoding {
            EncodingChoice::Auto => "dialog-csv-encoding-auto",
            EncodingChoice::Utf8 => "dialog-csv-encoding-utf8",
            EncodingChoice::ShiftJis => "dialog-csv-encoding-cp932",
        },
    )
}

/// Gives common delimiter choices familiar names and keeps other ones visible.
fn delimiter_label(ctx: &egui::Context, delimiter: char) -> String {
    match delimiter {
        '\t' => context_text(ctx, "dialog-csv-delimiter-tab"),
        ',' => context_text(ctx, "dialog-csv-delimiter-comma"),
        ';' => context_text(ctx, "dialog-csv-delimiter-semicolon"),
        _ => delimiter.to_string(),
    }
}

/// Keeps a failed encoding probe visible until the user chooses another encoding.
fn is_encoding_error(error: Option<&'static str>) -> bool {
    matches!(
        error,
        Some(
            "status-csv-encoding-invalid"
                | "status-csv-encoding-undetermined"
                | "status-csv-encoding-choice-required"
        )
    )
}

impl TexcelApp {
    // Draw file conflict and encoding prompts returned by file operations.
    pub(super) fn draw_file_prompts(&mut self, ctx: &egui::Context) {
        let prompt = self
            .pending_csv_opens
            .iter()
            .filter(|(id, pending)| {
                pending.prompt.is_some() && self.load_request_belongs_to_current_viewport(**id)
            })
            .find_map(|(id, pending)| pending.prompt.clone().map(|prompt| (*id, prompt)));
        let Some((document_id, prompt)) = prompt else {
            return;
        };
        let CsvLoadPrompt::Settings(mut settings) = prompt;
        let mut apply = false;
        let mut cancel = false;
        let mut modal_text_focus = None;
        let modal_id = Id::unique(egui::IdSalt::new(("csv_load_prompt", document_id)));
        let encoding_needs_selection = settings.needs_encoding
            && (settings.selected_encoding == EncodingChoice::Auto
                || matches!(
                    settings.error,
                    Some(
                        "status-csv-encoding-invalid"
                            | "status-csv-encoding-undetermined"
                            | "status-csv-encoding-choice-required"
                    )
                ));
        let apply_key = !encoding_needs_selection
            && self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Enter, self.edit.composing);
        let cancel_key =
            self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Escape, self.edit.composing);
        let original_encoding = settings.selected_encoding;
        egui::Modal::new(modal_id).show(ctx, |ui| {
            let width = (ctx.content_rect().width() - 40.0).clamp(320.0, 760.0);
            ui.set_min_width(width.min(520.0));
            ui.set_max_width(width);
            ui.set_max_height((ctx.content_rect().height() - 40.0).max(240.0));
            visual::dialog_surface(ui, |ui| {
                visual::prepare_dialog_body(ui);
                visual::dialog_title(ui, &context_text(ui.ctx(), "dialog-csv-settings-title"));
                visual::group_gap(ui);
                let footer_height = visual::BUTTON_HEIGHT + visual::SPACE_8;
                let body_height = (ui.available_height() - footer_height).max(100.0);
                egui::ScrollArea::vertical()
                    .max_height(body_height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.label(context_text(ui.ctx(), "dialog-csv-settings-help"));
                        let encoding_label = visual::section_title(
                            ui,
                            &context_text(ui.ctx(), "dialog-csv-encoding-label"),
                        );
                        egui::ComboBox::from_id_salt(("csv_load_encoding", document_id))
                            .selected_text(encoding_choice_label(
                                ui.ctx(),
                                settings.selected_encoding,
                                settings.error == Some("status-csv-encoding-undetermined"),
                            ))
                            .width(ui.available_width().min(360.0))
                            .show_ui(ui, |ui| {
                                visual::prepare_context_menu(ui);
                                ui.selectable_value(
                                    &mut settings.selected_encoding,
                                    EncodingChoice::Auto,
                                    context_text(ui.ctx(), "dialog-csv-encoding-auto"),
                                );
                                ui.selectable_value(
                                    &mut settings.selected_encoding,
                                    EncodingChoice::Utf8,
                                    context_text(ui.ctx(), "dialog-csv-encoding-utf8"),
                                );
                                ui.selectable_value(
                                    &mut settings.selected_encoding,
                                    EncodingChoice::ShiftJis,
                                    context_text(ui.ctx(), "dialog-csv-encoding-cp932"),
                                );
                            })
                            .response
                            .labelled_by(encoding_label.id);
                        visual::group_gap(ui);
                        visual::section_title(
                            ui,
                            &context_text(ui.ctx(), "dialog-csv-delimiter-label"),
                        );
                        ui.horizontal_wrapped(|ui| {
                            for delimiter in &settings.candidates {
                                if ui
                                    .selectable_value(
                                        &mut settings.delimiter_text,
                                        delimiter.to_string(),
                                        delimiter_label(ui.ctx(), *delimiter),
                                    )
                                    .changed()
                                    && !is_encoding_error(settings.error)
                                {
                                    settings.error = None;
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            let delimiter_label =
                                ui.label(context_text(ui.ctx(), "dialog-csv-custom-delimiter"));
                            let delimiter_edit = ui
                                .add(
                                    egui::TextEdit::singleline(&mut settings.delimiter_text)
                                        .id(Id::unique(egui::IdSalt::new((
                                            "csv_custom_delimiter",
                                            document_id,
                                        ))))
                                        .desired_width(72.0),
                                )
                                .labelled_by(delimiter_label.id);
                            if delimiter_edit.has_focus() {
                                modal_text_focus = Some(delimiter_edit.id);
                            }
                            if delimiter_edit.changed() && !is_encoding_error(settings.error) {
                                settings.error = None;
                            }
                        });
                        visual::group_gap(ui);
                        let selected_preview = settings.delimiter_text.chars().collect::<Vec<_>>();
                        visual::section_title(
                            ui,
                            &context_text(ui.ctx(), "dialog-csv-preview-label"),
                        );
                        if let Some(delimiter) = selected_preview.first().copied()
                            && selected_preview.len() == 1
                            && let Some(rows) = settings.previews.get(&delimiter)
                            && !rows.is_empty()
                        {
                            egui::ScrollArea::both().max_height(156.0).show(ui, |ui| {
                                egui::Grid::new(("csv_preview", document_id))
                                    .striped(true)
                                    .show(ui, |ui| {
                                        for row in rows {
                                            for value in row {
                                                ui.add_sized(
                                                    [124.0, 22.0],
                                                    egui::Label::new(value).truncate(),
                                                );
                                            }
                                            ui.end_row();
                                        }
                                    });
                            });
                        } else if settings.error == Some("status-csv-encoding-undetermined") {
                            ui.label(context_text(
                                ui.ctx(),
                                "dialog-csv-preview-encoding-undetermined",
                            ));
                        } else if settings.needs_encoding {
                            ui.label(context_text(ui.ctx(), "dialog-csv-preview-choose-encoding"));
                        } else {
                            ui.label(context_text(ui.ctx(), "dialog-csv-preview-unavailable"));
                        }
                        if settings.needs_encoding {
                            ui.label(context_text(ui.ctx(), "dialog-csv-encoding-warning"));
                        }
                        if let Some(message) = settings.error {
                            ui.colored_label(
                                ui.visuals().error_fg_color,
                                context_text(ui.ctx(), message),
                            );
                        }
                    });
                ui.separator();
                visual::dialog_footer(ui, |ui| {
                    #[cfg(target_os = "macos")]
                    {
                        if csv_prompt_button(
                            ui,
                            context_text(ui.ctx(), "dialog-csv-load"),
                            true,
                            !encoding_needs_selection,
                        ) {
                            apply = true;
                        }
                        if csv_prompt_button(
                            ui,
                            context_text(ui.ctx(), "action-cancel"),
                            false,
                            true,
                        ) {
                            cancel = true;
                        }
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        if csv_prompt_button(
                            ui,
                            context_text(ui.ctx(), "action-cancel"),
                            false,
                            true,
                        ) {
                            cancel = true;
                        }
                        if csv_prompt_button(
                            ui,
                            context_text(ui.ctx(), "dialog-csv-load"),
                            true,
                            !encoding_needs_selection,
                        ) {
                            apply = true;
                        }
                    }
                });
            });
        });
        if modal_text_focus.is_some() {
            self.active_modal_text_input_focus = modal_text_focus;
        }
        let encoding_changed = settings.selected_encoding != original_encoding;
        if encoding_changed {
            settings.error = None;
        }
        if cancel || cancel_key {
            self.pending_csv_opens.remove(&document_id);
            self.pending_file_load_paths.remove(&document_id);
            self.pending_load_previews.remove(&document_id);
            self.load_request_viewports.remove(&document_id);
        } else if encoding_changed {
            if let Some(pending) = self.pending_csv_opens.get_mut(&document_id) {
                pending.encoding = settings.selected_encoding;
                pending.delimiter = None;
            }
            self.submit_csv_options(document_id);
            if self
                .pending_csv_opens
                .get(&document_id)
                .is_some_and(|pending| pending.retry_submission.is_some())
            {
                settings.error = Some("status-submit-queue-full");
            }
        } else if apply || apply_key {
            let delimiter = settings.delimiter_text.chars().collect::<Vec<_>>();
            if delimiter.len() != 1 {
                settings.error = Some("status-csv-delimiter-one-character");
            } else if crate::csv_adapter::validate_delimiter_choice(delimiter[0]).is_err() {
                settings.error = Some("status-csv-delimiter-invalid");
            } else if settings.needs_encoding && settings.selected_encoding == EncodingChoice::Auto
            {
                settings.error = Some("status-csv-encoding-choice-required");
            } else if let Some(pending) = self.pending_csv_opens.get_mut(&document_id) {
                pending.delimiter = Some(delimiter[0]);
                pending.encoding = settings.selected_encoding;
                let _ = self.submit_csv_load(document_id);
                if self
                    .pending_csv_opens
                    .get(&document_id)
                    .is_some_and(|pending| pending.retry_submission.is_some())
                {
                    settings.error = Some("status-submit-queue-full");
                }
            }
        }
        if let Some(pending) = self.pending_csv_opens.get_mut(&document_id)
            && pending.prompt.is_some()
        {
            pending.prompt = Some(CsvLoadPrompt::Settings(settings));
        }
    }
}

/// Keeps CSV prompt actions aligned and highlights the load choice.
fn csv_prompt_button(ui: &mut egui::Ui, label: String, primary: bool, enabled: bool) -> bool {
    let button = egui::Button::new(label);
    let button = if primary {
        button.fill(ui.visuals().selection.bg_fill)
    } else {
        button
    };
    ui.add_enabled_ui(enabled, |ui| {
        ui.add_sized([132.0, visual::BUTTON_HEIGHT], button)
    })
    .inner
    .clicked()
}
