use super::*;

impl TexcelApp {
    // Set the active cell and optionally extend the current selection.
    pub(super) fn select_cell(&mut self, table_id: u64, address: CellAddress, extend: bool) {
        let Some(document) = self.workspace.active_document_mut() else {
            return;
        };
        let Some(view) = document.view_mut(table_id) else {
            return;
        };
        if extend {
            view.selection.anchor = view.active_cell();
            view.selection.head = address;
        } else {
            view.set_selection(CellRange::single(address));
        }
    }

    // Move the selected range by the requested row and column offsets.
    pub(super) fn move_selection(&mut self, row_delta: i32, column_delta: i32, extend: bool) {
        let Some(document_id) = self.workspace.active_document().map(|document| document.id) else {
            return;
        };
        let next = self
            .workspace
            .documents
            .get(&document_id)
            .and_then(|document| {
                let table = document.active_table()?;
                let view = document.view(table.id)?;
                let active = view.active_cell();
                let current = if extend && view.selection.anchor == active {
                    view.selection.head
                } else {
                    active
                };
                Some(grid_navigation_target(
                    current,
                    row_delta,
                    column_delta,
                    MAX_ROWS,
                    MAX_COLUMNS,
                ))
            });
        let Some(next) = next else {
            return;
        };
        let Some(document) = self.workspace.documents.get_mut(&document_id) else {
            return;
        };
        let Some(table_id) = document.active_table().map(|table| table.id) else {
            return;
        };
        let Some(view) = document.view_mut(table_id) else {
            return;
        };
        if extend {
            if view.selection.anchor != view.active_cell() {
                view.selection.anchor = view.active_cell();
            }
            view.selection.head = next;
        } else {
            view.set_selection(CellRange::single(next));
        }
        self.request_grid_focus = true;
        if extend {
            let table_id = self
                .workspace
                .active_document()
                .and_then(|document| document.active_table().map(|table| table.id));
            if let Some(table_id) = table_id {
                self.request_scroll_to_selection = false;
                self.request_scroll_to_address = Some((document_id, table_id, next));
            }
        } else {
            self.request_scroll_to_selection = true;
        }
    }

    // Return the active table and current selection bounds.
    pub(super) fn active_selection(&self) -> Option<(u64, CellAddress)> {
        let document = self.workspace.active_document()?;
        let table = document.active_table()?;
        let address = document.view(table.id)?.active_cell();
        Some((table.id, address))
    }

    // Check whether the active document has separated selected ranges.
    pub(super) fn active_document_has_disjoint_selection(&self) -> bool {
        self.workspace
            .active_document()
            .and_then(|document| {
                let table = document.active_table()?;
                document.view(table.id)
            })
            .is_some_and(|view| !view.additional_selections.is_empty())
    }

    // Undo or redo the active document and restore the saved view state.
    pub(super) fn apply_undo(&mut self, redo: bool) {
        if self.edit.is_editing() {
            return;
        }
        if self
            .workspace
            .active_document()
            .is_some_and(|document| self.document_bulk_edit_pending(document.id))
        {
            self.status = Some(Message::new("status-cell-edit-paused-during-bulk"));
            return;
        }
        if self
            .workspace
            .active_document()
            .is_some_and(|document| self.pending_saves.contains_key(&document.id))
        {
            self.status = Some(Message::new("status-undo-redo-after-save"));
            return;
        }
        let history_request = self
            .workspace
            .active_document()
            .and_then(|document| document.structure_history_restore_request(redo));
        if let Some(request) = history_request {
            let document_id = request.document_id;
            let revision = self
                .workspace
                .documents
                .get(&document_id)
                .map_or(0, |document| document.revision);
            let token = self.next_job_token(document_id, revision);
            match self.file_worker.restore_structure_history(token, request) {
                Ok(()) => {
                    self.pending_history_restores.insert(document_id, token);
                    self.status = None;
                }
                Err(error) => {
                    self.status = Some(
                        Message::new("status-structure-restore-failed")
                            .with_message("detail", submit_error_message(error)),
                    );
                }
            }
            return;
        }
        let mut changed_document = None;
        let mut calculation_reset = None;
        let mut source_after = None;
        if let Some(document) = self.workspace.active_document_mut() {
            let result = if redo {
                document.redo()
            } else {
                document.undo()
            };
            match result {
                Ok(Some((table_id, address))) => {
                    changed_document = Some(document.id);
                    document.select_table(table_id);
                    calculation_reset = document.take_calculation_reset_snapshot();
                    if calculation_reset.is_none()
                        && let Some(view) = document.view_mut(table_id)
                    {
                        view.selection = CellRange::single(address);
                    }
                    source_after = document.source.as_deref().cloned();
                    self.request_grid_focus = true;
                    self.request_scroll_to_selection = true;
                }
                Ok(None) => (),
                Err(error) => {
                    self.status = Some(
                        Message::new("status-history-apply-failed")
                            .with_message("detail", crate::i18n::document_error_message(error)),
                    );
                }
            }
        }
        if let Some(document_id) = changed_document {
            if let Some(backing) = self.file_backings.get_mut(&document_id)
                && calculation_reset.is_some()
            {
                backing.source = source_after;
            }
            if let Some(snapshot) = calculation_reset {
                self.clear_calculation_jobs_for_reset(document_id);
                let revision = self
                    .workspace
                    .documents
                    .get(&document_id)
                    .map_or(0, |document| document.calculation_revision);
                let token = self.next_job_token(document_id, revision);
                self.initialize_calculation_shared(token, snapshot);
            } else {
                self.schedule_calculation(document_id);
            }
        }
    }
}
