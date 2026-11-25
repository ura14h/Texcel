use super::*;

impl TexcelApp {
    /// Completes a source edit that needs exact worker-side size validation.
    pub(super) fn submit_text_source_changes(
        &mut self,
        pending: crate::document::PendingTextSourceChanges,
        completion_status: Option<Message>,
    ) {
        let crate::document::PendingTextSourceChanges {
            table_id,
            changes,
            string_overrides,
        } = pending;
        self.submit_structure_operation(
            table_id,
            StructureOperation::SetTextCells {
                table_id,
                changes,
                string_overrides,
                completion_status,
            },
            None,
            false,
            None,
        );
    }

    // Prepare a row, column, or table operation for the active document.
    pub(super) fn request_structure_action(&mut self, action: StructureAction) {
        if self
            .workspace
            .active_document()
            .is_some_and(|document| self.document_bulk_edit_pending(document.id))
        {
            self.status = Some(Message::new("status-cell-edit-paused-during-bulk"));
            return;
        }
        if matches!(
            action,
            StructureAction::AddTable
                | StructureAction::DuplicateTable
                | StructureAction::DeleteTable
                | StructureAction::RenameTable
                | StructureAction::MarkdownAlignLeft
                | StructureAction::MarkdownAlignCenter
                | StructureAction::MarkdownAlignRight
                | StructureAction::PromoteCsv
        ) {
            self.request_table_management(action, None, None);
            return;
        }
        if self.edit.is_editing() {
            self.commit_edit(None);
        }
        let Some(document) = self.workspace.active_document() else {
            return;
        };
        let document_id = document.id;
        if self.pending_saves.contains_key(&document_id) {
            self.status = Some(Message::new("status-row-column-change-after-save"));
            return;
        }
        if self.pending_structures.contains_key(&document_id) {
            self.status = Some(Message::new("status-row-column-change-preparing"));
            return;
        }
        if self.pending_history_restores.contains_key(&document_id) {
            self.status = Some(Message::new("status-structure-history-restoring"));
            return;
        }
        let Some(table) = document.active_table() else {
            return;
        };
        let table_id = table.id;
        let Some(view) = document.view(table_id) else {
            return;
        };
        if !view.additional_selections.is_empty() {
            self.status = Some(Message::new(
                "status-row-column-operation-single-range-required",
            ));
            return;
        }
        let (start, end) = view.selection.normalized();
        let selected_rows = end.row.saturating_sub(start.row).saturating_add(1);
        let selected_columns = end.column.saturating_sub(start.column).saturating_add(1);
        let operation = match action {
            StructureAction::InsertRows => StructureOperation::Insert {
                table_id,
                axis: StructureAxis::Rows,
                index: start.row,
                count: selected_rows,
            },
            StructureAction::InsertColumns => StructureOperation::Insert {
                table_id,
                axis: StructureAxis::Columns,
                index: start.column,
                count: selected_columns,
            },
            StructureAction::DeleteRows => StructureOperation::Delete {
                table_id,
                axis: StructureAxis::Rows,
                index: start.row,
                count: selected_rows,
            },
            StructureAction::DeleteColumns => StructureOperation::Delete {
                table_id,
                axis: StructureAxis::Columns,
                index: start.column,
                count: selected_columns,
            },
            StructureAction::DuplicateRows => StructureOperation::Duplicate {
                table_id,
                axis: StructureAxis::Rows,
                index: start.row,
                count: selected_rows,
            },
            StructureAction::DuplicateColumns => StructureOperation::Duplicate {
                table_id,
                axis: StructureAxis::Columns,
                index: start.column,
                count: selected_columns,
            },
            StructureAction::MoveRowsUp if start.row > 0 => StructureOperation::Move {
                table_id,
                axis: StructureAxis::Rows,
                index: start.row,
                count: selected_rows,
                destination: start.row - 1,
            },
            StructureAction::MoveRowsDown => StructureOperation::Move {
                table_id,
                axis: StructureAxis::Rows,
                index: start.row,
                count: selected_rows,
                destination: start.row.saturating_add(1),
            },
            StructureAction::MoveColumnsLeft if start.column > 0 => StructureOperation::Move {
                table_id,
                axis: StructureAxis::Columns,
                index: start.column,
                count: selected_columns,
                destination: start.column - 1,
            },
            StructureAction::MoveColumnsRight => StructureOperation::Move {
                table_id,
                axis: StructureAxis::Columns,
                index: start.column,
                count: selected_columns,
                destination: start.column.saturating_add(1),
            },
            StructureAction::FillCopy => {
                let seed = if selected_rows > 1 {
                    CellRange {
                        anchor: start,
                        head: CellAddress {
                            row: start.row,
                            column: end.column,
                        },
                    }
                } else if selected_columns > 1 {
                    CellRange {
                        anchor: start,
                        head: CellAddress {
                            row: end.row,
                            column: start.column,
                        },
                    }
                } else {
                    self.status = Some(Message::new("status-copy-fill-range-required"));
                    return;
                };
                StructureOperation::Fill {
                    table_id,
                    source: seed,
                    target: view.selection,
                    mode: crate::document::FillMode::Copy,
                }
            }
            StructureAction::FillDown => {
                if selected_rows < 2 {
                    self.status = Some(Message::new("status-fill-down-multiple-rows-required"));
                    return;
                }
                StructureOperation::Fill {
                    table_id,
                    source: CellRange {
                        anchor: start,
                        head: CellAddress {
                            row: start.row,
                            column: end.column,
                        },
                    },
                    target: view.selection,
                    mode: crate::document::FillMode::Copy,
                }
            }
            StructureAction::FillRight => {
                if selected_columns < 2 {
                    self.status = Some(Message::new("status-fill-right-multiple-columns-required"));
                    return;
                }
                StructureOperation::Fill {
                    table_id,
                    source: CellRange {
                        anchor: start,
                        head: CellAddress {
                            row: end.row,
                            column: start.column,
                        },
                    },
                    target: view.selection,
                    mode: crate::document::FillMode::Copy,
                }
            }
            StructureAction::FillSeries => {
                let seed = if selected_columns == 1 && selected_rows >= 3 {
                    CellRange {
                        anchor: start,
                        head: CellAddress {
                            row: start.row + 1,
                            column: start.column,
                        },
                    }
                } else if selected_rows == 1 && selected_columns >= 3 {
                    CellRange {
                        anchor: start,
                        head: CellAddress {
                            row: start.row,
                            column: start.column + 1,
                        },
                    }
                } else {
                    self.status = Some(Message::new("status-series-selection-invalid"));
                    return;
                };
                StructureOperation::Fill {
                    table_id,
                    source: seed,
                    target: view.selection,
                    mode: crate::document::FillMode::Series,
                }
            }
            StructureAction::MoveRowsUp | StructureAction::MoveColumnsLeft => {
                self.status = Some(Message::new("status-first-row-column-limit"));
                return;
            }
            StructureAction::AddTable
            | StructureAction::DuplicateTable
            | StructureAction::DeleteTable
            | StructureAction::RenameTable
            | StructureAction::MarkdownAlignLeft
            | StructureAction::MarkdownAlignCenter
            | StructureAction::MarkdownAlignRight
            | StructureAction::PromoteCsv => {
                unreachable!("table actions were handled above")
            }
        };
        self.submit_structure_operation(table_id, operation, None, false, None);
    }

