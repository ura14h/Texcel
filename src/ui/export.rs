use super::*;

impl TexcelApp {
    // Build export options and start background preflight checks.
    pub(super) fn start_export_preflight(
        &mut self,
        document_id: DocumentId,
        options: ExportOptions,
        destination: PathBuf,
    ) {
        if self.pending_saves.contains_key(&document_id) {
            self.status = Some(Message::new("status-export-paused-during-save"));
            return;
        }
        if !self.pending_exports.is_empty()
            || !self.pending_export_writes.is_empty()
            || self.active_export_confirmation.is_some()
        {
            self.status = Some(Message::new("status-export-in-progress"));
            return;
        }
        if self.edit.is_editing()
            && self
                .edit_document_id
                .or_else(|| self.workspace.active_document().map(|document| document.id))
                == Some(document_id)
        {
            self.commit_edit(None);
        }
        let Some(document) = self.workspace.documents.get(&document_id) else {
            self.status = Some(Message::new("status-export-document-required"));
            return;
        };
        let source = document.source.as_deref().map(|source| match source {
            SourceDocument::Csv(source) => ExportSource::Csv(Arc::clone(source)),
            SourceDocument::Markdown(source) => ExportSource::Markdown(Arc::clone(source)),
        });
        let source_path = self
            .file_backings
            .get(&document_id)
            .map(|backing| backing.path.clone());
        let source_report = (!self.save_as_export_documents.contains(&document_id))
            .then(|| self.import_reports.get(&document_id))
            .flatten()
            .map(ImportReportState::reopen);
        let tables = document
            .tables
            .iter()
            .map(|table| ExportTableMeta {
                id: table.id,
                name: table.name.clone(),
                rows: table.row_count(),
                columns: table.column_count(),
            })
            .collect();
        let changes = export_cell_changes(document);
        let revision = document.revision;
        let calculation_revision = document.calculation_revision;
        let formula_results = self
            .calculation_snapshots
            .get(&document_id)
            .filter(|(snapshot_revision, _)| *snapshot_revision == calculation_revision)
            .map(|(_, snapshot)| Arc::clone(snapshot));
        let request = ExportRequest {
            document_id,
            revision,
            options,
            destination: destination.clone(),
            source_path,
            source,
            source_report,
            tables,
            changes,
            format_changes: Vec::new(),
            formula_results,
        };
        let token = self.next_job_token(document_id, revision);
        match self.file_worker.prepare_export(token, request) {
            Ok(()) => {
                self.pending_exports.insert(document_id, token);
                self.export_configs
                    .insert(document_id, (options, destination));
                self.status = None;
            }
            Err(error) => {
                self.status = Some(
                    Message::new("status-export-prepare-failed")
                        .with_message("detail", submit_error_message(error)),
                );
            }
        }
    }

    // Open confirmation for a prepared export result.
    pub(super) fn begin_prepared_export(
        &mut self,
        document_id: DocumentId,
        prepared: PreparedExport,
    ) {
        let is_current = self
            .workspace
            .documents
            .get(&document_id)
            .is_some_and(|document| document.revision == prepared.report.document_revision);
        if !is_current {
            self.export_configs.remove(&document_id);
            self.save_as_export_documents.remove(&document_id);
            self.finish_save_intent_failed(document_id);
            self.status = Some(Message::new("status-export-document-changed"));
            return;
        }
        if self.save_as_export_documents.contains(&document_id)
            && !self.prepared_export_is_full_csv_save(document_id, &prepared)
            && self
                .pending_save_intents
                .get(&document_id)
                .is_some_and(|intent| {
                    intent.app_close_generation == Some(self.app_close_save_generation)
                })
        {
            self.app_close_save_failed = true;
        }
        if prepared.report.requires_confirmation() || !prepared.report.can_export() {
            let options = self.export_configs.get(&document_id).map_or(
                ExportOptions {
                    format: ExportFormat::Csv,
                    scope: ExportScope::AllTables,
                    formulas: FormulaOutput::Formula,
                    modifiers: ModifierOutput::Convert,
                    markdown_header: MarkdownHeader::UseFirstRow,
                },
                |(options, _)| *options,
            );
            let destination = self
                .export_configs
                .get(&document_id)
                .map_or_else(PathBuf::new, |(_, path)| path.clone());
            self.active_export_confirmation = Some(ExportConfirmation {
                document_id,
                options,
                destination,
                prepared,
                selected_group: 0,
                queue_error: None,
            });
            return;
        }
        self.submit_prepared_export(ExportConfirmation {
            document_id,
            options: self.export_configs.get(&document_id).map_or(
                ExportOptions {
                    format: ExportFormat::Csv,
                    scope: ExportScope::AllTables,
                    formulas: FormulaOutput::Formula,
                    modifiers: ModifierOutput::Convert,
                    markdown_header: MarkdownHeader::UseFirstRow,
                },
                |(options, _)| *options,
            ),
            destination: self
                .export_configs
                .get(&document_id)
                .map_or_else(PathBuf::new, |(_, path)| path.clone()),
            prepared,
            selected_group: 0,
            queue_error: None,
        });
    }

