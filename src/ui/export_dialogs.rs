use super::*;

impl TexcelApp {
    // Draw the shared Save As choices for format, formulas, and CSV table.
    pub(super) fn draw_export_settings(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.export_dialog.take() else {
            return;
        };
        let Some((document_name, tables, active_table_id)) = self
            .workspace
            .documents
            .get(&state.document_id)
            .map(|document| {
                let tables = document
                    .tables
                    .iter()
                    .map(|table| (table.id, table.name.clone()))
                    .collect::<Vec<_>>();
                (
                    document.name.clone(),
                    tables,
                    document.active_table().map(|table| table.id),
                )
            })
        else {
            self.status = Some(Message::new("status-export-target-document-required"));
            return;
        };
        if state.options.format.is_delimited() {
            normalize_csv_export_scope(&mut state.options.scope, active_table_id, &tables);
        } else {
            state.options.scope = ExportScope::AllTables;
        }
        let mut choose_destination = false;
        let mut close = false;
        let language = self.language;
        let modal_id = Id::unique(egui::IdSalt::new((
            "texcel_save_as_options",
            state.document_id,
        )));
        let start_key =
            self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Enter, self.edit.composing);
        let cancel_key =
            self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Escape, self.edit.composing);
        egui::Modal::new(modal_id).show(ctx, |ui| {
            let width = (ctx.content_rect().width() - 40.0).clamp(360.0, 760.0);
            ui.set_min_width(width.min(480.0));
            ui.set_max_width(width);
            ui.set_max_height((ctx.content_rect().height() - 40.0).max(260.0));
            visual::dialog_surface(ui, |ui| {
                visual::prepare_dialog_body(ui);
                visual::dialog_title(ui, &context_text(ui.ctx(), "export-settings-title"));
                visual::group_gap(ui);
                let body_height =
                    (ui.available_height() - visual::BUTTON_HEIGHT - 3.0 * visual::SPACE_8)
                        .max(120.0);
                egui::ScrollArea::vertical()
                    .max_height(body_height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        // Labels and controls share the settings dialog's row layout.
                        appearance_dialog::settings_row(
                            ui,
                            &context_text(ui.ctx(), "option-format-label"),
                            |ui| {
                                appearance_dialog::compact_choices(ui);
                                for format in
                                    [ExportFormat::Markdown, ExportFormat::Csv, ExportFormat::Tsv]
                                {
                                    ui.radio_value(
                                        &mut state.options.format,
                                        format,
                                        format.label(),
                                    );
                                }
                            },
                        );
                        if state.options.format.is_delimited() {
                            normalize_csv_export_scope(
                                &mut state.options.scope,
                                active_table_id,
                                &tables,
                            );
                        } else {
                            state.options.scope = ExportScope::AllTables;
                        }
                        if state.options.format.is_delimited() {
                            appearance_dialog::settings_row(
                                ui,
                                &context_text(ui.ctx(), "export-scope-label"),
                                |ui| {
                                    egui::ComboBox::from_id_salt((
                                        "export_scope",
                                        state.document_id,
                                    ))
                                    .selected_text(selected_scope_label(
                                        ui.ctx(),
                                        state.options.scope,
                                        &tables,
                                    ))
                                    .width(ui.available_width())
                                    .show_ui(ui, |ui| {
                                        for (id, name) in &tables {
                                            ui.selectable_value(
                                                &mut state.options.scope,
                                                ExportScope::Selected(*id),
                                                name,
                                            );
                                        }
                                    });
                                },
                            );
                        }
                        let allow_formula_results = state
                            .save_intent
                            .as_ref()
                            .is_none_or(|intent| intent.allow_formula_results);
                        if allow_formula_results {
                            appearance_dialog::settings_row(
                                ui,
                                &context_text(ui.ctx(), "option-content-label"),
                                |ui| {
                                    appearance_dialog::compact_choices(ui);
                                    ui.radio_value(
                                        &mut state.options.formulas,
                                        FormulaOutput::Formula,
                                        context_text(ui.ctx(), "option-content-formulas"),
                                    );
                                    ui.radio_value(
                                        &mut state.options.formulas,
                                        FormulaOutput::LatestResult,
                                        context_text(ui.ctx(), "option-content-values"),
                                    );
                                },
                            );
                        } else {
                            state.options.formulas = FormulaOutput::Formula;
                        }
                        if state.options.format.is_delimited()
                            && state.options.formulas == FormulaOutput::Formula
                        {
                            ui.colored_label(
                                ui.visuals().error_fg_color,
                                context_text(ui.ctx(), "export-csv-formula-warning"),
                            );
                        }
                        ui.label(
                            Message::new("export-document-line")
                                .with_text("name", &document_name)
                                .resolve(language),
                        );
                    });
                ui.separator();
                visual::dialog_footer(ui, |ui| {
                    #[cfg(target_os = "macos")]
                    {
                        let choose =
                            egui::Button::new(context_text(ui.ctx(), "export-choose-destination"))
                                .fill(ui.visuals().selection.bg_fill);
                        if ui
                            .add_enabled_ui(!tables.is_empty(), |ui| {
                                ui.add_sized([188.0, visual::BUTTON_HEIGHT], choose)
                            })
                            .inner
                            .clicked()
                        {
                            choose_destination = true;
                        }
                        if ui.button(context_text(ui.ctx(), "action-cancel")).clicked() {
                            close = true;
                        }
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        if ui.button(context_text(ui.ctx(), "action-cancel")).clicked() {
                            close = true;
                        }
                        let choose =
                            egui::Button::new(context_text(ui.ctx(), "export-choose-destination"))
                                .fill(ui.visuals().selection.bg_fill);
                        if ui
                            .add_enabled_ui(!tables.is_empty(), |ui| {
                                ui.add_sized([188.0, visual::BUTTON_HEIGHT], choose)
                            })
                            .inner
                            .clicked()
                        {
                            choose_destination = true;
                        }
                    }
                });
            });
        });
        if start_key && !tables.is_empty() {
            choose_destination = true;
        } else if cancel_key {
            close = true;
        }
        if choose_destination {
            let file_name = format!("{}.{}", document_name, state.options.format.extension());
            let dialog = rfd::FileDialog::new()
                .set_title(context_text(ctx, "dialog-save-title"))
                .set_file_name(file_name);
            let dialog = match state.options.format {
                ExportFormat::Csv => dialog.add_filter("CSV", &["csv"]),
                ExportFormat::Tsv => dialog.add_filter("TSV", &["tsv"]),
                ExportFormat::Markdown => dialog.add_filter("Markdown", &["md", "markdown"]),
            };
            #[cfg(test)]
            let destination = self
                .test_save_path_selection
                .take()
                .or_else(|| dialog.save_file());
            #[cfg(not(test))]
            let destination = dialog.save_file();
            if let Some(destination) = destination {
                if !state.options.format.matches_path(&destination) {
                    self.status = Some(Message::new("status-save-format-mismatch"));
                    self.export_dialog = Some(state);
                    return;
                }
                if let Some(intent) = state.save_intent {
                    self.save_as_to_path_with_options(
                        state.document_id,
                        PendingSaveIntent {
                            path: destination,
                            options: state.options,
                            track_recent: intent.track_recent,
                            close_after_save: intent.close_after_save,
                            close_edit: intent.close_edit,
                            app_close_generation: intent.app_close_generation,
                        },
                    );
                    self.advance_app_close_save_queue();
                } else {
                    self.start_export_preflight(state.document_id, state.options, destination);
                }
            } else {
                self.export_dialog = Some(state);
            }
        } else if close {
            if let Some(intent) = state.save_intent {
                self.cancel_save_as_dialog(state.document_id, intent);
            }
        } else {
            self.export_dialog = Some(state);
        }
    }

    // Show progress while the export worker is running.
    pub(super) fn draw_export_progress(&mut self, ctx: &egui::Context) {
        let Some(document_id) = self.workspace.active_document else {
            return;
        };
        let pending = self
            .pending_exports
            .get(&document_id)
            .map(|token| (document_id, *token, "dialog-export-preparing"))
            .or_else(|| {
                self.pending_export_writes
                    .get(&document_id)
                    .map(|token| (document_id, *token, "dialog-export-write-running"))
            });
        let Some((document_id, token, message_key)) = pending else {
            return;
        };
        let cancel_requested = self.cancelling_exports.get(&document_id) == Some(&token);
        let mut cancel = false;
        egui::Window::new(context_text(ctx, "dialog-export-processing"))
            .id(Id::unique(egui::IdSalt::new((
                "texcel_export_progress",
                document_id,
                token.job_id,
            ))))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 16.0))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.set_min_width(360.0);
                ui.set_max_width((ctx.content_rect().width() - 40.0).clamp(360.0, 520.0));
                visual::dialog_surface(ui, |ui| {
                    visual::prepare_dialog_body(ui);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(context_text(ui.ctx(), message_key));
                    });
                    visual::group_gap(ui);
                    if cancel_requested {
                        ui.label(context_text(ui.ctx(), "dialog-export-cancelling"));
                    }
                    ui.separator();
                    visual::dialog_footer(ui, |ui| {
                        if ui
                            .add_enabled(
                                !cancel_requested,
                                egui::Button::new(context_text(ui.ctx(), "action-cancel")),
                            )
                            .clicked()
                        {
                            cancel = true;
                        }
                    });
                });
            });
        if cancel {
            if self.file_worker.cancel(token) {
                self.cancelling_exports.insert(document_id, token);
            } else {
                self.status = Some(Message::new("status-export-cancel-failed"));
            }
        }
    }
}

/// Keeps CSV output bound to exactly one table that still exists in the document.
fn normalize_csv_export_scope(
    scope: &mut ExportScope,
    active_table_id: Option<TableId>,
    tables: &[(TableId, String)],
) {
    let selected = match *scope {
        ExportScope::Selected(id) if tables.iter().any(|(table_id, _)| *table_id == id) => Some(id),
        ExportScope::AllTables | ExportScope::Selected(_) => active_table_id
            .filter(|id| tables.iter().any(|(table_id, _)| table_id == id))
            .or_else(|| tables.first().map(|(id, _)| *id)),
    };
    if let Some(id) = selected {
        *scope = ExportScope::Selected(id);
    }
}

/// Shows the current table choice with a localized fallback for stale state.
fn selected_scope_label(
    ctx: &egui::Context,
    scope: ExportScope,
    tables: &[(TableId, String)],
) -> String {
    match scope {
        ExportScope::AllTables => context_text(ctx, "export-scope-all"),
        ExportScope::Selected(id) => tables
            .iter()
            .find(|(table_id, _)| *table_id == id)
            .map_or_else(
                || context_text(ctx, "export-scope-selected"),
                |(_, name)| name.clone(),
            ),
    }
}
