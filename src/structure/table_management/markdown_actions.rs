use super::*;

/// Promotes CSV and limits alignment to the columns in the generated separator row.
pub(super) fn promote_csv_and_apply_operation(
    csv: &crate::csv_adapter::CsvDocument,
    request: &StructureRequest,
    document: &Document,
    index: usize,
    operation: &MarkdownTableOperation,
    cancelled: &AtomicBool,
) -> Result<
    (
        crate::markdown_adapter::MarkdownDocument,
        Arc<crate::conversion_report::ExportReport>,
    ),
    Message,
> {
    let (markdown, report) = csv_to_markdown(csv, request, document, cancelled)?;
    let updated = match operation {
        MarkdownTableOperation::SetColumnAlignment { columns, alignment } => {
            let width = markdown.parser_width(index).unwrap_or(0);
            if width == 0 {
                return Err(Message::new("status-markdown-alignment-no-columns"));
            }
            markdown
                .set_column_alignment_ranges(index, columns, *alignment)
                .map_err(crate::i18n::markdown_error_message)?
        }
    };
    Ok((updated, report))
}

/// Converts CSV using the same escaping and header rules as the normal export path.
pub(super) fn csv_to_markdown(
    csv: &crate::csv_adapter::CsvDocument,
    request: &StructureRequest,
    document: &Document,
    cancelled: &AtomicBool,
) -> Result<
    (
        crate::markdown_adapter::MarkdownDocument,
        Arc<crate::conversion_report::ExportReport>,
    ),
    Message,
> {
    let mut cell_changes = request
        .text_overlay_cells
        .iter()
        .map(|(table_id, address, cell)| ((*table_id, *address), Some(cell.clone())))
        .collect::<BTreeMap<_, _>>();
    for (table_id, address, cell) in &request.changes {
        cell_changes.insert((*table_id, *address), cell.clone());
    }
    let export = crate::export::ExportRequest {
        document_id: request.document_id,
        revision: request.revision,
        options: crate::export::ExportOptions {
            format: crate::export::ExportFormat::Markdown,
            scope: crate::export::ExportScope::AllTables,
            formulas: crate::export::FormulaOutput::Formula,
            modifiers: crate::export::ModifierOutput::Convert,
            markdown_header: crate::export::MarkdownHeader::UseFirstRow,
        },
        destination: std::path::PathBuf::from("csv-promotion.md"),
        source_path: None,
        source: Some(crate::export::ExportSource::Csv(Arc::new(csv.clone()))),
        source_report: None,
        tables: document
            .tables
            .iter()
            .map(|table| crate::export::ExportTableMeta {
                id: table.id,
                name: table.name.clone(),
                rows: table.row_count(),
                columns: table.column_count(),
            })
            .collect(),
        changes: cell_changes
            .into_iter()
            .map(|((table_id, address), cell)| (table_id, address, cell))
            .collect(),
        format_changes: Vec::new(),
        formula_results: None,
    };
    let prepared = crate::export::prepare_export_cancellable(&export, None, cancelled)?;
    let artifact = prepared
        .artifacts
        .first()
        .ok_or_else(|| Message::new("status-csv-promotion-output-missing"))?;
    let markdown = crate::markdown_adapter::MarkdownDocument::parse(artifact.bytes.clone())
        .map_err(crate::i18n::markdown_error_message)?;
    Ok((markdown, Arc::clone(&prepared.report)))
}
