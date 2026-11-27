use super::*;

// Write selected table values as CSV while preserving supported source details.
pub(super) fn export_csv(
    context: &ExportBuildContext<'_, '_>,
    tables: &[ExportTable],
    entries: &mut Vec<ConversionEntry>,
) -> Result<Vec<u8>, crate::i18n::Message> {
    let request = context.request;
    let formula_index = context.formula_index;
    let markdown_context = context.markdown_context;
    let cancelled = context.cancelled;
    let delimiter = if request.options.format == ExportFormat::Tsv {
        b'\t'
    } else {
        b','
    };
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .delimiter(delimiter)
        .terminator(csv::Terminator::Any(b'\n'))
        .from_writer(Vec::new());
    for table in tables {
        check_cancelled(cancelled)?;
        for (row_index, row) in table.rows.iter().enumerate() {
            check_cancelled(cancelled)?;
            let mut values = Vec::with_capacity(row.len());
            for (column, cell) in row.iter().enumerate() {
                check_cancelled(cancelled)?;
                let address = CellAddress {
                    row: row_index,
                    column,
                };
                let value =
                    export_text_value(request, formula_index, table, address, cell, entries)?;
                let (visible, markdown_styled) = plain_output_text(
                    request,
                    markdown_context,
                    &value,
                    cell,
                    entries,
                    table,
                    address,
                );
                let _ = markdown_styled;
                let visible = if cell_formula_source(table, address, cell).is_some()
                    && request.options.formulas != FormulaOutput::Formula
                {
                    crate::document::encode_literal_text(&visible)
                } else if cell_formula_source(table, address, cell).is_some() {
                    visible
                } else {
                    let value = crate::document::literal_text_value(&visible);
                    crate::document::encode_literal_text(value)
                };
                if table.rich_text(address).is_some_and(|runs| {
                    runs.iter().any(|run| {
                        run.bold
                            || run.italic
                            || run.strike
                            || run.font_size.is_some()
                            || run.font_color.is_some()
                    })
                }) {
                    add_conversion_with_message(
                        entries,
                        table,
                        address,
                        "文字修飾をCSVで表せないため表示文字列に変換しました",
                        crate::i18n::Message::new("report-reason-csv-rich-text"),
                    );
                }
                if matches!(request.source, Some(ExportSource::Xlsx(_)))
                    && table.style(address) != crate::document::CellStyle::default()
                {
                    add_omission_with_message(
                        entries,
                        table,
                        address,
                        "XLSXセル書式はCSVでは保持できず表示文字列だけを出力しました",
                        crate::i18n::Message::new("report-reason-xlsx-style-csv"),
                    );
                }
                values.push(visible);
            }
            writer.write_record(values).map_err(|error| {
                crate::i18n::Message::new("status-export-csv-write-failed")
                    .with_text("detail", error)
            })?;
        }
    }
    writer.into_inner().map_err(|error| {
        crate::i18n::Message::new("status-export-csv-finish-failed")
            .with_text("detail", error.into_error())
    })
}

