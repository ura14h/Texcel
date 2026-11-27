//!  Worker-side operations for paste.

use super::*;

pub(crate) struct ExpandedClipboardPaste {
    pub(crate) cells: Vec<(CellAddress, Option<crate::document::Cell>)>,
    /// Pasted formatting for the kept cells. Editing cells never hold it, so Markdown output reads it here.
    pub(crate) formats: BTreeMap<CellAddress, crate::document::CellFormat>,
    pub(crate) raw_markdown_sources: BTreeSet<CellAddress>,
    pub(crate) markdown_feature_sources: BTreeSet<CellAddress>,
}

/// Parses, validates, and expands clipboard data into cells for a Markdown or CSV target.
pub(crate) fn expand_clipboard_paste(
    paste: &ClipboardPasteRequest,
    markdown_target: bool,
    csv_target: bool,
) -> Result<ExpandedClipboardPaste, Message> {
    let mut parsed =
        parse_clipboard(&paste.text, paste.html.as_deref()).map_err(|error| error.0)?;
    let internal_source = parsed.kind == crate::clipboard::ClipboardInputKind::Internal;
    if parsed.ranges.len() != 1 {
        return Err(Message::new("status-paste-multiple-ranges"));
    }
    // Copied formulas move their references only when Texcel data says where they came from.
    let copied_origin = parsed.origins.first().copied().filter(|_| internal_source);
    let mut formula_mover = None;
    let mut matrix = parsed.ranges.pop().unwrap_or_default();
    if paste.options.transpose {
        matrix = transpose(matrix);
    }
    if matrix.is_empty() || matrix[0].is_empty() {
        return Err(Message::new("status-paste-empty-range"));
    }
    let (start, end) = paste.selection.normalized();
    if end.row >= paste.rows || end.column >= paste.columns {
        return Err(Message::new("status-paste-target-outside-table"));
    }
    let whole_axis_selection = (start.row == 0 && end.row == paste.rows - 1)
        || (start.column == 0 && end.column == paste.columns - 1);
    let origin = if whole_axis_selection {
        paste.selection.anchor
    } else {
        start
    };
    let selection_width = end.column - start.column + 1;
    let selected_row_count = end.row.saturating_sub(start.row).saturating_add(1);
    let one_cell_source = matrix.len() == 1 && matrix[0].len() == 1;
    let selection_height = end.row.saturating_sub(start.row).saturating_add(1);
    let physical_selection_area = selection_height.saturating_mul(selection_width);
    let use_selected_rectangle = physical_selection_area > 1 && !whole_axis_selection;
    if use_selected_rectangle
        && !one_cell_source
        && (selected_row_count != matrix.len() || selection_width != matrix[0].len())
    {
        return Err(Message::new("status-paste-selection-size-mismatch"));
    }
    let target_row_count = if use_selected_rectangle {
        selected_row_count
    } else {
        matrix.len()
    };
    let target_column_count = if use_selected_rectangle {
        selection_width
    } else {
        matrix[0].len()
    };
    let area = target_row_count.saturating_mul(target_column_count);
    if area == 0 || area > crate::clipboard::MAX_CLIPBOARD_CELLS {
        return Err(Message::new("status-paste-area-limit")
            .with_number("limit", crate::clipboard::MAX_CLIPBOARD_CELLS as i64));
    }

    if !use_selected_rectangle {
        let last_column = origin.column.saturating_add(matrix[0].len() - 1);
        if last_column >= paste.columns {
            return Err(Message::new("status-paste-column-outside-table"));
        }
        let last_row = origin.row.saturating_add(matrix.len() - 1);
        if last_row >= paste.rows {
            return Err(Message::new("status-paste-row-outside-table"));
        }
    }

    let target_rows = if use_selected_rectangle {
        (start.row..=end.row).collect::<Vec<_>>()
    } else {
        let last_row = origin.row.saturating_add(matrix.len() - 1);
        (origin.row..=last_row).collect::<Vec<_>>()
    };
    let target_columns = if use_selected_rectangle {
        (start.column..=end.column).collect::<Vec<_>>()
    } else {
        (origin.column..origin.column + target_column_count).collect::<Vec<_>>()
    };
    if !one_cell_source
        && (target_rows.len() != matrix.len() || target_columns.len() != matrix[0].len())
    {
        return Err(Message::new("status-paste-selection-size-mismatch"));
    }
    let mut changes = Vec::with_capacity(area);
    let mut formats = BTreeMap::new();
    let mut raw_markdown_sources = BTreeSet::new();
    let mut markdown_feature_sources = BTreeSet::new();
    for (row_index, row) in target_rows.iter().copied().enumerate() {
        for (column_index, column) in target_columns.iter().copied().enumerate() {
            let source = if one_cell_source {
                &matrix[0][0]
            } else {
                &matrix[row_index][column_index]
            };
            let mut cell = convert_paste_cell(source, &paste.options, internal_source)?;
            // Raw Markdown is kept only when the cell keeps its copied source text.
            let preserve_raw_markdown = !paste.options.strip_formatting
                && cell.input == source.input
                && internal_source
                && source.raw_markdown_source
                && (markdown_target || (csv_target && source.markdown_has_features));
            if paste.options.skip_blank && matches!(source.input, crate::document::CellInput::Empty)
            {
                continue;
            }
            let address = CellAddress { row, column };
            let mut format = if paste.options.strip_formatting {
                crate::document::CellFormat::default()
            } else {
                source.format()
            };
            if csv_target
                && !paste.options.strip_formatting
                && cell.input == source.input
                && internal_source
                && source.raw_markdown_source
                && !source.markdown_has_features
                && let Some(plain_text) = source.markdown_plain_text.as_deref()
            {
                cell.input = crate::document::CellInput::Text(
                    crate::document::encode_literal_text(plain_text),
                );
                format = crate::document::CellFormat::default();
            }
            if let Some(copied_origin) = copied_origin {
                let (source_row, source_column) = match (one_cell_source, paste.options.transpose) {
                    (true, _) => (0, 0),
                    (false, false) => (row_index, column_index),
                    (false, true) => (column_index, row_index),
                };
                let source_address = CellAddress {
                    row: copied_origin.row + source_row,
                    column: copied_origin.column + source_column,
                };
                move_pasted_formula(
                    &mut cell,
                    &mut formula_mover,
                    source_address,
                    address,
                    paste.cut_source,
                )?;
            }
            let keep = cell.input != crate::document::CellInput::Empty || !format.is_empty();
            if keep && !format.is_empty() {
                formats.insert(address, format);
            }
            let after = keep.then_some(cell);
            if preserve_raw_markdown {
                raw_markdown_sources.insert(address);
            }
            if preserve_raw_markdown && source.markdown_has_features {
                markdown_feature_sources.insert(address);
            }
            changes.push((CellAddress { row, column }, after));
        }
    }
    if changes.is_empty() {
        return Err(Message::new("status-paste-all-cells-skipped"));
    }
    Ok(ExpandedClipboardPaste {
        cells: changes,
        formats,
        raw_markdown_sources,
        markdown_feature_sources,
    })
}

