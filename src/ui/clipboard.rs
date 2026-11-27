use super::*;

#[path = "clipboard/copy.rs"]
mod copy;
#[path = "clipboard/copy_content.rs"]
mod copy_content;
#[path = "clipboard/mark.rs"]
mod mark;
pub(super) use mark::{ClipboardMark, CopyMarquee};

#[cfg(test)]
#[path = "clipboard/copy_options_test.rs"]
mod copy_options_tests;

impl TexcelApp {
    // Clear the selected editable cells through one document command.
    pub(super) fn clear_selected_range(&mut self) {
        if let Some(document_id) = self.workspace.active_document().map(|document| document.id)
            && self.document_bulk_edit_pending(document_id)
        {
            self.status = Some(Message::new("status-cell-edit-paused-during-bulk"));
            return;
        }
        let Some((table_id, _address)) = self.active_selection() else {
            return;
        };
        let Some((document_id, ranges)) = self.workspace.active_document().and_then(|document| {
            let view = document.view(table_id)?;
            Some((document.id, view.selected_ranges().collect::<Vec<_>>()))
        }) else {
            return;
        };
        let mut changed_document = None;
        let mut resource_limit_notice = None;
        let mut pending_text_changes = None;
        let mut clear_error = None;
        if let Some(document) = self.workspace.documents.get_mut(&document_id) {
            match document.clear_ranges(table_id, &ranges) {
                Ok(true) => {
                    changed_document = Some(document.id);
                    resource_limit_notice = document.take_resource_limit_notice();
                }
                Ok(false) => (),
                Err(error) => {
                    pending_text_changes = document.take_pending_text_source_changes();
                    if pending_text_changes.is_none() {
                        clear_error = Some(error);
                    }
                }
            }
        }
        if let Some(error) = clear_error {
            self.status = Some(
                Message::new("status-cell-clear-failed")
                    .with_message("detail", crate::i18n::document_error_message(error)),
            );
        }
        if let Some(pending) = pending_text_changes {
            self.submit_text_source_changes(pending, None);
            return;
        }
        if let Some(document_id) = changed_document {
            if let Some(table_id) = self.workspace.documents.get(&document_id).and_then(|doc| {
                let table_id = doc.active_table()?.id;
                Some(table_id)
            }) && self
                .normalize_selection_for_layout(document_id, table_id)
                .is_none()
            {
                self.request_editor_focus = false;
            }
            self.schedule_calculation(document_id);
            if let Some(limit) = resource_limit_notice {
                self.status = Some(
                    Message::new("status-resource-limit-reached")
                        .with_message("limit", crate::i18n::resource_limit_message(limit)),
                );
            }
        }
    }

    // Parses clipboard text, protects source-only cells, and applies the grid paste.
    pub(super) fn paste_selected_grid(&mut self, text: String, paste_options: PasteOptions) {
        if self.edit.is_editing() || text.is_empty() {
            return;
        }
        if text.len() > crate::clipboard::MAX_CLIPBOARD_BYTES {
            self.status = Some(Message::new("status-paste-clipboard-limit-exceeded"));
            return;
        }
        // Text-only pastes never read Texcel data or HTML.
        let html = if paste_options.text_only {
            None
        } else {
            read_system_html_if_matching(&text)
        };
        let Some(document) = self.workspace.active_document() else {
            return;
        };
        let document_id = document.id;
        let Some(table) = document.active_table() else {
            return;
        };
        let table_id = table.id;
        let Some(view) = document.view(table_id) else {
            return;
        };
        if !view.additional_selections.is_empty() {
            self.status = Some(Message::new(
                "status-paste-target-single-rectangle-required",
            ));
            return;
        }
        let selection = view.selection;
        let rows = MAX_ROWS;
        let columns = MAX_COLUMNS;
        let scroll = view.scroll;
        let owner_id = html.as_deref().and_then(crate::clipboard::owner_id_of_html);
        let cut_source =
            self.cut_source_for_paste(owner_id.as_deref(), document_id, table_id, paste_options);
        self.submit_clipboard_paste(
            document_id,
            table_id,
            ClipboardPasteRequest {
                text,
                html,
                options: paste_options,
                selection,
                rows,
                columns,
                cut_source,
            },
            scroll,
        );
    }

    /// Prepares one atomic paste through the worker and shared structure history path.
    pub(super) fn submit_clipboard_paste(
        &mut self,
        document_id: DocumentId,
        table_id: u64,
        paste: ClipboardPasteRequest,
        scroll: crate::document::ScrollPosition,
    ) {
        if self.document_bulk_edit_pending(document_id) {
            self.status = Some(Message::new("status-cell-edit-paused-during-bulk"));
            return;
        }
        if self.pending_saves.contains_key(&document_id) {
            self.status = Some(Message::new("status-paste-after-save"));
            return;
        }
        if self.pending_structures.contains_key(&document_id) {
            self.status = Some(Message::new("status-bulk-operation-in-progress"));
            return;
        }
        if self.pending_history_restores.contains_key(&document_id) {
            self.status = Some(Message::new("status-paste-after-history-restore"));
            return;
        }
        let Some(document) = self.workspace.documents.get(&document_id) else {
            return;
        };
        if !document.tables.iter().any(|table| table.id == table_id) {
            self.status = Some(Message::new("status-paste-target-table-unavailable"));
            return;
        }
        let Some(view) = document.view(table_id) else {
            self.status = Some(Message::new("status-paste-target-view-unavailable"));
            return;
        };
        let source = document.source.as_ref().map(Arc::clone);
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
        let selection = paste.selection;
        let request = StructureRequest {
            document_id,
            revision: document.revision,
            calculation_revision: document.calculation_revision,
            source,
            operation: None,
            tables,
            changes: document.changed_cell_snapshot(),
            text_overlay_cells: document.text_overlay_cells(),
            text_overlay_revision: document.text_overlay_snapshot().revision,
            view_table_id: table_id,
            view_layout: Arc::clone(&view.layout),
            selection,
            clipboard_paste: Some(paste),
            prepared_selection_after: Some(selection),
            scroll,
            cross_sheet_references: false,
        };
        let revision = request.revision;
        let token = self.next_job_token(document_id, revision);
        match self.file_worker.prepare_structure(token, request) {
            Ok(()) => {
                self.pending_structures.insert(document_id, token);
                self.pending_bulk_pastes.insert(document_id);
                self.expect_completion(
                    document_id,
                    token,
                    Some(Message::new("status-done-pasted")),
                );
                self.status = None;
            }
            Err(error) => self.status = Some(submit_error_message(error)),
        }
    }
}
