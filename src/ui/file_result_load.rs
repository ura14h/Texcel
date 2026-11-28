use super::*;

impl TexcelApp {
    // Add a successfully loaded document and report import details.
    pub(super) fn handle_load_result(
        &mut self,
        document_id: DocumentId,
        token: JobToken,
        path: PathBuf,
        user_path: PathBuf,
        kind: FileResultKind,
    ) -> bool {
        match kind {
            FileResultKind::LoadedCsv {
                source,
                mut document,
                calculation,
                identity,
            } => {
                if self
                    .pending_csv_opens
                    .get(&document_id)
                    .is_some_and(|pending| pending.job_id == token.job_id)
                {
                    set_document_name(&mut document, &user_path, true);
                    self.pending_csv_opens.remove(&document_id);
                    self.close_startup_placeholder_for_load();
                    self.initialize_calculation(token, calculation);
                    document.set_source(Some(Arc::new(SourceDocument::Csv(Arc::clone(&source)))));
                    self.workspace.insert_loaded_document(document);
                    self.queue_opening_fit(document_id);
                    self.remember_recent_file(user_path.clone());
                    self.user_file_paths.insert(document_id, user_path.clone());
                    self.file_backings.insert(
                        document_id,
                        FileBacking {
                            path,
                            identity,
                            source: Some(SourceDocument::Csv(source)),
                            overwrite_confirmed: false,
                        },
                    );
                    self.grid_focused = false;
                    self.request_editor_focus = false;
                    self.request_grid_focus = true;
                }
            }
            FileResultKind::LoadedMarkdown {
                source,
                mut document,
                calculation,
                identity,
            } => {
                if self.pending_markdown_loads.get(&document_id) != Some(&token) {
                    return true;
                }
                set_document_name(&mut document, &user_path, true);
                self.pending_markdown_loads.remove(&document_id);
                self.close_startup_placeholder_for_load();
                self.initialize_calculation(token, calculation);
                document.set_source(Some(Arc::new(SourceDocument::Markdown(Arc::clone(
                    &source,
                )))));
                self.workspace.insert_loaded_document(document);
                self.queue_opening_fit(document_id);
                self.import_origins.insert(document_id, path.clone());
                self.remember_recent_file(user_path.clone());
                self.user_file_paths.insert(document_id, user_path.clone());
                self.file_backings.insert(
                    document_id,
                    FileBacking {
                        path,
                        identity,
                        source: Some(SourceDocument::Markdown(source)),
                        overwrite_confirmed: false,
                    },
                );
                if let Some(document) = self.workspace.documents.get_mut(&document_id) {
                    for view in document.views.values_mut() {
                        view.header_row = Some(0);
                    }
                }
                self.grid_focused = false;
                self.request_editor_focus = false;
                self.request_grid_focus = true;
            }
            FileResultKind::ImportedMarkdown {
                source,
                mut document,
                calculation,
                identity,
                mut report,
                input_format,
            } => {
                set_import_report_file_name(&mut report, &user_path);
                let pending = match input_format {
                    crate::file_worker::ImportedFormat::Html => &mut self.pending_html_loads,
                    crate::file_worker::ImportedFormat::Xlsx
                    | crate::file_worker::ImportedFormat::Ods => &mut self.pending_workbook_loads,
                };
                if pending.get(&document_id) != Some(&token) {
                    return true;
                }
                set_document_name(&mut document, &user_path, false);
                pending.remove(&document_id);
                self.close_startup_placeholder_for_load();
                self.initialize_calculation(token, calculation);
                document.set_source(Some(Arc::new(SourceDocument::Markdown(Arc::clone(
                    &source,
                )))));
                self.workspace.insert_loaded_document(document);
                self.queue_opening_fit(document_id);
                self.remember_recent_file(user_path.clone());
                self.user_file_paths.insert(document_id, user_path.clone());
                self.file_backings.insert(
                    document_id,
                    FileBacking {
                        path: path.clone(),
                        identity,
                        source: Some(SourceDocument::Markdown(source)),
                        overwrite_confirmed: false,
                    },
                );
                if let Some(document) = self.workspace.documents.get_mut(&document_id) {
                    for view in document.views.values_mut() {
                        view.header_row = Some(0);
                    }
                }
                let mut report_state =
                    ImportReportState::new(Arc::clone(&report), user_path.clone());
                if let Some(automatic) = report_state.take_automatic_dialog() {
                    self.queued_import_reports
                        .push_back((document_id, automatic));
                }
                self.import_reports.insert(document_id, report_state);
                self.activate_next_import_report();
                self.grid_focused = false;
                self.request_editor_focus = false;
                self.request_grid_focus = true;
                let _ = input_format;
                // Only a formatting-only loss needs a notice; a clean import needs none.
                self.import_notice = (report.has_formatting_notes() && !report.requires_dialog())
                    .then(|| (document_id, Message::new("status-import-formatting-only")));
                self.status = None;
            }
            FileResultKind::LoadNeedsEncodingChoice {
                suggested,
                analysis,
                error,
            } => {
                if let Some(pending) = self.pending_csv_opens.get_mut(&document_id)
                    && pending.job_id == token.job_id
                {
                    let analysis = analysis.as_ref();
                    if let Some(encoding) = suggested {
                        pending.encoding = encoding;
                    }
                    if let Some(analysis) = analysis {
                        pending.delimiter = analysis.suggested;
                    }
                    pending.prompt = Some(CsvLoadPrompt::Settings(CsvLoadSettingsPrompt {
                        candidates: analysis.map_or_else(Vec::new, |analysis| {
                            analysis
                                .candidates
                                .iter()
                                .map(|candidate| candidate.delimiter)
                                .collect()
                        }),
                        delimiter_text: pending.delimiter.unwrap_or(',').to_string(),
                        previews: analysis
                            .map_or_else(BTreeMap::new, |analysis| analysis.previews.clone()),
                        selected_encoding: pending.encoding,
                        needs_encoding: true,
                        error,
                    }));
                }
            }
            FileResultKind::LoadNeedsDelimiterChoice { analysis } => {
                if let Some(pending) = self.pending_csv_opens.get_mut(&document_id)
                    && pending.job_id == token.job_id
                {
                    let previews = analysis.previews.clone();
                    pending.prompt = Some(CsvLoadPrompt::Settings(CsvLoadSettingsPrompt {
                        candidates: analysis
                            .candidates
                            .iter()
                            .map(|candidate| candidate.delimiter)
                            .collect(),
                        delimiter_text: pending
                            .delimiter
                            .or(analysis.suggested)
                            .unwrap_or(',')
                            .to_string(),
                        previews,
                        selected_encoding: pending.encoding,
                        needs_encoding: false,
                        error: None,
                    }));
                }
            }
            _ => unreachable!("file result was routed to the wrong handler"),
        }
        false
    }
}

/// Uses the selected path spelling for the document's visible name.
fn set_document_name(document: &mut Document, path: &std::path::Path, include_extension: bool) {
    let name = if include_extension {
        path.file_name()
    } else {
        path.file_stem()
    };
    if let Some(name) = name {
        document.name = name.to_string_lossy().into_owned();
    }
}

/// Uses the selected path's full name in its import report.
fn set_import_report_file_name(report: &mut Arc<ImportReport>, path: &std::path::Path) {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| report.file_name.clone());
    Arc::make_mut(report).file_name = file_name;
}
