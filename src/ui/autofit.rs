use super::*;

pub(super) struct DeferredAutoFit {
    table_id: TableId,
    columns: Vec<usize>,
    layout_revision: u64,
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

    /// Submits the given columns of one table for a background width measurement.
    pub(super) fn request_columns_auto_fit(
        &mut self,
        ctx: &egui::Context,
        document_id: DocumentId,
        table_id: TableId,
        columns: Vec<usize>,
    ) {
        let Some(document) = self.workspace.documents.get(&document_id) else {
            return;
        };
        let Some(view) = document.view(table_id) else {
            return;
        };
        if self.workspace.active_document != Some(document_id) || self.edit.is_editing() {
            return;
        }
        let layout_revision = view.layout.revision();
        let deferred = DeferredAutoFit {
            table_id,
            columns: columns.clone(),
            layout_revision,
        };
        // The fit measures formula results, so it starts once the calculation is done.
        if self.pending_calculations.contains_key(&document_id)
            || self.pending_initial_calculations.contains_key(&document_id)
            || self.calculation_retries.contains(&document_id)
        {
            self.deferred_auto_fits.insert(document_id, deferred);
            return;
        }
        if self.pending_auto_fits.contains_key(&document_id) {
            return;
        }
        let (revision, calculation_revision, wrap_text, display_mode) = (
            document.revision,
            document.calculation_revision,
            view.wrap_text,
            document_cell_display_mode(document.display_mode),
        );
        let target = crate::file_worker::AutoFitTarget {
            table_id,
            columns: columns.clone(),
            layout: view.layout.as_ref().clone(),
            wrap_text,
            header_row: document.header_row(table_id),
        };
        let Some(request) = self.auto_fit_request(
            ctx,
            document_id,
            vec![target],
            None,
            self.grid_character_width,
        ) else {
            return;
        };
        let calculation_snapshot = request.calculation.clone();
        let pixels_per_point = request.pixels_per_point;
        let token = self.next_job_token(document_id, revision);
        match self.file_worker.auto_fit(token, request) {
            Ok(()) => (),
            // A busy worker (for example right after a file loads) takes the fit later.
            Err(SubmitError::QueueFull) => {
                self.deferred_auto_fits.insert(document_id, deferred);
                ctx.request_repaint_after(Duration::from_millis(50));
                return;
            }
            Err(SubmitError::Stopped) => {
                self.status = Some(Message::new("status-layout-change-failed"));
                return;
            }
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

    /// Builds one measurement request for the given tables of a document, with its current
    /// content, formula results, and the fonts at the current screen scale.
    pub(super) fn auto_fit_request(
        &self,
        ctx: &egui::Context,
        document_id: DocumentId,
        targets: Vec<crate::file_worker::AutoFitTarget>,
        row_limit: Option<usize>,
        character_width: f64,
    ) -> Option<crate::file_worker::AutoFitRequest> {
        let document = self.workspace.documents.get(&document_id)?;
        let calculation = self
            .calculation_snapshots
            .get(&document_id)
            .filter(|(version, _)| *version == document.calculation_revision)
            .map(|(_, snapshot)| Arc::clone(snapshot));
        let (font_definitions, text_options) =
            ctx.fonts(|fonts| (fonts.definitions().clone(), *fonts.options()));
        Some(crate::file_worker::AutoFitRequest {
            structure: auto_fit_structure_request(document, &targets),
            targets,
            display_mode: document_cell_display_mode(document.display_mode),
            language: self.language,
            show_invisibles: self.show_invisibles,
            calculation,
            font_definitions,
            text_options,
            pixels_per_point: ctx.pixels_per_point(),
            font_layout_generation: self.font_layout_generation,
            minimum_column_width_pixels: 32.0,
            maximum_column_width_pixels: (crate::document::MAX_COLUMN_WIDTH_UNITS * character_width
                + crate::document::COLUMN_TEXT_PADDING_PIXELS)
                as f32,
            character_width,
            row_limit,
        })
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
            // Once the calculation is done the fit starts, with its results when there are
            // any; a document without formulas has none, and its fit must not be dropped.
            if !calculation_is_pending {
                ready.push((*document_id, deferred.table_id, deferred.columns.clone()));
            }
        }
        for document_id in discard {
            self.deferred_auto_fits.remove(&document_id);
        }
        for (document_id, table_id, columns) in ready {
            self.deferred_auto_fits.remove(&document_id);
            self.request_columns_auto_fit(ctx, document_id, table_id, columns);
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
        widths: crate::file_worker::TableColumnWidths,
    ) {
        let Some(pending) = self.pending_auto_fits.get(&document_id) else {
            return;
        };
        // A user's fit measures one table.
        let widths = widths
            .into_iter()
            .find(|(table_id, _)| *table_id == pending.table_id)
            .map(|(_, widths)| widths)
            .unwrap_or_default();
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
