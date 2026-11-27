use super::*;

impl TexcelApp {
    /// Removes a recent-save candidate only for the matching worker request.
    fn clear_recent_save_candidate(&mut self, document_id: DocumentId, token: JobToken) {
        if self
            .recent_save_paths
            .get(&document_id)
            .is_some_and(|(stored_token, _)| *stored_token == token)
        {
            self.recent_save_paths.remove(&document_id);
        }
    }

    /// Adds the requested path to a worker's localized load failure.
    pub(super) fn handle_failure_result_with_path(
        &mut self,
        document_id: DocumentId,
        job_token: JobToken,
        operation: FileOperation,
        path: PathBuf,
        kind: FileResultKind,
    ) -> bool {
        let kind = match kind {
            FileResultKind::Failed(message)
                if matches!(
                    operation,
                    FileOperation::LoadCsv
                        | FileOperation::LoadMarkdown
                        | FileOperation::LoadHtml
                        | FileOperation::LoadWorkbook
                ) =>
            {
                FileResultKind::Failed(file_open_error(&path, message))
            }
            other => other,
        };
        self.handle_failure_result(document_id, job_token, operation, kind)
    }

    // Show a worker failure while keeping the document and its edits available.
    pub(super) fn handle_failure_result(
        &mut self,
        document_id: DocumentId,
        job_token: JobToken,
        operation: FileOperation,
        kind: FileResultKind,
    ) -> bool {
        if operation == FileOperation::PrepareStructure
            && self.pending_structures.get(&document_id) != Some(&job_token)
        {
            return false;
        }
        match kind {
            FileResultKind::Failed(message) => {
                if self.active_queued_open == Some(document_id)
                    && matches!(
                        operation,
                        FileOperation::LoadCsv
                            | FileOperation::LoadMarkdown
                            | FileOperation::LoadHtml
                            | FileOperation::LoadWorkbook
                    )
                {
                    self.queued_open_failure = Some(message.clone());
                }
                match operation {
                    FileOperation::LoadCsv => {
                        self.pending_csv_opens.remove(&document_id);
                    }
                    FileOperation::Save => {
                        self.pending_saves.remove(&document_id);
                        self.cancelling_saves.remove(&document_id);
                        if self
                            .pending_save_history_states
                            .get(&document_id)
                            .is_some_and(|(token, _)| *token == job_token)
                        {
                            self.pending_save_history_states.remove(&document_id);
                        }
                        self.clear_recent_save_candidate(document_id, job_token);
                        self.pending_save_preferences.remove(&document_id);
                        self.pending_save_recovery_generations.remove(&document_id);
                    }
                    FileOperation::SaveReport => {
                        self.pending_report_saves.remove(&document_id);
                    }
                    FileOperation::PrepareExport => {
                        self.pending_exports.remove(&document_id);
                        self.cancelling_exports.remove(&document_id);
                        self.finish_save_intent_failed(document_id);
                    }
                    FileOperation::Search => {
                        let pending = self.pending_searches.get(&document_id).cloned();
                        if let Some(pending) = pending
                            && pending.token == job_token
                        {
                            self.pending_searches.remove(&document_id);
                            let current = self.search_job_key_is_current(&pending.key);
                            if let Some(dialog) = self.search_dialog.as_mut()
                                && dialog.document_id == document_id
                            {
                                dialog.preview = None;
                                dialog.preview_key = None;
                                dialog.message = Some(if current {
                                    message.clone()
                                } else {
                                    Message::new("status-search-stale")
                                });
                            }
                        }
                    }
                    FileOperation::DataEdit => {
                        let pending = self.pending_data_edits.get(&document_id).cloned();
                        if let Some(pending) = pending
                            && pending.token == job_token
                        {
                            self.pending_data_edits.remove(&document_id);
                            let current = self.data_edit_job_key_is_current(&pending.key);
                            if let Some(dialog) = self.data_edit_dialog.as_mut()
                                && dialog.document_id == document_id
                            {
                                dialog.preview = None;
                                dialog.preview_key = None;
                                dialog.message = Some(if current {
                                    message.clone()
                                } else {
                                    Message::new("status-data-edit-stale")
                                });
                            }
                        }
                    }
                    FileOperation::PrepareStructure => {
                        if self.pending_structures.get(&document_id) == Some(&job_token) {
                            self.pending_structures.remove(&document_id);
                            self.finish_text_growth_failure(document_id);
                            self.pending_bulk_pastes.remove(&document_id);
                            self.pending_table_management.remove(&document_id);
                            self.finish_save_intent_failed(document_id);
                            self.pending_csv_promotion_cell_edits.remove(&document_id);
                            self.pending_csv_promotion_cell_batches.remove(&document_id);
                            self.pending_markdown_link_edits.remove(&document_id);
                        }
                    }
                    FileOperation::AutoFit => {
                        if self
                            .pending_auto_fits
                            .get(&document_id)
                            .is_some_and(|pending| pending.token == job_token)
                        {
                            self.pending_auto_fits.remove(&document_id);
                        }
                    }
                    FileOperation::RestoreStructureHistory => {
                        self.pending_history_restores
                            .retain(|id, token| *id != document_id || *token != job_token);
                    }
                    FileOperation::Export => {
                        self.pending_export_writes.remove(&document_id);
                        self.cancelling_exports.remove(&document_id);
                        self.finish_save_intent_failed(document_id);
                    }
                    FileOperation::ExportReport => {
                        self.pending_export_report_saves.remove(&document_id);
                    }
                    FileOperation::PrepareMarkdownDisplay => {
                        self.pending_markdown_displays
                            .retain(|_, pending| pending.0 != job_token);
                    }
                    FileOperation::LoadMarkdown => {
                        self.pending_markdown_loads.remove(&document_id);
                    }
                    FileOperation::LoadHtml => {
                        self.pending_html_loads.remove(&document_id);
                    }
                    FileOperation::LoadWorkbook => {
                        self.pending_workbook_loads.remove(&document_id);
                    }
                }
                if operation != FileOperation::PrepareMarkdownDisplay {
                    self.status = Some(message);
                }
                if operation == FileOperation::Save && self.close_after_save.remove(&document_id) {
                    self.pending_close_document = Some(document_id);
                    if let Some(edit) = self.close_after_save_edit.remove(&document_id) {
                        self.restore_close_edit(document_id, edit);
                    }
                }
                if operation == FileOperation::Save {
                    self.finish_app_close_save(document_id, job_token, false);
                }
            }
            FileResultKind::Cancelled => {
                match operation {
                    FileOperation::LoadCsv => {
                        self.pending_csv_opens.remove(&document_id);
                    }
                    FileOperation::Save => {
                        self.pending_saves.remove(&document_id);
                        self.cancelling_saves.remove(&document_id);
                        if self
                            .pending_save_history_states
                            .get(&document_id)
                            .is_some_and(|(token, _)| *token == job_token)
                        {
                            self.pending_save_history_states.remove(&document_id);
                        }
                        self.clear_recent_save_candidate(document_id, job_token);
                        self.pending_save_preferences.remove(&document_id);
                        self.pending_save_recovery_generations.remove(&document_id);
                        if self.close_after_save.remove(&document_id) {
                            self.pending_close_document = Some(document_id);
                            if let Some(edit) = self.close_after_save_edit.remove(&document_id) {
                                self.restore_close_edit(document_id, edit);
                            }
                        }
                        self.status = Some(Message::new("status-save-cancelled"));
                    }
                    FileOperation::SaveReport => {
                        self.pending_report_saves.remove(&document_id);
                    }
                    FileOperation::PrepareExport => {
                        self.pending_exports.remove(&document_id);
                        self.cancelling_exports.remove(&document_id);
                        self.export_configs.remove(&document_id);
                        self.finish_save_intent_failed(document_id);
                        self.status = Some(Message::new("status-export-cancelled"));
                    }
                    FileOperation::Search => {
                        let pending = self.pending_searches.get(&document_id).cloned();
                        if let Some(pending) = pending
                            && pending.token == job_token
                        {
                            self.pending_searches.remove(&document_id);
                            let current = self.search_job_key_is_current(&pending.key);
                            if let Some(dialog) = self.search_dialog.as_mut()
                                && dialog.document_id == document_id
                            {
                                dialog.preview = None;
                                dialog.preview_key = None;
                                dialog.message = Some(Message::new(if current {
                                    "dialog-search-cancelled"
                                } else {
                                    "dialog-search-stale"
                                }));
                            }
                        }
                    }
                    FileOperation::DataEdit => {
                        let pending = self.pending_data_edits.get(&document_id).cloned();
                        if let Some(pending) = pending
                            && pending.token == job_token
                        {
                            self.pending_data_edits.remove(&document_id);
                            let current = self.data_edit_job_key_is_current(&pending.key);
                            if let Some(dialog) = self.data_edit_dialog.as_mut()
                                && dialog.document_id == document_id
                            {
                                dialog.preview = None;
                                dialog.preview_key = None;
                                dialog.message = Some(Message::new(if current {
                                    "dialog-data-edit-cancelled"
                                } else {
                                    "dialog-data-edit-stale"
                                }));
                            }
                        }
                    }
                    FileOperation::PrepareStructure => {
                        if self.pending_structures.get(&document_id) == Some(&job_token) {
                            self.pending_structures.remove(&document_id);
                            let is_bulk_paste = self.pending_bulk_pastes.remove(&document_id);
                            let is_table_management =
                                self.pending_table_management.remove(&document_id);
                            self.finish_save_intent_failed(document_id);
                            self.pending_csv_promotion_cell_edits.remove(&document_id);
                            self.pending_csv_promotion_cell_batches.remove(&document_id);
                            self.status = Some(Message::new(if is_bulk_paste {
                                "status-paste-cancelled"
                            } else if is_table_management {
                                "status-table-change-cancelled"
                            } else {
                                "status-structure-cancelled"
                            }));
                        }
                    }
                    FileOperation::AutoFit => {
                        if self
                            .pending_auto_fits
                            .get(&document_id)
                            .is_some_and(|pending| pending.token == job_token)
                        {
                            self.pending_auto_fits.remove(&document_id);
                        }
                    }
                    FileOperation::RestoreStructureHistory => {
                        self.pending_history_restores
                            .retain(|id, token| *id != document_id || *token != job_token);
                        self.status =
                            Some(Message::new("status-structure-history-restore-cancelled"));
                    }
                    FileOperation::Export => {
                        self.pending_export_writes.remove(&document_id);
                        self.cancelling_exports.remove(&document_id);
                        self.finish_save_intent_failed(document_id);
                        self.status = Some(Message::new("status-export-cancelled"));
                    }
                    FileOperation::ExportReport => {
                        self.pending_export_report_saves.remove(&document_id);
                    }
                    FileOperation::PrepareMarkdownDisplay => {
                        self.pending_markdown_displays
                            .retain(|_, pending| pending.0 != job_token);
                    }
                    FileOperation::LoadMarkdown => {
                        self.pending_markdown_loads.remove(&document_id);
                    }
                    FileOperation::LoadHtml => {
                        self.pending_html_loads.remove(&document_id);
                    }
                    FileOperation::LoadWorkbook => {
                        self.pending_workbook_loads.remove(&document_id);
                    }
                }
                if operation == FileOperation::Save {
                    self.finish_app_close_save(document_id, job_token, false);
                }
            }
            _ => unreachable!("file result was routed to the wrong handler"),
        }
        false
    }
}
