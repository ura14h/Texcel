use super::*;

pub(super) struct SourceActionResult {
    pub(super) source_after: SourceDocument,
    pub(super) stable_ids_after: Vec<TableId>,
    pub(super) formula_updates: BTreeMap<(TableId, CellAddress), String>,
    pub(super) conversion_report: Option<Arc<crate::conversion_report::ExportReport>>,
}

// Apply the selected table action to the file-backed source representation.
pub(super) fn apply_source_table_action(
    request: &StructureRequest,
    document: &Document,
    source_before: &SourceDocument,
    action: &TableAction,
    stable_ids_before: &[TableId],
    source_index: Option<usize>,
    cancelled: &AtomicBool,
) -> Result<SourceActionResult, Message> {
    let mut stable_ids_after = stable_ids_before.to_vec();
    let formula_updates = BTreeMap::new();
    let mut conversion_report = None;
    let source_after = match (&source_before, action) {
        (SourceDocument::Markdown(markdown), TableAction::Add { new_table_id, .. }) => {
            ensure_new_table_id(stable_ids_before, *new_table_id)?;
            stable_ids_after.push(*new_table_id);
            SourceDocument::Markdown(Arc::new(
                markdown
                    .append_table()
                    .map_err(crate::i18n::markdown_error_message)?,
            ))
        }
        (
            SourceDocument::Markdown(markdown),
            TableAction::Duplicate {
                table_id: _,
                new_table_id,
                ..
            },
        ) => {
            ensure_new_table_id(stable_ids_before, *new_table_id)?;
            let index =
                source_index.ok_or_else(|| Message::new("status-table-copy-source-missing"))?;
            stable_ids_after.insert(index + 1, *new_table_id);
            SourceDocument::Markdown(Arc::new(
                markdown
                    .duplicate_table(index)
                    .map_err(crate::i18n::markdown_error_message)?,
            ))
        }
        (SourceDocument::Markdown(markdown), TableAction::Delete { .. }) => {
            let index =
                source_index.ok_or_else(|| Message::new("status-table-delete-target-missing"))?;
            stable_ids_after.remove(index);
            SourceDocument::Markdown(Arc::new(
                markdown
                    .delete_table(index)
                    .map_err(crate::i18n::markdown_error_message)?,
            ))
        }
        (SourceDocument::Markdown(markdown), TableAction::MarkdownFormat { operation, .. }) => {
            let index =
                source_index.ok_or_else(|| Message::new("status-markdown-table-target-missing"))?;
            let updated = match operation {
                MarkdownTableOperation::SetColumnAlignment { columns, alignment } => {
                    markdown.set_column_alignment_ranges(index, columns, *alignment)
                }
            }
            .map_err(crate::i18n::markdown_error_message)?;
            SourceDocument::Markdown(Arc::new(updated))
        }
        (SourceDocument::Markdown(_), TableAction::Rename { .. }) => {
            return Err(Message::new("status-markdown-table-name-order-source-only"));
        }
        (SourceDocument::Markdown(_), TableAction::PromoteCsv { .. }) => {
            return Err(Message::new("status-csv-promotion-source-required"));
        }
        (SourceDocument::Markdown(_), TableAction::PromoteCsvCell { .. }) => {
            return Err(Message::new("status-csv-promotion-source-required"));
        }
        (SourceDocument::Markdown(_), TableAction::PromoteCsvCells { .. }) => {
            return Err(Message::new("status-csv-promotion-source-required"));
        }
        (
            SourceDocument::Markdown(markdown),
            TableAction::MarkdownLinks {
                table_id,
                range,
                hyperlink,
            },
        ) => {
            let index =
                source_index.ok_or_else(|| Message::new("status-markdown-table-target-missing"))?;
            SourceDocument::Markdown(Arc::new(apply_markdown_links(
                markdown,
                index,
                document,
                *table_id,
                *range,
                hyperlink.as_ref(),
            )?))
        }
        (SourceDocument::Csv(csv), TableAction::PromoteCsv { .. }) => {
            let (markdown, report) =
                super::markdown_actions::csv_to_markdown(csv, request, document, cancelled)?;
            conversion_report = Some(report);
            SourceDocument::Markdown(Arc::new(markdown))
        }
        (SourceDocument::Csv(csv), TableAction::PromoteCsvCell { address, value, .. }) => {
            let index =
                source_index.ok_or_else(|| Message::new("status-markdown-table-target-missing"))?;
            let (markdown, report) =
                super::markdown_actions::csv_to_markdown(csv, request, document, cancelled)?;
            conversion_report = Some(report);
            let updated = markdown
                .apply_changes(&[crate::markdown_adapter::MarkdownCellChange {
                    table: index,
                    row: address.row,
                    column: address.column,
                    value: value.clone(),
                }])
                .map_err(crate::i18n::markdown_error_message)?;
            SourceDocument::Markdown(Arc::new(updated))
        }
        (SourceDocument::Csv(csv), TableAction::PromoteCsvCells { changes, .. }) => {
            let index =
                source_index.ok_or_else(|| Message::new("status-markdown-table-target-missing"))?;
            let (markdown, report) =
                super::markdown_actions::csv_to_markdown(csv, request, document, cancelled)?;
            conversion_report = Some(report);
            let updated = markdown
                .apply_changes(
                    &changes
                        .iter()
                        .map(
                            |(address, value)| crate::markdown_adapter::MarkdownCellChange {
                                table: index,
                                row: address.row,
                                column: address.column,
                                value: value.clone(),
                            },
                        )
                        .collect::<Vec<_>>(),
                )
                .map_err(crate::i18n::markdown_error_message)?;
            SourceDocument::Markdown(Arc::new(updated))
        }
        (
            SourceDocument::Csv(csv),
            TableAction::MarkdownLinks {
                table_id,
                range,
                hyperlink,
            },
        ) => {
            let index =
                source_index.ok_or_else(|| Message::new("status-markdown-table-target-missing"))?;
            let (markdown, report) =
                super::markdown_actions::csv_to_markdown(csv, request, document, cancelled)?;
            conversion_report = Some(report);
            SourceDocument::Markdown(Arc::new(apply_markdown_links(
                &markdown,
                index,
                document,
                *table_id,
                *range,
                hyperlink.as_ref(),
            )?))
        }
        (SourceDocument::Csv(csv), TableAction::Add { new_table_id, .. }) => {
            ensure_new_table_id(stable_ids_before, *new_table_id)?;
            let (markdown, report) =
                super::markdown_actions::csv_to_markdown(csv, request, document, cancelled)?;
            conversion_report = Some(report);
            stable_ids_after.push(*new_table_id);
            SourceDocument::Markdown(Arc::new(
                markdown
                    .append_table()
                    .map_err(crate::i18n::markdown_error_message)?,
            ))
        }
        (SourceDocument::Csv(csv), TableAction::Duplicate { new_table_id, .. }) => {
            ensure_new_table_id(stable_ids_before, *new_table_id)?;
            let index =
                source_index.ok_or_else(|| Message::new("status-table-copy-source-missing"))?;
            let (markdown, report) =
                super::markdown_actions::csv_to_markdown(csv, request, document, cancelled)?;
            conversion_report = Some(report);
            stable_ids_after.insert(index + 1, *new_table_id);
            SourceDocument::Markdown(Arc::new(
                markdown
                    .duplicate_table(index)
                    .map_err(crate::i18n::markdown_error_message)?,
            ))
        }
        (SourceDocument::Csv(csv), TableAction::MarkdownFormat { operation, .. }) => {
            let index =
                source_index.ok_or_else(|| Message::new("status-markdown-table-target-missing"))?;
            let (updated, report) = super::markdown_actions::promote_csv_and_apply_operation(
                csv, request, document, index, operation, cancelled,
            )?;
            conversion_report = Some(report);
            SourceDocument::Markdown(Arc::new(updated))
        }
        (SourceDocument::Csv(_), _) => {
            return Err(Message::new("status-csv-table-operation-unsupported"));
        }
    };
    check_cancelled(cancelled)?;
    Ok(SourceActionResult {
        source_after,
        stable_ids_after,
        formula_updates,
        conversion_report,
    })
}

