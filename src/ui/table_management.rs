use super::*;

impl TexcelApp {
    /// Routes table and sheet edits through the same pending-operation gate and worker history.
    pub(super) fn request_table_management(
        &mut self,
        action: StructureAction,
        requested_table_id: Option<u64>,
        rename_name: Option<String>,
    ) {
        let Some(document_id) = self.workspace.active_document else {
            return;
        };
        self.request_table_management_for_document(
            document_id,
            action,
            requested_table_id,
            rename_name,
        );
    }

    /// Builds one table operation from the named document without changing the active tab.
    pub(in crate::ui) fn request_table_management_for_document(
        &mut self,
        document_id: DocumentId,
        action: StructureAction,
        requested_table_id: Option<u64>,
        rename_name: Option<String>,
    ) {
        let target_has_edit = self.edit.is_editing()
            && self.edit_document_id.or(self.workspace.active_document) == Some(document_id);
        if target_has_edit && self.edit.composing {
            self.status = Some(Message::new("status-finish-ime-before-table-change"));
            return;
        }
        if self.document_bulk_edit_pending(document_id) {
            self.status = Some(Message::new("status-cell-edit-paused-during-bulk"));
            return;
        }
        let csv_cell_edit_pending = self
            .pending_csv_promotion_cell_edits
            .contains_key(&document_id)
            || self
                .pending_csv_promotion_cell_batches
                .contains_key(&document_id);
        if target_has_edit && !csv_cell_edit_pending {
            self.commit_edit(None);
        }
        let Some(document) = self.workspace.documents.get(&document_id) else {
            return;
        };
        if self.pending_saves.contains_key(&document_id)
            || self.pending_structures.contains_key(&document_id)
            || self.pending_history_restores.contains_key(&document_id)
            || self.pending_exports.contains_key(&document_id)
            || self.pending_export_writes.contains_key(&document_id)
        {
            self.status = Some(Message::new(
                "status-finish-pending-work-before-table-change",
            ));
            return;
        }
        let Some(source) = document.source.as_ref().map(Arc::clone) else {
            self.status = Some(Message::new("status-table-management-format-required"));
            return;
        };
        let is_markdown = matches!(source.as_ref(), SourceDocument::Markdown(_));
        let is_csv = matches!(source.as_ref(), SourceDocument::Csv(_));
        let csv_markdown_action = is_csv
            && matches!(
                action,
                StructureAction::AddTable
                    | StructureAction::DuplicateTable
                    | StructureAction::MarkdownAlignLeft
                    | StructureAction::MarkdownAlignCenter
                    | StructureAction::MarkdownAlignRight
                    | StructureAction::PromoteCsv
            );
        if !is_markdown && !csv_markdown_action {
            self.status = Some(Message::new("status-csv-table-structure-fixed"));
            return;
        }
        let active_table_id = document.active_table;
        let Some(view) = document.view(active_table_id) else {
            self.status = Some(Message::new("status-selected-table-view-unavailable"));
            return;
        };
        let target_table_id = requested_table_id.unwrap_or(active_table_id);
        if !document
            .tables
            .iter()
            .any(|table| table.id == target_table_id)
        {
            self.status = Some(Message::new("status-target-table-required"));
            return;
        }
        let target_table = document
            .tables
            .iter()
            .find(|table| table.id == target_table_id)
            .expect("target table was checked above");
        let target_name_generated = match source.as_ref() {
            SourceDocument::Markdown(markdown) => document
                .tables
                .iter()
                .position(|table| table.id == target_table_id)
                .and_then(|index| markdown.tables.get(index))
                .is_some_and(|table| table.name_generated),
            _ => false,
        };
        // Tables take their names from Markdown headings, so they cannot be renamed here.
        if action == StructureAction::RenameTable {
            self.status = Some(Message::new("status-xlsx-sheet-name-order-available"));
            return;
        }
        let next_table_id = document
            .tables
            .iter()
            .map(|table| table.id)
            .max()
            .unwrap_or(0)
            .checked_add(1);
        let table_action = match action {
            StructureAction::AddTable => {
                let Some(new_table_id) = next_table_id else {
                    self.status = Some(Message::new("status-table-id-allocation-failed"));
                    return;
                };
                crate::structure::TableAction::Add {
                    new_table_id,
                    name: generated_table_name(self.language, document.tables.len() + 1),
                }
            }
            StructureAction::PromoteCsv => {
                if let Some((table_id, range, hyperlink)) = self
                    .pending_markdown_link_edits
                    .get(&document_id)
                    .filter(|(table_id, _, _)| *table_id == target_table_id)
                {
                    crate::structure::TableAction::MarkdownLinks {
                        table_id: *table_id,
                        range: *range,
                        hyperlink: hyperlink.clone(),
                    }
                } else {
                    if let Some((table_id, changes)) = self
                        .pending_csv_promotion_cell_batches
                        .get(&document_id)
                        .filter(|(table_id, _)| *table_id == target_table_id)
                    {
                        crate::structure::TableAction::PromoteCsvCells {
                            table_id: *table_id,
                            changes: changes.clone(),
                        }
                    } else {
                        self.pending_csv_promotion_cell_edits
                            .get(&document_id)
                            .filter(|(table_id, _, _)| *table_id == target_table_id)
                            .map_or_else(
                                || crate::structure::TableAction::PromoteCsv {
                                    table_id: target_table_id,
                                },
                                |(table_id, address, value)| {
                                    crate::structure::TableAction::PromoteCsvCell {
                                        table_id: *table_id,
                                        address: *address,
                                        value: value.clone(),
                                    }
                                },
                            )
                    }
                }
            }
            StructureAction::DuplicateTable => {
                let Some(new_table_id) = next_table_id else {
                    self.status = Some(Message::new("status-table-id-allocation-failed"));
                    return;
                };
                crate::structure::TableAction::Duplicate {
                    table_id: target_table_id,
                    new_table_id,
                    name: if target_name_generated {
                        generated_table_name(self.language, document.tables.len() + 1)
                    } else {
                        target_table.name.clone()
                    },
                }
            }
            StructureAction::DeleteTable => crate::structure::TableAction::Delete {
                table_id: target_table_id,
            },
            StructureAction::RenameTable => crate::structure::TableAction::Rename {
                table_id: target_table_id,
                name: rename_name.expect("rename dialog supplied a confirmed name"),
            },
            StructureAction::MarkdownAlignLeft
            | StructureAction::MarkdownAlignCenter
            | StructureAction::MarkdownAlignRight => {
                if !is_markdown && !is_csv {
                    self.status = Some(Message::new("status-markdown-column-alignment-available"));
                    return;
                }
                let (start, end) = view.selection.normalized();
                let source_column_count = match source.as_ref() {
                    SourceDocument::Markdown(markdown) => document
                        .tables
                        .iter()
                        .position(|table| table.id == target_table_id)
                        .and_then(|index| markdown.parser_width(index))
                        .unwrap_or(0),
                    SourceDocument::Csv(_) => target_table.column_count(),
                };
                if source_column_count == 0 {
                    self.status = Some(Message::new("status-markdown-alignment-no-columns"));
                    return;
                }
                let whole_axis = start.column == 0 && end.column == MAX_COLUMNS - 1
                    || start.row == 0 && end.row == MAX_ROWS - 1;
                if whole_axis && start.column >= source_column_count {
                    self.status = Some(Message::new("status-markdown-alignment-no-columns"));
                    return;
                }
                let end_column = if whole_axis {
                    end.column.min(source_column_count - 1)
                } else {
                    end.column
                };
                let alignment = match action {
                    StructureAction::MarkdownAlignLeft => {
                        crate::markdown_adapter::MarkdownTableAlignment::Left
                    }
                    StructureAction::MarkdownAlignCenter => {
                        crate::markdown_adapter::MarkdownTableAlignment::Center
                    }
                    StructureAction::MarkdownAlignRight => {
                        crate::markdown_adapter::MarkdownTableAlignment::Right
                    }
                    _ => unreachable!("alignment action was matched above"),
                };
                crate::structure::TableAction::MarkdownFormat {
                    table_id: target_table_id,
                    operation: crate::structure::MarkdownTableOperation::SetColumnAlignment {
                        columns: start.column..=end_column,
                        alignment,
                    },
                }
            }
            _ => return,
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
        let request = StructureRequest {
            document_id,
            revision: document.revision,
            calculation_revision: document.calculation_revision,
            source: Some(source),
            operation: None,
            tables,
            changes: document.changed_cell_snapshot(),
            text_overlay_cells: document.text_overlay_cells(),
            text_overlay_revision: document.text_overlay_snapshot().revision,
            view_table_id: active_table_id,
            view_layout: Arc::clone(&view.layout),
            selection: view.selection,
            scroll: view.scroll,
            cross_sheet_references: false,
            clipboard_paste: None,
            prepared_selection_after: None,
        };
        let token = self.next_job_token(document_id, request.revision);
        match self
            .file_worker
            .prepare_table_structure(token, request, table_action)
        {
            Ok(()) => {
                self.pending_structures.insert(document_id, token);
                self.pending_table_management.insert(document_id);
                self.expect_completion(
                    document_id,
                    token,
                    completion_notice::table_completion_notice(action),
                );
                self.status = None;
            }
            Err(error) => self.status = Some(submit_error_message(error)),
        }
    }
}

/// Creates a numbered table name in the selected language.
fn generated_table_name(language: Language, number: usize) -> String {
    Message::new("table-default-name")
        .with_number("number", number as i64)
        .resolve_generated_name(language)
}
