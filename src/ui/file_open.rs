use super::*;

impl TexcelApp {
    /// Starts a format loader without changing the currently edited document.
    pub(super) fn start_path_load(&mut self, path: PathBuf) -> Option<DocumentId> {
        self.start_path_load_with_result(path)
            .unwrap_or_else(|error| {
                self.status = Some(submit_error_message(error));
                None
            })
    }

    /// Returns worker admission errors to the queue without consulting UI status text.
    pub(super) fn start_path_load_for_queue(
        &mut self,
        path: PathBuf,
    ) -> Result<Option<DocumentId>, SubmitError> {
        self.start_path_load_with_result(path).inspect_err(|error| {
            self.status = Some(submit_error_message(*error));
        })
    }

    /// Starts a loader and preserves the worker's typed queue admission result.
    fn start_path_load_with_result(
        &mut self,
        path: PathBuf,
    ) -> Result<Option<DocumentId>, SubmitError> {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase);
        match extension.as_deref() {
            Some("csv" | "tsv") => {
                let document_id = self.workspace.reserve_document_id();
                self.pending_csv_opens.insert(
                    document_id,
                    PendingCsvOpen {
                        path,
                        encoding: EncodingChoice::Auto,
                        delimiter: None,
                        prompt: None,
                        job_id: 0,
                        retry_submission: None,
                    },
                );
                if let Err(error) = self.submit_csv_load(document_id) {
                    self.pending_csv_opens.remove(&document_id);
                    return Err(error);
                }
                self.pending_file_load_paths.insert(
                    document_id,
                    self.pending_csv_opens[&document_id].path.clone(),
                );
                let started = self.pending_csv_opens.contains_key(&document_id);
                if started {
                    self.remember_load_request_viewport(document_id);
                }
                Ok(started.then_some(document_id))
            }
            Some("md" | "markdown") => {
                let document_id = self.workspace.reserve_document_id();
                let token = self.next_job_token(document_id, 0);
                let load_path = path.clone();
                match self
                    .file_worker
                    .load_markdown_with_language(token, path, self.language)
                {
                    Ok(()) => {
                        self.pending_markdown_loads.insert(document_id, token);
                        self.pending_file_load_paths.insert(document_id, load_path);
                        self.remember_load_request_viewport(document_id);
                        Ok(Some(document_id))
                    }
                    Err(error) => Err(error),
                }
            }
            Some("html" | "htm") => {
                let document_id = self.workspace.reserve_document_id();
                let token = self.next_job_token(document_id, 0);
                let load_path = path.clone();
                match self
                    .file_worker
                    .import_html_with_language(token, path, self.language)
                {
                    Ok(()) => {
                        self.pending_html_loads.insert(document_id, token);
                        self.pending_file_load_paths.insert(document_id, load_path);
                        self.remember_load_request_viewport(document_id);
                        Ok(Some(document_id))
                    }
                    Err(error) => Err(error),
                }
            }
            Some(extension @ ("xlsx" | "ods")) => {
                let format = if extension == "ods" {
                    crate::file_worker::WorkbookFormat::Ods
                } else {
                    crate::file_worker::WorkbookFormat::Xlsx
                };
                let document_id = self.workspace.reserve_document_id();
                let token = self.next_job_token(document_id, 0);
                let load_path = path.clone();
                match self.file_worker.import_workbook_with_language(
                    token,
                    path,
                    format,
                    self.language,
                ) {
                    Ok(()) => {
                        self.pending_workbook_loads.insert(document_id, token);
                        self.pending_file_load_paths.insert(document_id, load_path);
                        self.remember_load_request_viewport(document_id);
                        Ok(Some(document_id))
                    }
                    Err(error) => Err(error),
                }
            }
            _ => {
                self.status = Some(Message::new("status-file-format-required"));
                Ok(None)
            }
        }
    }

    /// Keeps each asynchronous load prompt in the viewport that started it. A window without
    /// a document (the root once its document closed) is hidden, so its prompts go to a
    /// document window instead of appearing nowhere.
    pub(super) fn remember_load_request_viewport(&mut self, document_id: DocumentId) {
        if self.multi_window_mode
            && let Some(viewport) = self.load_prompt_window().or(self.current_viewport)
        {
            self.load_request_viewports.insert(document_id, viewport);
        }
    }

    /// Checks whether one load prompt belongs to the window currently drawing.
    pub(super) fn load_request_belongs_to_current_viewport(&self, document_id: DocumentId) -> bool {
        if !self.multi_window_mode || self.current_viewport.is_none() {
            return true;
        }
        self.load_request_viewports
            .get(&document_id)
            .is_some_and(|owner| Some(*owner) == self.current_viewport)
    }

    /// Analyze CSV options before parsing or creating a document.
    pub(super) fn submit_csv_options(&mut self, document_id: DocumentId) {
        let Some(pending) = self.pending_csv_opens.get(&document_id) else {
            return;
        };
        let path = pending.path.clone();
        let encoding = pending.encoding;
        let token = self.next_job_token(document_id, 0);
        match self
            .file_worker
            .load_csv_options(token, path.clone(), encoding)
        {
            Ok(()) => {
                if let Some(pending) = self.pending_csv_opens.get_mut(&document_id) {
                    pending.job_id = token.job_id;
                    pending.prompt = None;
                    pending.retry_submission = None;
                }
            }
            Err(SubmitError::QueueFull) => {
                if let Some(pending) = self.pending_csv_opens.get_mut(&document_id) {
                    pending.retry_submission = Some(CsvSubmission::Options);
                    if let Some(CsvLoadPrompt::Settings(settings)) = pending.prompt.as_mut() {
                        settings.error = Some("status-submit-queue-full");
                    }
                }
                self.status = Some(submit_error_message(SubmitError::QueueFull));
            }
            Err(error @ SubmitError::Stopped) => {
                self.pending_csv_opens.remove(&document_id);
                self.pending_file_load_paths.remove(&document_id);
                self.load_request_viewports.remove(&document_id);
                self.status = Some(file_open_error(&path, submit_error_message(error)));
            }
        }
    }

    /// Submit CSV loading with the current encoding and delimiter choices.
    pub(super) fn submit_csv_load(&mut self, document_id: DocumentId) -> Result<(), SubmitError> {
        let Some(pending) = self.pending_csv_opens.get(&document_id) else {
            return Ok(());
        };
        let path = pending.path.clone();
        let encoding = pending.encoding;
        let delimiter = pending.delimiter;
        let path_for_error = pending.path.clone();
        let restore_prompt = pending.prompt.clone();
        let token = self.next_job_token(document_id, 0);
        if let Err(error) = self.file_worker.load_csv(token, path, encoding, delimiter) {
            if error == SubmitError::QueueFull
                && let Some(pending) = self.pending_csv_opens.get_mut(&document_id)
                && let Some(CsvLoadPrompt::Settings(mut settings)) = restore_prompt
            {
                settings.error = Some("status-submit-queue-full");
                pending.prompt = Some(CsvLoadPrompt::Settings(settings));
                pending.retry_submission = Some(CsvSubmission::Load);
            } else {
                self.pending_csv_opens.remove(&document_id);
                if error == SubmitError::Stopped {
                    self.pending_file_load_paths.remove(&document_id);
                    self.load_request_viewports.remove(&document_id);
                }
            }
            self.status = Some(if error == SubmitError::Stopped {
                file_open_error(&path_for_error, submit_error_message(error))
            } else {
                submit_error_message(error)
            });
            return Err(error);
        }
        if let Some(pending) = self.pending_csv_opens.get_mut(&document_id) {
            pending.job_id = token.job_id;
            pending.prompt = None;
            pending.retry_submission = None;
        }
        Ok(())
    }

    /// Retries a CSV choice after another worker request frees a queue slot.
    pub(super) fn retry_csv_submissions(&mut self, ctx: &egui::Context) {
        let retries = self
            .pending_csv_opens
            .iter()
            .filter_map(|(document_id, pending)| {
                pending.retry_submission.map(|kind| (*document_id, kind))
            })
            .collect::<Vec<_>>();
        for (document_id, kind) in retries {
            match kind {
                CsvSubmission::Options => self.submit_csv_options(document_id),
                CsvSubmission::Load => {
                    let _ = self.submit_csv_load(document_id);
                }
            }
        }
        if self
            .pending_csv_opens
            .values()
            .any(|pending| pending.retry_submission.is_some())
        {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }
}