/// Keeps only character formatting that a Markdown text table can represent.
pub(super) fn retain_markdown_paste_features(
    cells: &mut [(CellAddress, Option<crate::document::Cell>)],
    formats: &mut BTreeMap<CellAddress, crate::document::CellFormat>,
) {
    for format in formats.values_mut() {
        format.style = crate::document::CellStyle {
            bold: format.style.bold,
            italic: format.style.italic,
            strike: format.style.strike,
            ..crate::document::CellStyle::default()
        };
        if let Some(runs) = format.rich_text.as_mut() {
            for run in runs {
                run.font_name = None;
                run.font_size = None;
                run.font_color = None;
            }
        }
    }
    formats.retain(|_, format| !format.is_empty());
    for (address, cell) in cells.iter_mut() {
        if cell.as_ref().is_some_and(|cell| {
            cell.input == crate::document::CellInput::Empty && !formats.contains_key(address)
        }) {
            *cell = None;
        }
    }
}

/// Moves the references of one pasted formula from its copied cell to its new cell.
fn move_pasted_formula(
    cell: &mut crate::document::Cell,
    mover: &mut Option<crate::calc::PasteFormulaMover>,
    source: CellAddress,
    target: CellAddress,
    cut_area: Option<CellRange>,
) -> Result<(), Message> {
    let Some(formula) = crate::calc::formula_source(&cell.input, false) else {
        return Ok(());
    };
    if source == target {
        return Ok(());
    }
    let mover = match mover {
        Some(mover) => mover,
        None => mover.insert(crate::calc::PasteFormulaMover::new()?),
    };
    let moved = match cut_area {
        Some(area) => mover.cut(&formula, source, target, area)?,
        None => mover.copy(&formula, source, target)?,
    };
    cell.input = match cell.input {
        crate::document::CellInput::Formula(_) => crate::document::CellInput::Formula(moved),
        _ => crate::document::CellInput::Text(moved),
    };
    Ok(())
}