    // Prepare a fill operation from the selected source range to its endpoint.
    pub(super) fn request_fill_handle(&mut self, source: CellRange, endpoint: CellAddress) {
        let Some(target) = fill_handle_target(source, endpoint) else {
            let (start, end) = source.normalized();
            if endpoint.row < start.row
                || endpoint.row > end.row
                || endpoint.column < start.column
                || endpoint.column > end.column
            {
                self.status = Some(Message::new("status-fill-handle-direction"));
            }
            return;
        };
        let Some(table_id) = self
            .workspace
            .active_document()
            .and_then(|document| document.active_table().map(|table| table.id))
        else {
            return;
        };
        self.submit_structure_operation(
            table_id,
            StructureOperation::Fill {
                table_id,
                source,
                target,
                mode: crate::document::FillMode::Auto,
            },
            None,
            false,
            None,
        );
    }

    // Submit the prepared structure request to the worker.
    pub(super) fn submit_structure_operation(
        &mut self,
        table_id: u64,
        operation: StructureOperation,
        growth_movement: Option<(i32, i32)>,
        continue_tab_input: bool,
        return_column: Option<usize>,
    ) {
        let is_text_growth = matches!(&operation, StructureOperation::SetTextCell { .. });
        let is_text_batch = matches!(&operation, StructureOperation::SetTextCells { .. });
        if self.edit.is_editing() && !is_text_growth {
            self.commit_edit(None);
        }
        let Some(document) = self.workspace.active_document() else {
            return;
        };
        let document_id = document.id;
        if self.pending_saves.contains_key(&document_id) {
            self.status = Some(Message::new("status-row-column-change-after-save"));
            return;
        }
        if self.pending_structures.contains_key(&document_id) {
            self.status = Some(Message::new("status-row-column-change-preparing"));
            return;
        }
        if self.pending_history_restores.contains_key(&document_id) {
            self.status = Some(Message::new("status-structure-history-restoring"));
            return;
        }
        if document
            .active_table()
            .is_none_or(|table| table.id != table_id)
        {
            return;
        }
        let Some(view) = document.view(table_id) else {
            return;
        };
        if !is_text_growth && !is_text_batch && !view.additional_selections.is_empty() {
            self.status = Some(Message::new(
                "status-row-column-operation-single-range-required",
            ));
            return;
        }
        if let StructureOperation::SetTextCell { address, .. } = &operation
            && (self.edit.address != Some(*address) || view.active_cell() != *address)
        {
            return;
        }
        let Some(source) = document.source.as_ref().map(Arc::clone) else {
            self.status = Some(Message::new("status-open-document-first"));
            return;
        };
        let tables = document
            .tables
            .iter()
            .map(|table| {
                let view = document.view(table.id);
                StructureTableMeta {
                    id: table.id,
                    name: table.name.clone(),
                    rows: table.row_count(),
                    columns: table.column_count(),
                    string_cells: table.string_cells_shared(),
                    view_layout: view.map_or_else(
                        || Arc::new(TableLayout::default()),
                        |view| Arc::clone(&view.layout),
                    ),
                    selection: view.map_or_else(CellRange::default, |view| view.selection),
                    scroll: view.map_or_else(Default::default, |view| view.scroll),
                }
            })
            .collect();
        let growth_selection = view.selection;
        let growth_active_cell = view.active_cell();
        let growth_additional_selections = view.additional_selections.clone();
        let request = StructureRequest {
            document_id,
            revision: document.revision,
            calculation_revision: document.calculation_revision,
            source: Some(source),
            operation: Some(operation),
            tables,
            changes: document.changed_cell_snapshot(),
            text_overlay_cells: document.text_overlay_cells(),
            text_overlay_revision: document.text_overlay_snapshot().revision,
            view_table_id: table_id,
            view_layout: Arc::clone(&view.layout),
            selection: view.selection,
            scroll: view.scroll,
            cross_sheet_references: false,
            clipboard_paste: None,
            prepared_selection_after: is_text_growth.then_some(growth_selection),
        };
        let token = self.next_job_token(document_id, request.revision);
        match self.file_worker.prepare_structure(token, request) {
            Ok(()) => {
                self.pending_structures.insert(document_id, token);
                if is_text_growth {
                    self.pending_text_growth_sessions.insert(
                        document_id,
                        PendingTextGrowthEdit {
                            session: self.edit.clone(),
                            movement: growth_movement,
                            selection: growth_selection,
                            active_cell: growth_active_cell,
                            additional_selections: growth_additional_selections,
                            pending: true,
                            deferred_save_path: None,
                            close_after_save: false,
                            close_edit: None,
                            app_close_save: false,
                            continue_tab_input,
                            return_column,
                        },
                    );
                    self.edit.reset();
                    self.edit_document_id = None;
                    self.request_editor_focus = false;
                }
                // Background preparation is not reported; only its failure or result is.
                self.status = None;
            }
            Err(error) => self.status = Some(submit_error_message(error)),
        }
    }

    // Cancel pending structure work for one document.
    pub(super) fn cancel_structure_for(&mut self, document_id: DocumentId) {
        if let Some(token) = self.pending_structures.remove(&document_id) {
            self.file_worker.cancel(token);
            self.finish_text_growth_failure(document_id);
            let is_bulk_paste = self.pending_bulk_pastes.remove(&document_id);
            let is_table_management = self.pending_table_management.remove(&document_id);
            self.status = Some(if is_bulk_paste {
                Message::new("status-paste-cancelled")
            } else if is_table_management {
                Message::new("status-table-change-cancelled")
            } else {
                Message::new("status-structure-cancelled")
            });
        }
    }
}
