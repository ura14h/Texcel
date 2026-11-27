//! Worker-side preparation for clipboard paste operations.

use super::*;

// Prepare clipboard cell changes as one structure operation.
pub(crate) fn prepare_bulk_cells(
    request: &StructureRequest,
    cancelled: &AtomicBool,
) -> Result<crate::document::PreparedStructure, Message> {
    let paste = request
        .clipboard_paste
        .as_ref()
        .ok_or_else(|| Message::new("status-paste-data-missing"))?;
    check_cancelled(cancelled)?;
    let source_before = request
        .source
        .as_deref()
        .map(|source| materialize_current_source(request, source))
        .transpose()?;
    let source = source_before
        .as_ref()
        .ok_or_else(|| Message::new("status-paste-source-table-missing"))?;
    let mut document = {
        let table_ids = request
            .tables
            .iter()
            .map(|table| table.id)
            .collect::<Vec<_>>();
        source_to_document(source, request.document_id, &table_ids)?
    };
    apply_table_metadata(&mut document, &request.tables)?;
    let overlay_before = apply_requested_text_overlay(
        &mut document,
        request.text_overlay_revision,
        request.revision,
        &request.text_overlay_cells,
        &request.changes,
        true,
    )?;
    check_cancelled(cancelled)?;
    let text_format = matches!(
        source_before.as_ref(),
        Some(SourceDocument::Csv(_) | SourceDocument::Markdown(_))
    );
    if !document
        .tables
        .iter()
        .any(|table| table.id == request.view_table_id)
    {
        return Err(Message::new("status-paste-target-table-missing"));
    }
    let mut expanded = expand_clipboard_paste(
        paste,
        matches!(source_before.as_ref(), Some(SourceDocument::Markdown(_))),
        matches!(source_before.as_ref(), Some(SourceDocument::Csv(_))),
    )?;
    let mut cells = std::mem::take(&mut expanded.cells);
    let mut formats = std::mem::take(&mut expanded.formats);
    // A paste of cut cells in the same table moves them in this one Undo step.
    if let Some(area) = paste.cut_source {
        let table = document
            .tables
            .iter()
            .find(|table| table.id == request.view_table_id)
            .ok_or_else(|| Message::new("status-paste-target-table-missing"))?;
        let moved = super::paste_cut::cut_move_changes(table, area, &cells)?;
        // Rewritten formulas keep their Markdown source spelling instead of being escaped.
        expanded.raw_markdown_sources.extend(
            moved
                .iter()
                .filter(|(_, cell)| cell.is_some())
                .map(|(address, _)| *address),
        );
        cells.extend(moved);
    }
    let markdown_features = !expanded.markdown_feature_sources.is_empty();
    if text_format {
        super::paste::retain_markdown_paste_features(&mut cells, &mut formats);
    }
    if cells.is_empty() {
        return Err(Message::new("status-paste-empty-range"));
    }
    let before_calculation = Arc::new(CalculationSnapshot::from_document(
        &document,
        request.cross_sheet_references,
    ));
    let target_table = request.view_table_id;
    let selection_after = request
        .prepared_selection_after
        .unwrap_or(request.selection);
    let target = (target_table, selection_after.normalized().0);
    validate_recovery_root(&document.tables, &request.view_layout, request.scroll)?;
    let before_root = history_root_ref(
        &document.tables,
        source_before.as_ref(),
        &request.view_layout,
        StructureHistoryMetadata {
            document_id: request.document_id,
            view_table_id: target_table,
            selection: request.selection,
            scroll: request.scroll,
            target: (target_table, request.selection.normalized().0),
            cross_sheet_references: request.cross_sheet_references,
        },
    );
    let before_size = serialized_size(&before_root)?;
    let cells_size = serde_json::to_vec(&cells)
        .map_err(|error| Message::new("status-history-size-failed").with_text("detail", error))?
        .len() as u64;
    let history_file = if before_size
        .saturating_mul(2)
        .saturating_add(cells_size.saturating_mul(2))
        >= STRUCTURE_HISTORY_SPILL_THRESHOLD_BYTES
    {
        let file = create_history_file()?;
        write_history_root(&file, "before", &before_root)?;
        Some(file)
    } else {
        None
    };
    let keep_in_memory_history = history_file.is_none();
    let mut conversion_report = None;
    let mut source_after_growth = source_before.clone();
    if let Some(SourceDocument::Csv(csv)) = source_before.as_ref()
        && (markdown_features || contains_markdown_paste_features(&formats))
    {
        let (markdown, report) = super::table_management::promote_csv_for_bulk_paste(
            csv, request, &document, cancelled,
        )?;
        source_after_growth = Some(SourceDocument::Markdown(Arc::new(markdown)));
        conversion_report = Some(report);
    }
    for batch in cells.chunks(1_024) {
        check_cancelled(cancelled)?;
        document
            .install_prepared_cells(target_table, batch)
            .map_err(crate::i18n::document_error_message)?;
    }
    let mut formatting_omitted = false;
    let source_after = source_after_growth
        .as_ref()
        .map(|source| {
            let (source, omitted) = apply_bulk_cell_changes_for_table_with_raw_markdown_sources(
                source,
                request.view_table_id,
                &cells,
                &formats,
                &document,
                &expanded.raw_markdown_sources,
            )?;
            formatting_omitted = omitted;
            Ok(source)
        })
        .transpose()?;
    if let Some(source @ SourceDocument::Markdown(_)) = source_after.as_ref() {
        let stable_ids = request
            .tables
            .iter()
            .map(|table| table.id)
            .collect::<Vec<_>>();
        let mut parsed_document = source_to_document(source, request.document_id, &stable_ids)?;
        apply_table_metadata(&mut parsed_document, &request.tables)?;
        document.tables = parsed_document.tables;
        let pasted_addresses = cells
            .iter()
            .map(|(address, _)| *address)
            .collect::<std::collections::BTreeSet<_>>();
        for (table_id, address, cell) in overlay_before.iter() {
            if *table_id == target_table && pasted_addresses.contains(address) {
                continue;
            }
            if let Some(table) = document
                .tables
                .iter_mut()
                .find(|table| table.id == *table_id)
            {
                table.apply_snapshot_cell(*address, Some(cell.clone()));
            }
        }
    }
    if let Some(source) = source_after.as_ref() {
        document.set_source(Some(Arc::new(source.clone())));
    }
    if text_format && let Some(source) = source_after.as_ref() {
        let table = document
            .tables
            .iter_mut()
            .find(|table| table.id == target_table)
            .ok_or_else(|| Message::new("status-paste-target-table-missing"))?;
        let (source_rows, source_columns) = source_extent(source, target_table, &request.tables);
        let used = table.last_used_cell();
        table.set_dimensions(
            source_rows.max(used.map_or(0, |address| address.row.saturating_add(1))),
            source_columns.max(used.map_or(0, |address| address.column.saturating_add(1))),
        );
    }
    let text_overlay_after =
        collect_preserved_text_overlay(&document.tables, true, &overlay_before);
    check_cancelled(cancelled)?;

    let after_calculation = Arc::new(CalculationSnapshot::from_document(
        &document,
        request.cross_sheet_references,
    ));
    validate_recovery_root(&document.tables, &request.view_layout, request.scroll)?;
    if let Some(history_file) = &history_file {
        let after_root = history_root_ref(
            &document.tables,
            source_after.as_ref(),
            &request.view_layout,
            StructureHistoryMetadata {
                document_id: request.document_id,
                view_table_id: target_table,
                selection: selection_after,
                scroll: request.scroll,
                target,
                cross_sheet_references: request.cross_sheet_references,
            },
        );
        write_history_root(history_file, "after", &after_root)?;
    }
    Ok(crate::document::PreparedStructure {
        tables: document.tables,
        calculation_revision: request.calculation_revision,
        replace_table_set: false,
        text_overlay_before: overlay_before,
        text_overlay_after,
        source_before: keep_in_memory_history
            .then(|| source_before.map(Arc::new))
            .flatten(),
        source_after: source_after.map(Arc::new),
        view_table_id: request.view_table_id,
        view_layout_revision: request.view_layout.revision(),
        view_layout_after: Arc::clone(&request.view_layout),
        selection_after,
        scroll_after: request.scroll,
        target,
        target_before: None,
        before_calculation: keep_in_memory_history.then_some(before_calculation),
        after_calculation,
        history_file,
        conversion_report,
        status: formatting_omitted
            .then(|| Message::new("status-paste-markdown-formatting-omitted")),
    })
}

/// Checks whether the paste needs a Markdown source to keep its supported formatting.
fn contains_markdown_paste_features(
    formats: &BTreeMap<CellAddress, crate::document::CellFormat>,
) -> bool {
    formats.values().any(|format| {
        format.style.bold
            || format.style.italic
            || format.style.strike
            || format.hyperlink.is_some()
            || format.rich_text.as_ref().is_some_and(|runs| {
                runs.iter().any(|run| {
                    run.bold || run.italic || run.strike || run.code || run.hyperlink.is_some()
                })
            })
    })
}
