use super::*;

// Called by FileWorker. Stops conversion when the user cancels.
pub(crate) fn prepare_export_cancellable(
    request: &ExportRequest,
    formula_results: Option<&FormulaResultSnapshot>,
    cancelled: &AtomicBool,
) -> Result<PreparedExport, crate::i18n::Message> {
    check_cancelled(Some(cancelled))?;
    if request.options.format.is_delimited() && request.options.scope == ExportScope::AllTables {
        return Err(crate::i18n::Message::new(
            "status-export-csv-selected-table-required",
        ));
    }
    prepare_export_inner(request, formula_results, Some(cancelled))
}

// Validate the export request and build a prepared export result.
pub(super) fn prepare_export_inner(
    request: &ExportRequest,
    formula_results: Option<&FormulaResultSnapshot>,
    cancelled: Option<&AtomicBool>,
) -> Result<PreparedExport, crate::i18n::Message> {
    check_cancelled(cancelled)?;
    let output_name = request.destination.file_name().map_or_else(
        || "出力ファイル".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    let (document, selected) = match prepare_document_and_selection(request, cancelled) {
        Ok(prepared) => prepared,
        Err(reason) => return Ok(failed_export(request, &output_name, reason)),
    };
    let formula_index = FormulaResultIndex::new(formula_results, cancelled)?;
    let markdown_context = match markdown_export_context(request, cancelled) {
        Ok(context) => context,
        Err(reason) => return Ok(failed_export(request, &output_name, reason)),
    };
    let cell_changes = request
        .changes
        .iter()
        .map(|(table_id, address, cell)| ((*table_id, *address), cell.as_ref()))
        .collect();
    let build_context = ExportBuildContext {
        request,
        cell_changes,
        formula_index: &formula_index,
        markdown_context: markdown_context.as_ref(),
        cancelled,
    };
    let mut entries = Vec::new();
    let destinations = prepare_destinations(request, &selected, &mut entries);
    collect_table_entries(
        request,
        &document,
        &selected,
        &formula_index,
        markdown_context.as_ref(),
        &mut entries,
        cancelled,
    )?;
    merge_source_report_entries(request, &mut entries);
    let artifacts = build_export_artifacts(&build_context, &destinations, &mut entries, cancelled)?;
    let actual_outputs = destinations
        .iter()
        .map(|(path, _)| {
            path.file_name().map_or_else(
                || "output file".to_owned(),
                |name| name.to_string_lossy().into_owned(),
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    let selected_table_name = match request.options.scope {
        ExportScope::Selected(table_id) => request
            .tables
            .iter()
            .find(|table| table.id == table_id)
            .map(|table| table.name.as_str()),
        ExportScope::AllTables => None,
    };
    let settings = settings_summary(request.options, &actual_outputs, selected_table_name);
    let report = Arc::new(ExportReport::new(
        request.revision,
        request.options.format.label(),
        settings,
        entries,
    ));
    Ok(PreparedExport { report, artifacts })
}

/// Materialize edited source data and choose the output tables.
pub(super) fn prepare_document_and_selection(
    request: &ExportRequest,
    cancelled: Option<&AtomicBool>,
) -> Result<(Document, Vec<ExportTable>), crate::i18n::Message> {
    let document = materialize_document_cancellable(request, cancelled)?;
    let selected = selected_tables(&document, request, request.options.scope, cancelled)?;
    Ok((document, selected))
}

/// Return a report with one blocking issue and no output files.
pub(super) fn failed_export(
    request: &ExportRequest,
    output_name: &str,
    reason: crate::i18n::Message,
) -> PreparedExport {
    let fallback_reason = reason.resolve(crate::i18n::Language::English);
    let entry = ConversionEntry::new(
        ConversionClass::Unprocessable,
        None,
        None,
        None,
        fallback_reason,
    )
    .with_message(
        crate::i18n::Message::new("report-reason-export-failed").with_message("detail", reason),
    );
    let report = Arc::new(ExportReport::new(
        request.revision,
        request.options.format.label(),
        settings_summary(request.options, output_name, None),
        vec![entry],
    ));
    PreparedExport {
        report,
        artifacts: Vec::new(),
    }
}

#[cfg(test)]
// Build an export snapshot using the current formula results.
pub fn prepare_export(
    request: &ExportRequest,
    formula_results: Option<&FormulaResultSnapshot>,
) -> Result<PreparedExport, crate::i18n::Message> {
    prepare_export_inner(request, formula_results, None)
}
