use super::*;

pub(super) struct DeferredAutoFit {
    table_id: TableId,
    columns: Vec<usize>,
    layout_revision: u64,
    row_limit: Option<usize>,
}

pub(super) struct PendingAutoFit {
    pub(super) token: JobToken,
    table_id: TableId,
    columns: Vec<usize>,
    calculation_revision: u64,
    calculation_snapshot: Option<FormulaResultSnapshot>,
    layout_revision: u64,
    font_layout_generation: u64,
    display_mode: CellDisplayMode,
    language: Language,
    show_invisibles: bool,
    wrap_text: bool,
    pixels_per_point: f32,
}

// Maps each document display mode to its cell measurement mode.
fn document_cell_display_mode(mode: crate::document::DocumentDisplayMode) -> CellDisplayMode {
    match mode {
        crate::document::DocumentDisplayMode::Live => CellDisplayMode::View,
        crate::document::DocumentDisplayMode::Source => CellDisplayMode::Source,
    }
}

/// How long a newly loaded document's window waits for its first column fit.
const INITIAL_AUTO_FIT_WAIT: Duration = Duration::from_secs(2);
/// Rows measured by the fit on opening; a long file's later rows are not read for it.
const INITIAL_AUTO_FIT_ROWS: usize = 1_000;

impl TexcelApp {
    /// Returns the columns a header edge action applies to: every selected whole column when
    /// `column` is one of them, as in Excel, or else `column` alone. A selection of the whole
    /// sheet stops at the table's used columns (or `column`), not the sheet's 16,384.
    pub(super) fn header_action_columns(
        &self,
        document_id: DocumentId,
        table_id: TableId,
        column: usize,
    ) -> Vec<usize> {
        let Some(document) = self.workspace.documents.get(&document_id) else {
            return vec![column];
        };
        let (Some(table), Some(view)) = (
            document.tables.iter().find(|table| table.id == table_id),
            document.view(table_id),
        ) else {
            return vec![column];
        };
        let whole_columns = view
            .selected_ranges()
            .map(CellRange::normalized)
            .filter(|(start, end)| start.row == 0 && end.row + 1 >= MAX_ROWS)
            .collect::<Vec<_>>();
        if !whole_columns
            .iter()
            .any(|(start, end)| (start.column..=end.column).contains(&column))
        {
            return vec![column];
        }
        let last = table.extent_end().column.max(column);
        let mut columns = whole_columns
            .iter()
            .flat_map(|(start, end)| start.column..=end.column.min(last))
            .collect::<Vec<_>>();
        columns.sort_unstable();
        columns.dedup();
        columns
    }

    /// Submits every column up to the table's used extent for one background measurement.
    pub(super) fn request_all_columns_auto_fit(
        &mut self,
        ctx: &egui::Context,
        document_id: DocumentId,
        table_id: TableId,
    ) {
        let Some(last_column) = self
            .workspace
            .documents
            .get(&document_id)
            .and_then(|document| document.tables.iter().find(|table| table.id == table_id))
            .map(|table| table.extent_end().column)
        else {
            return;
        };
        self.request_columns_auto_fit(ctx, document_id, table_id, (0..=last_column).collect());
    }

    /// Submits the given columns for one background width measurement over every row.
    pub(super) fn request_columns_auto_fit(
        &mut self,
        ctx: &egui::Context,
        document_id: DocumentId,
        table_id: TableId,
        columns: Vec<usize>,
    ) {
        self.request_columns_auto_fit_within(ctx, document_id, table_id, columns, None);
    }

