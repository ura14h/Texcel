use super::*;

// Build the final export content and its conversion report.
pub(super) fn build_artifact(
    context: &ExportBuildContext<'_, '_>,
    tables: &[ExportTable],
    entries: &mut Vec<ConversionEntry>,
) -> Result<Vec<u8>, crate::i18n::Message> {
    let request = context.request;
    let formula_index = context.formula_index;
    let markdown_context = context.markdown_context;
    let cancelled = context.cancelled;
    check_cancelled(cancelled)?;
    // Patch changed Markdown cells while retaining the rest of the source file.
    // Replace only formula cells while retaining the rest of a Markdown source file.
    if request.options.format == ExportFormat::Markdown
        && request.options.scope == ExportScope::AllTables
        && request.options.formulas == FormulaOutput::Formula
        && request.options.modifiers == ModifierOutput::Convert
        && request.options.markdown_header == MarkdownHeader::UseFirstRow
        && let Some(ExportSource::Markdown(source)) = &request.source
    {
        let changes = request
            .changes
            .iter()
            .filter_map(|(table_id, address, cell)| {
                let table = request
                    .tables
                    .iter()
                    .position(|table| table.id == *table_id)?;
                Some(crate::markdown_adapter::MarkdownCellChange {
                    table,
                    row: address.row,
                    column: address.column,
                    value: cell
                        .as_ref()
                        .map_or_else(String::new, |cell| cell.input.source_text()),
                })
            })
            .collect::<Vec<_>>();
        let updated = source
            .apply_changes(&changes)
            .map_err(crate::i18n::markdown_error_message)?;
        return Ok(updated.source_bytes().to_vec());
    }
    if request.options.format == ExportFormat::Markdown
        && request.options.scope == ExportScope::AllTables
        && request.options.formulas == FormulaOutput::LatestResult
        && request.options.modifiers == ModifierOutput::Convert
        && request.options.markdown_header == MarkdownHeader::UseFirstRow
        && let Some(ExportSource::Markdown(source)) = &request.source
    {
        let mut changes = request
            .changes
            .iter()
            .filter_map(|(table_id, address, cell)| {
                let table = request
                    .tables
                    .iter()
                    .position(|table| table.id == *table_id)?;
                Some(crate::markdown_adapter::MarkdownCellChange {
                    table,
                    row: address.row,
                    column: address.column,
                    value: cell
                        .as_ref()
                        .map_or_else(String::new, |cell| cell.input.source_text()),
                })
            })
            .collect::<Vec<_>>();
        for table in tables {
            for (row, cells) in table.rows.iter().enumerate() {
                check_cancelled(cancelled)?;
                for (column, cell) in cells.iter().enumerate() {
                    check_cancelled(cancelled)?;
                    let address = CellAddress { row, column };
                    if cell_formula_source(table, address, cell).is_none() {
                        continue;
                    }
                    let result =
                        export_text_value(request, formula_index, table, address, cell, entries)?;
                    changes.push(crate::markdown_adapter::MarkdownCellChange {
                        table: table.order,
                        row,
                        column,
                        value: formula_result_markdown_source(&result),
                    });
                }
            }
        }
        let updated = source
            .apply_changes(&changes)
            .map_err(crate::i18n::markdown_error_message)?;
        return Ok(updated.source_bytes().to_vec());
    }
    // Patch the original CSV bytes only when the chosen format keeps its delimiter.
    if let Some(ExportSource::Csv(source)) = &request.source
        && request.options.format.keeps_csv_delimiter(source.delimiter)
        && tables.len() == 1
    {
        let table = &tables[0];
        let mut changes_by_address = BTreeMap::<(usize, usize), Option<String>>::new();
        for (_, address, cell) in request
            .changes
            .iter()
            .filter(|(table_id, _, _)| *table_id == table.id)
        {
            let value = match cell {
                None => None,
                Some(cell) => {
                    let raw =
                        export_text_value(request, formula_index, table, *address, cell, entries)?;
                    let (plain, _) = plain_output_text(
                        request,
                        markdown_context,
                        &raw,
                        cell,
                        entries,
                        table,
                        *address,
                    );
                    if !matches!(cell.input, CellInput::Text(_) | CellInput::Empty) {
                        add_conversion_with_message(
                            entries,
                            table,
                            *address,
                            "型付きの値をCSV文字列に変換しました",
                            crate::i18n::Message::new("report-reason-csv-typed-value"),
                        );
                    }
                    Some(plain)
                }
            };
            changes_by_address.insert((address.row, address.column), value);
        }
        if request.options.formulas != FormulaOutput::Formula {
            for (row, values) in table.rows.iter().enumerate() {
                check_cancelled(cancelled)?;
                for (column, cell) in values.iter().enumerate() {
                    check_cancelled(cancelled)?;
                    let address = CellAddress { row, column };
                    if cell_formula_source(table, address, cell).is_none() {
                        continue;
                    }
                    let value =
                        export_text_value(request, formula_index, table, address, cell, entries)?;
                    changes_by_address.insert(
                        (row, column),
                        Some(crate::document::encode_literal_text(&value)),
                    );
                }
            }
        }
        let changes = changes_by_address
            .into_iter()
            .map(|((row, column), value)| CsvCellChange { row, column, value })
            .collect::<Vec<_>>();
        let updated = source
            .apply_changes(&changes)
            .map_err(crate::i18n::csv_error_message)?;
        return Ok(updated.source_bytes().to_vec());
    }
    match request.options.format {
        ExportFormat::Csv | ExportFormat::Tsv => export_csv(context, tables, entries),
        ExportFormat::Markdown => export_markdown(context, tables, entries),
    }
}