// Convert a clipboard cell to the requested paste mode. Formatting is returned separately.
pub(crate) fn convert_paste_cell(
    source: &crate::clipboard::ClipboardCell,
    options: &crate::clipboard::PasteOptions,
    internal_source: bool,
) -> Result<crate::document::Cell, Message> {
    let mut cell = source.clone().into_cell();
    let formula =
        source.formula_source || matches!(source.input, crate::document::CellInput::Formula(_));
    if formula {
        if options.values_only {
            cell.input = source
                .fresh_formula_value
                .clone()
                .ok_or_else(|| Message::new("status-paste-current-formula-result-missing"))?;
        }
        return Ok(cell);
    }
    let crate::document::CellInput::Text(value) = &source.input else {
        return Ok(cell);
    };
    if internal_source && source.raw_markdown_source {
        // Texcel Markdown source: dropping formatting pastes the shown text as a string.
        if options.strip_formatting {
            let plain_text = source
                .markdown_plain_text
                .as_deref()
                .unwrap_or_else(|| crate::document::literal_text_value(value));
            cell.input =
                crate::document::CellInput::Text(crate::document::encode_literal_text(plain_text));
        }
    } else if options.values_only {
        // Only a Texcel-owned payload proves that the leading apostrophe is a saved marker.
        let has_internal_marker =
            internal_source && crate::document::literal_text_value(value) != value;
        if !has_internal_marker {
            cell.input =
                crate::document::CellInput::Text(crate::document::encode_literal_text(value));
        }
    }
    Ok(cell)
}

/// Applies one table's paste while keeping owned Markdown source cells unchanged.
pub(crate) fn apply_bulk_cell_changes_for_table_with_raw_markdown_sources(
    source: &SourceDocument,
    table_id: TableId,
    cells: &[(CellAddress, Option<crate::document::Cell>)],
    formats: &BTreeMap<CellAddress, crate::document::CellFormat>,
    document: &Document,
    raw_markdown_sources: &BTreeSet<CellAddress>,
) -> Result<(SourceDocument, bool), Message> {
    match source {
        SourceDocument::Csv(source) => {
            if document.tables.len() != 1
                || document.tables.first().map(|table| table.id) != Some(table_id)
            {
                return Err(Message::new("status-paste-csv-table-invalid")
                    .with_number("table_id", table_id as i64));
            }
            let changes = cells
                .iter()
                .map(|(address, cell)| CsvCellChange {
                    row: address.row,
                    column: address.column,
                    value: cell.as_ref().map(|cell| {
                        let is_string = document
                            .tables
                            .iter()
                            .find(|table| table.id == table_id)
                            .is_some_and(|table| table.is_string_cell(*address));
                        cell.source_text_for_text_file(is_string)
                    }),
                })
                .collect::<Vec<_>>();
            source
                .apply_changes(&changes)
                .map(|source| (SourceDocument::Csv(Arc::new(source)), false))
                .map_err(crate::i18n::csv_error_message)
        }
        SourceDocument::Markdown(source) => {
            let table_index = source_table_index(source, table_id, document)?;
            let mut formatting_omitted = false;
            let changes = cells
                .iter()
                .map(|(address, cell)| {
                    let value = cell.as_ref().map_or_else(
                        || (String::new(), false),
                        |cell| {
                            if raw_markdown_sources.contains(address) {
                                let is_string = document
                                    .tables
                                    .iter()
                                    .find(|source_table| source_table.id == table_id)
                                    .is_some_and(|source_table| {
                                        source_table.is_string_cell(*address)
                                    });
                                (cell.source_text_for_text_file(is_string), false)
                            } else {
                                let is_string = document
                                    .tables
                                    .iter()
                                    .find(|source_table| source_table.id == table_id)
                                    .is_some_and(|source_table| {
                                        source_table.is_string_cell(*address)
                                    });
                                crate::export::markdown_cell_source_for_paste(
                                    cell,
                                    formats.get(address),
                                    is_string,
                                )
                            }
                        },
                    );
                    formatting_omitted |= value.1;
                    MarkdownCellChange {
                        table: table_index,
                        row: address.row,
                        column: address.column,
                        value: value.0,
                    }
                })
                .collect::<Vec<_>>();
            source
                .apply_changes(&changes)
                .map(|source| {
                    (
                        SourceDocument::Markdown(Arc::new(source)),
                        formatting_omitted,
                    )
                })
                .map_err(crate::i18n::markdown_error_message)
        }
    }
}
