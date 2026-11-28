use super::*;
mod paths;
mod targets;

impl TexcelApp {
    // Requests cancellation without dropping the save's result tracking.
    pub(super) fn request_cancel_save(&mut self, document_id: DocumentId) {
        let Some(token) = self.pending_saves.get(&document_id).copied() else {
            return;
        };
        if self.cancelling_saves.get(&document_id) == Some(&token) {
            return;
        }
        if self.file_worker.cancel(token) {
            self.cancelling_saves.insert(document_id, token);
            self.status = Some(Message::new("status-save-cancel-requested"));
        } else {
            self.status = Some(Message::new("status-save-cancel-failed"));
        }
    }

    /// Submits a save after its destination has passed all document checks.
    pub(super) fn save_document_to_path(&mut self, document_id: DocumentId, path: PathBuf) {
        let Some(table_ids) = self.workspace.documents.get(&document_id).map(|document| {
            document
                .tables
                .iter()
                .map(|table| table.id)
                .collect::<Vec<_>>()
        }) else {
            self.status = Some(Message::new("status-save-document-required"));
            return;
        };
        let string_cells = self
            .workspace
            .documents
            .get(&document_id)
            .map(|document| {
                document
                    .tables
                    .iter()
                    .flat_map(|table| {
                        table.stored_cells().filter_map(move |(address, _)| {
                            table.is_string_cell(address).then_some((table.id, address))
                        })
                    })
                    .collect::<std::collections::BTreeSet<_>>()
            })
            .unwrap_or_default();
        if self.pending_saves.contains_key(&document_id) {
            self.status = Some(Message::new("status-document-save-in-progress"));
            return;
        }
        let existing = self.file_backings.get(&document_id).cloned();
        let existing_source = self
            .workspace
            .documents
            .get(&document_id)
            .and_then(|document| document.source.as_deref().cloned())
            .or_else(|| existing.as_ref().and_then(|backing| backing.source.clone()));
        // Every editable document has a Markdown or CSV source; there is nothing to save without one.
        let Some(source) = existing_source else {
            self.status = Some(Message::new("status-save-document-required"));
            return;
        };
        if self.edit.is_editing()
            && self
                .edit_document_id
                .or_else(|| self.workspace.active_document().map(|document| document.id))
                == Some(document_id)
        {
            self.commit_edit(None);
        }
        if let Some(growth) = self.pending_text_growth_sessions.get_mut(&document_id)
            && growth.pending
        {
            growth.deferred_save_path = Some(path);
            growth.app_close_save |= self.app_close_dialog_open;
            self.status = None;
            return;
        }
        if self.edit.is_editing() && self.edit_document_id == Some(document_id) {
            self.status = Some(Message::new("status-finish-pending-input-before-close"));
            return;
        }
        let Some((revision, history_state_id, text_overlay_snapshot, changes)) =
            self.workspace.documents.get(&document_id).map(|document| {
                (
                    document.revision,
                    document.history_state_id(),
                    document.text_overlay_snapshot(),
                    document.changed_cell_snapshot(),
                )
            })
        else {
            self.status = Some(Message::new("status-save-document-required"));
            return;
        };
        // A known file must still match what was opened, unless the user chose to overwrite it.
        let expected = existing
            .as_ref()
            .map_or(SaveExpectation::CurrentAtStart, |backing| {
                if backing.overwrite_confirmed && path == backing.path {
                    SaveExpectation::CurrentAtStart
                } else {
                    SaveExpectation::KnownPath {
                        source_path: backing.path.clone(),
                        identity: backing.identity.clone(),
                    }
                }
            });
        let token = self.next_job_token(document_id, revision);
        let submitted_path = path.clone();
        let recovery_generation = self.recovery_generation_for_save(document_id);
        let submitted = match source {
            SourceDocument::Csv(source) => {
                let table_id = table_ids.first().copied();
                let Some(csv_table_id) = table_id else {
                    self.status = Some(Message::new("status-save-table-required"));
                    return;
                };
                let overlay_changes = Arc::new(changes);
                let text_overlay = crate::file_worker::TextOverlaySaveRequest {
                    base: text_overlay_snapshot,
                    changes: Arc::clone(&overlay_changes),
                    table_ids: Arc::new(table_ids.clone()),
                    string_cells: Arc::new(string_cells.clone()),
                };
                let changes = overlay_changes
                    .iter()
                    .filter(|&(changed_table, _, _)| Some(*changed_table) == table_id)
                    .map(|(_, address, cell)| CsvCellChange {
                        row: address.row,
                        column: address.column,
                        value: cell.as_ref().map(|cell| {
                            let is_string = self
                                .workspace
                                .documents
                                .get(&document_id)
                                .and_then(|document| {
                                    document
                                        .tables
                                        .iter()
                                        .find(|table| table.id == csv_table_id)
                                })
                                .is_some_and(|table| table.is_string_cell(*address));
                            cell.source_text_for_text_file(is_string)
                        }),
                    })
                    .collect();
                self.file_worker.save_csv(
                    token,
                    path,
                    crate::file_worker::CsvSaveRequest {
                        source,
                        table_id: csv_table_id,
                        changes,
                        text_overlay,
                    },
                    expected,
                )
            }
            SourceDocument::Markdown(source) => {
                let overlay_changes = Arc::new(changes);
                let text_overlay = crate::file_worker::TextOverlaySaveRequest {
                    base: text_overlay_snapshot,
                    changes: Arc::clone(&overlay_changes),
                    table_ids: Arc::new(table_ids.clone()),
                    string_cells: Arc::new(string_cells.clone()),
                };
                let changes = overlay_changes
                    .iter()
                    .filter_map(|(table_id, address, cell)| {
                        table_ids
                            .iter()
                            .position(|existing_id| *existing_id == *table_id)
                            .map(|table| MarkdownCellChange {
                                table,
                                row: address.row,
                                column: address.column,
                                value: cell.as_ref().map_or_else(String::new, |cell| {
                                    let is_string = self
                                        .workspace
                                        .documents
                                        .get(&document_id)
                                        .and_then(|document| document.tables.get(table))
                                        .is_some_and(|table| table.is_string_cell(*address));
                                    cell.source_text_for_text_file(is_string)
                                }),
                            })
                    })
                    .collect();
                self.file_worker
                    .save_markdown(token, path, source, changes, text_overlay, expected)
            }
        };
        match submitted {
            Ok(()) => {
                self.pending_saves.insert(document_id, token);
                self.pending_save_history_states
                    .insert(document_id, (token, history_state_id));
                if self
                    .pending_save_intents
                    .get(&document_id)
                    .is_some_and(|intent| intent.track_recent)
                {
                    self.recent_save_paths
                        .insert(document_id, (token, submitted_path));
                }
                self.pending_save_recovery_generations
                    .insert(document_id, (token, recovery_generation));
            }
            Err(error) => self.status = Some(submit_error_message(error)),
        }
    }

    // Continues a requested save only after adjacent growth changes the document revision.
    pub(super) fn continue_deferred_growth_save(
        &mut self,
        document_id: DocumentId,
        growth: PendingTextGrowthEdit,
    ) {
        let Some(path) = growth.deferred_save_path else {
            return;
        };
        self.save_document_to_path(document_id, path);
        if let Some(token) = self.pending_saves.get(&document_id).copied() {
            if growth.close_after_save {
                self.close_after_save.insert(document_id);
                if let Some(edit) = growth.close_edit {
                    self.close_after_save_edit.insert(document_id, edit);
                }
            }
            if growth.app_close_save {
                self.app_close_save_tokens.insert(document_id, token);
            }
        } else {
            if growth.close_after_save {
                self.pending_close_document = Some(document_id);
            }
            if growth.app_close_save {
                self.app_close_save_failed = true;
            }
        }
    }
}
