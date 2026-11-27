use super::*;

const MAX_RECENT_FILES: usize = 20;

impl TexcelApp {
    /// Appends each selected or dropped path without starting worker jobs early.
    pub fn queue_open_paths(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        if self.active_queued_open.is_none() && self.queued_open_paths.is_empty() {
            self.queued_open_failure = None;
        }
        let current_dir = std::env::current_dir().ok();
        for path in paths {
            let path = current_dir.as_deref().map_or(path.clone(), |directory| {
                absolute_open_path(&path, directory)
            });
            let path = normalize_recent_path(&path);
            if let Some(document_id) = self.open_document_for_path(&path) {
                if self.multi_window_mode {
                    self.pending_document_focus = Some(document_id);
                } else {
                    self.workspace.select_document(document_id);
                }
                continue;
            }
            if self.path_is_pending_or_queued(&path) {
                continue;
            }
            self.queued_open_paths.push_back(path);
        }
    }

    /// Starts one queued path after open dialogs, close work, and other loads finish.
    pub(super) fn advance_queued_open(&mut self, ctx: &egui::Context) {
        if self
            .active_queued_open
            .is_some_and(|document_id| !self.has_pending_open(document_id))
        {
            self.active_queued_open = None;
        }
        if self.active_queued_open.is_some() || self.queued_open_paths.is_empty() {
            if self.active_queued_open.is_none()
                && self.queued_open_paths.is_empty()
                && let Some(failure) = self.queued_open_failure.clone()
            {
                self.status = Some(failure);
            }
            return;
        }
        if self.open_entry_is_blocked() || self.has_pending_open_work() {
            ctx.request_repaint_after(Duration::from_millis(50));
            return;
        }

        let path = self.queued_open_paths.front().cloned().unwrap_or_default();
        if let Some(document_id) = self.open_document_for_path(&path) {
            self.queued_open_paths.pop_front();
            self.focus_open_document(document_id, ctx);
            ctx.request_repaint();
            return;
        }
        if self
            .pending_file_load_paths
            .values()
            .any(|pending| same_open_path(pending, &path))
        {
            self.queued_open_paths.pop_front();
            return;
        }
        if !has_supported_open_extension(&path) {
            self.start_path_load(path.clone());
            if let Some(cause) = self.status.clone() {
                self.status = Some(file_open_error(&path, cause));
            }
            self.queued_open_paths.pop_front();
            self.record_queued_open_failure();
            ctx.request_repaint();
            return;
        }
        match self.start_path_load_for_queue(path.clone()) {
            Ok(Some(document_id)) => {
                self.queued_open_paths.pop_front();
                self.active_queued_open = Some(document_id);
            }
            Ok(None) | Err(SubmitError::Stopped) => {
                // A rejected format or stopped worker cannot accept another retry.
                self.queued_open_paths.pop_front();
                self.status = self
                    .status
                    .take()
                    .map(|cause| file_open_error(&path, cause));
                self.record_queued_open_failure();
                ctx.request_repaint();
            }
            Err(SubmitError::QueueFull) => {
                // Retain a supported path until an active worker request frees a slot.
                ctx.request_repaint_after(Duration::from_millis(50));
            }
        }
    }

    /// Retains the latest queued input error until the whole batch has finished.
    pub(super) fn record_queued_open_failure(&mut self) {
        if let Some(message) = self.status.clone() {
            self.queued_open_failure = Some(message);
        }
    }

    /// Keeps open requests behind modal decisions and active close or load work.
    fn open_entry_is_blocked(&self) -> bool {
        self.edit.composing
            || self.help_window_open
            || self.settings_window_open
            || self.active_import_report.is_some()
            || self.active_export_confirmation.is_some()
            || self.export_dialog.is_some()
            || self.cell_link_dialog.is_some()
            || self.layout_dialog.is_some()
            || self.sheet_zoom_dialog.is_some()
            || self.search_dialog.is_some()
            || self.paste_special_dialog.is_some()
            || self.data_edit_dialog.is_some()
            || self.pending_close_document.is_some()
            || self.app_close_dialog_open
            || self.app_close_discard_pending
            || self.app_close_after_documents
            || !self.recovery_restore_pending.is_empty()
            || self.save_conflict_dialog.is_some()
            || self
                .pending_csv_opens
                .values()
                .any(|pending| pending.prompt.is_some())
            || !self.pending_load_previews.is_empty()
    }

