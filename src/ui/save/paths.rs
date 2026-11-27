use super::*;
use std::path::Path;

impl TexcelApp {
    /// Saves the active document after selecting a supported output path.
    pub(in crate::ui) fn save_active(&mut self, save_as: bool) {
        let Some(document_id) = self.workspace.active_document().map(|document| document.id) else {
            self.status = Some(Message::new("status-save-document-required"));
            return;
        };
        if save_as || !self.backing_matches_document(document_id) {
            self.open_save_as_dialog(
                document_id,
                SaveDialogIntent {
                    track_recent: true,
                    // Save shows the same formula choice as Save As when it needs a destination.
                    allow_formula_results: true,
                    ..SaveDialogIntent::default()
                },
            );
            return;
        }
        if let Some(path) = self.save_path_for_document(document_id, false) {
            self.save_document_to_path(document_id, path);
        }
    }

    /// Opens the shared format and formula choices before selecting a new destination.
    pub(in crate::ui) fn open_save_as_dialog(
        &mut self,
        document_id: DocumentId,
        intent: SaveDialogIntent,
    ) {
        let Some((_document_name, _source)) = self.save_request_details(document_id) else {
            return;
        };
        let format = default_save_format(&self.workspace.documents[&document_id]);
        let active_table = Some(self.workspace.documents[&document_id].active_table);
        self.export_dialog = Some(ExportDialogState {
            document_id,
            options: ExportOptions {
                format,
                scope: active_table.map_or(ExportScope::AllTables, ExportScope::Selected),
                formulas: FormulaOutput::Formula,
                modifiers: ModifierOutput::Convert,
                markdown_header: MarkdownHeader::UseFirstRow,
            },
            save_intent: Some(intent),
        });
    }

    /// Selects and validates a save path without changing the current editor.
    pub(in crate::ui) fn save_path_for_document(
        &mut self,
        document_id: DocumentId,
        save_as: bool,
    ) -> Option<PathBuf> {
        let (document_name, existing_source) = self.save_request_details(document_id)?;
        let existing = self.file_backings.get(&document_id).cloned();
        let selected_path = if !save_as && self.backing_matches_document(document_id) {
            existing.as_ref().map(|backing| backing.path.clone())
        } else {
            None
        };
        #[cfg(test)]
        if let Some(path) = self.test_save_path_selection.take() {
            if !self.save_as_path_supported(document_id, &path) {
                self.status = Some(Message::new("status-save-format-mismatch"));
                return None;
            }
            return Some(path);
        }
        let language = self.language;
        let target = selected_path.or_else(|| {
            let dialog = rfd::FileDialog::new();
            match existing_source.as_ref() {
                Some(SourceDocument::Markdown(_)) => {
                    let dialog = dialog
                        .set_title(text(language, "dialog-save-markdown-title"))
                        .set_file_name(format!("{document_name}.md"))
                        .add_filter("Markdown", &["md", "markdown"]);
                    dialog
                        .add_filter("CSV", &["csv"])
                        .add_filter("TSV", &["tsv"])
                        .save_file()
                }
                Some(SourceDocument::Csv(csv)) => {
                    let extension = ExportFormat::for_csv_delimiter(csv.delimiter).extension();
                    let dialog = dialog
                        .set_title(text(language, "dialog-save-csv-title"))
                        .set_file_name(format!("{document_name}.{extension}"))
                        .add_filter("CSV", &["csv"])
                        .add_filter("TSV", &["tsv"]);
                    dialog
                        .add_filter("Markdown", &["md", "markdown"])
                        .save_file()
                }
                None => dialog
                    .set_title(text(language, "dialog-save-markdown-title"))
                    .set_file_name(format!("{document_name}.md"))
                    .add_filter("Markdown", &["md", "markdown"])
                    .add_filter("CSV", &["csv"])
                    .add_filter("TSV", &["tsv"])
                    .save_file(),
            }
        });
        let path = target?;
        if !self.save_as_path_supported(document_id, &path) {
            self.status = Some(Message::new("status-save-format-mismatch"));
            return None;
        }
        Some(path)
    }

