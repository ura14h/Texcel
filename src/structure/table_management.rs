//!  Worker-side operations for table management.

use super::*;

#[path = "table_management/markdown_actions.rs"]
mod markdown_actions;
mod source_action;
use self::source_action::{SourceActionResult, apply_source_table_action};

/// Converts a CSV source before one formatted paste so Undo restores the original CSV.
pub(crate) fn promote_csv_for_bulk_paste(
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
    markdown_actions::csv_to_markdown(csv, request, document, cancelled)
}

// Prepare a table creation, deletion, or related table operation.
pub(crate) fn prepare_table_structure(
    request: &StructureRequest,
    action: &TableAction,
    cancelled: &AtomicBool,
) -> Result<crate::document::PreparedStructure, Message> {
    check_cancelled(cancelled)?;
    let source = request
        .source
        .as_deref()
        .ok_or_else(|| Message::new("status-table-source-required"))?;
    let source_before = materialize_current_source(request, source)?;
    let stable_ids_before = request
        .tables
        .iter()
        .map(|table| table.id)
        .collect::<Vec<_>>();
    let mut document = source_to_document(&source_before, request.document_id, &stable_ids_before)?;
    apply_table_metadata(&mut document, &request.tables)?;
    let text_format = matches!(
        &source_before,
        SourceDocument::Csv(_) | SourceDocument::Markdown(_)
    );
    let overlay_before = apply_requested_text_overlay(
        &mut document,
        request.text_overlay_revision,
        request.revision,
        &request.text_overlay_cells,
        &request.changes,
        text_format,
    )?;
    check_cancelled(cancelled)?;

    let source_table_id = match action {
        TableAction::Add { .. } => None,
        TableAction::Duplicate { table_id, .. }
        | TableAction::Delete { table_id }
        | TableAction::Rename { table_id, .. }
        | TableAction::MarkdownFormat { table_id, .. }
        | TableAction::PromoteCsv { table_id }
        | TableAction::PromoteCsvCell { table_id, .. }
        | TableAction::PromoteCsvCells { table_id, .. }
        | TableAction::MarkdownLinks { table_id, .. } => Some(*table_id),
    };
    let source_index = source_table_id
        .map(|table_id| {
            stable_ids_before
                .iter()
                .position(|candidate| *candidate == table_id)
                .ok_or_else(|| {
                    Message::new("status-table-id-unavailable")
                        .with_number("table_id", table_id as i64)
                })
        })
        .transpose()?;
    let temporary_alignment = match action {
        TableAction::MarkdownFormat {
            table_id,
            operation: MarkdownTableOperation::SetColumnAlignment { columns, .. },
        } => {
            let source_columns = match &source_before {
                SourceDocument::Markdown(markdown) => source_index
                    .and_then(|index| markdown.parser_width(index))
                    .unwrap_or(0),
                SourceDocument::Csv(_) => {
                    source_extent(&source_before, *table_id, &request.tables).1
                }
            };
            columns.iter().any(|range| *range.end() >= source_columns)
        }
        _ => false,
    };
    let SourceActionResult {
        source_after,
        stable_ids_after,
        formula_updates,
        conversion_report,
    } = apply_source_table_action(
        request,
        &document,
        &source_before,
        action,
        &stable_ids_before,
        source_index,
        cancelled,
    )?;
    let mut parsed_after =
        source_to_document(&source_after, request.document_id, &stable_ids_after)?;
    let requested_name = match action {
        TableAction::Add { new_table_id, name }
        | TableAction::Duplicate {
            new_table_id, name, ..
        } => Some((*new_table_id, name.as_str())),
        TableAction::Rename { table_id, name } => Some((*table_id, name.as_str())),
        _ => None,
    };
    let mut old_tables = document
        .tables
        .iter()
        .map(|table| (table.id, table.clone()))
        .collect::<BTreeMap<_, _>>();
    let duplicated_from = match action {
        TableAction::Duplicate {
            table_id,
            new_table_id,
            ..
        } => Some((*new_table_id, *table_id)),
        _ => None,
    };
    let mut tables_after = Vec::with_capacity(parsed_after.tables.len());
    let csv_was_promoted = matches!(
        (&source_before, &source_after),
        (SourceDocument::Csv(_), SourceDocument::Markdown(_))
    );
    for parsed_table in parsed_after.tables.drain(..) {
        let id = parsed_table.id;
        let mut table = if let Some(table) = old_tables.remove(&id) {
            table
        } else if let Some((_, original_id)) = duplicated_from.filter(|(new_id, _)| *new_id == id) {
            let mut duplicate = document
                .tables
                .iter()
                .find(|table| table.id == original_id)
                .cloned()
                .ok_or_else(|| Message::new("status-table-copy-state-missing"))?;
            duplicate.id = id;
            duplicate
        } else {
            parsed_table.clone()
        };
        if csv_was_promoted && document.tables.iter().any(|old| old.id == id) {
            copy_promoted_csv_cells(&mut table, &parsed_table);
        }
        if let TableAction::MarkdownLinks {
            table_id, range, ..
        } = action
            && *table_id == id
        {
            copy_markdown_link_source_cells(&mut table, &parsed_table, *range);
        }
        if let Some((_, name)) = requested_name.filter(|(target_id, _)| *target_id == id) {
            table.name = name.to_owned();
        }
        tables_after.push(table);
    }
    for ((table_id, address), formula) in &formula_updates {
        let table = tables_after
            .iter_mut()
            .find(|table| table.id == *table_id)
            .ok_or_else(|| {
                Message::new("status-table-formula-target-missing")
                    .with_number("table_id", *table_id as i64)
            })?;
        let mut cell = table.cell(*address).cloned().ok_or_else(|| {
            Message::new("status-table-formula-cell-missing")
                .with_text("cell", crate::xlsx_adapter::cell_reference(*address))
        })?;
        cell.input = crate::document::CellInput::Formula(formula.clone());
        cell.imported_result = None;
        cell.calculation = None;
        table.apply_snapshot_cell(*address, Some(cell));
    }
    if tables_after.is_empty() || tables_after.len() != stable_ids_after.len() {
        return Err(Message::new("status-table-last-required"));
    }
    let source_after = Arc::new(source_after);
    let text_overlay_after =
        collect_preserved_text_overlay(&tables_after, text_format, &overlay_before);
    let after_document =
        document_with_tables(request.document_id, tables_after, source_after.as_ref());
    let before_calculation = Arc::new(CalculationSnapshot::from_document(
        &document,
        request.cross_sheet_references,
    ));
    let after_calculation = Arc::new(CalculationSnapshot::from_document(
        &after_document,
        request.cross_sheet_references,
    ));

    let table_view = |table_id| {
        if table_id == request.view_table_id {
            return (
                Arc::clone(&request.view_layout),
                request.selection,
                request.scroll,
            );
        }
        request
            .tables
            .iter()
            .find(|table| table.id == table_id)
            .map(|table| {
                (
                    Arc::clone(&table.view_layout),
                    table.selection,
                    table.scroll,
                )
            })
            .unwrap_or_else(|| {
                (
                    Arc::new(TableLayout::default()),
                    CellRange::default(),
                    ScrollPosition::default(),
                )
            })
    };
    let history_before_table_id = match action {
        TableAction::Delete { table_id }
        | TableAction::Rename { table_id, .. }
        | TableAction::MarkdownFormat { table_id, .. }
        | TableAction::PromoteCsv { table_id }
        | TableAction::PromoteCsvCell { table_id, .. }
        | TableAction::PromoteCsvCells { table_id, .. }
        | TableAction::MarkdownLinks { table_id, .. } => *table_id,
        TableAction::Add { .. } | TableAction::Duplicate { .. } => request.view_table_id,
    };
    let (_, before_selection, _) = table_view(history_before_table_id);
    let target_before = (history_before_table_id, before_selection.normalized().0);
    let target_table_id = match action {
        TableAction::Add { new_table_id, .. } | TableAction::Duplicate { new_table_id, .. } => {
            *new_table_id
        }
        TableAction::Delete { .. } => {
            let index =
                source_index.ok_or_else(|| Message::new("status-table-delete-target-missing"))?;
            stable_ids_after
                .get(index.saturating_sub(1))
                .or_else(|| stable_ids_after.get(index))
                .copied()
                .ok_or_else(|| Message::new("status-table-last-delete-blocked"))?
        }
        TableAction::Rename { table_id, .. }
        | TableAction::MarkdownFormat { table_id, .. }
        | TableAction::PromoteCsv { table_id }
        | TableAction::PromoteCsvCell { table_id, .. }
        | TableAction::PromoteCsvCells { table_id, .. }
        | TableAction::MarkdownLinks { table_id, .. } => *table_id,
    };
    let (before_layout, before_selection, before_scroll) = table_view(history_before_table_id);
    let (after_layout, after_selection, after_scroll) = table_view(target_table_id);
    let target = (target_table_id, after_selection.normalized().0);
    validate_recovery_root(&document.tables, &before_layout, before_scroll)?;
    let before_root = history_root_ref(
        &document.tables,
        Some(&source_before),
        &before_layout,
        StructureHistoryMetadata {
            document_id: request.document_id,
            view_table_id: history_before_table_id,
            selection: before_selection,
            scroll: before_scroll,
            target: target_before,
            cross_sheet_references: request.cross_sheet_references,
        },
    );
    let before_size = serialized_size(&before_root)?;
    let active_after = (after_layout.as_ref().clone(), after_selection, after_scroll);
    validate_recovery_root(&after_document.tables, &active_after.0, active_after.2)?;
    let after_root = history_root_ref(
        &after_document.tables,
        Some(&source_after),
        &active_after.0,
        StructureHistoryMetadata {
            document_id: request.document_id,
            view_table_id: target_table_id,
            selection: active_after.1,
            scroll: active_after.2,
            target,
            cross_sheet_references: request.cross_sheet_references,
        },
    );
    let after_size = serialized_size(&after_root)?;
    let estimated_history_bytes = before_size.saturating_add(after_size);
    let history_file = if estimated_history_bytes >= STRUCTURE_HISTORY_SPILL_THRESHOLD_BYTES {
        let file = create_history_file()?;
        write_history_root(&file, "before", &before_root)?;
        write_history_root(&file, "after", &after_root)?;
        Some(file)
    } else {
        None
    };
    Ok(crate::document::PreparedStructure {
        tables: after_document.tables,
        calculation_revision: request.calculation_revision,
        replace_table_set: true,
        text_overlay_before: overlay_before,
        text_overlay_after,
        source_before: history_file.is_none().then(|| Arc::new(source_before)),
        source_after: Some(source_after),
        view_table_id: target_table_id,
        view_layout_revision: after_layout.revision(),
        view_layout_after: after_layout,
        selection_after: after_selection,
        scroll_after: after_scroll,
        target,
        target_before: Some(target_before),
        before_calculation: history_file.is_none().then_some(before_calculation),
        after_calculation,
        history_file,
        conversion_report,
        status: temporary_alignment
            .then(|| Message::new("status-markdown-column-alignment-temporary")),
    })
}

