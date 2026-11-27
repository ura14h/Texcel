use super::*;

impl TexcelApp {
    /// Runs the regular result path for tests that do not have a frame context.
    #[cfg(test)]
    pub(super) fn process_file_results(&mut self) {
        self.process_file_results_inner(None, false);
    }

    /// Holds successful loads while a native menu event still targets this document.
    pub(super) fn process_file_results_with_native_interaction(
        &mut self,
        ctx: &egui::Context,
        native_interaction: bool,
    ) {
        self.process_file_results_inner(Some(ctx), native_interaction);
    }

    // Keeps tests and frame updates on the same result path.
    fn process_file_results_inner(
        &mut self,
        ctx: Option<&egui::Context>,
        native_interaction: bool,
    ) {
        let results = self.take_file_results_for_host();
        for result in results {
            self.process_one_file_result(ctx, native_interaction, result);
        }
        self.finish_file_result_batch(ctx);
    }

    /// Takes worker results once so the host can run each one in its owner's window state.
    pub(super) fn take_file_results_for_host(&mut self) -> Vec<crate::file_worker::FileResult> {
        self.flush_retired_structure_tables();
        let mut results = std::mem::take(&mut self.deferred_file_results);
        results.extend(self.file_worker.poll());
        results
    }

    /// Applies one routed result while its target window state is installed.
    pub(super) fn process_host_file_result(
        &mut self,
        ctx: &egui::Context,
        result: crate::file_worker::FileResult,
    ) {
        self.process_one_file_result(Some(ctx), false, result);
    }

    /// Creates terminal save failures after real results have been routed to their windows.
    pub(super) fn take_stopped_host_save_failures(
        &mut self,
    ) -> Vec<crate::file_worker::FileResult> {
        if !self.file_worker.is_stopped() {
            return Vec::new();
        }
        std::mem::take(&mut self.pending_saves)
            .into_iter()
            .map(|(document_id, token)| crate::file_worker::FileResult {
                token,
                operation: FileOperation::Save,
                path: self
                    .file_backings
                    .get(&document_id)
                    .map_or_else(PathBuf::new, |backing| backing.path.clone()),
                kind: FileResultKind::Failed(Message::new("status-file-worker-stopped")),
            })
            .collect()
    }

    /// Finishes host polling after every available result was routed to its target.
    pub(super) fn finish_host_file_result_batch(&mut self, ctx: &egui::Context) {
        self.finish_file_result_batch(Some(ctx));
    }

    /// Keeps worker results with their existing load, stale, and input-order checks.
    fn process_one_file_result(
        &mut self,
        ctx: Option<&egui::Context>,
        native_interaction: bool,
        result: crate::file_worker::FileResult,
    ) {
        if is_load_operation(result.operation)
            && !self.load_token_is_current(result.token, result.operation)
        {
            return;
        }
        if is_loaded_result(&result.kind)
            && let Some(existing_id) = self
                .open_document_for_path(&result.path)
                .filter(|existing_id| *existing_id != result.token.document_id)
        {
            self.discard_duplicate_load(result.token.document_id);
            if self.multi_window_mode {
                self.pending_document_focus = Some(existing_id);
            } else {
                self.workspace.select_document(existing_id);
            }
            return;
        }
        if is_loaded_result(&result.kind)
            && (self.edit.composing
                || native_interaction
                || ctx.is_some_and(current_frame_has_document_interaction))
        {
            self.deferred_file_results.push(result);
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
            return;
        }
        if is_loaded_result(&result.kind) && !self.prepare_editor_for_loaded_document() {
            self.deferred_file_results.push(result);
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
            return;
        }
        if is_load_operation(result.operation)
            && matches!(
                &result.kind,
                FileResultKind::LoadedCsv { .. }
                    | FileResultKind::LoadedMarkdown { .. }
                    | FileResultKind::ImportedMarkdown { .. }
                    | FileResultKind::LoadNeedsEncodingChoice { .. }
                    | FileResultKind::LoadNeedsDelimiterChoice { .. }
                    | FileResultKind::Cancelled
                    | FileResultKind::Failed(_)
            )
        {
            self.pending_load_previews.remove(&result.token.document_id);
        }
        let requested_path = is_load_operation(result.operation)
            .then(|| {
                self.pending_file_load_paths
                    .get(&result.token.document_id)
                    .cloned()
            })
            .flatten()
            .unwrap_or_else(|| result.path.clone());
        if is_load_operation(result.operation)
            && !matches!(
                &result.kind,
                FileResultKind::LoadNeedsEncodingChoice { .. }
                    | FileResultKind::LoadNeedsDelimiterChoice { .. }
            )
        {
            self.pending_file_load_paths
                .remove(&result.token.document_id);
        }
        self.dispatch_file_result_with_user_path(result, requested_path);
    }

