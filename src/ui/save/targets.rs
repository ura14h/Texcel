use super::paths::is_markdown_path;
use super::*;
use std::path::Path;

impl TexcelApp {
    /// Cancels a pending Save As choice without closing its document or app.
    pub(in crate::ui) fn cancel_save_as_dialog(
        &mut self,
        document_id: DocumentId,
        intent: SaveDialogIntent,
    ) {
        if intent.close_after_save {
            self.pending_close_document = None;
            if let Some(edit) = intent.close_edit {
                self.restore_close_edit(document_id, edit);
            }
        }
        if intent.app_close_generation == Some(self.app_close_save_generation) {
            self.pending_app_close_save_queue.clear();
            self.pending_app_close_save_targets.clear();
            self.app_close_save_collecting_targets = false;
            self.app_close_save_failed = true;
            self.finish_app_close_save_batch();
        }
    }

    #[cfg(test)]
    pub(in crate::ui) fn save_as_to_path(&mut self, document_id: DocumentId, path: PathBuf) {
        self.save_as_to_path_with_intent(document_id, path, true, false, None, None);
    }

    /// Keeps the requested destination and close intent through editing and conversion workers.
    pub(in crate::ui) fn save_as_to_path_with_intent(
        &mut self,
        document_id: DocumentId,
        path: PathBuf,
        track_recent: bool,
        close_after_save: bool,
        close_edit: Option<EditSession>,
        app_close_generation: Option<u64>,
    ) {
        let Some(options) = self.default_save_options(document_id, &path) else {
            self.status = Some(Message::new("status-save-document-required"));
            return;
        };
        self.save_as_to_path_with_options(
            document_id,
            PendingSaveIntent {
                path,
                options,
                track_recent,
                close_after_save,
                close_edit,
                app_close_generation,
            },
        );
    }

    /// Starts one Save As request with the options chosen in the shared save dialog.
    pub(in crate::ui) fn save_as_to_path_with_options(
        &mut self,
        document_id: DocumentId,
        intent: PendingSaveIntent,
    ) {
        let app_close_generation = intent.app_close_generation;
        if app_close_generation == Some(self.app_close_save_generation)
            && self.app_close_save_batch_active
            && self.app_close_save_collecting_targets
        {
            if !self.save_as_path_supported(document_id, &intent.path)
                || !intent.options.format.matches_path(&intent.path)
            {
                self.status = Some(Message::new("status-save-format-mismatch"));
                self.app_close_save_failed = true;
                self.pending_app_close_save_queue.clear();
                self.pending_app_close_save_targets.clear();
                return;
            }
            self.pending_app_close_save_targets.insert(
                document_id,
                (intent.path, intent.options, intent.track_recent),
            );
            return;
        }
        if !self.save_as_path_supported(document_id, &intent.path)
            || !intent.options.format.matches_path(&intent.path)
        {
            self.status = Some(Message::new("status-save-format-mismatch"));
            return;
        }
        self.pending_save_intents.insert(document_id, intent);
        let active_edit_document = self.edit_document_id.or(self.workspace.active_document);
        // Save All leaves an untouched editor open while another document saves.
        let preserve_clean_editor_for_exit = app_close_generation
            == Some(self.app_close_save_generation)
            && self.edit.is_editing()
            && active_edit_document.is_some_and(|active_id| {
                active_id != document_id && !self.document_has_unsaved_changes(active_id)
            });
        if self.edit.is_editing()
            && active_edit_document != Some(document_id)
            && !preserve_clean_editor_for_exit
        {
            self.commit_edit(None);
        }
        if !preserve_clean_editor_for_exit {
            self.workspace.select_document(document_id);
            self.restore_pending_text_growth_draft(document_id);
            self.restore_recovered_cell_edit(document_id);
        }
        if self.edit.is_editing() && self.edit_document_id == Some(document_id) {
            self.commit_edit(None);
        }
        if self
            .pending_text_growth_sessions
            .get(&document_id)
            .is_some_and(|growth| growth.pending)
        {
            return;
        }
        if self.edit.is_editing()
            && !(preserve_clean_editor_for_exit && self.edit_document_id != Some(document_id))
        {
            self.finish_save_intent_failed(document_id);
            return;
        }
        self.continue_save_intent(document_id);
    }

    /// Continues the same save request after a source-conversion or growth acknowledgement.
    pub(in crate::ui) fn continue_save_intent(&mut self, document_id: DocumentId) {
        let Some((path, options)) = self
            .pending_save_intents
            .get(&document_id)
            .map(|intent| (intent.path.clone(), intent.options))
        else {
            return;
        };
        let source = self
            .workspace
            .documents
            .get(&document_id)
            .and_then(|document| document.source.as_deref());
        let requires_export = options.formulas != FormulaOutput::Formula
            || (options.format.is_delimited()
                && !matches!(
                    source,
                    Some(SourceDocument::Csv(csv)) if options.format.keeps_csv_delimiter(csv.delimiter)
                ));
        if requires_export {
            self.save_as_export_documents.insert(document_id);
            self.start_export_preflight(document_id, options, path);
            if !self.pending_exports.contains_key(&document_id) {
                self.save_as_export_documents.remove(&document_id);
                self.finish_save_intent_failed(document_id);
            }
        } else if matches!(source, Some(SourceDocument::Csv(_))) && is_markdown_path(&path) {
            self.request_table_management_for_document(
                document_id,
                StructureAction::PromoteCsv,
                None,
                None,
            );
            if !self.pending_table_management.contains(&document_id) {
                self.finish_save_intent_failed(document_id);
            }
        } else {
            self.save_document_to_path(document_id, path);
            if self.pending_saves.contains_key(&document_id) {
                self.bind_save_intent_to_worker(document_id);
            } else if !self
                .pending_text_growth_sessions
                .get(&document_id)
                .is_some_and(|growth| growth.pending)
            {
                self.finish_save_intent_failed(document_id);
            }
        }
    }