// Reject a table ID that is zero or already in use.
fn ensure_new_table_id(existing: &[TableId], new_table_id: TableId) -> Result<(), Message> {
    if new_table_id == 0 || existing.contains(&new_table_id) {
        Err(Message::new("status-table-id-duplicate-or-invalid")
            .with_number("table_id", new_table_id as i64))
    } else {
        Ok(())
    }
}

// Replaces CSV cell text with the escaped source values parsed from the new Markdown.
fn copy_promoted_csv_cells(table: &mut crate::document::Table, parsed: &crate::document::Table) {
    let old_addresses = table
        .stored_cells()
        .map(|(address, _)| address)
        .collect::<Vec<_>>();
    for address in old_addresses {
        table.apply_snapshot_cell(address, None);
    }
    for (address, cell) in parsed.stored_cells() {
        table.apply_snapshot_cell(address, Some(cell.clone()));
    }
    table.set_dimensions(parsed.row_count(), parsed.column_count());
    table.set_loaded_string_cells(parsed.string_cells_shared());
}

// Keeps the live cell input aligned with Markdown edited by the link worker.
fn copy_markdown_link_source_cells(
    table: &mut crate::document::Table,
    parsed: &crate::document::Table,
    range: CellRange,
) {
    let (start, end) = range.normalized();
    for row in start.row..=end.row {
        for column in start.column..=end.column {
            let address = CellAddress { row, column };
            let Some(mut current) = table.cell(address).cloned() else {
                continue;
            };
            let Some(source_cell) = parsed.cell(address) else {
                continue;
            };
            current.input.clone_from(&source_cell.input);
            current.imported_result = None;
            current.calculation = None;
            table.apply_snapshot_cell(address, Some(current));
        }
    }
}

// Build a document model from the supplied tables and source settings.
pub(crate) fn document_with_tables(
    document_id: u64,
    tables: Vec<crate::document::Table>,
    source: &SourceDocument,
) -> Document {
    let names = tables
        .iter()
        .map(|table| table.name.as_str())
        .collect::<Vec<_>>();
    let mut document = Document::new(document_id, "文書", &names);
    document.active_table = tables.first().map_or(1, |table| table.id);
    document.views = tables
        .iter()
        .map(|table| (table.id, crate::document::TableViewState::default()))
        .collect();
    document.tables = tables;
    document.source = Some(Arc::new(source.clone()));
    document
}
