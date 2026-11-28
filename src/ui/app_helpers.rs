use super::*;

impl TexcelApp {}

/// Builds the structure request for a fill handle drag.
pub(super) fn fill_handle_target(source: CellRange, endpoint: CellAddress) -> Option<CellRange> {
    let (source_start, source_end) = source.normalized();
    let target_start = CellAddress {
        row: source_start.row.min(endpoint.row),
        column: source_start.column.min(endpoint.column),
    };
    let target_end = CellAddress {
        row: source_end.row.max(endpoint.row),
        column: source_end.column.max(endpoint.column),
    };
    if target_start == source_start && target_end == source_end {
        return None;
    }
    let extends_rows = target_start.row < source_start.row || target_end.row > source_end.row;
    let extends_columns =
        target_start.column < source_start.column || target_end.column > source_end.column;
    (extends_rows != extends_columns).then_some(CellRange {
        anchor: target_start,
        head: target_end,
    })
}

/// Builds the worker request for a structure search.
pub(super) fn search_structure_request(
    document: &Document,
    view_table_id: u64,
) -> StructureRequest {
    let view = document
        .view(view_table_id)
        .expect("search target table has view state");
    StructureRequest {
        document_id: document.id,
        revision: document.revision,
        calculation_revision: document.calculation_revision,
        source: document.source.as_ref().map(Arc::clone),
        operation: None,
        tables: structure_table_metadata(document),
        changes: document.changed_cell_snapshot(),
        text_overlay_cells: document.text_overlay_cells(),
        text_overlay_revision: document.text_overlay_snapshot().revision,
        view_table_id,
        view_layout: Arc::clone(&view.layout),
        selection: view.selection,
        scroll: view.scroll,
        cross_sheet_references: false,
        clipboard_paste: None,
        prepared_selection_after: None,
    }
}

/// Builds lightweight table metadata without copying cell payloads.
fn structure_table_metadata(document: &Document) -> Vec<StructureTableMeta> {
    document
        .tables
        .iter()
        .map(|table| {
            let view = document.view(table.id);
            StructureTableMeta {
                id: table.id,
                name: table.name.clone(),
                rows: table.row_count(),
                columns: table.column_count(),
                string_cells: table.string_cells_shared(),
                view_layout: view.map_or_else(
                    || Arc::new(TableLayout::default()),
                    |view| Arc::clone(&view.layout),
                ),
                selection: view.map_or_else(CellRange::default, |view| view.selection),
                scroll: view.map_or_else(Default::default, |view| view.scroll),
            }
        })
        .collect()
}

/// Captures only the edited cells and layout inputs needed to auto-fit one column.
pub(super) fn auto_fit_structure_request(
    document: &Document,
    targets: &[crate::file_worker::AutoFitTarget],
) -> StructureRequest {
    // The structure request names one table for its view; the targets name the rest.
    let view_table_id = targets
        .first()
        .map_or(document.active_table, |target| target.table_id);
    let view = document
        .view(view_table_id)
        .expect("auto-fit target table has view state");
    StructureRequest {
        document_id: document.id,
        revision: document.revision,
        calculation_revision: document.calculation_revision,
        source: document.source.as_ref().map(Arc::clone),
        operation: None,
        tables: structure_table_metadata(document),
        changes: targets
            .iter()
            .flat_map(|target| {
                target.columns.iter().flat_map(|column| {
                    document.changed_cell_snapshot_for_column(target.table_id, *column)
                })
            })
            .collect(),
        text_overlay_cells: document.text_overlay_cells(),
        text_overlay_revision: document.text_overlay_snapshot().revision,
        view_table_id,
        view_layout: Arc::clone(&view.layout),
        selection: view.selection,
        scroll: view.scroll,
        cross_sheet_references: false,
        clipboard_paste: None,
        prepared_selection_after: None,
    }
}

/// Returns the cell ranges covered by a search scope.
pub(super) fn search_scope_ranges(
    document: &Document,
    table_id: u64,
    scope: SearchScope,
) -> Option<Vec<(u64, CellRange)>> {
    document.tables.iter().find(|table| table.id == table_id)?;
    let view = document.view(table_id)?;
    Some(match scope {
        SearchScope::Selection => view
            .selected_ranges()
            .map(|range| (table_id, range))
            .collect(),
        SearchScope::Table => vec![(
            table_id,
            CellRange {
                anchor: CellAddress::default(),
                head: document.extent_end(table_id)?,
            },
        )],
        SearchScope::Document => document
            .tables
            .iter()
            .map(|table| {
                (
                    table.id,
                    CellRange {
                        anchor: CellAddress::default(),
                        head: document.extent_end(table.id).unwrap_or_default(),
                    },
                )
            })
            .collect(),
    })
}

/// Captures the query and document version for a search job.
pub(super) fn search_job_key(
    document: &Document,
    state: &SearchDialogState,
) -> Option<SearchJobKey> {
    let view = document.view(state.table_id)?;
    Some(SearchJobKey {
        document_id: document.id,
        document_revision: document.revision,
        calculation_revision: document.calculation_revision,
        table_id: state.table_id,
        view_layout_revision: view.layout.revision(),
        query: state.query.clone(),
        replacement: state.replacement.clone(),
        scope: state.scope,
        representation: state.representation,
        case_sensitive: state.case_sensitive,
        regular_expression: state.regular_expression,
        whole_cell: state.whole_cell,
        replace_mode: state.replace_mode,
        ranges: search_scope_ranges(document, state.table_id, state.scope)?,
    })
}

/// Captures selection and document version for a data edit job.
pub(super) fn data_edit_job_key(
    document: &Document,
    state: &DataEditDialogState,
) -> Option<DataEditJobKey> {
    let view = document.view(state.table_id)?;
    Some(DataEditJobKey {
        document_id: document.id,
        document_revision: document.revision,
        calculation_revision: document.calculation_revision,
        table_id: state.table_id,
        view_layout_revision: view.layout.revision(),
        current_selection: view.selected_ranges().collect(),
        kind: state.kind,
        range: state.range,
        range_text: state.range_text.clone(),
        selected_ranges: state.selected_ranges.clone(),
        delimiter: state.delimiter.clone(),
        destination_column: state.destination_column.clone(),
        overwrite: state.overwrite,
        sort_keys: state.sort_keys.clone(),
    })
}
