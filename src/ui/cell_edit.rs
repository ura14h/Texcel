use super::*;
mod source_limit;

use source_limit::{source_resource_limit_error, text_source_extent};

impl TexcelApp {
    /// Starts editing the specified cell.
    pub(super) fn begin_cell_edit(&mut self, table_id: u64, address: CellAddress) {
        let Some(document_id) = self.workspace.active_document().map(|document| document.id) else {
            self.request_editor_focus = false;
            return;
        };
        self.restore_pending_text_growth_draft(document_id);
        if self.close_after_save.contains(&document_id) {
            self.status = Some(Message::new("status-save-before-edit"));
            self.request_editor_focus = false;
            return;
        }
        if self.cell_edit_locked(document_id) {
            self.status = Some(Message::new("status-cell-edit-paused-during-bulk"));
            self.request_editor_focus = false;
            return;
        }
        let address_is_active = self
            .workspace
            .documents
            .get(&document_id)
            .and_then(|document| document.view(table_id))
            .is_some_and(|view| {
                let (start, end) = view.selection.normalized();
                view.active_cell() == address
                    && address.row >= start.row
                    && address.row <= end.row
                    && address.column >= start.column
                    && address.column <= end.column
            });
        if !address_is_active {
            self.select_cell(table_id, address, false);
        }
        let Some(address) = self.normalize_selection_for_layout(document_id, table_id) else {
            self.request_editor_focus = false;
            return;
        };
        let original = self
            .workspace
            .active_document()
            .and_then(|document| document.tables.iter().find(|table| table.id == table_id))
            .and_then(|table| table.cell(address))
            .map_or(CellInput::Empty, |cell| cell.input.clone());
        self.begin_edit_session(
            document_id,
            table_id,
            address,
            original,
            crate::document::EditTarget::Cell,
        );
        self.request_editor_focus = true;
        self.request_scroll_to_selection = true;
        self.grid_focused = true;
    }

    /// Starts editing the selected cell without replacing its text.
    pub(super) fn begin_selected_edit(&mut self) {
        if self.active_document_has_disjoint_selection() {
            self.status = Some(Message::new("status-cell-edit-single-range-required"));
            return;
        }
        let selection = self.active_selection();
        let Some((table_id, address)) = selection else {
            return;
        };
        self.begin_cell_edit(table_id, address);
    }

    /// Starts editing the selected cell with replacement text.
    pub(super) fn begin_selected_replacing(&mut self, text: String) {
        if self.active_document_has_disjoint_selection() {
            self.status = Some(Message::new("status-cell-edit-single-range-required"));
            return;
        }
        let Some(document_id) = self.workspace.active_document().map(|document| document.id) else {
            return;
        };
        if self.close_after_save.contains(&document_id) {
            self.status = Some(Message::new("status-save-before-edit"));
            self.request_editor_focus = false;
            return;
        }
        if self.cell_edit_locked(document_id) {
            self.status = Some(Message::new("status-cell-edit-paused-during-bulk"));
            self.request_editor_focus = false;
            return;
        }
        let Some(table_id) = self
            .workspace
            .active_document()
            .and_then(|document| document.active_table())
            .map(|table| table.id)
        else {
            return;
        };
        let Some(address) = self.normalize_selection_for_layout(document_id, table_id) else {
            self.request_editor_focus = false;
            return;
        };
        let original = self
            .workspace
            .active_document()
            .and_then(|document| document.tables.iter().find(|table| table.id == table_id))
            .and_then(|table| table.cell(address))
            .map_or(CellInput::Empty, |cell| cell.input.clone());
        self.begin_edit_session(
            document_id,
            table_id,
            address,
            original,
            crate::document::EditTarget::Cell,
        );
        self.edit.buffer = text;
        self.edit.mode = EditMode::Input;
        self.edit.mark_buffer_changed();
        self.edit.anchor_byte = self.edit.buffer.len();
        self.edit.cursor_byte = self.edit.buffer.len();
        self.request_editor_focus = true;
        self.start_input_sequence(document_id, table_id, address);
        self.refresh_formula_references();
    }

    /// Creates a shared edit session for a cell or editor panel.
    pub(super) fn begin_edit_session(
        &mut self,
        document_id: DocumentId,
        table_id: u64,
        address: CellAddress,
        original: CellInput,
        target: crate::document::EditTarget,
    ) {
        if self.cell_edit_locked(document_id) {
            self.status = Some(Message::new("status-cell-edit-paused-during-bulk"));
            return;
        }
        self.long_text_panel_open = target == crate::document::EditTarget::DetailPanel;
        self.long_text_panel_dismissed_session = None;
        self.edit.begin(table_id, address, original, target);
        self.edit_document_id = Some(document_id);
    }