    /// Submits the given columns for one background width measurement, optionally over the
    /// first `row_limit` rows only.
    fn request_columns_auto_fit_within(
        &mut self,
        ctx: &egui::Context,
        document_id: DocumentId,
        table_id: TableId,
        columns: Vec<usize>,
        row_limit: Option<usize>,
    ) {
        let Some(document) = self.workspace.documents.get(&document_id) else {
            return;
        };
        let Some(view) = document.view(table_id) else {
            return;
        };
        if self.workspace.active_document != Some(document_id)
            || document.active_table != table_id
            || self.edit.is_editing()
        {
            return;
        }
        let layout_revision = view.layout.revision();
        if self.pending_calculations.contains_key(&document_id)
            || self.pending_initial_calculations.contains_key(&document_id)
            || self.calculation_retries.contains(&document_id)
        {
            self.deferred_auto_fits.insert(
                document_id,
                DeferredAutoFit {
                    table_id,
                    columns,
                    layout_revision,
                    row_limit,
                },
            );
            return;
        }
        let (revision, calculation_revision, layout_revision, wrap_text, display_mode, layout) = {
            let Some(document) = self.workspace.documents.get(&document_id) else {
                return;
            };
            let (Some(_), Some(view)) = (
                document.tables.iter().find(|table| table.id == table_id),
                document.view(table_id),
            ) else {
                return;
            };
            (
                document.revision,
                document.calculation_revision,
                view.layout.revision(),
                view.wrap_text,
                document_cell_display_mode(document.display_mode),
                view.layout.as_ref().clone(),
            )
        };
        if self.pending_auto_fits.contains_key(&document_id) {
            return;
        }
        let calculation = self
            .calculation_snapshots
            .get(&document_id)
            .filter(|(version, _)| *version == calculation_revision)
            .map(|(_, snapshot)| Arc::clone(snapshot));
        let calculation_snapshot = calculation.clone();
        let (font_definitions, text_options) =
            ctx.fonts(|fonts| (fonts.definitions().clone(), *fonts.options()));
        let pixels_per_point = ctx.pixels_per_point();
        let token = self.next_job_token(document_id, revision);
        let request = crate::file_worker::AutoFitRequest {
            structure: auto_fit_structure_request(
                self.workspace
                    .documents
                    .get(&document_id)
                    .expect("auto-fit document remains open"),
                table_id,
                &columns,
            ),
            columns: columns.clone(),
            layout,
            display_mode,
            language: self.language,
            wrap_text,
            show_invisibles: self.show_invisibles,
            calculation,
            font_definitions,
            text_options,
            pixels_per_point,
            font_layout_generation: self.font_layout_generation,
            minimum_column_width_pixels: 32.0,
            maximum_column_width_pixels: (crate::document::MAX_COLUMN_WIDTH_UNITS
                * self.grid_character_width
                + crate::document::COLUMN_TEXT_PADDING_PIXELS)
                as f32,
            character_width: self.grid_character_width,
            row_limit,
        };
        if self.file_worker.auto_fit(token, request).is_err() {
            self.status = Some(Message::new("status-layout-change-failed"));
            return;
        }
        self.pending_auto_fits.insert(
            document_id,
            PendingAutoFit {
                token,
                table_id,
                columns,
                calculation_revision,
                calculation_snapshot,
                layout_revision,
                font_layout_generation: self.font_layout_generation,
                display_mode,
                language: self.language,
                show_invisibles: self.show_invisibles,
                wrap_text,
                pixels_per_point,
            },
        );
        ctx.request_repaint();
    }

