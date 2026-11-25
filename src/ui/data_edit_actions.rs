use super::*;

impl TexcelApp {
    // Open the dialog for the requested table data operation.
    pub(super) fn start_data_edit_dialog(&mut self, kind: DataEditKind) {
        let Some(document) = self.workspace.active_document() else {
            return;
        };
        let Some(table) = document.active_table() else {
            return;
        };
        let Some(view) = document.view(table.id) else {
            return;
        };
        let range = view.selection;
        let (start, end) = range.normalized();
        let selected_ranges = view.selected_ranges().collect::<Vec<_>>();
        self.data_edit_dialog = Some(DataEditDialogState {
            document_id: document.id,
            table_id: table.id,
            kind,
            range,
            range_text: format!(
                "{}:{}",
                crate::xlsx_adapter::cell_reference(start),
                crate::xlsx_adapter::cell_reference(end)
            ),
            selected_ranges,
            delimiter: ",".to_owned(),
            destination_column: column_name(end.column.saturating_add(1)),
            overwrite: false,
            sort_keys: vec![SortKeyEditor {
                column: column_name(start.column),
                kind: crate::structure::SortValueKind::Text,
                descending: false,
            }],
            preview: None,
            preview_key: None,
            message: None,
        });
    }

    // Send the confirmed data operation to the worker.
    pub(super) fn submit_data_edit(&mut self, state: &DataEditDialogState) {
        if let Some(dialog) = self.data_edit_dialog.as_mut()
            && dialog.document_id == state.document_id
        {
            dialog.preview = None;
            dialog.preview_key = None;
        }
        if self.edit.is_editing() && self.edit_document_id == Some(state.document_id) {
            self.commit_edit(None);
            if let Some(dialog) = self.data_edit_dialog.as_mut() {
                dialog.preview = None;
                dialog.message = Some(Message::new("dialog-edit-committed"));
            }
            return;
        }
        let Some(document) = self.workspace.documents.get(&state.document_id) else {
            return;
        };
        if self.pending_structures.contains_key(&document.id)
            || self.pending_history_restores.contains_key(&document.id)
            || self.pending_saves.contains_key(&document.id)
            || self.pending_data_edits.contains_key(&document.id)
        {
            if let Some(dialog) = self.data_edit_dialog.as_mut() {
                dialog.message = Some(Message::new("dialog-operation-busy"));
            }
            return;
        }
        if !document
            .tables
            .iter()
            .any(|table| table.id == state.table_id)
        {
            return;
        }
        let Some(key) = data_edit_job_key(document, state) else {
            if let Some(dialog) = self.data_edit_dialog.as_mut() {
                dialog.message = Some(Message::new("dialog-no-target"));
            }
            return;
        };
        if !matches!(
            state.kind,
            DataEditKind::TrimWhitespace | DataEditKind::SortRows
        ) && state.selected_ranges.len() > 1
        {
            if let Some(dialog) = self.data_edit_dialog.as_mut() {
                dialog.message = Some(Message::new("dialog-multi-range-limit"));
            }
            return;
        }
        let range = if matches!(
            state.kind,
            DataEditKind::TrimWhitespace | DataEditKind::SortRows
        ) {
            state.range
        } else if let Some(range) = parse_cell_range_text(&state.range_text) {
            range
        } else {
            if let Some(dialog) = self.data_edit_dialog.as_mut() {
                dialog.message = Some(Message::new("dialog-range-format"));
            }
            return;
        };
        let (start, end) = range.normalized();
        let destination_column = if matches!(
            state.kind,
            DataEditKind::SplitColumn | DataEditKind::JoinColumns
        ) {
            match parse_column_index(&state.destination_column) {
                Some(column) => Some(column),
                None => {
                    if let Some(dialog) = self.data_edit_dialog.as_mut() {
                        dialog.message = Some(Message::new("dialog-destination-column-format"));
                    }
                    return;
                }
            }
        } else {
            None
        };
        let action = match state.kind {
            DataEditKind::TrimWhitespace => crate::structure::DataEditAction::TrimWhitespace {
                ranges: state
                    .selected_ranges
                    .iter()
                    .copied()
                    .map(|range| (state.table_id, range))
                    .collect(),
            },
            DataEditKind::SplitColumn => crate::structure::DataEditAction::SplitColumn {
                table_id: state.table_id,
                rows: CellRange {
                    anchor: CellAddress {
                        row: start.row,
                        column: start.column,
                    },
                    head: CellAddress {
                        row: end.row,
                        column: start.column,
                    },
                },
                source_column: start.column,
                delimiter: state.delimiter.clone(),
                destination_column: destination_column.unwrap_or(0),
                overwrite: state.overwrite,
            },
            DataEditKind::JoinColumns => crate::structure::DataEditAction::JoinColumns {
                table_id: state.table_id,
                range,
                delimiter: state.delimiter.clone(),
                destination_column: destination_column.unwrap_or(0),
                overwrite: state.overwrite,
            },
            DataEditKind::Transpose => crate::structure::DataEditAction::Transpose {
                table_id: state.table_id,
                range,
                overwrite: state.overwrite,
            },
            DataEditKind::SortRows => {
                let mut keys = Vec::new();
                let mut seen = BTreeSet::new();
                for editor in &state.sort_keys {
                    let Some(column) = parse_column_index(&editor.column) else {
                        if let Some(dialog) = self.data_edit_dialog.as_mut() {
                            dialog.message = Some(
                                Message::new("dialog-sort-column-invalid")
                                    .with_text("column", &editor.column),
                            );
                        }
                        return;
                    };
                    if !seen.insert(column) {
                        if let Some(dialog) = self.data_edit_dialog.as_mut() {
                            dialog.message = Some(Message::new("dialog-sort-duplicate-column"));
                        }
                        return;
                    }
                    keys.push(crate::structure::SortKey {
                        column,
                        kind: editor.kind,
                        descending: editor.descending,
                    });
                }
                let header_row = document.header_row(state.table_id);
                crate::structure::DataEditAction::SortRows {
                    table_id: state.table_id,
                    keys,
                    header_row,
                }
            }
            DataEditKind::DetectDuplicates | DataEditKind::RemoveDuplicates => {
                let header_row = document.header_row(state.table_id);
                crate::structure::DataEditAction::Duplicates {
                    table_id: state.table_id,
                    range,
                    header_row,
                    delete: state.kind == DataEditKind::RemoveDuplicates,
                }
            }
        };
        let calculation_snapshot = self
            .calculation_snapshots
            .get(&document.id)
            .filter(|(revision, _)| *revision == document.calculation_revision);
        let request = crate::structure::DataEditRequest {
            structure: search_structure_request(document, state.table_id),
            action,
            formula_results_revision: calculation_snapshot.map(|(revision, _)| *revision),
            formula_results: calculation_snapshot.map(|(_, snapshot)| Arc::clone(snapshot)),
        };
        let document_id = document.id;
        let revision = document.revision;
        let token = self.next_job_token(document_id, revision);
        match self.file_worker.prepare_data_edit(token, request) {
            Ok(()) => {
                self.pending_data_edits
                    .insert(document_id, PendingDataEdit { token, key });
                if let Some(dialog) = self.data_edit_dialog.as_mut() {
                    dialog.preview = None;
                    dialog.preview_key = None;
                    dialog.message = Some(Message::new("dialog-preparing"));
                }
                self.status = None;
            }
            Err(error) => {
                if let Some(dialog) = self.data_edit_dialog.as_mut() {
                    dialog.message = Some(submit_error_message(error));
                }
            }
        }
    }

