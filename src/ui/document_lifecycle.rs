use super::*;

impl TexcelApp {
    // Open the system file picker for a supported document.
    pub(super) fn choose_file_to_open(&mut self) {
        let selected = rfd::FileDialog::new()
            .set_title(text(self.language, "dialog-open-title"))
            .add_filter(text(self.language, "file-filter-csv-tsv"), &["csv", "tsv"])
            .add_filter(
                text(self.language, "file-filter-markdown"),
                &["md", "markdown"],
            )
            .add_filter(text(self.language, "file-filter-html"), &["html", "htm"])
            .add_filter(text(self.language, "file-filter-xlsx"), &["xlsx"])
            .add_filter(text(self.language, "file-filter-ods"), &["ods"])
            .pick_files();
        if let Some(paths) = selected {
            self.queue_open_paths(paths);
        }
    }

    // Create a new editable Markdown document.
    pub(super) fn create_new_document(&mut self, name: String) -> Result<(), Message> {
        if self.edit.is_editing() {
            self.commit_edit(None);
        }
        let document_id = self.workspace.reserve_document_id();
        let name = if name.trim().is_empty() {
            text(self.language, "document-default-name")
        } else {
            name.trim().to_owned()
        };
        // GFM needs a header, separator, and body row for a new editable table.
        let mut source =
            crate::markdown_adapter::MarkdownDocument::parse(b"|  |\n| --- |\n|  |\n".to_vec())
                .map_err(|error| {
                    Message::new("status-new-document-failed")
                        .with_message("detail", crate::i18n::markdown_error_message(error))
                })?;
        if source.tables.len() != 1 {
            return Err(Message::new("status-new-markdown-table-failed"));
        }
        source.make_blank_new_document_body_trimmable();
        let mut document =
            source.to_document_with_language(document_id, name.clone(), self.language);
        if let Some(table) = document.tables.first_mut() {
            let (rows, columns) = source.source_extent(0).unwrap_or((1, 1));
            // Keep the parser's blank body row in source, but not in the new sheet extent.
            let placeholder_cells = table
                .stored_cells()
                .filter(|(address, cell)| {
                    (address.row >= rows || address.column >= columns)
                        && matches!(&cell.input, CellInput::Text(value) if value.is_empty())
                })
                .map(|(address, _)| address)
                .collect::<Vec<_>>();
            for address in placeholder_cells {
                table.apply_snapshot_cell(address, None);
            }
            table.set_dimensions(rows, columns);
        }
        let source = SourceDocument::Markdown(Arc::new(source));
        document.set_source(Some(Arc::new(source)));
        for view in document.views.values_mut() {
            view.header_row = Some(0);
        }
        let calculation = CalculationSnapshot::from_document(&document, false);
        self.workspace.insert_loaded_document(document);
        let token = self.next_job_token(document_id, 0);
        self.initialize_calculation(token, calculation);
        self.grid_focused = false;
        self.request_grid_focus = true;
        self.request_scroll_to_selection = false;
        Ok(())
    }

    // Check pending edits and begin the requested document close.
    pub(super) fn request_close_document(&mut self, document_id: DocumentId) {
        self.restore_pending_text_growth_draft(document_id);
        if self.pending_structures.contains_key(&document_id)
            || self.pending_history_restores.contains_key(&document_id)
            || self.pending_exports.contains_key(&document_id)
            || self.pending_export_writes.contains_key(&document_id)
        {
            self.status = Some(Message::new("status-finish-pending-operation-before-close"));
            return;
        }
        if self.pending_saves.contains_key(&document_id) {
            self.close_after_save.insert(document_id);
            self.status = Some(Message::new("status-document-closes-after-save"));
            return;
        }
        if self.edit.is_editing()
            && self.edit_document_id == Some(document_id)
            && !self.document_has_unsaved_changes(document_id)
        {
            self.commit_edit(None);
        }
        let requires_save = self.document_has_unsaved_changes(document_id);
        if requires_save {
            self.pending_close_document = Some(document_id);
        } else {
            self.close_document_now(document_id);
        }
    }

