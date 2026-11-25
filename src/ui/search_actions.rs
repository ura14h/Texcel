use super::*;

impl TexcelApp {
    /// Opens search and replace for the active selection.
    pub(super) fn start_search_dialog(&mut self) {
        let Some(document) = self.workspace.active_document() else {
            return;
        };
        let Some(table) = document.active_table() else {
            return;
        };
        self.search_dialog = Some(SearchDialogState {
            document_id: document.id,
            table_id: table.id,
            query: String::new(),
            replacement: String::new(),
            scope: SearchScope::Selection,
            representation: crate::structure::SearchRepresentation::Source,
            case_sensitive: false,
            regular_expression: false,
            whole_cell: false,
            replace_mode: false,
            message: None,
            preview: None,
            preview_key: None,
            focus_query: true,
        });
    }

    /// Submits a search request for the current document version.
    pub(super) fn submit_search(&mut self, state: &SearchDialogState) {
        if let Some(dialog) = self.search_dialog.as_mut()
            && dialog.document_id == state.document_id
        {
            dialog.preview = None;
            dialog.preview_key = None;
        }
        if self.edit.is_editing() && self.edit_document_id == Some(state.document_id) {
            self.commit_edit(None);
            if let Some(dialog) = self.search_dialog.as_mut() {
                dialog.preview = None;
                dialog.preview_key = None;
                dialog.message = Some(Message::new("dialog-search-edit-committed"));
            }
            return;
        }
        let Some(document) = self.workspace.documents.get(&state.document_id) else {
            return;
        };
        if self.pending_structures.contains_key(&document.id)
            || self.pending_history_restores.contains_key(&document.id)
            || self.pending_saves.contains_key(&document.id)
        {
            if let Some(dialog) = self.search_dialog.as_mut() {
                dialog.message = Some(Message::new("dialog-search-busy"));
            }
            return;
        }
        let Some(key) = search_job_key(document, state) else {
            if let Some(dialog) = self.search_dialog.as_mut() {
                dialog.message = Some(Message::new("dialog-search-target-required"));
            }
            return;
        };
        let request = crate::structure::SearchRequest {
            structure: search_structure_request(document, state.table_id),
            language: self.language,
            query: state.query.clone(),
            replacement: state.replace_mode.then(|| state.replacement.clone()),
            ranges: key.ranges.clone(),
            case_sensitive: state.case_sensitive,
            regular_expression: state.regular_expression,
            whole_cell: state.whole_cell,
            representation: state.representation,
            formula_results_revision: self
                .calculation_snapshots
                .get(&document.id)
                .filter(|(revision, _)| *revision == document.calculation_revision)
                .map(|(revision, _)| *revision),
            formula_results: self
                .calculation_snapshots
                .get(&document.id)
                .filter(|(revision, _)| *revision == document.calculation_revision)
                .map(|(_, snapshot)| Arc::clone(snapshot)),
        };
        let document_id = document.id;
        let revision = document.revision;
        let token = self.next_job_token(document_id, revision);
        match self.file_worker.search(token, request) {
            Ok(()) => {
                self.pending_searches
                    .insert(document_id, PendingSearch { token, key });
                if let Some(dialog) = self.search_dialog.as_mut() {
                    dialog.preview = None;
                    dialog.preview_key = None;
                    dialog.message = Some(Message::new("dialog-search-preparing"));
                }
                self.status = None;
            }
            Err(error) => {
                if let Some(dialog) = self.search_dialog.as_mut() {
                    dialog.message = Some(submit_error_message(error));
                }
            }
        }
    }

    /// Checks that a search result still matches the active query.
    pub(super) fn search_job_key_is_current(&self, key: &SearchJobKey) -> bool {
        self.search_dialog
            .as_ref()
            .filter(|dialog| dialog.document_id == key.document_id)
            .and_then(|dialog| {
                self.workspace
                    .documents
                    .get(&key.document_id)
                    .and_then(|document| search_job_key(document, dialog))
            })
            .as_ref()
            == Some(key)
    }

    /// Checks that a data edit result still matches its request.
    pub(super) fn data_edit_job_key_is_current(&self, key: &DataEditJobKey) -> bool {
        self.data_edit_dialog
            .as_ref()
            .filter(|dialog| dialog.document_id == key.document_id)
            .and_then(|dialog| {
                self.workspace
                    .documents
                    .get(&key.document_id)
                    .and_then(|document| data_edit_job_key(document, dialog))
            })
            .as_ref()
            == Some(key)
    }

    /// Applies a search replacement after rechecking its target.
    pub(super) fn apply_search_prepared(
        &mut self,
        document_id: DocumentId,
        prepared: crate::document::PreparedStructure,
        expected_revision: u64,
        expected_calculation_revision: u64,
    ) {
        let preview_key = self
            .search_dialog
            .as_ref()
            .filter(|dialog| dialog.document_id == document_id)
            .and_then(|dialog| dialog.preview_key.as_ref());
        let selection_and_conditions_current =
            preview_key.is_some_and(|key| self.search_job_key_is_current(key));
        let current = selection_and_conditions_current
            && self
                .workspace
                .documents
                .get(&document_id)
                .is_some_and(|document| {
                    document.revision == expected_revision
                        && document.calculation_revision == expected_calculation_revision
                        && prepared.calculation_revision == expected_calculation_revision
                        && document.view(prepared.view_table_id).is_some_and(|view| {
                            view.layout.revision() == prepared.view_layout_revision
                        })
                });
        if !current {
            if let Some(dialog) = self.search_dialog.as_mut()
                && dialog.document_id == document_id
            {
                dialog.preview = None;
                dialog.preview_key = None;
                dialog.message = Some(Message::new("dialog-search-replace-stale"));
            }
            self.status = Some(Message::new(
                "status-search-replace-document-or-view-changed",
            ));
            return;
        }
        let replaced = self
            .search_dialog
            .as_ref()
            .and_then(|dialog| dialog.preview.as_ref())
            .map_or(0, |preview| preview.total_matches);
        let applied = self.workspace.documents.get_mut(&document_id).map_or_else(
            || Err(Message::new("status-document-not-found")),
            |document| {
                let (target, retired) =
                    document
                        .execute_prepared_structure(prepared)
                        .map_err(|error| {
                            Message::new("status-operation-failed-detail")
                                .with_message("detail", crate::i18n::document_error_message(error))
                        })?;
                document.select_table(target.0);
                let reset = document.take_calculation_reset_snapshot();
                let source = document.source.as_deref().cloned();
                Ok((retired, reset, source))
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
                self.search_dialog = None;
                self.status = Some(
                    Message::new("status-done-replaced").with_number("count", replaced as i64),
                );
            }
            Err(message) => {
                if let Some(dialog) = self.search_dialog.as_mut() {
                    dialog.message = Some(message.clone());
                }
                self.status = Some(message);
            }
        }
    }
}