    /// Transfers close intent to a real save token, after all preparation has succeeded.
    pub(in crate::ui) fn bind_save_intent_to_worker(&mut self, document_id: DocumentId) {
        let Some(intent) = self.pending_save_intents.remove(&document_id) else {
            return;
        };
        if intent.close_after_save {
            self.close_after_save.insert(document_id);
            if let Some(edit) = intent.close_edit {
                self.close_after_save_edit.insert(document_id, edit);
            }
        }
        if intent.app_close_generation == Some(self.app_close_save_generation)
            && self.app_close_dialog_open
            && let Some(token) = self.pending_saves.get(&document_id).copied()
        {
            self.app_close_save_tokens.insert(document_id, token);
        }
    }

    /// Leaves a failed conversion open and asks again before any requested close or exit.
    pub(in crate::ui) fn finish_save_intent_failed(&mut self, document_id: DocumentId) {
        let Some(intent) = self.pending_save_intents.remove(&document_id) else {
            return;
        };
        if intent.close_after_save {
            self.pending_close_document = Some(document_id);
            if let Some(edit) = intent.close_edit {
                self.restore_close_edit(document_id, edit);
            }
        }
        if intent.app_close_generation == Some(self.app_close_save_generation)
            && self.app_close_dialog_open
        {
            self.app_close_save_failed = true;
            self.finish_app_close_save_batch();
        }
    }

    /// Picks defaults for non-UI callers while preserving the file's natural format.
    pub(in crate::ui) fn default_save_options(
        &self,
        document_id: DocumentId,
        path: &Path,
    ) -> Option<ExportOptions> {
        let document = self.workspace.documents.get(&document_id)?;
        let format = [ExportFormat::Csv, ExportFormat::Tsv]
            .into_iter()
            .find(|format| format.matches_path(path))
            .unwrap_or(ExportFormat::Markdown);
        Some(ExportOptions {
            format,
            scope: if format.is_delimited() {
                ExportScope::Selected(document.active_table)
            } else {
                ExportScope::AllTables
            },
            formulas: FormulaOutput::Formula,
            modifiers: ModifierOutput::Convert,
            markdown_header: MarkdownHeader::UseFirstRow,
        })
    }

    /// Saves a fully representable Markdown document as CSV with a normal save acknowledgement.
    pub(in crate::ui) fn save_csv_export_as_document(
        &mut self,
        document_id: DocumentId,
        artifact: crate::export::ExportArtifact,
    ) {
        let path = artifact.path;
        let delimiter = if path
            .extension()
            .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("tsv"))
        {
            '\t'
        } else {
            ','
        };
        let expected = if artifact.replace_existing {
            artifact
                .expected_identity
                .map_or(SaveExpectation::Absent, SaveExpectation::Matches)
        } else {
            SaveExpectation::Absent
        };
        let Some(document) = self.workspace.documents.get(&document_id) else {
            self.status = Some(Message::new("status-save-document-required"));
            return;
        };
        if self.pending_saves.contains_key(&document_id) {
            self.status = Some(Message::new("status-document-save-in-progress"));
            return;
        }
        let revision = document.revision;
        let history_state_id = document.history_state_id();
        let structure_revision = document.structure_revision;
        let preferences = snapshot_table_preferences(document);
        let recovery_generation = self.recovery_generation_for_save(document_id);
        let coordinate_generation = self
            .coordinate_preference_generations
            .get(&document_id)
            .copied()
            .unwrap_or_default();
        let token = self.next_job_token(document_id, revision);
        match self.file_worker.save_converted_csv(
            token,
            path.clone(),
            artifact.bytes,
            delimiter,
            expected,
        ) {
            Ok(()) => {
                self.pending_saves.insert(document_id, token);
                self.pending_save_history_states
                    .insert(document_id, (token, history_state_id));
                if self
                    .pending_save_intents
                    .get(&document_id)
                    .is_some_and(|intent| intent.track_recent)
                {
                    self.recent_save_paths.insert(document_id, (token, path));
                }
                self.pending_save_recovery_generations
                    .insert(document_id, (token, recovery_generation));
                self.pending_save_preferences.insert(
                    document_id,
                    (
                        token,
                        preferences,
                        structure_revision,
                        coordinate_generation,
                    ),
                );
                self.bind_save_intent_to_worker(document_id);
            }
            Err(error) => {
                self.finish_save_intent_failed(document_id);
                self.status = Some(submit_error_message(error));
            }
        }
    }
}
