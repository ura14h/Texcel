use super::*;

// Format a typed cell value for CSV or Markdown output.
pub(super) fn export_text_value(
    request: &ExportRequest,
    formula_index: &FormulaResultIndex<'_>,
    table: &ExportTable,
    address: CellAddress,
    cell: &Cell,
    entries: &mut Vec<ConversionEntry>,
) -> Result<String, crate::i18n::Message> {
    if let Some(formula) = cell_formula_source(table, address, cell) {
        let formula = if matches!(request.source, Some(ExportSource::Markdown(_)))
            && matches!(&cell.input, CellInput::Text(source) if source.starts_with('='))
        {
            crate::calc::decode_markdown_formula_source(&formula)
        } else {
            formula
        };
        return match request.options.formulas {
            FormulaOutput::Formula => {
                if request.options.format.is_delimited() {
                    add_conversion_with_message(
                        entries,
                        table,
                        address,
                        "式を文字列の式として出力します。CSVを開くアプリが評価する可能性があります",
                        crate::i18n::Message::new("report-reason-formula-csv-text"),
                    );
                }
                Ok(formula)
            }
            FormulaOutput::LatestResult => formula_index
                .value(table.id, address)
                .map(|value| {
                    add_conversion_with_message(
                        entries,
                        table,
                        address,
                        "数式を同じ文書版の最新計算結果に置き換えました",
                        crate::i18n::Message::new("report-reason-formula-latest-value"),
                    );
                    calc_value_to_string(value)
                })
                .ok_or_else(|| {
                    crate::i18n::Message::new("status-export-latest-result-missing")
                        .with_text("table", &table.name)
                        .with_text("cell", cell_address(address))
                }),
        };
    }
    let source_text = cell.input.source_text();
    if let Some(ExportSource::Html(_)) = &request.source {
        let source_cell = match &request.source {
            Some(ExportSource::Html(source)) => source
                .tables
                .get(table.order)
                .and_then(|source_table| source_table.rows.get(address.row))
                .and_then(|source_row| source_row.get(address.column)),
            _ => None,
        };
        let source_display_text = crate::document::literal_text_value(&source_text);
        let display_data = source_cell
            .filter(|source_cell| source_cell.display_value == source_display_text)
            .map(|source_cell| crate::html_adapter::HtmlCellDisplayData {
                display: source_cell.display_value.clone(),
                rich_text: source_cell.rich_text.clone(),
                hyperlink: source_cell.hyperlink.clone(),
                has_partial_link: source_cell.has_partial_link,
                has_unsupported_markup: source_cell.has_unsupported_markup,
            })
            .or_else(|| {
                (request.options.format == ExportFormat::Markdown
                    && table.rich_text(address).is_some())
                .then(|| crate::html_adapter::HtmlCellDisplayData {
                    display: source_display_text.to_owned(),
                    rich_text: table.rich_text(address).cloned(),
                    hyperlink: table.hyperlink(address).cloned(),
                    has_partial_link: source_cell.is_some_and(|cell| cell.has_partial_link),
                    has_unsupported_markup: source_cell
                        .is_some_and(|cell| cell.has_unsupported_markup),
                })
            })
            .or_else(|| crate::html_adapter::HtmlDocument::display_fragment(&source_text))
            .ok_or_else(|| crate::i18n::Message::new("status-html-cell-display-unavailable"))?;
        let display = display_data.display.clone();
        let source_markup_present =
            source_cell.is_some_and(|source_cell| source_cell.value != source_cell.display_value);
        let markup_loss = (display != cell.input.source_text() || source_markup_present)
            && (request.options.format.is_delimited() || display_data.has_unsupported_markup);
        if markup_loss {
            add_conversion_with_message(
                entries,
                table,
                address,
                "HTMLのセル内マークアップを実行せず表示文字列へ変換しました",
                crate::i18n::Message::new("report-reason-html-markup-text"),
            );
        }
        if display_data.has_partial_link
            && display_data.rich_text.is_none()
            && request.options.format == ExportFormat::Markdown
        {
            add_conversion_with_message(
                entries,
                table,
                address,
                "リンク範囲の一部は出力形式で表せないため省略しました",
                crate::i18n::Message::new("report-reason-html-partial-link-dropped"),
            );
        }
        return Ok(display);
    }
    Ok(source_text)
}
