//! Pastes that move formula references: copies between cells and cut-and-paste moves.

use super::*;
use std::sync::atomic::AtomicBool;

/// Opens a Markdown fixture as a document whose source is the fixture.
fn markdown_document(text: &str) -> Document {
    let markdown = MarkdownDocument::parse(text.as_bytes().to_vec()).expect("fixture parses");
    let source = Arc::new(SourceDocument::Markdown(Arc::new(markdown)));
    let SourceDocument::Markdown(markdown) = source.as_ref() else {
        unreachable!();
    };
    let mut document = markdown.to_document(91, "move.md");
    document.set_source(Some(Arc::clone(&source)));
    document
}

/// Writes one copied Markdown cell as this process's clipboard data.
fn owned_clipboard(input: &str, origin: CellAddress, text: &str) -> String {
    let cell = Cell::text(input);
    let mut copied = crate::clipboard::ClipboardCell::from_cell_with_formula_context(
        &cell,
        input.starts_with('='),
        None,
    );
    copied.raw_markdown_source = true;
    let internal = vec![vec![copied]];
    let html = crate::clipboard::create_html_clipboard(
        std::slice::from_ref(&internal),
        std::slice::from_ref(&internal),
        &[origin],
        text,
    )
    .expect("clipboard data encodes");
    crate::clipboard::tests::mark_test_owned_html(&html);
    html
}

/// Builds a paste request for the document's only table.
fn paste_request(
    document: &Document,
    text: &str,
    html: String,
    target: CellAddress,
    cut_source: Option<CellRange>,
) -> StructureRequest {
    let table = &document.tables[0];
    let view = document.view(table.id).expect("the table view exists");
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
            text: text.to_owned(),
            html: Some(html),
            options: PasteOptions::default(),
            selection: CellRange::single(target),
            rows: crate::document::MAX_ROWS,
            columns: crate::document::MAX_COLUMNS,
            cut_source,
        }),
        prepared_selection_after: Some(CellRange::single(target)),
        scroll: view.scroll,
        cross_sheet_references: false,
    }
}

/// Returns the Markdown text after a prepared paste.
fn pasted_markdown(request: &StructureRequest) -> String {
    let prepared = prepare_structure(request, &AtomicBool::new(false)).expect("paste prepares");
    let Some(SourceDocument::Markdown(source)) = prepared.source_after.as_deref() else {
        panic!("the paste keeps a Markdown source");
    };
    String::from_utf8(source.source_bytes().to_vec()).expect("UTF-8 Markdown")
}

#[test]
fn pasted_copy_moves_relative_references_with_the_cell() {
    let document = markdown_document("| A | B | C |\n| --- | --- | --- |\n| 1 | 2 | =A2+$B$2 |\n");
    let html = owned_clipboard("=A2+$B$2", CellAddress { row: 1, column: 2 }, "3");
    let request = paste_request(
        &document,
        "3",
        html,
        CellAddress { row: 2, column: 2 },
        None,
    );
    let markdown = pasted_markdown(&request);
    assert!(markdown.contains("=A3+$B$2"), "{markdown}");
    assert!(
        markdown.contains("=A2+$B$2"),
        "the copy keeps its source: {markdown}"
    );
}

#[test]
fn pasting_a_cut_cell_moves_it_and_its_dependents_follow() {
    let document = markdown_document("| A | B |\n| --- | --- |\n| 7 | =A2*2 |\n");
    let source = CellAddress { row: 1, column: 0 };
    let html = owned_clipboard("7", source, "7");
    let target = CellAddress { row: 3, column: 0 };
    let request = paste_request(
        &document,
        "7",
        html,
        target,
        Some(CellRange::single(source)),
    );
    let markdown = pasted_markdown(&request);
    let rows = markdown.lines().collect::<Vec<_>>();
    assert!(rows[2].contains("=A4*2"), "{markdown}");
    assert!(
        !rows[2].contains('7'),
        "the cut cell is emptied: {markdown}"
    );
    assert!(rows[4].contains('7'), "{markdown}");
}

#[test]
fn content_choices_keep_or_drop_formulas_and_formatting_separately() {
    let mut text = crate::clipboard::ClipboardCell::from(&Cell::text("**bold**"));
    text.raw_markdown_source = true;
    text.markdown_plain_text = Some("bold".to_owned());
    let mut formula = crate::clipboard::ClipboardCell::from(&Cell::text("=1+1"));
    formula.formula_source = true;
    // Paste Formulas: formulas stay, formatting goes.
    let formulas_only = PasteOptions {
        strip_formatting: true,
        ..PasteOptions::default()
    };
    let pasted = paste::convert_paste_cell(&text, &formulas_only, true).expect("text pastes");
    assert_eq!(pasted.input, CellInput::Text("bold".to_owned()));
    let pasted = paste::convert_paste_cell(&formula, &formulas_only, true).expect("formula");
    assert_eq!(pasted.input, CellInput::Text("=1+1".to_owned()));
    // Paste Values and Formatting: formulas need a result, formatting stays.
    let values_and_formatting = PasteOptions {
        values_only: true,
        ..PasteOptions::default()
    };
    let pasted =
        paste::convert_paste_cell(&text, &values_and_formatting, true).expect("text pastes");
    assert_eq!(pasted.input, CellInput::Text("**bold**".to_owned()));
    assert!(paste::convert_paste_cell(&formula, &values_and_formatting, true).is_err());
}

#[test]
fn paste_as_string_keeps_external_formula_text_as_text() {
    let as_string = PasteOptions {
        text_only: true,
        values_only: true,
        ..PasteOptions::default()
    };
    let external = crate::clipboard::ClipboardCell::from(&Cell::text("=A1+B1"));
    let pasted = paste::convert_paste_cell(&external, &as_string, false).expect("text pastes");
    assert_eq!(pasted.input, CellInput::Text("'=A1+B1".to_owned()));
    let pasted =
        paste::convert_paste_cell(&external, &PasteOptions::TEXT, false).expect("text pastes");
    assert_eq!(pasted.input, CellInput::Text("=A1+B1".to_owned()));
}

#[test]
fn external_formula_text_stays_a_working_formula_in_markdown() {
    let document = markdown_document("| A |\n| --- |\n| 1 |\n");
    let mut request = paste_request(
        &document,
        "=A2*2",
        String::new(),
        CellAddress { row: 2, column: 0 },
        None,
    );
    request.clipboard_paste.as_mut().expect("paste").html = None;
    let markdown = pasted_markdown(&request);
    // Escaping `*` would leave `=A2\*2`, which the engine cannot read.
    assert!(markdown.contains("| =A2*2 |"), "{markdown}");
}