    // Apply a prepared data operation if the document still matches its snapshot.
    pub(super) fn apply_data_edit_prepared(
        &mut self,
        prepared: crate::structure::PreparedDataEdit,
    ) {
        let document_id = prepared.document_id;
        let Some(prepared_structure) = prepared.prepared.as_ref() else {
            return;
        };
        let preview_key = self
            .data_edit_dialog
            .as_ref()
            .filter(|dialog| dialog.document_id == document_id)
            .and_then(|dialog| dialog.preview_key.as_ref());
        let selection_and_conditions_current =
            preview_key.is_some_and(|key| self.data_edit_job_key_is_current(key));
        let current = selection_and_conditions_current
            && self
                .workspace
                .documents
                .get(&document_id)
                .is_some_and(|document| {
                    document.revision == prepared.document_revision
                        && document.calculation_revision == prepared.calculation_revision
                        && prepared_structure.calculation_revision == prepared.calculation_revision
                        && document
                            .view(prepared_structure.view_table_id)
                            .is_some_and(|view| {
                                view.layout.revision() == prepared_structure.view_layout_revision
                            })
                });
        if !current {
            if let Some(dialog) = self.data_edit_dialog.as_mut()
                && dialog.document_id == document_id
            {
                dialog.preview = None;
                dialog.preview_key = None;
                dialog.message = Some(Message::new("dialog-data-edit-stale"));
            }
            return;
        }
        let applied = self.workspace.documents.get_mut(&document_id).map_or_else(
            || Err(Message::new("status-document-not-found")),
            |document| {
                let (target, retired) = document
                    .execute_prepared_structure(
                        prepared
                            .prepared
                            .expect("prepared structure was checked before applying"),
                    )
                    .map_err(|error| {
                        Message::new("status-operation-failed-detail")
                            .with_message("detail", crate::i18n::document_error_message(error))
                    })?;
                document.select_table(target.0);
                let reset = document.take_calculation_reset_snapshot();
                Ok((retired, reset, document.source.as_deref().cloned()))
            },
        );
        match applied {
            Ok((retired, reset, source)) => {
                if let Some(tables) = retired {
                    self.queue_retired_structure_tables(tables);
                }
                if let Some(backing) = self.file_backings.get_mut(&document_id) {
                    backing.source = source;
                }
                self.clear_calculation_jobs_for_reset(document_id);
                if let Some(snapshot) = reset {
                    let revision = self
                        .workspace
                        .documents
                        .get(&document_id)
                        .map_or(0, |document| document.calculation_revision);
                    let token = self.next_job_token(document_id, revision);
                    self.initialize_calculation_shared(token, snapshot);
                }
                self.data_edit_dialog = None;
                self.status = Some(Message::new("status-done-data-edit-applied"));
            }
            Err(message) => {
                if let Some(dialog) = self.data_edit_dialog.as_mut() {
                    dialog.message = Some(message.clone());
                }
                self.status = Some(message);
            }
        }
    }
}