    /// Checks save preconditions shared by the path picker and the Save As options dialog.
    fn save_request_details(
        &mut self,
        document_id: DocumentId,
    ) -> Option<(String, Option<SourceDocument>)> {
        let text_growth_pending = self
            .pending_text_growth_sessions
            .get(&document_id)
            .is_some_and(|growth| growth.pending);
        if self.pending_structures.contains_key(&document_id) && !text_growth_pending
            || self.pending_history_restores.contains_key(&document_id)
        {
            self.status = Some(Message::new("status-finish-bulk-operation-before-save"));
            return None;
        }
        if self.pending_exports.contains_key(&document_id)
            || self.pending_export_writes.contains_key(&document_id)
            || self
                .active_export_confirmation
                .as_ref()
                .is_some_and(|export| export.document_id == document_id)
        {
            self.status = Some(Message::new("status-save-paused-during-export"));
            return None;
        }
        let Some((document_name, table_count, source)) =
            self.workspace.documents.get(&document_id).map(|document| {
                (
                    document.name.clone(),
                    document.tables.len(),
                    document.source.as_deref().cloned(),
                )
            })
        else {
            self.status = Some(Message::new("status-save-document-required"));
            return None;
        };
        if table_count == 0 {
            self.status = Some(Message::new("status-save-table-required"));
            return None;
        }
        if self.pending_saves.contains_key(&document_id) {
            self.status = Some(Message::new("status-document-save-in-progress"));
            return None;
        }
        let existing = self.file_backings.get(&document_id);
        let existing_source =
            source.or_else(|| existing.and_then(|backing| backing.source.clone()));
        Some((document_name, existing_source))
    }

    /// Routes alternate-format Save As through export or source conversion.
    pub(in crate::ui) fn save_as_path_supported(
        &self,
        document_id: DocumentId,
        path: &Path,
    ) -> bool {
        let source = self
            .workspace
            .documents
            .get(&document_id)
            .and_then(|document| document.source.as_deref())
            .or_else(|| {
                self.file_backings
                    .get(&document_id)
                    .and_then(|backing| backing.source.as_ref())
            });
        match source {
            Some(SourceDocument::Csv(_) | SourceDocument::Markdown(_)) | None => {
                is_csv_path(path) || is_markdown_path(path)
            }
        }
    }

    /// Requires a saved path whose extension matches the current editable source.
    pub(in crate::ui) fn backing_matches_document(&self, document_id: DocumentId) -> bool {
        let Some(backing) = self.file_backings.get(&document_id) else {
            return false;
        };
        let Some(source) = self
            .workspace
            .documents
            .get(&document_id)
            .and_then(|document| document.source.as_deref())
        else {
            return false;
        };
        path_matches_source(&backing.path, source)
    }

    /// Treats converted sources as needing a matching saved destination.
    pub(in crate::ui) fn document_requires_save(&self, document_id: DocumentId) -> bool {
        self.workspace
            .documents
            .get(&document_id)
            .is_some_and(|document| document.is_dirty())
            || !self.backing_matches_document(document_id)
    }

    /// Reports edits that need a user choice when the document closes.
    pub(in crate::ui) fn document_has_unsaved_changes(&self, document_id: DocumentId) -> bool {
        let document_changed = self
            .workspace
            .documents
            .get(&document_id)
            .is_some_and(|document| document.is_dirty());
        let editor_changed = self.app_close_window_has_changed_edit(document_id);
        document_changed
            || editor_changed
            || self.recovered_documents_needing_save.contains(&document_id)
            || self.pending_text_growth_sessions.contains_key(&document_id)
            || self.pending_recovered_cell_edits.contains_key(&document_id)
    }
}

/// Compares a file path with the current editable source kind.
pub(super) fn path_matches_source(path: &Path, source: &SourceDocument) -> bool {
    match source {
        SourceDocument::Markdown(_) => is_markdown_path(path),
        SourceDocument::Csv(_) => is_csv_path(path),
    }
}

/// Checks a Markdown file extension without interpreting the path name.
pub(super) fn is_markdown_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown")
        })
}

/// Checks a CSV or TSV file extension without interpreting the path name.
pub(super) fn is_csv_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("csv") || extension.eq_ignore_ascii_case("tsv")
        })
}
