use super::*;

use crate::source::SourceDocument;
use std::cell::RefCell;

thread_local! {
    static AUTOFIT_FONTS: RefCell<Option<(u64, u32, eframe::egui::epaint::text::Fonts)>> = const { RefCell::new(None) };
}

/// Queues a sparse axis measurement with the normal file worker.
impl FileWorker {
    // Sends the measurement request to the worker thread.
    pub fn auto_fit(&self, token: JobToken, request: AutoFitRequest) -> Result<(), SubmitError> {
        self.submit(
            token,
            WorkerMessage::AutoFit {
                token,
                request: Box::new(request),
                cancelled: Arc::new(AtomicBool::new(false)),
            },
        )
    }
}

/// Measures the requested columns with the same grid layout jobs used for painting.
pub(crate) fn run_auto_fit(
    token: JobToken,
    request: AutoFitRequest,
    cancelled: Arc<AtomicBool>,
) -> FileResult {
    let path = PathBuf::new();
    let result = if cancelled.load(Ordering::Acquire) {
        Err(Message::new("status-autofit-cancelled"))
    } else {
        measure_auto_fit(&request, &cancelled)
    };
    let kind = if cancelled.load(Ordering::Acquire) {
        FileResultKind::Cancelled
    } else {
        match result {
            Ok(widths) => FileResultKind::AutoFitPrepared { widths },
            Err(error) => FileResultKind::Failed(error),
        }
    };
    FileResult {
        token,
        operation: FileOperation::AutoFit,
        path,
        kind,
    }
}

/// Rebuilds the current sparse table state on the worker before measuring it.
fn measure_auto_fit(
    request: &AutoFitRequest,
    cancelled: &AtomicBool,
) -> Result<Vec<(usize, f64)>, Message> {
    let mut document = materialize_autofit_document(request, cancelled)?;
    let table_id = request.structure.view_table_id;
    let source = document.source.clone();
    let table = document
        .tables
        .iter_mut()
        .find(|table| table.id == table_id)
        .ok_or_else(|| Message::new("status-table-id-unavailable"))?;
    let markdown = match source.as_deref() {
        Some(SourceDocument::Markdown(source)) => Some(source.as_ref()),
        _ => None,
    };
    let markdown_index = markdown.and_then(|source| {
        request
            .structure
            .tables
            .iter()
            .position(|table| table.id == table_id)
            .filter(|index| *index < source.tables.len())
    });
    let result = AUTOFIT_FONTS.with(|cache| {
        let mut cache = cache.borrow_mut();
        let key = (
            request.font_layout_generation,
            request.pixels_per_point.to_bits(),
        );
        if cache
            .as_ref()
            .is_none_or(|(generation, ppp, _)| (*generation, *ppp) != key)
        {
            *cache = Some((
                key.0,
                key.1,
                eframe::egui::epaint::text::Fonts::new(
                    request.text_options,
                    request.font_definitions.clone(),
                ),
            ));
        }
        let fonts = &mut cache.as_mut().expect("autofit fonts were initialized").2;
        let mut fonts = fonts.with_pixels_per_point(request.pixels_per_point);
        let mut widths = Vec::with_capacity(request.columns.len());
        for &column in &request.columns {
            let mut measured = request.minimum_column_width_pixels;
            let cells = table.stored_cells_in_column(column);
            for (address, cell) in cells {
                if cancelled.load(Ordering::Acquire) {
                    return Err(Message::new("status-autofit-cancelled"));
                }
                let calculation = request
                    .calculation
                    .as_ref()
                    .filter(|snapshot| snapshot.revision == request.structure.calculation_revision)
                    .and_then(|snapshot| snapshot.get(table_id, address))
                    .map(|(status, value)| crate::document::CalcResult {
                        revision: request.structure.calculation_revision,
                        status,
                        value: value.cloned(),
                    });
                let is_formula = crate::structure::is_formula_cell(table, address, cell);
                let style = crate::ui::CellTextStyle {
                    wrap_text: request.wrap_text,
                    alignment: None,
                };
                // Measure the unwrapped text width without a column limit.
                let width = f32::INFINITY;
                let text = if request.display_mode == crate::document::CellDisplayMode::Source {
                    cell.input.source_text()
                } else {
                    crate::ui::auto_fit_cell_view_text(cell, calculation.as_ref()).into_owned()
                };
                let (mut job, _) =
                    if let (Some(source), Some(index), crate::document::CellInput::Text(raw)) =
                        (markdown, markdown_index, &cell.input)
                        && !is_formula
                        && request.display_mode != crate::document::CellDisplayMode::Source
                    {
                        let display = source
                            .cell_display_for_source(index, address.row, address.column, raw)
                            .cloned()
                            .unwrap_or_else(|| MarkdownCellDisplay::parse_fragment(raw));
                        crate::ui::auto_fit_markdown_cell_layout_job(
                            &display,
                            crate::ui::CellTextFormat {
                                style: &style,
                                color: eframe::egui::Color32::BLACK,
                                width,
                                show_invisibles: request.show_invisibles,
                                scale: 1.0,
                            },
                            Some(source),
                            request.language,
                        )
                    } else {
                        crate::ui::auto_fit_cell_layout_job(
                            &text,
                            crate::ui::CellTextFormat {
                                style: &style,
                                color: eframe::egui::Color32::BLACK,
                                width,
                                show_invisibles: request.show_invisibles,
                                scale: 1.0,
                            },
                        )
                    };
                if request.display_mode == crate::document::CellDisplayMode::Source {
                    crate::ui::auto_fit_apply_grid_monospace(&mut job);
                }
                if text.contains('\n') {
                    job.wrap.max_rows = 256;
                }
                let size = fonts.layout_job(job).size();
                measured = measured.max(
                    (size.x + crate::document::COLUMN_TEXT_PADDING_PIXELS as f32)
                        .min(request.maximum_column_width_pixels),
                );
            }
            // Convert pixels to half-width monospace characters, as stored in the layout.
            let width = crate::document::column_width_units_for_pixels(
                f64::from(measured),
                request.character_width,
            );
            widths.push((column, width));
        }
        Ok(widths)
    });
    if cancelled.load(Ordering::Acquire) {
        Err(Message::new("status-autofit-cancelled"))
    } else {
        result
    }
}