    // Submit the confirmed export to the file worker.
    pub(super) fn submit_prepared_export(&mut self, mut export: ExportConfirmation) {
        if self
            .workspace
            .documents
            .get(&export.document_id)
            .is_none_or(|document| document.revision != export.prepared.report.document_revision)
        {
            self.active_export_confirmation = Some(export);
            return;
        }
        if !export.prepared.report.can_export() || export.prepared.artifacts.is_empty() {
            self.active_export_confirmation = Some(export);
            return;
        }
        let full_csv_save =
            self.prepared_export_is_full_csv_save(export.document_id, &export.prepared);
        if full_csv_save {
            self.save_as_export_documents.remove(&export.document_id);
            let artifact = std::mem::take(&mut export.prepared.artifacts)
                .into_iter()
                .next()
                .expect("one complete CSV artifact was checked");
            self.save_csv_export_as_document(export.document_id, artifact);
            return;
        }
        let token =
            self.next_job_token(export.document_id, export.prepared.report.document_revision);
        let artifacts = std::mem::take(&mut export.prepared.artifacts);
        match self.file_worker.save_export(token, artifacts) {
            Ok(()) => {
                self.pending_export_writes.insert(export.document_id, token);
                self.save_as_export_documents.remove(&export.document_id);
                self.active_export_confirmation = None;
                self.status = None;
            }
            Err((error, artifacts)) => {
                export.prepared.artifacts = artifacts;
                export.queue_error = Some(submit_error_message(error));
                self.status = Some(Message::new("status-save-unavailable"));
                self.active_export_confirmation = Some(export);
            }
        }
    }

    // Recognizes the only CSV export that can acknowledge the whole Markdown document.
    fn prepared_export_is_full_csv_save(
        &self,
        document_id: DocumentId,
        prepared: &PreparedExport,
    ) -> bool {
        let options = self
            .export_configs
            .get(&document_id)
            .map(|(options, _)| *options);
        let Some(options) = options else {
            return false;
        };
        let summary = prepared.report.summary();
        self.save_as_export_documents.contains(&document_id)
            && options.format == ExportFormat::Csv
            && summary.omitted == 0
            && summary.unprocessable == 0
            && csv_save_report_preserves_document(&prepared.report)
            && prepared.artifacts.len() == 1
            && self
                .workspace
                .documents
                .get(&document_id)
                .is_some_and(|document| {
                    document.tables.len() == 1
                        && matches!(
                            document.source.as_deref(),
                            Some(SourceDocument::Markdown(_))
                        )
                        && options.scope == ExportScope::Selected(document.active_table)
                })
    }

