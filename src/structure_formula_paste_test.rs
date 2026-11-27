use super::*;
use crate::document::CalcValue;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

/// The Paste Values choices: formula results without formatting.
const VALUES_PASTE: crate::clipboard::PasteOptions = crate::clipboard::PasteOptions {
    text_only: false,
    values_only: true,
    strip_formatting: true,
    transpose: false,
    skip_blank: false,
};

#[test]
// Paste only a fresh value from a text formula and reject its missing result.
fn values_only_paste_uses_fresh_text_formula_values() {
    let formula_cell = Cell::text("=1+1");
    let fresh = crate::document::CalcResult {
        revision: 7,
        status: CalcStatus::Fresh,
        value: Some(crate::document::CalcValue::Number(2.0)),
    };
    let copied = crate::clipboard::ClipboardCell::from_cell_with_formula_context(
        &formula_cell,
        true,
        Some(&fresh),
    );
    let pasted = paste::convert_paste_cell(&copied, &VALUES_PASTE, true)
        .expect("the current text-formula result is available");
    assert_eq!(pasted.input, CellInput::Number(2.0));

    let stale = crate::document::CalcResult {
        status: CalcStatus::Stale,
        ..fresh
    };
    let copied_stale = crate::clipboard::ClipboardCell::from_cell_with_formula_context(
        &formula_cell,
        true,
        Some(&stale),
    );
    assert_eq!(
        paste::convert_paste_cell(&copied_stale, &VALUES_PASTE, true)
            .expect_err("cancelled or stale text formulas have no current value"),
        Message::new("status-paste-current-formula-result-missing")
    );

    let forced_text =
        crate::clipboard::ClipboardCell::from_cell_with_formula_context(&formula_cell, false, None);
    assert_eq!(
        paste::convert_paste_cell(&forced_text, &VALUES_PASTE, false)
            .expect("a forced-text cell is not treated as a formula")
            .input,
        CellInput::Text("'=1+1".to_owned())
    );

    let marked_text = crate::clipboard::ClipboardCell::from(&Cell::text("'=1+1"));
    assert_eq!(
        paste::convert_paste_cell(&marked_text, &VALUES_PASTE, true)
            .expect("an existing literal marker stays unchanged")
            .input,
        CellInput::Text("'=1+1".to_owned())
    );
    let external = crate::clipboard::parse_clipboard("'=1+1", None)
        .expect("plain external text parses as a clipboard range");
    assert_eq!(external.kind, crate::clipboard::ClipboardInputKind::Text);
    let external_cell = &external.ranges[0][0][0];
    let preserved_external = paste::convert_paste_cell(
        external_cell,
        &VALUES_PASTE,
        external.kind == crate::clipboard::ClipboardInputKind::Internal,
    )
    .expect("an external apostrophe remains part of the value");
    assert_eq!(
        preserved_external.input,
        CellInput::Text("''=1+1".to_owned())
    );
    assert_eq!(
        crate::document::literal_text_value(preserved_external.input.source_text().as_str()),
        "'=1+1"
    );
}