    // Remove a document and update the active tab selection.
    pub(super) fn close_document_now(&mut self, document_id: DocumentId) {
        self.close_document_now_with_recovery(document_id, false);
    }

    /// Replaces only the untouched document created for an empty application start.
    pub(super) fn close_startup_placeholder_for_load(&mut self) {
        let Some((document_id, initial_revision)) = self.startup_placeholder else {
            return;
        };
        let untouched = self
            .workspace
            .documents
            .get(&document_id)
            .is_some_and(|document| document.revision == initial_revision)
            && !self.document_has_unsaved_changes(document_id)
            && !(self.edit.is_editing() && self.edit_document_id == Some(document_id));
        if untouched {
            self.close_document_now(document_id);
        }
    }

    // Removes one document, keeping or deleting its recovery record as asked.
    pub(super) fn close_document_now_with_recovery(
        &mut self,
        document_id: DocumentId,
        keep_recovery: bool,
    ) {
        if !keep_recovery {
            self.restore_pending_text_growth_draft(document_id);
        }
        if !keep_recovery
            && self.edit.is_editing()
            && self
                .edit_document_id
                .or_else(|| self.workspace.active_document().map(|document| document.id))
                == Some(document_id)
        {
            self.pending_close_document = Some(document_id);
            self.status = Some(Message::new("status-finish-pending-input-before-close"));
            return;
        }
        if !keep_recovery && self.pending_recovered_cell_edits.contains_key(&document_id) {
            self.pending_close_document = Some(document_id);
            self.status = Some(Message::new("status-finish-pending-input-before-close"));
            return;
        }
        let saved_preferences = self
            .workspace
            .documents
            .get(&document_id)
            .filter(|_| !self.document_requires_save(document_id))
            .and_then(|_| {
                let backing = self.file_backings.get(&document_id)?;
                let key = document_key(
                    user_file_path(self, document_id, backing),
                    &backing.identity,
                );
                let snapshot = self.snapshot_settings();
                snapshot
                    .document_shared(&key)
                    .map(|preferences| preferences.as_ref().clone())
            });
        if let Some(preferences) = saved_preferences {
            self.preserve_saved_document_preferences(preferences);
        }
        if self.workspace.remove_document(document_id).is_none() {
            return;
        }
        self.cancel_auto_fit_for(document_id);
        self.pending_recovered_cell_edits.remove(&document_id);
        self.recovered_documents_needing_save.remove(&document_id);
        if self.edit_document_id == Some(document_id) {
            self.edit.reset();
            self.edit_document_id = None;
        }
        if self
            .startup_placeholder
            .is_some_and(|(placeholder, _)| placeholder == document_id)
        {
            self.startup_placeholder = None;
        }
        let user_path = self.user_file_paths.remove(&document_id);
        let closed_path = self
            .file_backings
            .remove(&document_id)
            .map(|backing| normalize_recent_path(user_path.as_deref().unwrap_or(&backing.path)));
        self.import_origins.remove(&document_id);
        if let Some(path) = closed_path
            && !self.file_backings.iter().any(|(other_id, backing)| {
                normalize_recent_path(user_file_path(self, *other_id, backing)) == path
            })
        {
            self.preferred_settings_identity_by_path.remove(&path);
        }
        self.coordinate_preference_generations.remove(&document_id);
        self.frozen_coordinate_preference_generations
            .remove(&document_id);
        self.document_preference_changes.remove(&document_id);
        self.settings_applied_documents.remove(&document_id);
        self.forget_accesskit_document(document_id);
        if self
            .save_conflict_dialog
            .is_some_and(|prompt| prompt.document_id == document_id)
        {
            self.save_conflict_dialog = None;
        }
        if keep_recovery {
            self.recovery_keys.remove(&document_id);
        } else if let Some(key) = self.recovery_keys.remove(&document_id) {
            self.queue_recovery_remove(key);
        }
        self.recovery_schedules.remove(&document_id);
        self.pending_recovery_flushes.remove(&document_id);
        self.pending_save_recovery_generations.remove(&document_id);
        self.import_reports.remove(&document_id);
        self.export_configs.remove(&document_id);
        self.clear_calculation_jobs_for_reset(document_id);
        self.grid_row_layouts
            .retain(|(stored_document, _), _| *stored_document != document_id);
        self.pending_close_document = None;
        self.close_after_save.remove(&document_id);
        self.close_after_save_edit.remove(&document_id);
        self.request_grid_focus = true;
        self.request_scroll_to_selection = false;
    }