    /// Handles worker shutdown and retries queued CSV submissions after a batch.
    fn finish_file_result_batch(&mut self, ctx: Option<&egui::Context>) {
        if self.file_worker.is_stopped() {
            let pending_csv = std::mem::take(&mut self.pending_csv_opens);
            for (document_id, pending) in pending_csv {
                self.pending_file_load_paths
                    .entry(document_id)
                    .or_insert(pending.path);
                self.fail_stopped_open(
                    document_id,
                    JobToken {
                        document_id,
                        revision: 0,
                        job_id: pending.job_id,
                    },
                    FileOperation::LoadCsv,
                );
            }
            for (document_id, token) in std::mem::take(&mut self.pending_markdown_loads) {
                self.fail_stopped_open(document_id, token, FileOperation::LoadMarkdown);
            }
            for (document_id, token) in std::mem::take(&mut self.pending_html_loads) {
                self.fail_stopped_open(document_id, token, FileOperation::LoadHtml);
            }
            for (document_id, token) in std::mem::take(&mut self.pending_workbook_loads) {
                self.fail_stopped_open(document_id, token, FileOperation::LoadWorkbook);
            }
            let pending_saves = std::mem::take(&mut self.pending_saves);
            for (document_id, token) in pending_saves {
                self.handle_failure_result(
                    document_id,
                    token,
                    FileOperation::Save,
                    FileResultKind::Failed(Message::new("status-file-worker-stopped")),
                );
            }
            self.fail_stopped_auto_fits();
        }
        if let Some(ctx) = ctx {
            self.retry_csv_submissions(ctx);
        }
    }

    /// Fails a stopped open with its submitted path and lets queued inputs advance.
    pub(super) fn fail_stopped_open(
        &mut self,
        document_id: DocumentId,
        token: JobToken,
        operation: FileOperation,
    ) {
        self.pending_load_previews.remove(&document_id);
        let failure = FileResultKind::Failed(Message::new("status-file-worker-stopped"));
        if let Some(path) = self.pending_file_load_paths.remove(&document_id) {
            self.handle_failure_result_with_path(document_id, token, operation, path, failure);
        } else {
            self.handle_failure_result(document_id, token, operation, failure);
        }
    }

    /// Commits an editor only after a current load has succeeded.
    fn prepare_editor_for_loaded_document(&mut self) -> bool {
        if !self.edit.is_editing() {
            return true;
        }
        let Some(edit_document_id) = self.edit_document_id else {
            return true;
        };
        self.commit_edit(None);
        !self.edit.is_editing() || self.edit_document_id != Some(edit_document_id)
    }