/// Converts the current CSV snapshot with the same escaping and header rules as export.
fn apply_markdown_links(
    markdown: &crate::markdown_adapter::MarkdownDocument,
    source_index: usize,
    document: &Document,
    table_id: TableId,
    range: crate::document::CellRange,
    hyperlink: Option<&crate::document::CellHyperlink>,
) -> Result<crate::markdown_adapter::MarkdownDocument, Message> {
    if hyperlink.is_some_and(|link| link.location) {
        return Err(Message::new("status-markdown-link-unrepresentable"));
    }
    let table = document
        .tables
        .iter()
        .find(|table| table.id == table_id)
        .ok_or_else(|| Message::new("status-table-id-unavailable"))?;
    let (start, end) = range.normalized();
    let mut changes = Vec::new();
    for row in start.row..=end.row {
        for column in start.column..=end.column {
            let address = CellAddress { row, column };
            let cell = table
                .cell(address)
                .ok_or_else(|| Message::new("status-markdown-link-unrepresentable"))?;
            if crate::structure::is_formula_cell(table, address, cell) {
                return Err(Message::new("status-markdown-link-unrepresentable"));
            }
            let value = crate::export::markdown_link_cell_source(
                markdown,
                source_index,
                address,
                hyperlink,
            )
            .ok_or_else(|| Message::new("status-markdown-link-unrepresentable"))?;
            changes.push(crate::markdown_adapter::MarkdownCellChange {
                table: source_index,
                row,
                column,
                value,
            });
        }
    }
    markdown
        .apply_changes(&changes)
        .map_err(crate::i18n::markdown_error_message)
}