    // Save the conversion report for the latest export.
    pub(super) fn save_active_export_report(&mut self) {
        let Some(export) = self.active_export_confirmation.as_ref() else {
            return;
        };
        let document_id = export.document_id;
        let report = Arc::clone(&export.prepared.report);
        let Some(path) = rfd::FileDialog::new()
            .set_title(text(self.language, "dialog-save-export-report-title"))
            .set_file_name(text(self.language, "dialog-save-export-report-name"))
            .add_filter(text(self.language, "dialog-filter-text"), &["txt"])
            .save_file()
        else {
            return;
        };
        if !path
            .extension()
            .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("txt"))
        {
            self.status = Some(Message::new("status-export-report-txt-required"));
            return;
        }
        if self.pending_export_report_saves.contains_key(&document_id) {
            self.status = Some(Message::new("status-report-saving"));
            return;
        }
        let source_path = self
            .file_backings
            .get(&document_id)
            .map(|backing| backing.path.clone());
        let revision = report.document_revision;
        let token = self.next_job_token(document_id, revision);
        match self
            .file_worker
            .save_export_report(token, path, source_path, report, self.language)
        {
            Ok(()) => {
                self.pending_export_report_saves.insert(document_id, token);
                self.status = None;
            }
            Err(error) => {
                self.status = Some(
                    Message::new("status-report-save-failed")
                        .with_message("detail", submit_error_message(error)),
                )
            }
        }
    }

    // Selects one pending report to display next.
    pub(super) fn activate_next_import_report(&mut self) {
        if self.active_import_report.is_none() {
            self.active_import_report = self.queued_import_reports.pop_front();
            self.selected_import_group = 0;
        }
    }

    // Shows the report from load time again from the menu.
    pub(super) fn reopen_import_report(&mut self, document_id: DocumentId) {
        if let Some(report) = self
            .import_reports
            .get(&document_id)
            .map(ImportReportState::reopen)
        {
            self.queued_import_reports.push_back((document_id, report));
            self.activate_next_import_report();
        }
    }

    // Chooses a report destination and asks the file worker to save its UTF-8 content.
    pub(super) fn save_active_import_report(&mut self) {
        let Some((document_id, report)) = self.active_import_report.clone() else {
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .set_title(text(self.language, "dialog-save-import-report-title"))
            .set_file_name(text(self.language, "dialog-save-import-report-name"))
            .add_filter(text(self.language, "dialog-filter-text"), &["txt"])
            .save_file()
        else {
            return;
        };
        if !path
            .extension()
            .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("txt"))
        {
            self.status = Some(Message::new("status-import-report-txt-required"));
            return;
        }
        if self.pending_report_saves.contains_key(&document_id) {
            self.status = Some(Message::new("status-report-save-in-progress"));
            return;
        }
        let revision = self
            .workspace
            .documents
            .get(&document_id)
            .map_or(0, |document| document.revision);
        let token = self.next_job_token(document_id, revision);
        let Some(source_path) = self
            .import_reports
            .get(&document_id)
            .map(|state| state.source_path().to_path_buf())
        else {
            self.status = Some(Message::new("status-source-file-info-unavailable"));
            return;
        };
        match self
            .file_worker
            .save_report(token, path, source_path, report, self.language)
        {
            Ok(()) => {
                self.pending_report_saves.insert(document_id, token);
                self.status = None;
            }
            Err(error) => self.status = Some(submit_error_message(error)),
        }
    }
}

/// Combines sparse source overlays with later cell edits for export workers.
fn export_cell_changes(document: &Document) -> Vec<(TableId, CellAddress, Option<Cell>)> {
    let mut changes = document
        .text_overlay_cells()
        .iter()
        .map(|(table_id, address, cell)| ((*table_id, *address), Some(cell.clone())))
        .collect::<BTreeMap<_, _>>();
    for (table_id, address, cell) in document.changed_cell_snapshot() {
        changes.insert((table_id, address), cell);
    }
    changes
        .into_iter()
        .map(|((table_id, address), cell)| (table_id, address, cell))
        .collect()
}

// Allows a formula safety note when the exported CSV still preserves the Markdown document.
fn csv_save_report_preserves_document(report: &crate::conversion_report::ExportReport) -> bool {
    report.entries.iter().all(|entry| {
        entry.class == crate::conversion_report::ConversionClass::Retained
            || (entry.class == crate::conversion_report::ConversionClass::Converted
                && entry
                    .message
                    .as_ref()
                    .is_some_and(|message| message.key() == "report-reason-formula-csv-text"))
    })
}