/// Applies current source and memory edits without scanning the document on the UI thread.
fn materialize_autofit_document(
    autofit: &AutoFitRequest,
    cancelled: &AtomicBool,
) -> Result<crate::document::Document, Message> {
    let request = &autofit.structure;
    crate::structure::check_cancelled(cancelled)?;
    // Every editable document has a Markdown or CSV source to rebuild from.
    let source = request
        .source
        .as_deref()
        .ok_or_else(|| Message::new("status-structure-source-missing"))?;
    let mut document = {
        let ids = request
            .tables
            .iter()
            .map(|table| table.id)
            .collect::<Vec<_>>();
        let mut document = crate::structure::source_to_document_with_cancel(
            source,
            request.document_id,
            &ids,
            cancelled,
        )?;
        crate::structure::apply_table_metadata_with_cancel(
            &mut document,
            &request.tables,
            cancelled,
        )?;
        crate::structure::apply_requested_text_overlay_with_cancel(
            &mut document,
            request.text_overlay_revision,
            request.revision,
            &request.text_overlay_cells,
            &request.changes,
            true,
            cancelled,
        )?;
        crate::structure::apply_changed_cells_with_cancel(&mut document, request, cancelled)?;
        document
    };
    crate::structure::check_cancelled(cancelled)?;
    let table = document
        .tables
        .iter_mut()
        .find(|table| table.id == request.view_table_id)
        .ok_or_else(|| Message::new("status-table-id-unavailable"))?;
    for (column, value) in autofit.layout.columns() {
        crate::structure::check_cancelled(cancelled)?;
        table.layout.apply_value(
            crate::document::LayoutTarget::Column(column),
            crate::document::LayoutValue::Column(Some(value)),
        );
    }
    Ok(document)
}
