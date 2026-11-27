use super::*;

impl HostRuntime {
    /// Applies each worker result while its document's local window state is active.
    pub(super) fn process_file_results(&mut self, ctx: &egui::Context) {
        for result in self.app.take_file_results_for_host() {
            let document_id = result.token.document_id;
            let operation = result.operation;
            let loaded_result = is_host_loaded_result(&result.kind);
            let needs_load_choice = matches!(
                &result.kind,
                crate::file_worker::FileResultKind::LoadNeedsEncodingChoice { .. }
                    | crate::file_worker::FileResultKind::LoadNeedsDelimiterChoice { .. }
            );
            let successful_new_document = loaded_result
                && self
                    .app
                    .open_document_for_path(&result.path)
                    .is_none_or(|existing| existing == document_id);
            let viewport = if successful_new_document {
                let viewport = egui::ViewportId::from_hash_of(("texcel-document", document_id));
                self.document_viewports.insert(document_id, viewport);
                self.app.set_window_document(viewport, Some(document_id));
                viewport
            } else if is_host_load_operation(operation) {
                self.app
                    .load_request_viewports
                    .get(&document_id)
                    .copied()
                    .unwrap_or(self.root_viewport)
            } else {
                self.document_viewports
                    .get(&document_id)
                    .copied()
                    .unwrap_or(self.root_viewport)
            };
            self.app
                .with_window_state(viewport, Some(document_id), |app| {
                    app.process_host_file_result(ctx, result);
                });
            ctx.request_repaint_of(viewport);
            let load_is_still_pending = self.app.pending_file_load_paths.contains_key(&document_id)
                || self.app.pending_csv_opens.contains_key(&document_id)
                || self.app.pending_markdown_loads.contains_key(&document_id)
                || self.app.pending_html_loads.contains_key(&document_id)
                || self.app.pending_workbook_loads.contains_key(&document_id);
            if is_host_load_operation(operation) && !needs_load_choice && !load_is_still_pending {
                self.app.load_request_viewports.remove(&document_id);
            }
        }
        self.route_stopped_save_failures(ctx);
        self.app
            .with_window_state(self.root_viewport, self.root_document, |app| {
                app.finish_host_file_result_batch(ctx)
            });
    }

    /// Delivers worker shutdown errors to the window that owns each pending save.
    fn route_stopped_save_failures(&mut self, ctx: &egui::Context) {
        for result in self.app.take_stopped_host_save_failures() {
            let document_id = result.token.document_id;
            let viewport = self
                .document_viewports
                .get(&document_id)
                .copied()
                .unwrap_or(self.root_viewport);
            self.app
                .with_window_state(viewport, Some(document_id), |app| {
                    app.process_host_file_result(ctx, result);
                });
            ctx.request_repaint_of(viewport);
        }
    }
}

/// Identifies results that create or replace a document from a load request.
fn is_host_loaded_result(kind: &crate::file_worker::FileResultKind) -> bool {
    matches!(
        kind,
        crate::file_worker::FileResultKind::LoadedCsv { .. }
            | crate::file_worker::FileResultKind::LoadedMarkdown { .. }
            | crate::file_worker::FileResultKind::ImportedMarkdown { .. }
    )
}

/// Identifies operations that belong to a user request to open a file.
fn is_host_load_operation(operation: crate::file_worker::FileOperation) -> bool {
    matches!(
        operation,
        crate::file_worker::FileOperation::LoadCsv
            | crate::file_worker::FileOperation::LoadMarkdown
            | crate::file_worker::FileOperation::LoadHtml
            | crate::file_worker::FileOperation::LoadWorkbook
    )
}
