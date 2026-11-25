use super::*;

impl TexcelApp {
    /// Shows the first-row header change after a CSV source becomes Markdown.
    fn record_csv_promotion_report(
        &mut self,
        document_id: DocumentId,
        report: Arc<crate::conversion_report::ExportReport>,
    ) {
        let source_path = self
            .file_backings
            .get(&document_id)
            .map_or_else(PathBuf::new, |backing| backing.path.clone());
        let file_name = source_path.file_name().map_or_else(
            || {
                self.workspace
                    .documents
                    .get(&document_id)
                    .map_or_else(String::new, |document| document.name.clone())
            },
            |name| name.to_string_lossy().into_owned(),
        );
        let import_report = Arc::new(crate::conversion_report::ImportReport::now(
            file_name,
            "CSV",
            report.entries.clone(),
        ));
        let mut state = ImportReportState::new(import_report, source_path);
        if let Some(report) = state.take_automatic_dialog() {
            self.queued_import_reports.push_back((document_id, report));
        }
        self.import_reports.insert(document_id, state);
        self.activate_next_import_report();
    }

    // Apply a prepared structure change to the matching document snapshot.
    pub(super) fn handle_structure_result(
        &mut self,
        document_id: DocumentId,
        token: JobToken,
        kind: FileResultKind,
    ) -> bool {
        match kind {
            FileResultKind::MarkdownCellDisplayPrepared {
                table_id,
                address,
                cell_input_revision,
                display,
            } => {
                let key = (document_id, table_id, address);
                if self.pending_markdown_displays.get(&key) != Some(&(token, cell_input_revision)) {
                    return true;
                }
                self.pending_markdown_displays.remove(&key);
                let Some(document) = self.workspace.documents.get(&document_id) else {
                    return true;
                };
                let still_current = document.cell_input_revision(table_id, address)
                    == cell_input_revision
                    && document.cell_input_is_dirty(table_id, address)
                    && matches!(
                        document.source.as_deref(),
                        Some(SourceDocument::Markdown(_))
                    )
                    && document
                        .tables
                        .iter()
                        .find(|table| table.id == table_id)
                        .and_then(|table| table.cell(address))
                        .is_some_and(|cell| matches!(&cell.input, CellInput::Text(_)));
                if still_current {
                    self.markdown_cell_displays.insert(
                        key,
                        CachedMarkdownCellDisplay {
                            document_revision: document.revision,
                            calculation_revision: document.calculation_revision,
                            cell_input_revision: Some(cell_input_revision),
                            raw_text: None,
                            edit_session_id: None,
                            display,
                        },
                    );
                }
            }
            FileResultKind::StructurePrepared(prepared) => {
                if self.pending_structures.get(&document_id) != Some(&token) {
                    return true;
                }
                self.pending_structures.remove(&document_id);
                let completion = self.take_completion(document_id, token);
                let is_bulk_paste = self.pending_bulk_pastes.remove(&document_id);
                let is_table_management = self.pending_table_management.remove(&document_id);
                let csv_cell_promotion = self
                    .pending_csv_promotion_cell_edits
                    .remove(&document_id)
                    .is_some()
                    || self
                        .pending_csv_promotion_cell_batches
                        .remove(&document_id)
                        .is_some();
                self.pending_markdown_link_edits.remove(&document_id);
                let conversion_report = prepared.conversion_report.clone();
                let operation_status = prepared.status.clone();
                let current = self
                    .workspace
                    .documents
                    .get(&document_id)
                    .is_some_and(|document| {
                        document.revision == token.revision
                            && document.calculation_revision == prepared.calculation_revision
                            && (prepared.replace_table_set
                                || document.view(prepared.view_table_id).is_some_and(|view| {
                                    view.layout.revision() == prepared.view_layout_revision
                                }))
                    });
                if !current {
                    self.finish_text_growth_failure(document_id);
                    self.finish_save_intent_failed(document_id);
                    self.status = Some(Message::new(if is_bulk_paste {
                        "status-paste-stale"
                    } else if is_table_management {
                        "status-table-history-recorded-stale"
                    } else {
                        "status-structure-stale"
                    }));
                    return true;
                }
                let growth_view_before = self
                    .pending_text_growth_sessions
                    .get(&document_id)
                    .and_then(|growth| {
                        let table_id = growth.session.table_id?;
                        let document = self.workspace.documents.get(&document_id)?;
                        let view = document.view(table_id)?;
                        Some((
                            table_id,
                            document.active_table()?.id,
                            view.selection,
                            view.active_cell(),
                            view.additional_selections.clone(),
                            self.workspace.active_document == Some(document_id)
                                && document.active_table()?.id == table_id
                                && view.selection == growth.selection
                                && view.active_cell() == growth.active_cell
                                && view.additional_selections == growth.additional_selections,
                        ))
                    });
                let applied = self.workspace.documents.get_mut(&document_id).map_or_else(
                    || Err(Message::new("status-document-not-found")),
                    |document| {
                        let (target, retired_tables) = document
                            .execute_prepared_structure(prepared)
                            .map_err(|error| {
                                Message::new("status-operation-failed-detail").with_message(
                                    "detail",
                                    crate::i18n::document_error_message(error),
                                )
                            })?;
                        if let Some((table_id, active_table, selection, active, additional, _)) =
                            &growth_view_before
                        {
                            if let Some(view) = document.view_mut(*table_id) {
                                view.selection = *selection;
                                view.active_cell = Some(*active);
                                view.additional_selections = additional.clone();
                            }
                            document.select_table(*active_table);
                        } else {
                            document.select_table(target.0);
                        }
                        let reset = document.take_calculation_reset_snapshot();
                        let current_source = document.source.as_deref().cloned();
                        Ok((
                            target,
                            retired_tables,
                            reset,
                            current_source,
                            growth_view_before.as_ref().is_some_and(|state| state.5),
                        ))
                    },
                );
                match applied {
                    Ok((
                        (_target_table, _target_cell),
                        retired_tables,
                        reset,
                        current_source,
                        growth_target_unchanged,
                    )) => {
                        let growth_edit = self.pending_text_growth_sessions.remove(&document_id);
                        if self.workspace.active_document == Some(document_id)
                            && growth_edit.is_none()
                        {
                            self.request_grid_focus = true;
                            self.request_scroll_to_selection = true;
                        }
                        if csv_cell_promotion && self.edit_document_id == Some(document_id) {
                            self.cancel_edit();
                        }
                        if let Some(tables) = retired_tables {
                            self.queue_retired_structure_tables(tables);
                        }
                        if let Some(backing) = self.file_backings.get_mut(&document_id) {
                            backing.source = current_source;
                        }
                        if let Some(report) = conversion_report {
                            self.record_csv_promotion_report(document_id, report);
                        }
                        self.clear_calculation_jobs_for_reset(document_id);
                        if let Some(snapshot) = reset {
                            let revision = self
                                .workspace
                                .documents
                                .get(&document_id)
                                .map_or(token.revision, |document| document.calculation_revision);
                            let token = self.next_job_token(document_id, revision);
                            self.initialize_calculation_shared(token, snapshot);
                        }
                        if self.workspace.active_document == Some(document_id)
                            && growth_target_unchanged
                            && growth_edit.as_ref().is_some_and(|growth| {
                                self.workspace
                                    .active_document()
                                    .and_then(|document| document.active_table())
                                    .is_some_and(|table| Some(table.id) == growth.session.table_id)
                            })
                        {
                            if let Some((row_delta, column_delta)) =
                                growth_edit.as_ref().and_then(|growth| growth.movement)
                            {
                                let return_column =
                                    growth_edit.as_ref().and_then(|growth| growth.return_column);
                                let moved_to_origin = return_column.is_some_and(|column| {
                                    growth_edit
                                        .as_ref()
                                        .and_then(|growth| growth.session.table_id)
                                        .is_some_and(|table_id| {
                                            self.move_to_input_sequence_column(
                                                document_id,
                                                table_id,
                                                column,
                                                row_delta,
                                            )
                                        })
                                });
                                let range_move = return_column.is_none()
                                    && row_delta != 0
                                    && column_delta == 0
                                    && self
                                        .workspace
                                        .active_document()
                                        .and_then(|document| {
                                            let table = document.active_table()?;
                                            document.view(table.id)
                                        })
                                        .is_some_and(|view| {
                                            let (start, end) = view.selection.normalized();
                                            start != end
                                        });
                                if !moved_to_origin
                                    && (!range_move || !self.cycle_selection_cell(row_delta < 0))
                                {
                                    self.move_selection(row_delta, column_delta, false);
                                }
                            }
                            self.request_grid_focus = true;
                            self.request_scroll_to_selection = true;
                        }
                        if let Some(growth) = growth_edit.as_ref()
                            && let Some(table_id) = growth.session.table_id
                        {
                            if growth_target_unchanged {
                                self.finish_input_sequence_move(
                                    document_id,
                                    table_id,
                                    growth.continue_tab_input,
                                );
                            } else {
                                self.input_sequence_origin = None;
                            }
                        }
                        let reached_limit = self
                            .workspace
                            .documents
                            .get(&document_id)
                            .and_then(Document::reached_text_source_limit);
                        // A warning outranks the notice that the user's command finished.
                        if let Some(limit) = reached_limit {
                            self.status =
                                Some(Message::new("status-resource-limit-reached").with_message(
                                    "limit",
                                    crate::i18n::resource_limit_message(limit),
                                ));
                        } else if let Some(status) = operation_status.or(completion) {
                            self.status = Some(status);
                        }
                        if growth_edit.is_some() {
                            self.apply_pending_growth_selection(document_id);
                        }
                        if let Some(growth) = growth_edit {
                            if self.pending_save_intents.contains_key(&document_id) {
                                self.continue_save_intent(document_id);
                            } else {
                                self.continue_deferred_growth_save(document_id, growth);
                            }
                        } else if self.pending_save_intents.contains_key(&document_id) {
                            self.continue_save_intent(document_id);
                        }
                    }
                    Err(message) => {
                        self.pending_growth_selection = None;
                        self.finish_text_growth_failure(document_id);
                        self.finish_save_intent_failed(document_id);
                        self.status = Some(message);
                    }
                }
            }
            FileResultKind::StructureHistoryRestored(prepared) => {
                if self.pending_history_restores.get(&document_id) != Some(&token) {
                    return true;
                }
                self.pending_history_restores.remove(&document_id);
                if !self
                    .workspace
                    .documents
                    .get(&document_id)
                    .is_some_and(|document| document.revision == token.revision)
                {
                    self.status = Some(Message::new("status-structure-history-document-changed"));
                    return true;
                }
                let applied = self.workspace.documents.get_mut(&document_id).map_or_else(
                    || Err(Message::new("status-document-not-found")),
                    |document| {
                        let (target, retired_tables) = document
                            .apply_prepared_structure_restore(prepared)
                            .map_err(|error| {
                                Message::new("status-history-failed").with_message(
                                    "detail",
                                    crate::i18n::document_error_message(error),
                                )
                            })?;
                        document.select_table(target.0);
                        let reset = document.take_calculation_reset_snapshot();
                        let current_source = document.source.as_deref().cloned();
                        Ok((target, retired_tables, reset, current_source))
                    },
                );
                match applied {
                    Ok((_target, retired_tables, reset, current_source)) => {
                        self.queue_retired_structure_tables(retired_tables);
                        if let Some(backing) = self.file_backings.get_mut(&document_id) {
                            backing.source = current_source;
                        }
                        self.clear_calculation_jobs_for_reset(document_id);
                        if let Some(snapshot) = reset {
                            let revision = self
                                .workspace
                                .documents
                                .get(&document_id)
                                .map_or(token.revision, |document| document.calculation_revision);
                            let token = self.next_job_token(document_id, revision);
                            self.initialize_calculation_shared(token, snapshot);
                        }
                        if self.workspace.active_document == Some(document_id) {
                            self.request_grid_focus = true;
                            self.request_scroll_to_selection = true;
                        }
                    }
                    Err(message) => self.status = Some(message),
                }
            }
            _ => unreachable!("file result was routed to the wrong handler"),
        }
        false
    }
}