// Write selected table values and inline styles as Markdown.
pub(super) fn export_markdown(
    context: &ExportBuildContext<'_, '_>,
    tables: &[ExportTable],
    entries: &mut Vec<ConversionEntry>,
) -> Result<Vec<u8>, crate::i18n::Message> {
    let request = context.request;
    let formula_index = context.formula_index;
    let markdown_context = context.markdown_context;
    let cancelled = context.cancelled;
    let mut output = String::new();
    for (table_index, table) in tables.iter().enumerate() {
        check_cancelled(cancelled)?;
        if table_index > 0 {
            output.push_str("\n\n");
        }
        let width = table.rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
        let add_header = request.options.markdown_header == MarkdownHeader::AddBlankHeader;
        let rows = if add_header {
            let mut rows = vec![vec![Cell::text(""); width]];
            rows.extend(table.rows.clone());
            rows
        } else {
            table.rows.clone()
        };
        for (row_index, row) in rows.iter().enumerate() {
            check_cancelled(cancelled)?;
            output.push('|');
            for column in 0..width {
                check_cancelled(cancelled)?;
                let cell = row.get(column).cloned().unwrap_or_else(|| Cell::text(""));
                let original_row = row_index.saturating_sub(usize::from(add_header));
                let address = CellAddress {
                    row: original_row,
                    column,
                };
                let value = if add_header && row_index == 0 {
                    String::new()
                } else {
                    export_text_value(request, formula_index, table, address, &cell, entries)?
                };
                let formula_source_output = request.options.formulas == FormulaOutput::Formula
                    && cell_formula_source(table, address, &cell).is_some();
                let formula_result_output = request.options.formulas != FormulaOutput::Formula
                    && cell_formula_source(table, address, &cell).is_some();
                let escaped = if add_header && row_index == 0 {
                    String::new()
                } else if formula_source_output {
                    report_formula_markdown_losses(context, table, address, entries);
                    escape_table_cell(&value)
                } else if formula_result_output {
                    escape_table_cell(&formula_result_markdown_source(&value))
                } else if matches!(request.source, Some(ExportSource::Markdown(_)))
                    && table.rich_text(address).is_none()
                {
                    if request.options.modifiers == ModifierOutput::TextOnly {
                        let (plain, changed) = plain_output_text(
                            request,
                            markdown_context,
                            &value,
                            &cell,
                            entries,
                            table,
                            address,
                        );
                        if changed {
                            add_conversion_with_message(
                                entries,
                                table,
                                address,
                                "Markdown修飾を表示文字列に変換しました",
                                crate::i18n::Message::new("report-reason-markdown-modifier-text"),
                            );
                        }
                        escape_table_cell(&escape_markdown_text(&plain))
                    } else if markdown_context
                        .and_then(|context| context.cells.get(&(table.id, address)))
                        .is_some_and(|content| content.has_reference_link)
                    {
                        add_omission_with_message(
                            entries,
                            table,
                            address,
                            "表外の参照リンク定義を出力へ引き継げないため、リンク先を表示文字列へ変換しました",
                            crate::i18n::Message::new("report-reason-reference-link-text"),
                        );
                        let content = markdown_context
                            .and_then(|context| context.cells.get(&(table.id, address)))
                            .expect("reference-link content was checked");
                        let (markup, omitted, retry_limit_reached) =
                            markdown_runs_as_markup_checked(&content.runs);
                        if omitted {
                            report_markdown_run_omission(
                                entries,
                                table,
                                address,
                                retry_limit_reached,
                            );
                        }
                        escape_table_cell(&markup)
                    } else {
                        escape_table_cell(&value)
                    }
                } else if table.rich_text(address).is_some()
                    || matches!(request.source, Some(ExportSource::Xlsx(_)))
                {
                    markdown_markup(
                        context,
                        markdown_context,
                        &cell,
                        &value,
                        request.options.modifiers,
                        entries,
                        (table, address),
                    )
                } else {
                    escape_table_cell(&escape_markdown_text(&value))
                };
                output.push(' ');
                output.push_str(&escaped);
                output.push_str(" |");
            }
            output.push('\n');
            if row_index == 0 {
                output.push('|');
                for column in 0..width {
                    output.push(' ');
                    output.push_str(markdown_alignment_separator(
                        markdown_column_alignment(request, markdown_context, table, column)
                            .as_ref(),
                    ));
                    output.push_str(" |");
                }
                output.push('\n');
            }
        }
    }
    Ok(output.into_bytes())
}

// Uses source alignment when present and otherwise applies export defaults.
fn markdown_column_alignment(
    request: &ExportRequest,
    markdown_context: Option<&MarkdownExportContext>,
    table: &ExportTable,
    column: usize,
) -> Option<Alignment> {
    if let Some(alignment) = markdown_context
        .and_then(|context| context.alignments.get(&table.id))
        .and_then(|alignments| alignments.get(column))
    {
        return Some(*alignment);
    }
    if !matches!(
        request.source,
        Some(ExportSource::Xlsx(_) | ExportSource::Html(_))
    ) {
        return None;
    }
    match imported_markdown_alignment(table, column) {
        MarkdownAlignmentResolution::Uniform(alignment) => alignment,
        MarkdownAlignmentResolution::Mixed => None,
    }
}