    /// Commits the edit buffer through the document command path.
    pub(super) fn commit_edit(&mut self, movement: Option<(i32, i32)>) {
        self.commit_edit_with_tab(movement, false, false);
    }

    /// Commits one edit and optionally keeps its Tab entry column for the next cell.
    pub(super) fn commit_edit_with_tab(
        &mut self,
        movement: Option<(i32, i32)>,
        continue_tab_input: bool,
        return_to_tab_column: bool,
    ) {
        self.cancel_formula_reference_drag();
        let edit_document_id = self
            .edit_document_id
            .or_else(|| self.workspace.active_document().map(|document| document.id));
        if edit_document_id.is_some_and(|document_id| self.cell_edit_locked(document_id)) {
            self.status = Some(Message::new("status-cell-edit-paused-during-bulk"));
            return;
        }
        self.long_text_panel_open = false;
        let Some(table_id) = self.edit.table_id else {
            self.edit.reset();
            self.edit_document_id = None;
            return;
        };
        let Some(address) = self.edit.address else {
            self.edit.reset();
            self.edit_document_id = None;
            return;
        };
        let sequence_origin = self.input_sequence_origin.filter(|origin| {
            edit_document_id == Some(origin.document_id)
                && origin.table_id == table_id
                && origin.expected_cell == address
        });
        let return_column = return_to_tab_column
            .then_some(sequence_origin.map(|origin| origin.column))
            .flatten()
            .filter(|_| {
                movement
                    .is_some_and(|(row_delta, column_delta)| row_delta != 0 && column_delta == 0)
            });
        let continue_tab_input = sequence_origin.is_some()
            && continue_tab_input
            && movement.is_some_and(|(_, column_delta)| column_delta != 0);
        let Some(input) = self.edit.completed_input() else {
            self.edit.reset();
            self.edit_document_id = None;
            return;
        };
        let Some(document_id) = edit_document_id else {
            self.edit.reset();
            self.edit_document_id = None;
            return;
        };
        let existing = self
            .workspace
            .documents
            .get(&document_id)
            .and_then(|document| document.tables.iter().find(|table| table.id == table_id))
            .and_then(|table| table.cell(address))
            .cloned();
        let changed_input = changed_editor_input(input, existing.as_ref());
        let expands_text_source = changed_input.is_some()
            && self
                .workspace
                .documents
                .get(&document_id)
                .and_then(|document| text_source_extent(document, table_id))
                .is_some_and(|(rows, columns)| address.row >= rows || address.column >= columns);
        let mut changed_document = None;
        if let Some(input) = changed_input {
            let cell = Cell {
                input,
                imported_result: None,
                calculation: None,
            };
            let capacity = self.workspace.documents.get(&document_id).map_or(
                crate::document::TextCellResourceCheck::NotApplicable,
                |document| document.check_text_cell_resource_limit(table_id, address, Some(&cell)),
            );
            if let crate::document::TextCellResourceCheck::ExceedsLimit(limit) = capacity {
                self.status = Some(source_resource_limit_error(
                    self.workspace.documents.get(&document_id),
                    limit,
                ));
                self.request_editor_focus = true;
                return;
            }
            let needs_worker = expands_text_source
                || capacity == crate::document::TextCellResourceCheck::NeedsWorker;
            if needs_worker {
                self.submit_structure_operation(
                    table_id,
                    StructureOperation::SetTextCell {
                        table_id,
                        address,
                        cell: Box::new(cell),
                    },
                    movement,
                    continue_tab_input,
                    return_column,
                );
                return;
            }
            if let Some(document) = self.workspace.documents.get_mut(&document_id) {
                match document.set_cell(table_id, address, Some(cell)) {
                    Ok(()) => {
                        changed_document = Some(document_id);
                    }
                    Err(error) => {
                        self.status = Some(
                            Message::new("status-cell-edit-failed")
                                .with_message("detail", crate::i18n::document_error_message(error)),
                        );
                        self.request_editor_focus = true;
                        return;
                    }
                }
            }
            if let crate::document::TextCellResourceCheck::ReachedLimit(limit) = capacity {
                self.status = Some(
                    Message::new("status-resource-limit-reached")
                        .with_message("limit", crate::i18n::resource_limit_message(limit)),
                );
            }
        }
        self.edit.reset();
        self.edit_document_id = None;
        if let Some(document_id) = changed_document {
            self.schedule_calculation(document_id);
        }
        if self.workspace.active_document == Some(document_id)
            && let Some((row_delta, column_delta)) = movement
        {
            let moved_to_origin = return_column.is_some_and(|column| {
                self.move_to_input_sequence_column(document_id, table_id, column, row_delta)
            });
            let range_move = return_column.is_none()
                && row_delta != 0
                && column_delta == 0
                && self
                    .workspace
                    .active_document()
                    .and_then(|document| {
                        let table = document.active_table()?;
                        document.view(table.id)
                    })
                    .is_some_and(|view| {
                        let (start, end) = view.selection.normalized();
                        start != end
                    });
            if !moved_to_origin && (!range_move || !self.cycle_selection_cell(row_delta < 0)) {
                self.move_selection(row_delta, column_delta, false);
            }
            self.finish_input_sequence_move(document_id, table_id, continue_tab_input);
        } else {
            self.input_sequence_origin = None;
            self.request_grid_focus = true;
        }
    }