    /// Fits the active table of a newly loaded document once, before its window is shown:
    /// every used column that has no saved width. Returns once the fit is done, could not
    /// start, or took too long, so the window then appears.
    pub(super) fn advance_initial_auto_fit(
        &mut self,
        ctx: &egui::Context,
        document_id: DocumentId,
    ) {
        let Some((loaded_at, requested)) = self.initial_auto_fits.get(&document_id).copied() else {
            return;
        };
        if loaded_at.elapsed() > INITIAL_AUTO_FIT_WAIT {
            self.initial_auto_fits.remove(&document_id);
            return;
        }
        self.start_ready_auto_fits(ctx);
        let running = self.pending_auto_fits.contains_key(&document_id)
            || self.deferred_auto_fits.contains_key(&document_id);
        if requested {
            if !running {
                self.initial_auto_fits.remove(&document_id);
            }
            return;
        }
        let Some(document) = self.workspace.documents.get(&document_id) else {
            self.initial_auto_fits.remove(&document_id);
            return;
        };
        let table_id = document.active_table;
        let columns = document
            .tables
            .iter()
            .find(|table| table.id == table_id)
            .zip(document.view(table_id))
            .map(|(table, view)| {
                (0..=table.extent_end().column)
                    .filter(|column| view.layout.column(*column).is_none())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if columns.is_empty() || running {
            self.initial_auto_fits.remove(&document_id);
            return;
        }
        if let Some(entry) = self.initial_auto_fits.get_mut(&document_id) {
            entry.1 = true;
        }
        self.request_columns_auto_fit_within(
            ctx,
            document_id,
            table_id,
            columns,
            Some(INITIAL_AUTO_FIT_ROWS),
        );
    }

    /// Starts deferred measurements after the visible calculation root is published.
    pub(super) fn start_ready_auto_fits(&mut self, ctx: &egui::Context) {
        let mut ready = Vec::new();
        let mut discard = Vec::new();
        for (document_id, deferred) in &self.deferred_auto_fits {
            let Some(document) = self.workspace.documents.get(document_id) else {
                discard.push(*document_id);
                continue;
            };
            let layout_is_current = self.workspace.active_document == Some(*document_id)
                && document.active_table == deferred.table_id
                && document
                    .view(deferred.table_id)
                    .is_some_and(|view| view.layout.revision() == deferred.layout_revision);
            if !layout_is_current || self.calculation_retry_available.contains(document_id) {
                discard.push(*document_id);
                continue;
            }
            let calculation_is_pending = self.pending_calculations.contains_key(document_id)
                || self.pending_initial_calculations.contains_key(document_id)
                || self.calculation_retries.contains(document_id);
            let has_current_root = self
                .calculation_snapshots
                .get(document_id)
                .is_some_and(|(revision, _)| *revision == document.calculation_revision);
            if !calculation_is_pending {
                if has_current_root {
                    ready.push((
                        *document_id,
                        deferred.table_id,
                        deferred.columns.clone(),
                        deferred.row_limit,
                    ));
                } else {
                    discard.push(*document_id);
                }
            }
        }
        for document_id in discard {
            self.deferred_auto_fits.remove(&document_id);
        }
        for (document_id, table_id, columns, row_limit) in ready {
            self.deferred_auto_fits.remove(&document_id);
            self.request_columns_auto_fit_within(ctx, document_id, table_id, columns, row_limit);
        }
    }

    /// Cancels queued or running auto-fit work for one document.
    pub(super) fn cancel_auto_fit_for(&mut self, document_id: DocumentId) {
        self.deferred_auto_fits.remove(&document_id);
        if let Some(pending) = self.pending_auto_fits.remove(&document_id) {
            self.file_worker.cancel(pending.token);
            self.status = Some(Message::new("status-autofit-cancelled"));
        }
    }

    /// Clears AutoFit requests when its shared file worker can no longer report results.
    pub(super) fn fail_stopped_auto_fits(&mut self) {
        if self.pending_auto_fits.is_empty() && self.deferred_auto_fits.is_empty() {
            return;
        }
        for pending in self.pending_auto_fits.values() {
            self.file_worker.cancel(pending.token);
        }
        self.pending_auto_fits.clear();
        self.deferred_auto_fits.clear();
        self.status = Some(Message::new("status-file-worker-stopped"));
    }

    /// Applies a measured local dimension only while its captured display state still matches.
    pub(super) fn handle_auto_fit_result(
        &mut self,
        document_id: DocumentId,
        token: JobToken,
        widths: Vec<(usize, f64)>,
    ) {
        let Some(pending) = self.pending_auto_fits.get(&document_id) else {
            return;
        };
        if pending.token != token {
            return;
        }
        let pending = self
            .pending_auto_fits
            .remove(&document_id)
            .expect("pending fit exists");
        let calculation_snapshot = self
            .calculation_snapshots
            .get(&document_id)
            .filter(|(revision, _)| *revision == pending.calculation_revision)
            .map(|(_, snapshot)| snapshot);
        let calculation_matches =
            match (pending.calculation_snapshot.as_ref(), calculation_snapshot) {
                (None, None) => true,
                (Some(expected), Some(current)) => Arc::ptr_eq(expected, current),
                _ => false,
            };
        let current = self
            .workspace
            .documents
            .get(&document_id)
            .is_some_and(|document| {
                self.workspace.active_document == Some(document_id)
                    && document.active_table == pending.table_id
                    && document.revision == token.revision
                    && document.calculation_revision == pending.calculation_revision
                    && document_cell_display_mode(document.display_mode) == pending.display_mode
                    && document.view(pending.table_id).is_some_and(|view| {
                        view.layout.revision() == pending.layout_revision
                            && view.wrap_text == pending.wrap_text
                    })
            });
        if !current
            || !calculation_matches
            || widths
                .iter()
                .map(|(column, _)| *column)
                .ne(pending.columns.iter().copied())
            || self.font_layout_generation != pending.font_layout_generation
            || self.language != pending.language
            || self.show_invisibles != pending.show_invisibles
            || (self.last_pixels_per_point - pending.pixels_per_point).abs() > f32::EPSILON
        {
            return;
        }
        let Some(document) = self.workspace.documents.get_mut(&document_id) else {
            return;
        };
        let Some(view) = document.view_mut(pending.table_id) else {
            return;
        };
        for (column, width) in widths {
            let mut layout = view.layout.column(column).unwrap_or_default();
            layout.width = Some(width);
            view.set_layout_value(
                LayoutTarget::Column(column),
                LayoutValue::Column(Some(layout)),
            );
        }
        self.mark_document_preference_change(
            document_id,
            pending.table_id,
            TablePreferenceField::ViewLayout,
        );
    }
}