    /// Discards the selected document's draft after the user chooses to close it.
    pub(super) fn discard_and_close_document(&mut self, document_id: DocumentId) {
        self.pending_close_document = None;
        if self.edit.is_editing() && self.edit_document_id == Some(document_id) {
            self.cancel_edit();
        }
        self.pending_recovered_cell_edits.remove(&document_id);
        self.close_document_now(document_id);
    }

    // Draw and handle the unsaved document close prompt.
    pub(super) fn draw_close_document_dialog(&mut self, ctx: &egui::Context) {
        let Some(document_id) = self.pending_close_document else {
            return;
        };
        #[cfg(test)]
        self.test_close_button_rects.clear();
        let name = self.workspace.documents.get(&document_id).map_or_else(
            || text(self.language, "dialog-document-name"),
            |document| document.name.clone(),
        );
        let mut save = false;
        let mut discard = false;
        let mut cancel = false;
        let modal_id = Id::unique("close_document_dialog");
        let save_key =
            self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Enter, self.edit.composing);
        let cancel_key =
            self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Escape, self.edit.composing);
        egui::Modal::new(modal_id).show(ctx, |ui| {
            let width = (ctx.content_rect().width() - 40.0).clamp(320.0, 620.0);
            ui.set_min_width(width.min(460.0));
            ui.set_max_width(width);
            visual::dialog_surface(ui, |ui| {
                visual::prepare_dialog_body(ui);
                visual::dialog_title(ui, &text(self.language, "dialog-unsaved-title"));
                visual::group_gap(ui);
                ui.label(
                    Message::new("dialog-unsaved-body")
                        .with_text("name", &name)
                        .resolve(self.language),
                );
                ui.separator();
                visual::dialog_footer(ui, |ui| {
                    #[cfg(target_os = "macos")]
                    {
                        if self.close_dialog_button(
                            ui,
                            "save",
                            text(self.language, "action-save-close"),
                            true,
                        ) {
                            save = true;
                        }
                        if self.close_dialog_button(
                            ui,
                            "discard",
                            text(self.language, "action-discard-changes"),
                            false,
                        ) {
                            discard = true;
                        }
                        if self.close_dialog_button(
                            ui,
                            "cancel",
                            text(self.language, "action-cancel"),
                            false,
                        ) {
                            cancel = true;
                        }
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        if self.close_dialog_button(
                            ui,
                            "cancel",
                            text(self.language, "action-cancel"),
                            false,
                        ) {
                            cancel = true;
                        }
                        if self.close_dialog_button(
                            ui,
                            "discard",
                            text(self.language, "action-discard-changes"),
                            false,
                        ) {
                            discard = true;
                        }
                        if self.close_dialog_button(
                            ui,
                            "save",
                            text(self.language, "action-save-close"),
                            true,
                        ) {
                            save = true;
                        }
                    }
                });
            });
        });
        if cancel || cancel_key {
            self.pending_close_document = None;
        } else if discard {
            self.discard_and_close_document(document_id);
        } else if save || save_key {
            self.save_and_close_document(document_id);
        }
    }

    /// Draws one close choice and exposes its real rectangle to scenarios.
    fn close_dialog_button(
        &mut self,
        ui: &mut egui::Ui,
        _choice: &'static str,
        label: String,
        primary: bool,
    ) -> bool {
        let button = egui::Button::new(label);
        let button = if primary {
            button.fill(ui.visuals().selection.bg_fill)
        } else {
            button
        };
        let response = ui.add_sized([144.0, visual::BUTTON_HEIGHT], button);
        #[cfg(test)]
        self.test_close_button_rects.insert(_choice, response.rect);
        response.clicked()
    }
}