    /// Applies only progress messages that still match a live load request.
    pub(super) fn process_load_progress(&mut self, ctx: &egui::Context) {
        let frame = ctx.cumulative_frame_nr();
        if self.last_load_progress_frame == Some(frame) {
            if self.has_active_load_request() {
                ctx.request_repaint_after(Duration::from_millis(50));
            }
            return;
        }
        self.last_load_progress_frame = Some(frame);
        for message in self.file_worker.poll_load_progress() {
            if !self.load_progress_is_active(message.token, message.operation) {
                continue;
            }
            let state = self
                .pending_load_previews
                .entry(message.token.document_id)
                .or_insert_with(|| LoadPreviewState {
                    token: message.token,
                    path: message.path.clone(),
                    stage: LoadStage::ReadingSource,
                    selected_table: 0,
                    tables: BTreeMap::new(),
                });
            if state.token != message.token {
                continue;
            }
            match message.kind {
                LoadProgressKind::Stage(stage) => state.stage = stage,
                LoadProgressKind::Chunk(chunk) => {
                    let table =
                        state
                            .tables
                            .entry(chunk.table_index)
                            .or_insert_with(|| LoadPreviewTable {
                                name: chunk.table_name.clone(),
                                row_count: 0,
                                column_count: 0,
                                chunks: BTreeMap::new(),
                            });
                    table.name.clone_from(&chunk.table_name);
                    table.add_chunk(chunk);
                }
            }
        }
        if self.has_active_load_request() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    /// Rejects progress chunks after a loader has returned a user choice.
    fn load_progress_is_active(&self, token: JobToken, operation: FileOperation) -> bool {
        self.load_token_is_current(token, operation)
            && match operation {
                FileOperation::LoadCsv => self
                    .pending_csv_opens
                    .get(&token.document_id)
                    .is_some_and(|pending| {
                        pending.job_id == token.job_id && pending.prompt.is_none()
                    }),
                FileOperation::LoadMarkdown => {
                    self.pending_markdown_loads.get(&token.document_id) == Some(&token)
                }
                FileOperation::LoadHtml => {
                    self.pending_html_loads.get(&token.document_id) == Some(&token)
                }
                FileOperation::LoadWorkbook => {
                    self.pending_workbook_loads.get(&token.document_id) == Some(&token)
                }
                _ => false,
            }
    }

    /// Keeps worker progress moving when the visible app is not being painted.
    fn has_active_load_request(&self) -> bool {
        self.pending_csv_opens
            .values()
            .any(|pending| pending.prompt.is_none())
            || !self.pending_markdown_loads.is_empty()
            || !self.pending_html_loads.is_empty()
            || !self.pending_workbook_loads.is_empty()
    }

    /// Checks the current request without retaining finished tokens forever.
    fn load_token_is_current(&self, token: JobToken, operation: FileOperation) -> bool {
        match operation {
            FileOperation::LoadCsv => self
                .pending_csv_opens
                .get(&token.document_id)
                .is_some_and(|pending| pending.job_id == token.job_id),
            FileOperation::LoadMarkdown => {
                self.pending_markdown_loads.get(&token.document_id) == Some(&token)
            }
            FileOperation::LoadHtml => {
                self.pending_html_loads.get(&token.document_id) == Some(&token)
            }
            FileOperation::LoadWorkbook => {
                self.pending_workbook_loads.get(&token.document_id) == Some(&token)
            }
            _ => false,
        }
    }

    /// Route each result kind to the handler that owns its state updates.
    #[cfg(test)]
    pub(super) fn dispatch_file_result(&mut self, result: crate::file_worker::FileResult) -> bool {
        let user_path = result.path.clone();
        self.dispatch_file_result_with_user_path(result, user_path)
    }

    /// Keeps the chosen path for display while the worker path remains the I/O target.
    fn dispatch_file_result_with_user_path(
        &mut self,
        result: crate::file_worker::FileResult,
        user_path: PathBuf,
    ) -> bool {
        let document_id = result.token.document_id;
        let token = result.token;
        let operation = result.operation;
        match result.kind {
            kind @ (FileResultKind::MarkdownCellDisplayPrepared { .. }
            | FileResultKind::StructurePrepared(_)
            | FileResultKind::StructureHistoryRestored(_)) => {
                self.handle_structure_result(document_id, token, kind)
            }
            kind @ (FileResultKind::SearchPrepared(_) | FileResultKind::DataEditPrepared(_)) => {
                self.handle_analysis_result(document_id, token, kind)
            }
            FileResultKind::AutoFitPrepared { widths } => {
                self.handle_auto_fit_result(document_id, token, widths);
                true
            }
            kind @ (FileResultKind::ExportPrepared(_) | FileResultKind::Exported { .. }) => {
                self.handle_export_result(document_id, token, kind)
            }
            kind @ (FileResultKind::LoadedCsv { .. }
            | FileResultKind::LoadedMarkdown { .. }
            | FileResultKind::ImportedMarkdown { .. }
            | FileResultKind::LoadNeedsEncodingChoice { .. }
            | FileResultKind::LoadNeedsDelimiterChoice { .. }) => {
                self.handle_load_result(document_id, token, result.path, user_path, kind)
            }
            kind @ (FileResultKind::Saved { .. } | FileResultKind::ReportSaved) => {
                self.handle_save_result(document_id, token, result.path, kind)
            }
            FileResultKind::SaveConflict => self.handle_save_conflict(document_id, token),
            kind @ (FileResultKind::Failed(_) | FileResultKind::Cancelled) => {
                let path = if is_load_operation(operation) {
                    user_path
                } else {
                    result.path
                };
                self.handle_failure_result_with_path(document_id, token, operation, path, kind)
            }
        }
    }

    /// Drops the second load request without replacing the already open source.
    fn discard_duplicate_load(&mut self, document_id: DocumentId) {
        self.pending_csv_opens.remove(&document_id);
        self.pending_markdown_loads.remove(&document_id);
        self.pending_html_loads.remove(&document_id);
        self.pending_workbook_loads.remove(&document_id);
        self.pending_file_load_paths.remove(&document_id);
        self.pending_load_previews.remove(&document_id);
    }
}

/// Identifies successful load results before they can change the active document.
fn is_loaded_result(kind: &FileResultKind) -> bool {
    matches!(
        kind,
        FileResultKind::LoadedCsv { .. }
            | FileResultKind::LoadedMarkdown { .. }
            | FileResultKind::ImportedMarkdown { .. }
    )
}

/// Defers adoption until current keyboard and pointer actions reach the old document.
fn current_frame_has_document_interaction(ctx: &egui::Context) -> bool {
    ctx.input(|input| {
        input.pointer.any_down()
            || input.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Copy
                        | egui::Event::Cut
                        | egui::Event::Paste(_)
                        | egui::Event::PasteImage(_)
                        | egui::Event::MiddleClickPaste { .. }
                        | egui::Event::Text(_)
                        | egui::Event::Key { pressed: true, .. }
                        | egui::Event::PointerButton { .. }
                        | egui::Event::Touch { .. }
                        | egui::Event::MouseWheel { .. }
                        | egui::Event::Zoom(_)
                        | egui::Event::Rotate(_)
                        | egui::Event::Ime(_)
                        | egui::Event::AccessKitActionRequest(_)
                )
            })
    })
}

/// Checks whether a result belongs to one of the document loaders.
fn is_load_operation(operation: FileOperation) -> bool {
    matches!(
        operation,
        FileOperation::LoadCsv
            | FileOperation::LoadMarkdown
            | FileOperation::LoadHtml
            | FileOperation::LoadWorkbook
    )
}