/// Recalculates one document through the normal worker and installs its complete root.
fn recalculate_document(document: &mut Document, job_id: u64) {
    let snapshot = document
        .take_calculation_reset_snapshot()
        .unwrap_or_else(|| {
            Arc::new(crate::calc::CalculationSnapshot::from_document(
                document, false,
            ))
        });
    let token = crate::file_worker::JobToken {
        document_id: document.id,
        revision: document.calculation_revision,
        job_id,
    };
    let worker = crate::calc::CalculationWorker::new();
    worker
        .initialize_shared_retaining(token, snapshot)
        .map_err(|(error, _)| error)
        .expect("the test calculation request is queued");

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        for result in worker.poll() {
            assert_eq!(result.token, token);
            assert!(result.error.is_none(), "{:?}", result.error);
            if result.done {
                let root = result.full_snapshot.expect("the calculation is complete");
                assert!(document.install_calculation_results(root));
                return;
            }
        }
        assert!(Instant::now() < deadline, "calculation timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Builds a worker request for a values-only paste into the third cell.
fn values_only_paste_request(document: &Document, html: String) -> StructureRequest {
    let table = &document.tables[0];
    let view = document.view(table.id).expect("the table view exists");
    let target = CellAddress { row: 0, column: 2 };
    StructureRequest {
        document_id: document.id,
        revision: document.revision,
        calculation_revision: document.calculation_revision,
        source: document.source.as_ref().map(Arc::clone),
        operation: None,
        tables: vec![StructureTableMeta {
            id: table.id,
            name: table.name.clone(),
            rows: table.row_count(),
            columns: table.column_count(),
            string_cells: table.string_cells_shared(),
            ..StructureTableMeta::default()
        }],
        changes: document.changed_cell_snapshot(),
        text_overlay_cells: document.text_overlay_cells(),
        text_overlay_revision: document.text_overlay_snapshot().revision,
        view_table_id: table.id,
        view_layout: Arc::clone(&view.layout),
        selection: CellRange::single(target),
        clipboard_paste: Some(ClipboardPasteRequest {
            cut_source: None,
            text: "=1+1".to_owned(),
            html: Some(html),
            options: VALUES_PASTE,
            selection: CellRange::single(target),
            rows: table.row_count(),
            columns: table.column_count(),
        }),
        prepared_selection_after: None,
        scroll: view.scroll,
        cross_sheet_references: false,
    }
}

/// Checks that a pasted leading-equals value stays literal through history replay.
fn assert_values_only_literal_undo_redo(source: crate::clipboard::ClipboardCell, job_id: u64) {
    // A CSV file whose A1 counts the number in C1.
    let csv = crate::csv_adapter::CsvDocument::parse(
        b"=COUNT(C1),,5\n".to_vec(),
        crate::csv_adapter::EncodingChoice::Auto,
        ',',
    )
    .expect("CSV fixture parses");
    let mut document = csv.to_document(800 + job_id, "literal-paste.csv");
    document.set_source(Some(Arc::new(crate::source::SourceDocument::Csv(
        Arc::new(csv),
    ))));
    let table_id = document.tables[0].id;
    let formula_address = CellAddress { row: 0, column: 0 };
    let target = CellAddress { row: 0, column: 2 };
    recalculate_document(&mut document, job_id * 10);
    assert_eq!(
        document
            .calculation_result_for(table_id, formula_address)
            .and_then(|result| result.value),
        Some(CalcValue::Number(1.0))
    );

    let visible = crate::clipboard::ClipboardCell::from(&Cell::text("=1+1"));
    let expected_source_text = source.input.source_text();
    let expected_formula_source = source.formula_source;
    let expected_fresh_value = source.fresh_formula_value.clone();
    let html = crate::clipboard::create_html_clipboard(
        &[vec![vec![source]]],
        &[vec![vec![visible]]],
        &[],
        "=1+1",
    )
    .expect("the clipboard payload is valid");
    crate::clipboard::tests::mark_test_owned_html(&html);
    let parsed = crate::clipboard::parse_clipboard("=1+1", Some(&html))
        .expect("owned clipboard data is restored");
    assert_eq!(parsed.kind, crate::clipboard::ClipboardInputKind::Internal);
    assert_eq!(
        parsed.ranges[0][0][0].input.source_text(),
        expected_source_text
    );
    assert_eq!(
        parsed.ranges[0][0][0].formula_source,
        expected_formula_source
    );
    assert_eq!(
        parsed.ranges[0][0][0].fresh_formula_value,
        expected_fresh_value
    );
    let request = values_only_paste_request(&document, html);
    let prepared = prepare_bulk_cells(&request, &AtomicBool::new(false))
        .expect("the worker prepares the paste");
    document
        .execute_prepared_structure(prepared)
        .expect("the paste enters document history");
    assert!(!document.tables[0].is_string_cell(target));
    assert_eq!(
        document.tables[0].cell(target).map(|cell| &cell.input),
        Some(&CellInput::Text("'=1+1".to_owned()))
    );
    recalculate_document(&mut document, job_id * 10 + 1);
    assert_eq!(
        document
            .calculation_result_for(table_id, formula_address)
            .and_then(|result| result.value),
        Some(CalcValue::Number(0.0)),
        "the pasted text is not reinterpreted as a formula"
    );

    document.undo().expect("undo restores the prior structure");
    assert!(!document.tables[0].is_string_cell(target));
    assert_eq!(
        document.tables[0].cell(target).map(|cell| &cell.input),
        Some(&CellInput::Text("5".to_owned()))
    );
    recalculate_document(&mut document, job_id * 10 + 2);
    assert_eq!(
        document
            .calculation_result_for(table_id, formula_address)
            .and_then(|result| result.value),
        Some(CalcValue::Number(1.0))
    );

    document.redo().expect("redo restores the literal paste");
    assert!(!document.tables[0].is_string_cell(target));
    recalculate_document(&mut document, job_id * 10 + 3);
    assert_eq!(
        document
            .calculation_result_for(table_id, formula_address)
            .and_then(|result| result.value),
        Some(CalcValue::Number(0.0))
    );
}

#[test]
// Preserve Fresh results, raw text, and saved literal markers through Undo.
fn values_only_literal_paste_and_string_source_survive_recalculation_undo_redo() {
    let formula = Cell::text("=\"=1+1\"");
    let fresh = crate::document::CalcResult {
        revision: 1,
        status: CalcStatus::Fresh,
        value: Some(CalcValue::Text("=1+1".to_owned())),
    };
    let calculated = crate::clipboard::ClipboardCell::from_cell_with_formula_context(
        &formula,
        true,
        Some(&fresh),
    );
    assert_values_only_literal_undo_redo(calculated, 1);

    let explicit_text = crate::clipboard::ClipboardCell::from_cell_with_formula_context(
        &Cell::text("=1+1"),
        false,
        None,
    );
    assert_values_only_literal_undo_redo(explicit_text, 2);

    let marked_text = crate::clipboard::ClipboardCell::from_cell_with_formula_context(
        &Cell::text("'=1+1"),
        false,
        None,
    );
    assert_values_only_literal_undo_redo(marked_text, 3);
}