    /// Waits for every existing load path before starting a queued user request.
    fn has_pending_open_work(&self) -> bool {
        !self.pending_csv_opens.is_empty()
            || !self.pending_markdown_loads.is_empty()
            || !self.pending_html_loads.is_empty()
            || !self.pending_workbook_loads.is_empty()
    }

    /// Finds a loaded document whose path already names this open request.
    pub(super) fn open_document_for_path(&self, path: &std::path::Path) -> Option<DocumentId> {
        self.file_backings
            .iter()
            .find_map(|(document_id, backing)| {
                let user_path = user_file_path(self, *document_id, backing);
                (same_open_path(&backing.path, path) || same_open_path(user_path, path))
                    .then_some(*document_id)
            })
            .or_else(|| {
                self.import_origins
                    .iter()
                    .find_map(|(document_id, origin)| {
                        same_open_path(origin, path).then_some(*document_id)
                    })
            })
    }

    /// Prevents duplicate OS and command-line deliveries from adding extra windows.
    fn path_is_pending_or_queued(&self, path: &std::path::Path) -> bool {
        self.pending_file_load_paths
            .values()
            .any(|pending| same_open_path(pending, path))
            || self
                .queued_open_paths
                .iter()
                .any(|queued| same_open_path(queued, path))
    }

    /// Activates an existing window instead of loading a second copy of its file.
    fn focus_open_document(&mut self, document_id: DocumentId, ctx: &egui::Context) {
        if !self.multi_window_mode {
            self.workspace.select_document(document_id);
            return;
        }
        let viewport = if self.workspace.active_document == Some(document_id) {
            self.current_viewport
        } else {
            self.window_states.iter().find_map(|(viewport, state)| {
                (state.document_id == Some(document_id)).then_some(*viewport)
            })
        };
        if let Some(viewport) = viewport {
            ctx.send_viewport_cmd_to(viewport, egui::ViewportCommand::Focus);
        }
    }

    /// Checks whether the worker still owns a file load for one document.
    fn has_pending_open(&self, document_id: DocumentId) -> bool {
        self.pending_csv_opens.contains_key(&document_id)
            || self.pending_markdown_loads.contains_key(&document_id)
            || self.pending_html_loads.contains_key(&document_id)
            || self.pending_workbook_loads.contains_key(&document_id)
    }

    /// Places a successful input or full Save As path at the front of recent files.
    pub(super) fn remember_recent_file(&mut self, path: PathBuf) {
        let normalized = normalize_recent_path(&path);
        self.recent_files
            .retain(|existing| normalize_recent_path(existing) != normalized);
        self.recent_files.push_front(path);
        self.recent_files.truncate(MAX_RECENT_FILES);
        if self.settings_loaded {
            self.settings_file.recent_files = self.recent_files.iter().cloned().collect();
            self.mark_settings_write();
        } else {
            self.recent_files_changed_before_load = true;
        }
    }

    /// Merges settings loaded late with paths opened successfully this session.
    pub(super) fn restore_recent_files(&mut self, saved: Vec<PathBuf>) {
        let mut merged = VecDeque::new();
        for path in self.recent_files.drain(..).chain(saved) {
            let normalized = normalize_recent_path(&path);
            if !merged
                .iter()
                .any(|existing: &PathBuf| normalize_recent_path(existing) == normalized)
            {
                merged.push_back(path);
            }
            if merged.len() == MAX_RECENT_FILES {
                break;
            }
        }
        self.recent_files = merged;
        self.settings_file.recent_files = self.recent_files.iter().cloned().collect();
    }
}

/// Compares lexical paths; canonical file paths arrive from the background loader.
fn same_open_path(left: &std::path::Path, right: &std::path::Path) -> bool {
    normalize_recent_path(left) == normalize_recent_path(right)
}

/// Recognizes the file types handled by the shared load path.
fn has_supported_open_extension(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "md" | "markdown" | "csv" | "tsv" | "html" | "htm" | "xlsx" | "ods"
            )
        })
}

/// Resolves a relative entry path against the launch working directory without disk access.
fn absolute_open_path(path: &std::path::Path, current_dir: &std::path::Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        current_dir.join(path)
    }
}

#[cfg(test)]
#[path = "open_queue_test.rs"]
mod tests;
