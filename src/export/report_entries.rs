use super::*;

use crate::i18n::Message;

/// Record conversion details for every selected table and formula.
pub(super) fn collect_table_entries(
    request: &ExportRequest,
    document: &Document,
    selected: &[ExportTable],
    formula_index: &FormulaResultIndex<'_>,
    markdown_context: Option<&MarkdownExportContext>,
    entries: &mut Vec<ConversionEntry>,
    cancelled: Option<&AtomicBool>,
) -> Result<(), crate::i18n::Message> {
    for table in selected {
        check_cancelled(cancelled)?;
        entries.push(
            ConversionEntry::new(
                ConversionClass::Retained,
                Some(table.order + 1),
                Some(table.name.clone()),
                None,
                format!("{}形式で表の値を出力", request.options.format.label()),
            )
            .with_message(
                Message::new("report-reason-export-table-values")
                    .with_text("format", request.options.format.label()),
            ),
        );
        if request.options.format == ExportFormat::Markdown
            && !matches!(request.source, Some(ExportSource::Markdown(_)))
        {
            let reason = match request.options.markdown_header {
                MarkdownHeader::UseFirstRow => "先頭行をMarkdownのヘッダーとして使用しました",
                MarkdownHeader::AddBlankHeader => "空のMarkdownヘッダー行を追加しました",
            };
            let reason_message = Message::new(match request.options.markdown_header {
                MarkdownHeader::UseFirstRow => "report-reason-markdown-header-first",
                MarkdownHeader::AddBlankHeader => "report-reason-markdown-header-blank",
            });
            let entry = ConversionEntry::new(
                ConversionClass::Converted,
                Some(table.order + 1),
                Some(table.name.clone()),
                Some(
                    if request.options.markdown_header == MarkdownHeader::UseFirstRow {
                        "1行目".to_owned()
                    } else {
                        "追加行".to_owned()
                    },
                ),
                reason,
            )
            .with_message(reason_message);
            entries.push(
                if request.options.markdown_header == MarkdownHeader::UseFirstRow {
                    entry.formatting_only()
                } else {
                    entry
                },
            );
        } else if request.options.format == ExportFormat::Markdown
            && request.options.markdown_header == MarkdownHeader::AddBlankHeader
        {
            entries.push(
                ConversionEntry::new(
                    ConversionClass::Converted,
                    Some(table.order + 1),
                    Some(table.name.clone()),
                    Some("追加行".to_owned()),
                    "空のMarkdownヘッダー行を追加しました",
                )
                .with_message(Message::new("report-reason-markdown-header-blank")),
            );
        }
        if request.options.format == ExportFormat::Markdown {
            add_import_alignment_entries(request, table, entries);
        }
        if request.options.format != ExportFormat::Markdown
            && matches!(request.source, Some(ExportSource::Markdown(_)))
            && markdown_context
                .and_then(|context| context.alignments.get(&table.id))
                .is_some_and(|alignments| {
                    alignments
                        .iter()
                        .any(|alignment| *alignment != Alignment::None)
                })
        {
            entries.push(
                ConversionEntry::new(
                    ConversionClass::Omitted,
                    Some(table.order + 1),
                    Some(table.name.clone()),
                    None,
                    "Markdown column alignment is not represented in this output format.",
                )
                .formatting_only()
                .with_message(Message::new("report-reason-markdown-alignment-omitted")),
            );
        }
        if let (Some(ExportSource::Xlsx(source)), ExportScope::Selected(selected_id)) =
            (&request.source, request.options.scope)
            && source
                .sheets
                .iter()
                .any(|sheet| sheet.table_id != selected_id)
        {
            let omitted = source
                .sheets
                .iter()
                .filter(|sheet| sheet.table_id != selected_id)
                .map(|sheet| sheet.name.clone())
                .collect::<Vec<_>>();
            entries.push(
                ConversionEntry::new(
                    ConversionClass::Omitted,
                    Some(table.order + 1),
                    Some(table.name.clone()),
                    None,
                    "Selected workbook sheets are not exported.",
                )
                .with_message(
                    Message::new("report-reason-unselected-sheets")
                        .with_text_list("sheets", omitted),
                ),
            );
        }
        if let Some(ExportSource::Xlsx(source)) = &request.source
            && let Some(sheet) = source
                .sheets
                .iter()
                .find(|sheet| sheet.table_id == table.id)
            && let Some(source_table) = document
                .tables
                .iter()
                .find(|candidate| candidate.id == table.id)
        {
            let merges = source_table
                .layout
                .merges()
                .map(|range| format!("{}:{}", cell_address(range.start), cell_address(range.end)))
                .collect::<Vec<_>>();
            if !merges.is_empty() {
                entries.push(
                    ConversionEntry::new(
                        ConversionClass::Converted,
                        Some(table.order + 1),
                        Some(table.name.clone()),
                        Some(merges.join(", ")),
                        "結合セルは左上の値を残し、ほかのセルを空にしました",
                    )
                    .with_message(Message::new("report-reason-merge-collapsed")),
                );
            }
            let covered_payload = source_table
                .stored_cells()
                .filter(|(address, cell)| {
                    source_table
                        .layout
                        .merge_at(*address)
                        .is_some_and(|merge| merge.start != *address)
                        && (cell.input != CellInput::Empty
                            || source_table.format(*address).is_some_and(|format| {
                                format.hyperlink.is_some() || format.rich_text.is_some()
                            })
                            || cell.imported_result.is_some()
                            || cell.calculation.is_some())
                })
                .map(|(address, _)| cell_address(address))
                .collect::<Vec<_>>();
            if !covered_payload.is_empty() {
                entries.push(
                    ConversionEntry::new(
                        ConversionClass::Omitted,
                        Some(table.order + 1),
                        Some(table.name.clone()),
                        Some(covered_payload.join(", ")),
                        "結合範囲の覆いセルに保存された内容を出力できません",
                    )
                    .with_message(Message::new("report-reason-merge-covered-content")),
                );
            }
            if sheet.cells.values().any(|cell| {
                cell.style.fill_color.is_some()
                    || cell.style.number_format.is_some()
                    || cell.style.wrap_text
                    || cell.style.font_size.is_some()
                    || cell.style.font_color.is_some()
                    || (request.options.format.is_delimited()
                        && (cell.style.bold || cell.style.italic || cell.style.strike))
            }) {
                entries.push(
                    ConversionEntry::new(
                        ConversionClass::Omitted,
                        Some(table.order + 1),
                        Some(table.name.clone()),
                        None,
                        "セルの背景・配置・数値書式などは出力形式で保持できません",
                    )
                    .formatting_only()
                    .with_message(Message::new("report-reason-cell-style-unsupported")),
                );
            }
            if source_table.layout.has_imported_row_layout()
                || source_table.layout.columns().next().is_some()
                || source_table.layout.frozen().rows > 0
                || source_table.layout.frozen().columns > 0
            {
                entries.push(
                    ConversionEntry::new(
                        ConversionClass::Omitted,
                        Some(table.order + 1),
                        Some(table.name.clone()),
                        None,
                        "行列寸法、非表示、固定表示は出力形式で保持できません",
                    )
                    .formatting_only()
                    .with_message(Message::new("report-reason-table-layout-unsupported")),
                );
            }
        }
        let source_table = document
            .tables
            .iter()
            .find(|candidate| candidate.id == table.id);
        if let Some(source_table) = source_table {
            for (address, cell) in source_table.stored_cells() {
                check_cancelled(cancelled)?;
                if cell_formula_source(table, address, cell).is_none() {
                    continue;
                }
                match request.options.formulas {
                    FormulaOutput::Formula => (),
                    FormulaOutput::LatestResult => {
                        if formula_index.value(table.id, address).is_none() {
                            entries.push(
                                ConversionEntry::new(
                                    ConversionClass::Unprocessable,
                                    Some(table.order + 1),
                                    Some(table.name.clone()),
                                    Some(cell_address(address)),
                                    "同じ文書版の最新計算結果がありません",
                                )
                                .with_message(Message::new("report-reason-formula-latest-missing")),
                            );
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Reports retained or mixed imported alignment as a presentation-only change.
fn add_import_alignment_entries(
    request: &ExportRequest,
    table: &ExportTable,
    entries: &mut Vec<ConversionEntry>,
) {
    let retained_key = match &request.source {
        Some(ExportSource::Xlsx(_)) => "report-reason-xlsx-markdown-alignment",
        Some(ExportSource::Html(_)) => "report-reason-html-markdown-alignment",
        _ => return,
    };
    let mut retained = false;
    let mut mixed = false;
    let width = table.rows.iter().map(Vec::len).max().unwrap_or(0);
    for column in 0..width {
        match imported_markdown_alignment(table, column) {
            MarkdownAlignmentResolution::Uniform(Some(_)) => retained = true,
            MarkdownAlignmentResolution::Uniform(None) => (),
            MarkdownAlignmentResolution::Mixed => mixed = true,
        }
    }
    if retained {
        entries.push(
            ConversionEntry::new(
                ConversionClass::Converted,
                Some(table.order + 1),
                Some(table.name.clone()),
                None,
                "Supported horizontal alignment was written into Markdown table columns.",
            )
            .formatting_only()
            .with_message(Message::new(retained_key)),
        );
    }
    if mixed {
        entries.push(
            ConversionEntry::new(
                ConversionClass::Omitted,
                Some(table.order + 1),
                Some(table.name.clone()),
                None,
                "Column alignment could not be unified from value-bearing cells.",
            )
            .formatting_only()
            .with_message(Message::new(
                "report-reason-markdown-column-alignment-mixed",
            )),
        );
    }
}