    // Restores a failed adjacent edit when its document is active again.
    pub(super) fn finish_text_growth_failure(&mut self, document_id: DocumentId) {
        let Some(mut pending) = self.pending_text_growth_sessions.remove(&document_id) else {
            return;
        };
        if matches!(
            self.pending_growth_selection.as_ref(),
            Some(
                grid_events::PendingGrowthSelection::Header { document_id: target, .. }
                    | grid_events::PendingGrowthSelection::Table { document_id: target, .. }
            ) if *target == document_id
        ) {
            self.pending_growth_selection = None;
        }
        pending.pending = false;
        if self.workspace.active_document == Some(document_id)
            && (!self.edit.is_editing() || self.edit_document_id == Some(document_id))
        {
            self.edit = pending.session;
            self.edit_document_id = Some(document_id);
            self.request_editor_focus = true;
        } else {
            if let (Some(table_id), Some(address), Some(original)) = (
                pending.session.table_id,
                pending.session.address,
                pending.session.original.clone(),
            ) {
                self.pending_recovered_cell_edits.insert(
                    document_id,
                    crate::recovery::RecoveryCellEdit {
                        table_id,
                        address,
                        original,
                        buffer: pending.session.buffer.clone(),
                        cursor_byte: pending.session.cursor_byte,
                        anchor_byte: pending.session.anchor_byte,
                    },
                );
            }
        }
        if pending.close_after_save {
            self.pending_close_document = Some(document_id);
        }
        if pending.app_close_save {
            self.app_close_save_failed = true;
        }
    }

    // Reattaches a failed cell draft after the user returns to its document.
    pub(super) fn restore_pending_text_growth_draft(&mut self, document_id: DocumentId) {
        if self.workspace.active_document != Some(document_id)
            || self.edit.is_editing()
            || self
                .pending_text_growth_sessions
                .get(&document_id)
                .is_none_or(|pending| pending.pending)
        {
            return;
        }
        let Some(pending) = self.pending_text_growth_sessions.remove(&document_id) else {
            return;
        };
        self.edit = pending.session;
        self.edit_document_id = Some(document_id);
        self.request_editor_focus = true;
    }

    /// Discards the active edit buffer and restores selection.
    pub(super) fn cancel_edit(&mut self) {
        self.cancel_formula_reference_drag();
        self.clear_pending_edit_input();
        self.long_text_panel_open = false;
        self.edit.reset();
        self.edit_document_id = None;
        self.request_editor_focus = false;
        self.request_grid_focus = true;
    }

    /// Inserts a line break into the active cell edit.
    /// Markdown cells take the `<br>` tag they are saved with; CSV cells take a real newline.
    pub(super) fn insert_line_break(&mut self, ctx: &egui::Context) {
        let Some((document_id, inserted)) = self.workspace.active_document().map(|document| {
            let markdown = matches!(
                document.source.as_deref(),
                Some(crate::source::SourceDocument::Markdown(_))
            );
            (document.id, if markdown { "<br>" } else { "\n" })
        }) else {
            return;
        };
        let id = editor_id(document_id, self.edit.session_id);
        let state = egui::widgets::text_edit::TextEditState::load(ctx, id);
        let char_range = state
            .as_ref()
            .and_then(|state| state.cursor.char_range())
            .unwrap_or_else(|| {
                egui::text::CCursorRange::one(egui::text::CCursor::end_of_str(&self.edit.buffer))
            });
        let range = char_range.as_sorted_char_range();
        let char_start = range.start.0;
        let char_end = range.end.0;
        let byte_start = byte_index_for_char(&self.edit.buffer, char_start);
        let byte_end = byte_index_for_char(&self.edit.buffer, char_end);
        self.edit
            .buffer
            .replace_range(byte_start..byte_end, inserted);
        self.edit.mark_buffer_changed();
        if let Some(mut state) = state {
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(
                    egui::text::CCursor::new(char_start + inserted.chars().count()),
                )));
            state.store(ctx, id);
        }
    }
}
