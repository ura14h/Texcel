use super::history_validation::validate_table_history;
use super::paste::convert_paste_cell;
use super::*;

/// The Paste Values choices: formula results without formatting.
const VALUES_PASTE: crate::clipboard::PasteOptions = crate::clipboard::PasteOptions {
    text_only: false,
    values_only: true,
    strip_formatting: true,
    transpose: false,
    skip_blank: false,
};

// Create a CSV source fixture for structure operation tests.
fn csv_fixture(bytes: Vec<u8>, operation: StructureOperation) -> (Document, StructureRequest) {
    let csv = crate::csv_adapter::CsvDocument::parse(bytes, EncodingChoice::Auto, ',')
        .expect("CSV fixture parses");
    let source = Arc::new(SourceDocument::Csv(Arc::new(csv)));
    let mut document = match source.as_ref() {
        SourceDocument::Csv(source) => source.to_document(71, "fixture.csv"),
        _ => unreachable!(),
    };
    document.set_source(Some(Arc::clone(&source)));
    let table = &document.tables[0];
    let view = document.view(table.id).expect("table view exists");
    let request = StructureRequest {
        document_id: document.id,
        revision: document.revision,
        calculation_revision: document.calculation_revision,
        source: Some(source),
        operation: Some(operation),
        tables: vec![StructureTableMeta {
            id: table.id,
            name: table.name.clone(),
            rows: table.row_count(),
            columns: table.column_count(),
            string_cells: Arc::new(BTreeSet::new()),
            ..StructureTableMeta::default()
        }],
        changes: Vec::new(),
        text_overlay_cells: Arc::new(Vec::new()),
        text_overlay_revision: 0,
        view_table_id: table.id,
        view_layout: Arc::clone(&view.layout),
        selection: CellRange::single(CellAddress { row: 1, column: 0 }),
        scroll: ScrollPosition::default(),
        cross_sheet_references: false,
        clipboard_paste: None,
        prepared_selection_after: None,
    };
    (document, request)
}

// Create a CSV-backed document from rows of text, as opening a CSV file does.
fn csv_document(id: u64, rows: &[&[&str]]) -> Document {
    let text = rows
        .iter()
        .map(|row| row.join(","))
        .collect::<Vec<_>>()
        .join("\n");
    let csv = crate::csv_adapter::CsvDocument::parse(
        format!("{text}\n").into_bytes(),
        EncodingChoice::Auto,
        ',',
    )
    .expect("CSV fixture parses");
    let mut document = csv.to_document(id, "fixture.csv");
    document.set_source(Some(Arc::new(SourceDocument::Csv(Arc::new(csv)))));
    document
}

// Build a structure request from the current document snapshot.
fn snapshot_structure_request(document: &Document) -> StructureRequest {
    let view_table_id = document.active_table;
    let view = document
        .view(view_table_id)
        .expect("active table view exists");
    StructureRequest {
        document_id: document.id,
        revision: document.revision,
        calculation_revision: document.calculation_revision,
        source: document.source.as_ref().map(Arc::clone),
        operation: None,
        tables: document
            .tables
            .iter()
            .map(|table| StructureTableMeta {
                id: table.id,
                name: table.name.clone(),
                rows: table.row_count(),
                columns: table.column_count(),
                string_cells: table.string_cells_shared(),
                view_layout: document.view(table.id).map_or_else(
                    || Arc::new(TableLayout::default()),
                    |view| Arc::clone(&view.layout),
                ),
                selection: document
                    .view(table.id)
                    .map_or_else(CellRange::default, |view| view.selection),
                scroll: document
                    .view(table.id)
                    .map_or_else(ScrollPosition::default, |view| view.scroll),
            })
            .collect(),
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

// Build a data edit request from a document's current source and unsaved edits.
fn data_edit_request(document: &Document, action: DataEditAction) -> DataEditRequest {
    let table = document.active_table().expect("table exists");
    let view = document.view(table.id).expect("table view exists");
    DataEditRequest {
        structure: StructureRequest {
            document_id: document.id,
            revision: document.revision,
            calculation_revision: document.calculation_revision,
            source: document.source.as_ref().map(Arc::clone),
            operation: None,
            tables: document
                .tables
                .iter()
                .map(|table| StructureTableMeta {
                    id: table.id,
                    name: table.name.clone(),
                    rows: table.row_count(),
                    columns: table.column_count(),
                    string_cells: table.string_cells_shared(),
                    view_layout: document.view(table.id).map_or_else(
                        || Arc::new(TableLayout::default()),
                        |view| Arc::clone(&view.layout),
                    ),
                    selection: document
                        .view(table.id)
                        .map_or_else(CellRange::default, |view| view.selection),
                    scroll: document
                        .view(table.id)
                        .map_or_else(ScrollPosition::default, |view| view.scroll),
                })
                .collect(),
            changes: document.changed_cell_snapshot(),
            text_overlay_cells: document.text_overlay_cells(),
            text_overlay_revision: document.text_overlay_snapshot().revision,
            view_table_id: table.id,
            view_layout: Arc::clone(&view.layout),
            selection: view.selection,
            clipboard_paste: None,
            prepared_selection_after: None,
            scroll: view.scroll,
            cross_sheet_references: false,
        },
        action,
        formula_results_revision: None,
        formula_results: None,
    }
}

// Refresh a structure request with the latest document state.
fn refresh_structure_request(request: &mut StructureRequest, document: &Document) {
    let view = document
        .view(request.view_table_id)
        .expect("request target view exists");
    request.document_id = document.id;
    request.revision = document.revision;
    request.calculation_revision = document.calculation_revision;
    request.source = document.source.as_ref().map(Arc::clone);
    request.tables = document
        .tables
        .iter()
        .map(|table| StructureTableMeta {
            id: table.id,
            name: table.name.clone(),
            rows: table.row_count(),
            columns: table.column_count(),
            string_cells: table.string_cells_shared(),
            view_layout: document.view(table.id).map_or_else(
                || Arc::new(TableLayout::default()),
                |view| Arc::clone(&view.layout),
            ),
            selection: document
                .view(table.id)
                .map_or_else(CellRange::default, |view| view.selection),
            scroll: document
                .view(table.id)
                .map_or_else(ScrollPosition::default, |view| view.scroll),
        })
        .collect();
    request.changes = document.changed_cell_snapshot();
    request.text_overlay_cells = document.text_overlay_cells();
    request.text_overlay_revision = document.text_overlay_snapshot().revision;
    request.view_layout = Arc::clone(&view.layout);
    request.selection = view.selection;
    request.scroll = view.scroll;
}

#[test]
// Verify that search preview uses a snapshot and replacement applies in one undo step.
fn search_preview_uses_snapshot_and_applies_confirmed_replace_as_one_undo() {
    let (mut document, mut structure) = csv_fixture(
        b"Alpha,beta\nalpha,keep\n".to_vec(),
        StructureOperation::Fill {
            table_id: 1,
            source: CellRange::single(CellAddress::default()),
            target: CellRange::single(CellAddress::default()),
            mode: crate::document::FillMode::Copy,
        },
    );
    structure.operation = None;
    let request = SearchRequest {
        language: crate::i18n::Language::English,
        structure,
        query: "alpha".to_owned(),
        replacement: Some("omega".to_owned()),
        ranges: vec![(
            1,
            CellRange {
                anchor: CellAddress::default(),
                head: CellAddress { row: 1, column: 1 },
            },
        )],
        case_sensitive: false,
        regular_expression: false,
        whole_cell: false,
        representation: SearchRepresentation::Source,
        formula_results_revision: None,
        formula_results: None,
    };
    let prepared_search = prepare_search(&request, &AtomicBool::new(false))
        .expect("search and replacement preview prepares");
    assert_eq!(prepared_search.total_matches, 2);
    assert_eq!(prepared_search.hits.len(), 2);
    assert_eq!(
        prepared_search.hits[0].replacement_text.as_deref(),
        Some("omega")
    );
    let prepared = prepared_search
        .prepared
        .expect("replacement root is prepared");
    document
        .execute_prepared_structure(prepared)
        .expect("confirmed replacement uses the document command history");
    let Some(SourceDocument::Csv(source)) = document.source.as_deref() else {
        panic!("CSV source remains the canonical document source");
    };
    assert_eq!(source.source_bytes(), b"omega,beta\nomega,keep\n");
    document
        .undo()
        .expect("one undo restores the pre-replace root");
    let Some(SourceDocument::Csv(source)) = document.source.as_deref() else {
        panic!("CSV source remains after undo");
    };
    assert_eq!(source.source_bytes(), b"Alpha,beta\nalpha,keep\n");
}

#[test]
// Verify that literal replacement keeps dollar signs and regex mode expands captures.
fn non_regex_replacement_keeps_dollar_text_and_regex_mode_expands_captures() {
    let (_document, mut structure) = csv_fixture(
        b"foo\n".to_vec(),
        StructureOperation::Fill {
            table_id: 1,
            source: CellRange::single(CellAddress::default()),
            target: CellRange::single(CellAddress::default()),
            mode: crate::document::FillMode::Copy,
        },
    );
    structure.operation = None;
    let make_request = |query: &str, replacement: &str, regular_expression| SearchRequest {
        language: crate::i18n::Language::English,
        structure: structure.clone(),
        query: query.to_owned(),
        replacement: Some(replacement.to_owned()),
        ranges: vec![(1, CellRange::single(CellAddress::default()))],
        case_sensitive: true,
        regular_expression,
        whole_cell: false,
        representation: SearchRepresentation::Source,
        formula_results_revision: None,
        formula_results: None,
    };
    let literal = prepare_search(
        &make_request("foo", "$5/$USD", false),
        &AtomicBool::new(false),
    )
    .expect("literal replacement prepares");
    assert_eq!(literal.total_matches, 1);
    let Some(SourceDocument::Csv(literal_source)) = literal
        .prepared
        .as_ref()
        .and_then(|root| root.source_after.as_deref())
    else {
        panic!("literal replacement produces a CSV source");
    };
    assert_eq!(literal_source.source_bytes(), b"$5/$USD\n");

    let expanded = prepare_search(
        &make_request("(?P<word>foo)", "${word}-bar", true),
        &AtomicBool::new(false),
    )
    .expect("regex capture replacement prepares");
    let Some(SourceDocument::Csv(expanded_source)) = expanded
        .prepared
        .as_ref()
        .and_then(|root| root.source_after.as_deref())
    else {
        panic!("capture replacement produces a CSV source");
    };
    assert_eq!(expanded_source.source_bytes(), b"foo-bar\n");
}

#[test]
// Verify that overlapping selection ranges are deduplicated before search results are counted.
fn search_deduplicates_overlapping_selection_ranges_before_counting_and_replacing() {
    let (_document, mut structure) = csv_fixture(
        b"foo foo\nfoo\n".to_vec(),
        StructureOperation::Fill {
            table_id: 1,
            source: CellRange::single(CellAddress::default()),
            target: CellRange::single(CellAddress::default()),
            mode: crate::document::FillMode::Copy,
        },
    );
    structure.operation = None;
    let first = CellRange {
        anchor: CellAddress::default(),
        head: CellAddress { row: 1, column: 0 },
    };
    let request = SearchRequest {
        language: crate::i18n::Language::English,
        structure,
        query: "foo".to_owned(),
        replacement: Some("bar".to_owned()),
        ranges: vec![
            (1, first),
            (1, CellRange::single(CellAddress::default())),
            (1, first),
        ],
        case_sensitive: true,
        regular_expression: false,
        whole_cell: false,
        representation: SearchRepresentation::Source,
        formula_results_revision: None,
        formula_results: None,
    };
    let preview = prepare_search(&request, &AtomicBool::new(false))
        .expect("overlapping ranges prepare one snapshot");
    assert_eq!(preview.total_matches, 3);
    assert_eq!(preview.matched_cells, 2);
    assert_eq!(preview.hits.len(), 2);
    let Some(SourceDocument::Csv(source)) = preview
        .prepared
        .as_ref()
        .and_then(|root| root.source_after.as_deref())
    else {
        panic!("replacement produces a CSV source");
    };
    assert_eq!(source.source_bytes(), b"bar bar\nbar\n");
}

#[test]
// Verify that Markdown search replaces text beyond the grid preview limit.
fn markdown_display_search_replaces_tail_beyond_grid_fragment_preview_limit() {
    let middle = "x".repeat(5_000);
    let cell_text = format!("prefix{middle}tailneedle");
    let markdown = Arc::new(
        crate::markdown_adapter::MarkdownDocument::parse(
            format!("| Value |\n| --- |\n| {cell_text} |\n").into_bytes(),
        )
        .expect("long Markdown table parses"),
    );
    let source = Arc::new(SourceDocument::Markdown(Arc::clone(&markdown)));
    let mut document = markdown.to_document(95, "long.md");
    document.set_source(Some(Arc::clone(&source)));
    let structure = snapshot_structure_request(&document);
    let request = SearchRequest {
        language: crate::i18n::Language::English,
        structure,
        query: "tailneedle".to_owned(),
        replacement: Some("tailchanged".to_owned()),
        ranges: vec![(
            document.active_table,
            CellRange {
                anchor: CellAddress { row: 1, column: 0 },
                head: CellAddress { row: 1, column: 0 },
            },
        )],
        case_sensitive: true,
        regular_expression: false,
        whole_cell: false,
        representation: SearchRepresentation::Display,
        formula_results_revision: None,
        formula_results: None,
    };
    let preview = prepare_search(&request, &AtomicBool::new(false))
        .expect("full Markdown display text is searchable");
    assert_eq!(preview.total_matches, 1);
    assert_eq!(preview.hits[0].searched_text, cell_text);
    let Some(SourceDocument::Markdown(after)) = preview
        .prepared
        .as_ref()
        .and_then(|root| root.source_after.as_deref())
    else {
        panic!("Markdown display replacement retains its source format");
    };
    let after_text = std::str::from_utf8(after.source_bytes()).expect("Markdown stays UTF-8");
    assert!(after_text.contains(&format!("prefix{middle}tailchanged")));
}

#[test]
// Verify that search parses heading-like cell text in its table context.
fn markdown_display_search_preserves_table_context_for_heading_like_cell_text() {
    let markdown = Arc::new(
        crate::markdown_adapter::MarkdownDocument::parse(
            b"| Value |\n| --- |\n| # heading |\n".to_vec(),
        )
        .expect("Markdown table parses"),
    );
    let source = Arc::new(SourceDocument::Markdown(Arc::clone(&markdown)));
    let mut document = markdown.to_document(96, "context.md");
    document.set_source(Some(Arc::clone(&source)));
    let address = CellAddress { row: 1, column: 0 };
    let table_id = document.active_table;
    let request = SearchRequest {
        language: crate::i18n::Language::English,
        structure: snapshot_structure_request(&document),
        query: "heading".to_owned(),
        replacement: Some("title".to_owned()),
        ranges: vec![(table_id, CellRange::single(address))],
        case_sensitive: true,
        regular_expression: false,
        whole_cell: false,
        representation: SearchRepresentation::Display,
        formula_results_revision: None,
        formula_results: None,
    };
    let preview = prepare_search(&request, &AtomicBool::new(false))
        .expect("table-context display is searchable");
    assert_eq!(preview.total_matches, 1);
    assert_eq!(preview.hits[0].searched_text, "# heading");
    let Some(SourceDocument::Markdown(after)) = preview
        .prepared
        .as_ref()
        .and_then(|root| root.source_after.as_deref())
    else {
        panic!("replacement retains the Markdown source");
    };
    assert_eq!(after.tables[0].rows[1][0].value, "# title");

    document
        .set_cell(table_id, address, Some(Cell::text("# changed heading")))
        .expect("source edit is tracked before worker search");
    let dirty_request = SearchRequest {
        language: crate::i18n::Language::English,
        structure: snapshot_structure_request(&document),
        query: "heading".to_owned(),
        replacement: Some("label".to_owned()),
        ranges: vec![(table_id, CellRange::single(address))],
        case_sensitive: true,
        regular_expression: false,
        whole_cell: false,
        representation: SearchRepresentation::Display,
        formula_results_revision: None,
        formula_results: None,
    };
    let dirty_preview = prepare_search(&dirty_request, &AtomicBool::new(false))
        .expect("dirty Markdown is materialized before display search");
    assert_eq!(dirty_preview.total_matches, 1);
    assert_eq!(dirty_preview.hits[0].searched_text, "# changed heading");
    let Some(SourceDocument::Markdown(after_dirty)) = dirty_preview
        .prepared
        .as_ref()
        .and_then(|root| root.source_after.as_deref())
    else {
        panic!("dirty replacement retains the Markdown source");
    };
    assert_eq!(after_dirty.tables[0].rows[1][0].value, "# changed label");
}

#[test]
// Verify that invalid and oversized regex patterns are rejected before changes.
fn regex_search_rejects_invalid_or_oversized_patterns_before_editing() {
    let (_document, mut structure) = csv_fixture(
        b"value\n".to_vec(),
        StructureOperation::Fill {
            table_id: 1,
            source: CellRange::single(CellAddress::default()),
            target: CellRange::single(CellAddress::default()),
            mode: crate::document::FillMode::Copy,
        },
    );
    structure.operation = None;
    let request = SearchRequest {
        language: crate::i18n::Language::English,
        structure,
        query: "(".to_owned(),
        replacement: Some("x".to_owned()),
        ranges: vec![(1, CellRange::single(CellAddress::default()))],
        case_sensitive: false,
        regular_expression: true,
        whole_cell: false,
        representation: SearchRepresentation::Source,
        formula_results_revision: None,
        formula_results: None,
    };
    assert!(
        prepare_search(&request, &AtomicBool::new(false))
            .unwrap_err()
            .resolve(crate::i18n::Language::Japanese)
            .contains("正規表現が不正")
    );
}

#[test]
// Verify that preview limits count cells separately from repeated matches.
fn search_preview_cap_counts_cells_separately_from_match_occurrences() {
    let single_cell = format!("{}\n", "a".repeat(10_001));
    let (_document, mut structure) = csv_fixture(
        single_cell.into_bytes(),
        StructureOperation::Fill {
            table_id: 1,
            source: CellRange::single(CellAddress::default()),
            target: CellRange::single(CellAddress::default()),
            mode: crate::document::FillMode::Copy,
        },
    );
    structure.operation = None;
    let request = SearchRequest {
        language: crate::i18n::Language::English,
        structure,
        query: "a".to_owned(),
        replacement: None,
        ranges: vec![(1, CellRange::single(CellAddress::default()))],
        case_sensitive: true,
        regular_expression: false,
        whole_cell: false,
        representation: SearchRepresentation::Source,
        formula_results_revision: None,
        formula_results: None,
    };
    let preview =
        prepare_search(&request, &AtomicBool::new(false)).expect("long one-cell search prepares");
    assert_eq!(preview.total_matches, 10_001);
    assert_eq!(preview.matched_cells, 1);
    assert_eq!(preview.hits.len(), 1);
    assert!(!preview.hits_truncated);

    let mut many_cells = String::with_capacity(20_002);
    for _ in 0..10_001 {
        many_cells.push_str("x\n");
    }
    let (_document, mut structure) = csv_fixture(
        many_cells.into_bytes(),
        StructureOperation::Fill {
            table_id: 1,
            source: CellRange::single(CellAddress::default()),
            target: CellRange::single(CellAddress::default()),
            mode: crate::document::FillMode::Copy,
        },
    );
    structure.operation = None;
    let request = SearchRequest {
        language: crate::i18n::Language::English,
        structure,
        query: "x".to_owned(),
        replacement: Some("y".to_owned()),
        ranges: vec![(
            1,
            CellRange {
                anchor: CellAddress::default(),
                head: CellAddress {
                    row: 10_000,
                    column: 0,
                },
            },
        )],
        case_sensitive: true,
        regular_expression: false,
        whole_cell: false,
        representation: SearchRepresentation::Source,
        formula_results_revision: None,
        formula_results: None,
    };
    let preview = prepare_search(&request, &AtomicBool::new(false))
        .expect("multi-cell result preview prepares");
    assert_eq!(preview.total_matches, 10_001);
    assert_eq!(preview.matched_cells, 10_001);
    assert_eq!(preview.hits.len(), 10_000);
    assert!(preview.hits_truncated);
    let Some(SourceDocument::Csv(source)) = preview
        .prepared
        .as_ref()
        .and_then(|root| root.source_after.as_deref())
    else {
        panic!("all search replacements are prepared in the CSV source");
    };
    assert_eq!(source.source_bytes(), "y\n".repeat(10_001).as_bytes());
}

#[test]
// Verify that stable multi-column sorting rewrites CSV and undo restores its bytes.
fn stable_multicolumn_sort_rewrites_csv_and_undo_restores_source_bytes() {
    let original =
        b"key,value,note\nb,2,first\na,1,early\na,1,late\nb,2,second\n,3,blank\na,2,mid\n";
    let (mut document, mut structure) = csv_fixture(
        original.to_vec(),
        StructureOperation::Fill {
            table_id: 1,
            source: CellRange::single(CellAddress::default()),
            target: CellRange::single(CellAddress::default()),
            mode: crate::document::FillMode::Copy,
        },
    );
    structure.operation = None;
    let request = DataEditRequest {
        structure,
        action: DataEditAction::SortRows {
            table_id: 1,
            keys: vec![
                SortKey {
                    column: 0,
                    kind: SortValueKind::Text,
                    descending: false,
                },
                SortKey {
                    column: 1,
                    kind: SortValueKind::Number,
                    descending: true,
                },
            ],
            header_row: Some(0),
        },
        formula_results_revision: None,
        formula_results: None,
    };
    let prepared = prepare_data_edit(&request, &AtomicBool::new(false))
        .expect("worker prepares stable full-row sort");
    assert_eq!(prepared.changed_rows, Some(4));
    document
        .execute_prepared_structure(prepared.prepared.expect("sort root is prepared"))
        .expect("one prepared root applies the sorted table");
    let Some(SourceDocument::Csv(source)) = document.source.as_deref() else {
        panic!("CSV source remains canonical");
    };
    assert_eq!(
        source.source_bytes(),
        b"key,value,note\na,2,mid\na,1,early\na,1,late\nb,2,first\nb,2,second\n,3,blank\n"
    );
    document.undo().expect("sort is one undo command");
    let Some(SourceDocument::Csv(source)) = document.source.as_deref() else {
        panic!("CSV source remains after Undo");
    };
    assert_eq!(source.source_bytes(), original);
}

#[test]
// Sort loaded CSV text numerically without reclassifying identifiers as numbers.
fn csv_number_sort_uses_the_shared_source_number_rule() {
    let (_document, mut structure) = csv_fixture(
        b"id,note\n00123,leading\n+5,plus\n12345678901234567890,long\n1200,regular\n20,small\n"
            .to_vec(),
        StructureOperation::Fill {
            table_id: 1,
            source: CellRange::single(CellAddress::default()),
            target: CellRange::single(CellAddress::default()),
            mode: crate::document::FillMode::Copy,
        },
    );
    structure.operation = None;
    let request = DataEditRequest {
        structure,
        action: DataEditAction::SortRows {
            table_id: 1,
            keys: vec![SortKey {
                column: 0,
                kind: SortValueKind::Number,
                descending: false,
            }],
            header_row: Some(0),
        },
        formula_results_revision: None,
        formula_results: None,
    };

    let prepared = prepare_data_edit(&request, &AtomicBool::new(false))
        .expect("numeric sort prepares on the file worker path");
    let Some(SourceDocument::Csv(source)) = prepared
        .prepared
        .as_ref()
        .and_then(|root| root.source_after.as_deref())
    else {
        panic!("sorted CSV source remains available");
    };
    assert_eq!(
        source.source_bytes(),
        b"id,note\n20,small\n1200,regular\n00123,leading\n+5,plus\n12345678901234567890,long\n"
    );
}

#[test]
// Sort Markdown cells by their visible text while preserving original cell markup.
fn markdown_number_sort_uses_the_plain_display_projection() {
    let markdown = Arc::new(
        crate::markdown_adapter::MarkdownDocument::parse(
            b"| id | note |\n| --- | --- |\n| **20** | bold |\n| [20](https://example.test) | link |\n| **00123** | identifier |\n| 5 | small |\n".to_vec(),
        )
        .expect("Markdown fixture parses"),
    );
    let mut document = markdown.to_document(93, "numeric-sort.md");
    document.set_source(Some(Arc::new(SourceDocument::Markdown(Arc::clone(
        &markdown,
    )))));
    let request = DataEditRequest {
        structure: snapshot_structure_request(&document),
        action: DataEditAction::SortRows {
            table_id: document.tables[0].id,
            keys: vec![SortKey {
                column: 0,
                kind: SortValueKind::Number,
                descending: false,
            }],
            header_row: Some(0),
        },
        formula_results_revision: None,
        formula_results: None,
    };

    let prepared = prepare_data_edit(&request, &AtomicBool::new(false))
        .expect("numeric Markdown sort uses its visible-cell worker projection");
    let Some(SourceDocument::Markdown(sorted)) = prepared
        .prepared
        .as_ref()
        .and_then(|root| root.source_after.as_deref())
    else {
        panic!("sorted Markdown source remains available");
    };
    assert_eq!(
        sorted.tables[0].rows[1..]
            .iter()
            .map(|row| row[0].value.as_str())
            .collect::<Vec<_>>(),
        ["5", "**20**", "[20](https://example.test)", "**00123**"]
    );
}

#[test]
// Verify that formula sorting skips headers and orders errors between values and blanks.
fn formula_sort_skips_header_and_keeps_errors_between_valid_values_and_blanks() {
    let (document, mut structure) = csv_fixture(
        b"=1/0,header\n=1/0,error\n=1,valid\n,blank\n".to_vec(),
        StructureOperation::Fill {
            table_id: 1,
            source: CellRange::single(CellAddress::default()),
            target: CellRange::single(CellAddress::default()),
            mode: crate::document::FillMode::Copy,
        },
    );
    structure.operation = None;
    let formulas: crate::calc::FormulaResultSnapshot =
        Arc::new(crate::calc::FormulaResultRoot::new(
            structure.calculation_revision,
            vec![
                (
                    1,
                    CellAddress { row: 1, column: 0 },
                    crate::document::CalcStatus::Error,
                    Some(crate::document::CalcValue::Error("#DIV/0!".to_owned())),
                ),
                (
                    1,
                    CellAddress { row: 2, column: 0 },
                    crate::document::CalcStatus::Fresh,
                    Some(crate::document::CalcValue::Number(1.0)),
                ),
            ],
        ));
    for descending in [false, true] {
        let request = DataEditRequest {
            structure: structure.clone(),
            action: DataEditAction::SortRows {
                table_id: 1,
                keys: vec![SortKey {
                    column: 0,
                    kind: SortValueKind::Number,
                    descending,
                }],
                header_row: Some(0),
            },
            formula_results_revision: Some(structure.calculation_revision),
            formula_results: Some(Arc::clone(&formulas)),
        };
        let prepared = prepare_data_edit(&request, &AtomicBool::new(false))
            .expect("header formula is ignored and same-version errors sort as invalid");
        let Some(SourceDocument::Csv(source)) = prepared
            .prepared
            .as_ref()
            .and_then(|root| root.source_after.as_deref())
        else {
            panic!("formula sort keeps the CSV source");
        };
        assert_eq!(
            source.source_bytes(),
            b"=1/0,header\n=1,valid\n=1/0,error\n,blank\n"
        );
    }
    assert_eq!(document.revision, structure.revision);
}

#[test]
// Verify that sorting ignores an unsupported header formula but rejects moving it.
fn row_sort_ignores_unsupported_header_formula_but_rejects_a_moved_one() {
    let (_document, mut structure) = csv_fixture(
        b"key,=RAND()\nb,header\na,header\n".to_vec(),
        StructureOperation::Fill {
            table_id: 1,
            source: CellRange::single(CellAddress::default()),
            target: CellRange::single(CellAddress::default()),
            mode: crate::document::FillMode::Copy,
        },
    );
    structure.operation = None;
    let request = DataEditRequest {
        structure: structure.clone(),
        action: DataEditAction::SortRows {
            table_id: 1,
            keys: vec![SortKey {
                column: 0,
                kind: SortValueKind::Text,
                descending: false,
            }],
            header_row: Some(0),
        },
        formula_results_revision: None,
        formula_results: None,
    };
    let prepared = prepare_data_edit(&request, &AtomicBool::new(false))
        .expect("unsupported formula in a fixed header does not block sorting");
    let Some(SourceDocument::Csv(source)) = prepared
        .prepared
        .as_ref()
        .and_then(|root| root.source_after.as_deref())
    else {
        panic!("sort keeps the CSV source");
    };
    assert_eq!(source.source_bytes(), b"key,=RAND()\na,header\nb,header\n");

    let (document, mut structure) = csv_fixture(
        b"key,formula\nb,=RAND()\na,=1\n".to_vec(),
        StructureOperation::Fill {
            table_id: 1,
            source: CellRange::single(CellAddress::default()),
            target: CellRange::single(CellAddress::default()),
            mode: crate::document::FillMode::Copy,
        },
    );
    structure.operation = None;
    let request = DataEditRequest {
        structure,
        action: DataEditAction::SortRows {
            table_id: 1,
            keys: vec![SortKey {
                column: 0,
                kind: SortValueKind::Text,
                descending: false,
            }],
            header_row: Some(0),
        },
        formula_results_revision: None,
        formula_results: None,
    };
    let error = prepare_data_edit(&request, &AtomicBool::new(false)).unwrap_err();
    let message = error.resolve(crate::i18n::Language::Japanese);
    assert!(message.contains("B2"), "{message}");
    assert!(message.contains("参照を安全に更新できません"), "{message}");
    assert_eq!(document.revision, request.structure.revision);
}

#[test]
// Verify that row sorting updates formulas within moved rows.
fn row_sort_updates_formula_references_inside_moved_rows() {
    let (mut document, mut structure) = csv_fixture(
        b"key,value,formula\nb,2,=B2\na,1,=B3\n".to_vec(),
        StructureOperation::Fill {
            table_id: 1,
            source: CellRange::single(CellAddress::default()),
            target: CellRange::single(CellAddress::default()),
            mode: crate::document::FillMode::Copy,
        },
    );
    structure.operation = None;
    let request = DataEditRequest {
        structure,
        action: DataEditAction::SortRows {
            table_id: 1,
            keys: vec![SortKey {
                column: 0,
                kind: SortValueKind::Text,
                descending: false,
            }],
            header_row: Some(0),
        },
        formula_results_revision: None,
        formula_results: None,
    };
    let prepared = prepare_data_edit(&request, &AtomicBool::new(false))
        .expect("formula-aware row sort prepares");
    document
        .execute_prepared_structure(prepared.prepared.expect("sort root is prepared"))
        .expect("formula-aware row sort installs as one root");
    let Some(SourceDocument::Csv(source)) = document.source.as_deref() else {
        panic!("CSV source remains canonical");
    };
    assert_eq!(
        source.source_bytes(),
        b"key,value,formula\na,1,=B2\nb,2,=B3\n"
    );
    document
        .undo()
        .expect("Undo restores original formula rows");
    let Some(SourceDocument::Csv(source)) = document.source.as_deref() else {
        panic!("CSV source remains after Undo");
    };
    assert_eq!(
        source.source_bytes(),
        b"key,value,formula\nb,2,=B2\na,1,=B3\n"
    );
}

#[test]
// Verify that duplicate row removal preserves irregular CSV source and is undoable.
fn duplicate_detection_and_csv_row_removal_preserve_irregular_source_and_undo() {
    let original = b"id,value,extra\nA,one,x\nA,one\nB,two,z\n";
    let (mut document, mut structure) = csv_fixture(
        original.to_vec(),
        StructureOperation::Fill {
            table_id: 1,
            source: CellRange::single(CellAddress::default()),
            target: CellRange::single(CellAddress::default()),
            mode: crate::document::FillMode::Copy,
        },
    );
    structure.operation = None;
    let range = CellRange {
        anchor: CellAddress { row: 0, column: 0 },
        head: CellAddress { row: 3, column: 1 },
    };
    let detect = DataEditRequest {
        structure: structure.clone(),
        action: DataEditAction::Duplicates {
            table_id: 1,
            range,
            header_row: Some(0),
            delete: false,
        },
        formula_results_revision: None,
        formula_results: None,
    };
    let detected = prepare_data_edit(&detect, &AtomicBool::new(false))
        .expect("duplicate detection prepares without an edit");
    assert_eq!(detected.duplicate_count, 1);
    assert_eq!(detected.duplicate_groups, 1);
    assert!(detected.prepared.is_none());
    assert_eq!(
        document.source.as_deref().and_then(|source| match source {
            SourceDocument::Csv(source) => Some(source.source_bytes()),
            _ => None,
        }),
        Some(original.as_slice())
    );

    let remove = DataEditRequest {
        structure,
        action: DataEditAction::Duplicates {
            table_id: 1,
            range,
            header_row: Some(0),
            delete: true,
        },
        formula_results_revision: None,
        formula_results: None,
    };
    let prepared = prepare_data_edit(&remove, &AtomicBool::new(false))
        .expect("duplicate deletion prepares a single row plan");
    assert_eq!(prepared.changed_rows, Some(1));
    document
        .execute_prepared_structure(
            prepared
                .prepared
                .expect("duplicate removal root is prepared"),
        )
        .expect("duplicate deletion applies as one common undo");
    let Some(SourceDocument::Csv(source)) = document.source.as_deref() else {
        panic!("CSV source remains canonical");
    };
    assert_eq!(source.source_bytes(), b"id,value,extra\nA,one,x\nB,two,z\n");
    document
        .undo()
        .expect("one undo restores duplicate row bytes");
    let Some(SourceDocument::Csv(source)) = document.source.as_deref() else {
        panic!("CSV source remains after undo");
    };
    assert_eq!(source.source_bytes(), original);
}

#[test]
// Verify that duplicate removal updates retained formula references and undo restores them.
fn duplicate_row_removal_updates_retained_formula_coordinates_and_undoes() {
    let original = b"key,value,formula\nb,2,=A4\na,1,=B3\na,1,=B4\n";
    let (mut document, mut structure) = csv_fixture(
        original.to_vec(),
        StructureOperation::Fill {
            table_id: 1,
            source: CellRange::single(CellAddress::default()),
            target: CellRange::single(CellAddress::default()),
            mode: crate::document::FillMode::Copy,
        },
    );
    structure.operation = None;
    let request = DataEditRequest {
        structure,
        action: DataEditAction::Duplicates {
            table_id: 1,
            range: CellRange {
                anchor: CellAddress { row: 0, column: 0 },
                head: CellAddress { row: 3, column: 1 },
            },
            header_row: Some(0),
            delete: true,
        },
        formula_results_revision: None,
        formula_results: None,
    };
    let prepared = prepare_data_edit(&request, &AtomicBool::new(false))
        .expect("formula-bearing duplicate rows prepare");
    document
        .execute_prepared_structure(prepared.prepared.expect("duplicate row plan is prepared"))
        .expect("formula-bearing row removal uses one structure root");
    let Some(SourceDocument::Csv(source)) = document.source.as_deref() else {
        panic!("CSV source remains canonical");
    };
    assert_eq!(
        source.source_bytes(),
        b"key,value,formula\nb,2,=#REF!\na,1,=B3\n"
    );
    document
        .undo()
        .expect("duplicate removal undo restores formula rows");
    let Some(SourceDocument::Csv(source)) = document.source.as_deref() else {
        panic!("CSV source remains after undo");
    };
    assert_eq!(source.source_bytes(), original);
}

#[test]
// Verify that trim, split, and join edits restore source bytes on undo.
fn trim_split_and_join_prepare_diffs_and_restore_source_bytes() {
    let original = b"raw,tail,out,extra\n  alpha|beta  ,keep,old,old2\n";
    let (mut document, mut structure) = csv_fixture(
        original.to_vec(),
        StructureOperation::Fill {
            table_id: 1,
            source: CellRange::single(CellAddress::default()),
            target: CellRange::single(CellAddress::default()),
            mode: crate::document::FillMode::Copy,
        },
    );
    structure.operation = None;
    let request = DataEditRequest {
        structure: structure.clone(),
        action: DataEditAction::TrimWhitespace {
            ranges: vec![(
                1,
                CellRange {
                    anchor: CellAddress { row: 1, column: 0 },
                    head: CellAddress { row: 1, column: 0 },
                },
            )],
        },
        formula_results_revision: None,
        formula_results: None,
    };
    let prepared = prepare_data_edit(&request, &AtomicBool::new(false))
        .expect("worker trims only the selected source text");
    document
        .execute_prepared_structure(prepared.prepared.expect("trim root is prepared"))
        .expect("trim uses one undo root");
    let Some(SourceDocument::Csv(source)) = document.source.as_deref() else {
        panic!("CSV source remains canonical");
    };
    assert_eq!(
        source.source_bytes(),
        b"raw,tail,out,extra\nalpha|beta,keep,old,old2\n"
    );
    document.undo().expect("trim undo restores exact CSV bytes");
    assert_eq!(
        document.source.as_deref().and_then(|source| match source {
            SourceDocument::Csv(source) => Some(source.source_bytes()),
            _ => None,
        }),
        Some(original.as_slice())
    );
    refresh_structure_request(&mut structure, &document);

    let make_split = |overwrite| DataEditRequest {
        structure: structure.clone(),
        action: DataEditAction::SplitColumn {
            table_id: 1,
            rows: CellRange {
                anchor: CellAddress { row: 1, column: 0 },
                head: CellAddress { row: 1, column: 0 },
            },
            source_column: 0,
            delimiter: "|".to_owned(),
            destination_column: 2,
            overwrite,
        },
        formula_results_revision: None,
        formula_results: None,
    };
    let rejected = prepare_data_edit(&make_split(false), &AtomicBool::new(false));
    assert!(
        rejected
            .unwrap_err()
            .resolve(crate::i18n::Language::Japanese)
            .contains("既存値または書式")
    );
    let split = prepare_data_edit(&make_split(true), &AtomicBool::new(false))
        .expect("confirmed split protects the source and replaces destinations");
    document
        .execute_prepared_structure(split.prepared.expect("split root is prepared"))
        .expect("split uses one undo root");
    let Some(SourceDocument::Csv(source)) = document.source.as_deref() else {
        panic!("CSV source remains canonical");
    };
    assert_eq!(
        source.source_bytes(),
        b"raw,tail,out,extra\n  alpha|beta  ,keep,  alpha,beta  \n"
    );
    document
        .undo()
        .expect("split undo restores irregular source bytes");
    assert_eq!(
        document.source.as_deref().and_then(|source| match source {
            SourceDocument::Csv(source) => Some(source.source_bytes()),
            _ => None,
        }),
        Some(original.as_slice())
    );
    refresh_structure_request(&mut structure, &document);

    let make_join = |overwrite| DataEditRequest {
        structure: structure.clone(),
        action: DataEditAction::JoinColumns {
            table_id: 1,
            range: CellRange {
                anchor: CellAddress { row: 1, column: 0 },
                head: CellAddress { row: 1, column: 1 },
            },
            delimiter: "+".to_owned(),
            destination_column: 2,
            overwrite,
        },
        formula_results_revision: None,
        formula_results: None,
    };
    let rejected = prepare_data_edit(&make_join(false), &AtomicBool::new(false));
    assert!(
        rejected
            .unwrap_err()
            .resolve(crate::i18n::Language::Japanese)
            .contains("既存値または書式")
    );
    let join = prepare_data_edit(&make_join(true), &AtomicBool::new(false))
        .expect("confirmed join uses source strings without displaying formulas");
    document
        .execute_prepared_structure(join.prepared.expect("join root is prepared"))
        .expect("join uses one undo root");
    let Some(SourceDocument::Csv(source)) = document.source.as_deref() else {
        panic!("CSV source remains canonical");
    };
    assert_eq!(
        source.source_bytes(),
        b"raw,tail,out,extra\n  alpha|beta  ,keep,  alpha|beta  +keep,old2\n"
    );
    document
        .undo()
        .expect("join undo restores exact input bytes");
    assert_eq!(
        document.source.as_deref().and_then(|source| match source {
            SourceDocument::Csv(source) => Some(source.source_bytes()),
            _ => None,
        }),
        Some(original.as_slice())
    );
}

#[test]
// Verify that transpose needs destination confirmation and the change is undoable.
fn transpose_rejects_unconfirmed_destination_and_undoes_confirmed_change() {
    let mut document = csv_document(
        82,
        &[&["a", "b", "c"], &["d", "e", "f"], &["occupied", "", ""]],
    );
    let range = CellRange {
        anchor: CellAddress { row: 0, column: 0 },
        head: CellAddress { row: 1, column: 2 },
    };
    let make_transpose = |overwrite| {
        data_edit_request(
            &document,
            DataEditAction::Transpose {
                table_id: 1,
                range,
                overwrite,
            },
        )
    };
    assert!(
        prepare_data_edit(&make_transpose(false), &AtomicBool::new(false))
            .unwrap_err()
            .resolve(crate::i18n::Language::Japanese)
            .contains("既存値または書式")
    );
    let transpose = prepare_data_edit(&make_transpose(true), &AtomicBool::new(false))
        .expect("confirmed transpose prepares explicit source/destination diff");
    document
        .execute_prepared_structure(transpose.prepared.expect("transpose root is prepared"))
        .expect("transpose uses one undo root");
    let table = &document.tables[0];
    assert_eq!(
        table.cell(CellAddress { row: 0, column: 0 }).unwrap().input,
        CellInput::Text("a".to_owned())
    );
    assert_eq!(
        table.cell(CellAddress { row: 0, column: 1 }).unwrap().input,
        CellInput::Text("d".to_owned())
    );
    assert_eq!(
        table.cell(CellAddress { row: 1, column: 0 }).unwrap().input,
        CellInput::Text("b".to_owned())
    );
    assert_eq!(
        table.cell(CellAddress { row: 2, column: 1 }).unwrap().input,
        CellInput::Text("f".to_owned())
    );
    assert_eq!(
        table.cell(CellAddress { row: 2, column: 0 }).unwrap().input,
        CellInput::Text("c".to_owned())
    );
    document
        .undo()
        .expect("one undo restores the overwritten destination and input");
    assert_eq!(
        document.tables[0]
            .cell(CellAddress { row: 2, column: 0 })
            .unwrap()
            .input,
        CellInput::Text("occupied".to_owned())
    );
    assert_eq!(
        document.tables[0]
            .cell(CellAddress { row: 0, column: 2 })
            .unwrap()
            .input,
        CellInput::Text("c".to_owned())
    );
}

// Build a request for a worksheet management operation.
fn table_management_request(document: &Document) -> StructureRequest {
    let source = Arc::clone(document.source.as_ref().expect("fixture has a source"));
    let view = document
        .view(document.active_table)
        .expect("active table has a view");
    StructureRequest {
        document_id: document.id,
        revision: document.revision,
        calculation_revision: document.calculation_revision,
        source: Some(source.clone()),
        operation: None,
        tables: document
            .tables
            .iter()
            .map(|table| {
                let table_view = document.view(table.id);
                StructureTableMeta {
                    id: table.id,
                    name: table.name.clone(),
                    rows: table.row_count(),
                    columns: table.column_count(),
                    string_cells: table.string_cells_shared(),
                    view_layout: table_view.map_or_else(
                        || Arc::new(TableLayout::default()),
                        |view| Arc::clone(&view.layout),
                    ),
                    selection: table_view.map_or_else(CellRange::default, |view| view.selection),
                    scroll: table_view.map_or_else(Default::default, |view| view.scroll),
                }
            })
            .collect(),
        changes: document.changed_cell_snapshot(),
        text_overlay_cells: document.text_overlay_cells(),
        text_overlay_revision: document.text_overlay_snapshot().revision,
        view_table_id: document.active_table,
        view_layout: Arc::clone(&view.layout),
        selection: view.selection,
        scroll: view.scroll,
        cross_sheet_references: false,
        clipboard_paste: None,
        prepared_selection_after: None,
    }
}

// Create a CSV document with an unsaved edit for paste tests.
fn paste_fixture(text: &str, selection: CellRange) -> (Document, StructureRequest) {
    let mut document = csv_document(91, &[&["base"], &["hidden"]]);
    document
        .set_cells(
            1,
            vec![(
                CellAddress { row: 0, column: 0 },
                Some(crate::document::Cell::text("before-paste")),
            )],
        )
        .expect("pre-paste edit applies");
    let view = document.view_mut(1).expect("table view exists");
    view.set_selection(selection);
    let table = &document.tables[0];
    let view = document.view(1).expect("table view exists");
    let request = StructureRequest {
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
        selection,
        clipboard_paste: Some(ClipboardPasteRequest {
            cut_source: None,
            text: text.to_owned(),
            html: None,
            options: PasteOptions::default(),
            selection,
            rows: crate::document::MAX_ROWS,
            columns: crate::document::MAX_COLUMNS,
        }),
        prepared_selection_after: Some(selection),
        scroll: view.scroll,
        cross_sheet_references: false,
    };
    (document, request)
}

// Build a CSV-backed clipboard paste request.
fn csv_paste_request(
    document: &Document,
    text: &str,
    html: Option<String>,
    selection: CellRange,
) -> StructureRequest {
    let table = document
        .active_table()
        .expect("CSV document has an active table");
    let view = document.view(table.id).expect("CSV table has a view");
    StructureRequest {
        document_id: document.id,
        revision: document.revision,
        calculation_revision: document.calculation_revision,
        source: document.source.as_ref().map(Arc::clone),
        operation: None,
        tables: document
            .tables
            .iter()
            .map(|table| StructureTableMeta {
                id: table.id,
                name: table.name.clone(),
                rows: table.row_count(),
                columns: table.column_count(),
                string_cells: table.string_cells_shared(),
                ..StructureTableMeta::default()
            })
            .collect(),
        changes: document.changed_cell_snapshot(),
        text_overlay_cells: document.text_overlay_cells(),
        text_overlay_revision: document.text_overlay_snapshot().revision,
        view_table_id: table.id,
        view_layout: Arc::clone(&view.layout),
        selection,
        clipboard_paste: Some(ClipboardPasteRequest {
            cut_source: None,
            text: text.to_owned(),
            html,
            options: PasteOptions::default(),
            selection,
            rows: table.row_count(),
            columns: table.column_count(),
        }),
        prepared_selection_after: Some(selection),
        scroll: view.scroll,
        cross_sheet_references: false,
    }
}

#[test]
// Verify stable CSV table IDs keep staged edits and pasted values in the source.
fn csv_source_with_non_one_table_id_keeps_edits_and_paste() {
    let selection = CellRange::single(CellAddress { row: 0, column: 1 });
    let (mut document, mut request) = csv_fixture(
        b"before,neighbor\n".to_vec(),
        StructureOperation::Insert {
            table_id: 1,
            axis: StructureAxis::Rows,
            index: 0,
            count: 1,
        },
    );
    let old_id = document.tables[0].id;
    let view = document.views.remove(&old_id).expect("CSV view exists");
    let stable_id = 2;
    document.tables[0].id = stable_id;
    document.views.insert(stable_id, view);
    document.active_table = stable_id;
    request.tables[0].id = stable_id;
    request.view_table_id = stable_id;
    request.selection = selection;
    request.changes.push((
        stable_id,
        CellAddress { row: 0, column: 0 },
        Some(crate::document::Cell::text("staged")),
    ));
    request.clipboard_paste = Some(ClipboardPasteRequest {
        cut_source: None,
        text: "pasted".to_owned(),
        html: None,
        options: PasteOptions::default(),
        selection,
        rows: document.tables[0].row_count(),
        columns: document.tables[0].column_count(),
    });
    request.prepared_selection_after = Some(selection);

    let prepared = prepare_structure(&request, &AtomicBool::new(false))
        .expect("worker applies the staged change and paste");
    let Some(SourceDocument::Csv(source)) = prepared.source_after.as_deref() else {
        panic!("CSV source stays attached to the stable table ID");
    };
    assert_eq!(source.records[0].fields[0].value, "staged");
    assert_eq!(source.records[0].fields[1].value, "pasted");
    document
        .execute_prepared_structure(prepared)
        .expect("the prepared changes apply");
    assert_eq!(document.tables[0].id, stable_id);
}

#[test]
// Verify that clipboard parsing runs on a worker and the paste survives undo and redo.
fn clipboard_paste_parses_on_worker_and_survives_undo_redo() {
    let selection = CellRange::single(CellAddress { row: 0, column: 1 });
    let (mut document, request) = paste_fixture("x\t\"two\tparts\"", selection);
    let prepared =
        prepare_structure(&request, &AtomicBool::new(false)).expect("worker expands quoted TSV");
    document
        .execute_prepared_structure(prepared)
        .expect("one root-swap paste applies");
    assert_eq!(
        document.tables[0]
            .cell(CellAddress { row: 0, column: 0 })
            .unwrap()
            .input
            .source_text(),
        "before-paste"
    );
    assert_eq!(
        document.tables[0]
            .cell(CellAddress { row: 0, column: 1 })
            .unwrap()
            .input
            .source_text(),
        "x"
    );
    assert_eq!(
        document.tables[0]
            .cell(CellAddress { row: 0, column: 2 })
            .unwrap()
            .input
            .source_text(),
        "two\tparts"
    );
    document.undo().expect("one undo reverts the whole paste");
    assert_eq!(
        document.tables[0]
            .cell(CellAddress { row: 0, column: 0 })
            .unwrap()
            .input
            .source_text(),
        "before-paste"
    );
    assert!(
        document.tables[0]
            .cell(CellAddress { row: 0, column: 1 })
            .is_none()
    );
    document.redo().expect("redo restores the whole paste");
    assert_eq!(
        document.tables[0]
            .cell(CellAddress { row: 0, column: 2 })
            .unwrap()
            .input
            .source_text(),
        "two\tparts"
    );
}

#[test]
// Keep pasted text-table dimensions equal to source and stored cell bounds.
fn markdown_paste_extent_ends_at_the_source_or_pasted_cell() {
    let markdown = crate::markdown_adapter::MarkdownDocument::parse(
        b"| Name | Value |\n| --- | --- |\n| alpha | one |\n".to_vec(),
    )
    .expect("the Markdown fixture parses");
    let source = Arc::new(SourceDocument::Markdown(Arc::new(markdown.clone())));
    let mut document = markdown.to_document(901, "paste extent.md");
    document.set_source(Some(source));
    document.tables[0].set_dimensions(10, 10);
    let address = CellAddress { row: 4, column: 3 };
    let request = csv_paste_request(&document, "pasted", None, CellRange::single(address));

    let prepared = prepare_structure(&request, &AtomicBool::new(false))
        .expect("the sparse Markdown paste is prepared");
    document
        .execute_prepared_structure(prepared)
        .expect("the paste and dimensions apply together");

    let table = &document.tables[0];
    assert_eq!(table.row_count(), 5);
    assert_eq!(table.column_count(), 4);
    assert_eq!(document.extent_end(table.id), Some(address));
    assert_eq!(
        table.cell(address).map(|cell| cell.input.source_text()),
        Some("pasted".to_owned())
    );
    let Some(SourceDocument::Markdown(saved)) = document.source.as_deref() else {
        panic!("the Markdown source remains attached");
    };
    assert_eq!(saved.tables[0].rows.len(), 5);
    assert_eq!(saved.parser_width(0), Some(4));
}

#[test]
// Pastes only clipboard-sized content when the selection covers the whole sheet.
fn one_cell_paste_into_full_sheet_uses_payload_size() {
    let full_sheet = CellRange {
        anchor: CellAddress::default(),
        head: CellAddress {
            row: crate::document::MAX_ROWS - 1,
            column: crate::document::MAX_COLUMNS - 1,
        },
    };
    let (mut document, request) = paste_fixture("fill", full_sheet);
    let prepared = prepare_structure(&request, &AtomicBool::new(false))
        .expect("a full-sheet selection uses the one-cell clipboard payload");
    document
        .execute_prepared_structure(prepared)
        .expect("the bounded paste applies through the structure path");
    assert_eq!(
        document.tables[0]
            .cell(CellAddress::default())
            .unwrap()
            .input
            .source_text(),
        "fill"
    );
    assert_eq!(
        document.tables[0]
            .cell(CellAddress { row: 1, column: 0 })
            .unwrap()
            .input
            .source_text(),
        "hidden",
        "the whole-sheet selection does not repeat one value over the sheet"
    );
    assert_eq!(document.tables[0].stored_cells().count(), 2);
}

#[test]
// Verify that formatted paste promotes CSV and writes only supported Markdown features.
fn text_source_paste_keeps_markdown_modifier_and_drops_xlsx_only_styles() {
    let source = Arc::new(SourceDocument::Csv(Arc::new(
        crate::csv_adapter::CsvDocument::parse(
            b"a,**literal**\n".to_vec(),
            EncodingChoice::Auto,
            ',',
        )
        .expect("CSV fixture parses"),
    )));
    let mut document = match source.as_ref() {
        SourceDocument::Csv(source) => source.to_document(72, "styles.csv"),
        _ => unreachable!(),
    };
    document.set_source(Some(Arc::clone(&source)));
    let mut rich = crate::clipboard::ClipboardCell::from(&crate::document::Cell::text("strong"));
    rich.style.bold = true;
    rich.style.font_color = Some("#ff0000".to_owned());
    rich.style.fill_color = Some("#ffff00".to_owned());
    rich.style.number_format = Some("0.00".to_owned());
    rich.style.horizontal_alignment = Some("center".to_owned());
    rich.style.top_border = Some(crate::document::CellBorder {
        style: "thin".to_owned(),
        color: Some("#000000".to_owned()),
    });
    let rich_matrix = vec![vec![rich]];
    let rich_range = std::slice::from_ref(&rich_matrix);
    let html = crate::clipboard::create_html_clipboard(rich_range, rich_range, &[], "strong")
        .expect("internal rich HTML is valid");
    crate::clipboard::tests::mark_test_owned_html(&html);

    let first_selection = CellRange::single(CellAddress { row: 0, column: 0 });
    let first = csv_paste_request(&document, "strong", Some(html), first_selection);
    let prepared = prepare_structure(&first, &AtomicBool::new(false)).expect("rich paste prepares");
    let pasted = prepared.tables[0]
        .cell(CellAddress { row: 0, column: 0 })
        .unwrap();
    assert_eq!(pasted.input.source_text(), "**strong**");
    // Markdown keeps the bold marker in the source, so the table holds no imported formatting.
    assert!(
        prepared.tables[0]
            .format(CellAddress { row: 0, column: 0 })
            .is_none()
    );
    assert!(matches!(
        prepared.source_after.as_deref(),
        Some(SourceDocument::Markdown(_))
    ));
    assert_eq!(
        prepared.tables[0]
            .cell(CellAddress { row: 0, column: 0 })
            .unwrap()
            .input
            .source_text(),
        "**strong**"
    );
    assert_eq!(
        prepared.tables[0]
            .cell(CellAddress { row: 0, column: 1 })
            .unwrap()
            .input
            .source_text(),
        "\\*\\*literal\\*\\*"
    );
    let SourceDocument::Markdown(pasted_source) = prepared.source_after.as_deref().unwrap() else {
        unreachable!();
    };
    assert!(
        std::str::from_utf8(pasted_source.source_bytes())
            .unwrap()
            .contains("**strong**")
    );
    assert_eq!(
        pasted_source.tables[0].rows[0][1].value,
        prepared.tables[0]
            .cell(CellAddress { row: 0, column: 1 })
            .unwrap()
            .input
            .source_text()
    );
    let reparsed = MarkdownDocument::parse(pasted_source.source_bytes().to_vec())
        .expect("formatted paste remains valid Markdown");
    assert!(reparsed.cell_display(0, 0, 0).unwrap().runs[0].bold);
    let displayed_unpasted = reparsed.cell_display(0, 0, 1).unwrap();
    let displayed_unpasted = displayed_unpasted
        .runs
        .iter()
        .filter_map(|run| run.text_for_language(Some(&reparsed), crate::i18n::Language::English))
        .map(|text| text.into_owned())
        .collect::<String>();
    assert_eq!(displayed_unpasted, "**literal**");
    document
        .execute_prepared_structure(prepared)
        .expect("rich paste applies");
    assert!(matches!(
        document.source.as_deref(),
        Some(SourceDocument::Markdown(_))
    ));
    assert_eq!(
        document.tables[0]
            .cell(CellAddress { row: 0, column: 0 })
            .unwrap()
            .input
            .source_text(),
        "**strong**"
    );
    let mut roundtrip = document.clone();
    roundtrip
        .set_cell(
            roundtrip.active_table,
            CellAddress { row: 0, column: 0 },
            Some(Cell::text("**strong**!")),
        )
        .expect("one raw character edit applies");
    let roundtrip_request = snapshot_structure_request(&roundtrip);
    let materialized = materialize_current_source(
        &roundtrip_request,
        roundtrip_request
            .source
            .as_deref()
            .expect("Markdown source exists"),
    )
    .expect("edited source materializes on the worker");
    let SourceDocument::Markdown(materialized) = materialized else {
        unreachable!();
    };
    let reloaded = source_to_document(
        &SourceDocument::Markdown(materialized),
        roundtrip.id,
        &[roundtrip.active_table],
    )
    .expect("edited Markdown source reloads with the stable table ID");
    assert_eq!(
        reloaded.tables[0]
            .cell(CellAddress { row: 0, column: 0 })
            .unwrap()
            .input
            .source_text(),
        "**strong**!"
    );

    let second_selection = CellRange::single(CellAddress { row: 0, column: 1 });
    let second = csv_paste_request(&document, "plain", None, second_selection);
    document
        .execute_prepared_structure(
            prepare_structure(&second, &AtomicBool::new(false))
                .expect("text paste preserves text-source overlay"),
        )
        .expect("text paste applies");
    assert_eq!(
        document.tables[0]
            .cell(CellAddress { row: 0, column: 0 })
            .unwrap()
            .input
            .source_text(),
        "**strong**"
    );
    assert_eq!(
        document.tables[0]
            .cell(CellAddress { row: 0, column: 1 })
            .unwrap()
            .input
            .source_text(),
        "plain"
    );
    document.undo().expect("second paste has one undo");
    assert_eq!(
        document.tables[0]
            .cell(CellAddress { row: 0, column: 0 })
            .unwrap()
            .input
            .source_text(),
        "**strong**"
    );
    document.redo().expect("redo restores second paste");
    assert!(matches!(
        document.source.as_deref(),
        Some(SourceDocument::Markdown(_))
    ));
    document.undo().expect("undo second paste first");
    document
        .undo()
        .expect("one undo restores CSV before the formatted paste");
    assert!(matches!(
        document.source.as_deref(),
        Some(SourceDocument::Csv(_))
    ));
    crate::clipboard::tests::mark_test_owned_html("");
}

#[test]
// Verify Markdown-owned markup stays raw while external HTML text stays literal.
fn markdown_paste_preserves_owned_source_and_escapes_html_text() {
    let markdown = MarkdownDocument::parse(b"| Name |\n| --- |\n| start |\n".to_vec())
        .expect("fixture parses");
    let source = Arc::new(SourceDocument::Markdown(Arc::new(markdown)));
    let SourceDocument::Markdown(markdown) = source.as_ref() else {
        unreachable!();
    };
    let mut document = markdown.to_document(73, "paste.md");
    document.set_source(Some(Arc::clone(&source)));
    let target = CellAddress { row: 1, column: 0 };
    let selection = CellRange::single(target);

    let mut source_cell = crate::clipboard::ClipboardCell::from(&Cell::text("**word**"));
    source_cell.raw_markdown_source = true;
    let internal = vec![vec![source_cell]];
    let internal_range = std::slice::from_ref(&internal);
    let owned_html =
        crate::clipboard::create_html_clipboard(internal_range, internal_range, &[], "**word**")
            .expect("internal Markdown payload encodes");
    crate::clipboard::tests::mark_test_owned_html(&owned_html);
    let mut owned_request = snapshot_structure_request(&document);
    owned_request.clipboard_paste = Some(ClipboardPasteRequest {
        cut_source: None,
        text: "**word**".to_owned(),
        html: Some(owned_html),
        options: PasteOptions::default(),
        selection,
        rows: document.tables[0].row_count(),
        columns: document.tables[0].column_count(),
    });
    let owned = prepare_structure(&owned_request, &AtomicBool::new(false))
        .expect("owned Markdown source pastes");
    let SourceDocument::Markdown(owned_source) = owned.source_after.as_deref().unwrap() else {
        unreachable!();
    };
    assert!(
        std::str::from_utf8(owned_source.source_bytes())
            .unwrap()
            .contains("**word**")
    );

    let mut external_request = snapshot_structure_request(&document);
    external_request.clipboard_paste = Some(ClipboardPasteRequest {
        cut_source: None,
        text: "<b>A</b>".to_owned(),
        html: Some("<table><tr><td>&lt;b&gt;A&lt;/b&gt;</td></tr></table>".to_owned()),
        options: PasteOptions::default(),
        selection,
        rows: document.tables[0].row_count(),
        columns: document.tables[0].column_count(),
    });
    let external = prepare_structure(&external_request, &AtomicBool::new(false))
        .expect("external HTML text pastes as plain content");
    let SourceDocument::Markdown(external_source) = external.source_after.as_deref().unwrap()
    else {
        unreachable!();
    };
    let reparsed = MarkdownDocument::parse(external_source.source_bytes().to_vec())
        .expect("escaped HTML text remains valid Markdown");
    let display = reparsed.cell_display(0, target.row, target.column).unwrap();
    let display_text = display
        .runs
        .iter()
        .filter_map(|run| run.text_for_language(Some(&reparsed), crate::i18n::Language::English))
        .map(|text| text.into_owned())
        .collect::<String>();
    assert_eq!(display_text, "<b>A</b>");
    assert!(display.runs.iter().all(|run| !run.bold));

    let mut rich_request = snapshot_structure_request(&document);
    rich_request.clipboard_paste = Some(ClipboardPasteRequest {
        cut_source: None,
        text: "Styled".to_owned(),
        html: Some("<table><tr><td><strong>Styled</strong></td></tr></table>".to_owned()),
        options: PasteOptions::default(),
        selection,
        rows: document.tables[0].row_count(),
        columns: document.tables[0].column_count(),
    });
    let rich = prepare_structure(&rich_request, &AtomicBool::new(false))
        .expect("rich HTML modifiers become Markdown source");
    let SourceDocument::Markdown(rich_source) = rich.source_after.as_deref().unwrap() else {
        unreachable!();
    };
    let rich_markdown = MarkdownDocument::parse(rich_source.source_bytes().to_vec())
        .expect("rich HTML paste remains valid Markdown");
    assert!(
        std::str::from_utf8(rich_source.source_bytes())
            .unwrap()
            .contains("**Styled**")
    );
    assert!(
        rich_markdown
            .cell_display(0, target.row, target.column)
            .unwrap()
            .runs[0]
            .bold
    );
    assert_eq!(
        rich.tables[0].cell(target).unwrap().input.source_text(),
        "**Styled**"
    );

    let mut checked_request = snapshot_structure_request(&document);
    checked_request.clipboard_paste = Some(ClipboardPasteRequest {
        cut_source: None,
        text: "A（字）B safe".to_owned(),
        html: Some(
            "<table><tr><td>A<strong>（字）</strong>B <em>safe</em></td></tr></table>".to_owned(),
        ),
        options: PasteOptions::default(),
        selection,
        rows: document.tables[0].row_count(),
        columns: document.tables[0].column_count(),
    });
    let checked = prepare_structure(&checked_request, &AtomicBool::new(false))
        .expect("unsafe full-width modifier is safely omitted");
    assert_eq!(
        checked.status.as_ref().map(crate::i18n::Message::key),
        Some("status-paste-markdown-formatting-omitted")
    );
    assert!(checked.conversion_report.is_none());
    let SourceDocument::Markdown(checked_source) = checked.source_after.as_deref().unwrap() else {
        unreachable!();
    };
    let checked_text = std::str::from_utf8(checked_source.source_bytes()).unwrap();
    assert!(
        checked_text.contains("A（字）B&#32;*safe*"),
        "{checked_text}"
    );
    let checked_markdown =
        MarkdownDocument::parse(checked_source.source_bytes().to_vec()).expect("output parses");
    let checked_display = checked_markdown
        .cell_display(0, target.row, target.column)
        .unwrap();
    let checked_plain = checked_display
        .runs
        .iter()
        .filter_map(|run| {
            run.text_for_language(Some(&checked_markdown), crate::i18n::Language::English)
        })
        .map(|text| text.into_owned())
        .collect::<String>();
    assert_eq!(checked_plain, "A（字）B safe");
    assert!(checked_display.runs.iter().all(|run| !run.bold));
    assert!(checked_display.runs.iter().any(|run| run.italic));
    crate::clipboard::tests::mark_test_owned_html("");
}

#[test]
// Verify owned Markdown paste separates source markup from its rendered value.
fn markdown_owned_paste_promotes_csv_but_values_only_keeps_plain_literal_text() {
    fn make_owned_clipboard() -> String {
        let mut source_cell = crate::clipboard::ClipboardCell::from(&Cell::text("'=**語**"));
        source_cell.raw_markdown_source = true;
        source_cell.markdown_plain_text = Some("=語".to_owned());
        source_cell.markdown_has_features = true;
        let internal = vec![vec![source_cell]];
        let visible = vec![vec![crate::clipboard::ClipboardCell::from(&Cell::text(
            "=語",
        ))]];
        let html = crate::clipboard::create_html_clipboard(
            std::slice::from_ref(&internal),
            std::slice::from_ref(&visible),
            &[],
            "=語",
        )
        .expect("owned Markdown clipboard encodes");
        crate::clipboard::tests::mark_test_owned_html(&html);
        html
    }

    let source = Arc::new(SourceDocument::Csv(Arc::new(
        crate::csv_adapter::CsvDocument::parse(b"old\n".to_vec(), EncodingChoice::Auto, ',')
            .expect("CSV fixture parses"),
    )));
    let mut document = match source.as_ref() {
        SourceDocument::Csv(source) => source.to_document(74, "owned.csv"),
        _ => unreachable!(),
    };
    document.set_source(Some(Arc::clone(&source)));
    let table_id = document.active_table;
    let selection = CellRange::single(CellAddress { row: 0, column: 0 });
    let html = make_owned_clipboard();
    let request = csv_paste_request(&document, "=語", Some(html), selection);
    let prepared = prepare_structure(&request, &AtomicBool::new(false))
        .expect("formatted Markdown paste promotes CSV");
    assert!(matches!(
        prepared.source_after.as_deref(),
        Some(SourceDocument::Markdown(_))
    ));
    let SourceDocument::Markdown(source) = prepared.source_after.as_deref().unwrap() else {
        unreachable!();
    };
    let source_text = std::str::from_utf8(source.source_bytes()).unwrap();
    assert!(source_text.contains("'=**語**"));
    assert_eq!(prepared.tables[0].id, table_id);
    assert_eq!(
        prepared.tables[0]
            .cell(selection.anchor)
            .unwrap()
            .input
            .source_text(),
        "'=**語**"
    );
    crate::clipboard::tests::mark_test_owned_html("");

    let source = Arc::new(SourceDocument::Csv(Arc::new(
        crate::csv_adapter::CsvDocument::parse(b"old\n".to_vec(), EncodingChoice::Auto, ',')
            .expect("CSV fixture parses"),
    )));
    let mut document = match source.as_ref() {
        SourceDocument::Csv(source) => source.to_document(75, "values.csv"),
        _ => unreachable!(),
    };
    document.set_source(Some(Arc::clone(&source)));
    let html = make_owned_clipboard();
    let mut request = csv_paste_request(&document, "=語", Some(html), selection);
    request
        .clipboard_paste
        .as_mut()
        .expect("paste request exists")
        .options = VALUES_PASTE;
    let prepared =
        prepare_structure(&request, &AtomicBool::new(false)).expect("values-only paste keeps CSV");
    let SourceDocument::Csv(source) = prepared.source_after.as_deref().unwrap() else {
        panic!("values-only paste must not promote the CSV");
    };
    assert_eq!(source.records[0].fields[0].value, "'=語");
    assert_eq!(
        crate::document::literal_text_value(&source.records[0].fields[0].value),
        "=語"
    );
    assert_eq!(
        prepared.status.as_ref().map(crate::i18n::Message::key),
        None
    );
    crate::clipboard::tests::mark_test_owned_html("");
}

#[test]
// Verify that values-only paste uses the fresh formula result and drops formatting.
fn values_only_uses_fresh_formula_value() {
    let mut formula =
        crate::document::Cell::formula("=1+1", Some(crate::document::CalcValue::Number(2.0)));
    let mut copied = crate::clipboard::ClipboardCell::from_cell(&formula, 0);
    let value = convert_paste_cell(&copied, &VALUES_PASTE, true)
        .expect("fresh result can be pasted as a value");
    assert_eq!(value.input, crate::document::CellInput::Number(2.0));

    let text_formula = crate::document::Cell::formula(
        "=\"=1+1\"",
        Some(crate::document::CalcValue::Text("=1+1".to_owned())),
    );
    let text_copied = crate::clipboard::ClipboardCell::from_cell(&text_formula, 0);
    let literal = convert_paste_cell(&text_copied, &VALUES_PASTE, true)
        .expect("formula text result can be pasted as a value");
    assert_eq!(
        literal.input,
        crate::document::CellInput::Text("'=1+1".to_owned())
    );
    assert_eq!(
        crate::document::literal_text_value(literal.input.source_text().as_str()),
        "=1+1"
    );
    assert_eq!(Cell::text("'=1+1").source_text_for_text_file(true), "'=1+1");
    let literal_source = literal.input.source_text();
    let csv = crate::csv_adapter::CsvDocument::parse(
        b"old\n".to_vec(),
        crate::csv_adapter::EncodingChoice::Auto,
        ',',
    )
    .expect("CSV source parses");
    let csv = csv
        .apply_changes(&[crate::csv_adapter::CsvCellChange {
            row: 0,
            column: 0,
            value: Some(literal_source.clone()),
        }])
        .expect("CSV output accepts the encoded text");
    let csv_document = csv.to_document(90, "literal.csv");
    let csv_cell = csv_document.tables[0]
        .cell(CellAddress::default())
        .expect("CSV reload keeps the value");
    assert_eq!(csv_cell.input.source_text(), "'=1+1");
    assert!(crate::calc::formula_source(&csv_cell.input, false).is_none());

    let markdown =
        crate::markdown_adapter::MarkdownDocument::parse(b"| value |\n| --- |\n| old |\n".to_vec())
            .expect("Markdown source parses");
    let markdown = markdown
        .apply_changes(&[crate::markdown_adapter::MarkdownCellChange {
            table: 0,
            row: 1,
            column: 0,
            value: literal_source,
        }])
        .expect("Markdown output accepts the encoded text");
    let markdown_document = markdown.to_document(91, "literal.md");
    let markdown_cell = markdown_document.tables[0]
        .cell(CellAddress { row: 1, column: 0 })
        .expect("Markdown reload keeps the value");
    assert_eq!(markdown_cell.input.source_text(), "'=1+1");
    assert!(crate::calc::formula_source(&markdown_cell.input, false).is_none());

    formula.calculation.as_mut().unwrap().status = crate::document::CalcStatus::Pending;
    copied = crate::clipboard::ClipboardCell::from_cell(&formula, 0);
    assert!(
        convert_paste_cell(&copied, &VALUES_PASTE, true)
            .unwrap_err()
            .resolve(crate::i18n::Language::Japanese)
            .contains("数式に最新の計算結果がありません")
    );
    formula.calculation.as_mut().unwrap().revision = 99;
    copied = crate::clipboard::ClipboardCell::from_cell(&formula, 0);
    assert!(convert_paste_cell(&copied, &VALUES_PASTE, true).is_err());
}

#[test]
// Verify that row insertion preserves CSV shape through shared undo and redo.
fn row_insert_preserves_csv_shape_and_shared_undo_redo() {
    let original = b"name,value\r\nA,1\r\nB,2\r\n".to_vec();
    let (mut document, request) = csv_fixture(
        original.clone(),
        StructureOperation::Insert {
            table_id: 1,
            axis: StructureAxis::Rows,
            index: 1,
            count: 1,
        },
    );
    let prepared =
        prepare_structure(&request, &AtomicBool::new(false)).expect("structure prepares");
    assert!(prepared.history_file.is_none());
    let SourceDocument::Csv(after) = prepared.source_after.as_deref().unwrap() else {
        panic!("CSV source remains CSV");
    };
    assert_eq!(after.source_bytes(), b"name,value\r\n,\r\nA,1\r\nB,2\r\n");
    let ((table_id, _), retired) = document
        .execute_prepared_structure(prepared)
        .expect("prepared root swaps");
    assert!(retired.is_none());
    assert_eq!(
        document.tables[0]
            .cell(CellAddress { row: 2, column: 0 })
            .unwrap()
            .input,
        crate::document::CellInput::Text("A".to_owned())
    );
    document.mark_saved(document.revision);
    assert_eq!(
        document.undo().unwrap(),
        Some((table_id, CellAddress { row: 1, column: 0 }))
    );
    let SourceDocument::Csv(source) = document.source.as_deref().unwrap() else {
        panic!("undo restores CSV source");
    };
    assert_eq!(source.source_bytes(), original);
    document.mark_saved(document.revision);
    assert_eq!(
        document.redo().unwrap(),
        Some((table_id, CellAddress { row: 1, column: 0 }))
    );
    assert_eq!(
        document.tables[0]
            .cell(CellAddress { row: 2, column: 0 })
            .unwrap()
            .input,
        crate::document::CellInput::Text("A".to_owned())
    );
}

#[test]
// Verify that row and column operations retain the selected item order.
fn row_and_column_structure_operations_keep_the_selected_order() {
    let cases = [
        (
            b"a,1\r\nb,2\r\nc,3\r\n".to_vec(),
            StructureOperation::Delete {
                table_id: 1,
                axis: StructureAxis::Rows,
                index: 1,
                count: 1,
            },
            b"a,1\r\nc,3\r\n".to_vec(),
        ),
        (
            b"a,1\r\nb,2\r\nc,3\r\n".to_vec(),
            StructureOperation::Duplicate {
                table_id: 1,
                axis: StructureAxis::Rows,
                index: 1,
                count: 1,
            },
            b"a,1\r\nb,2\r\nb,2\r\nc,3\r\n".to_vec(),
        ),
        (
            b"a,1\r\nb,2\r\nc,3\r\n".to_vec(),
            StructureOperation::Move {
                table_id: 1,
                axis: StructureAxis::Rows,
                index: 0,
                count: 1,
                destination: 1,
            },
            b"b,2\r\na,1\r\nc,3\r\n".to_vec(),
        ),
        (
            b"a,b,c\r\n1,2,3\r\n".to_vec(),
            StructureOperation::Delete {
                table_id: 1,
                axis: StructureAxis::Columns,
                index: 1,
                count: 1,
            },
            b"a,c\r\n1,3\r\n".to_vec(),
        ),
        (
            b"a,b,c\r\n1,2,3\r\n".to_vec(),
            StructureOperation::Duplicate {
                table_id: 1,
                axis: StructureAxis::Columns,
                index: 1,
                count: 1,
            },
            b"a,b,b,c\r\n1,2,2,3\r\n".to_vec(),
        ),
        (
            b"a,b,c\r\n1,2,3\r\n".to_vec(),
            StructureOperation::Move {
                table_id: 1,
                axis: StructureAxis::Columns,
                index: 0,
                count: 1,
                destination: 1,
            },
            b"b,a,c\r\n2,1,3\r\n".to_vec(),
        ),
    ];
    for (source, operation, expected) in cases {
        let (_document, request) = csv_fixture(source, operation.clone());
        let prepared = prepare_structure(&request, &AtomicBool::new(false))
            .unwrap_or_else(|error| panic!("{operation:?}: {error}"));
        let SourceDocument::Csv(after) = prepared.source_after.as_deref().unwrap() else {
            panic!("source format remains CSV");
        };
        assert_eq!(after.source_bytes(), expected, "{operation:?}");
    }
}

#[test]
// Verify that formula references change only when the calculation engine moves their target.
fn formula_references_rewrite_only_when_the_engine_moves_the_target() {
    let source = b"1,=A3\r\n2,\"=SUM(1,2)\"\r\n4,6\r\n".to_vec();
    let (document, request) = csv_fixture(
        source,
        StructureOperation::Insert {
            table_id: 1,
            axis: StructureAxis::Rows,
            index: 1,
            count: 1,
        },
    );
    let prepared = prepare_structure(&request, &AtomicBool::new(false))
        .expect("supported formulas can be structurally rewritten");
    let SourceDocument::Csv(after) = prepared.source_after.as_deref().unwrap() else {
        panic!("CSV source remains CSV");
    };
    assert!(
        after
            .source_bytes()
            .starts_with(b"1,=A4\r\n,\r\n2,\"=SUM(1,2)\""),
        "actual CSV: {:?}",
        after.source_bytes()
    );
    let moved_formula = prepared.tables[0]
        .cell(CellAddress { row: 0, column: 1 })
        .expect("formula cell remains present");
    assert_eq!(moved_formula.input.source_text(), "=A4");
    let untouched_formula = prepared.tables[0]
        .cell(CellAddress { row: 2, column: 1 })
        .expect("unaffected formula cell remains present");
    assert_eq!(untouched_formula.input.source_text(), "=SUM(1,2)");
    assert_eq!(document.revision, request.revision);
}

#[test]
// Verify that inserted columns rewrite formulas through the calculation engine.
fn inserted_column_rewrites_references_through_ironcalc() {
    let (document, request) = csv_fixture(
        b"1,=C1,3\r\n".to_vec(),
        StructureOperation::Insert {
            table_id: 1,
            axis: StructureAxis::Columns,
            index: 1,
            count: 1,
        },
    );
    let prepared = prepare_structure(&request, &AtomicBool::new(false))
        .expect("supported formulas can be rewritten after column insertion");
    let SourceDocument::Csv(after) = prepared.source_after.as_deref().unwrap() else {
        panic!("CSV source remains CSV");
    };
    assert_eq!(after.source_bytes(), b"1,,=D1,3\r\n");
    assert_eq!(
        prepared.tables[0]
            .cell(CellAddress { row: 0, column: 2 })
            .expect("formula moves with its column")
            .input
            .source_text(),
        "=D1"
    );
    assert_eq!(document.revision, request.revision);
}

#[test]
// Verify that fill copies translate formula references and update CSV source.
fn fill_copy_translates_formula_references_and_updates_csv_source() {
    let (document, request) = csv_fixture(
        b"id,=A1\r\n1,\r\n2,\r\n".to_vec(),
        StructureOperation::Fill {
            table_id: 1,
            source: CellRange::single(CellAddress { row: 0, column: 1 }),
            target: CellRange {
                anchor: CellAddress { row: 0, column: 1 },
                head: CellAddress { row: 2, column: 1 },
            },
            mode: crate::document::FillMode::Copy,
        },
    );
    let prepared = prepare_structure(&request, &AtomicBool::new(false))
        .expect("formula copies use IronCalc reference translation");
    let SourceDocument::Csv(after) = prepared.source_after.as_deref().unwrap() else {
        panic!("CSV source remains CSV");
    };
    assert_eq!(after.source_bytes(), b"id,=A1\r\n1,=A2\r\n2,=A3\r\n");
    assert_eq!(
        prepared.tables[0]
            .cell(CellAddress { row: 2, column: 1 })
            .expect("copied formula is present")
            .input
            .source_text(),
        "=A3"
    );
    assert_eq!(document.revision, request.revision);
}

#[test]
// Verify that explicit series fill keeps leading zeros and rejects ambiguous text.
fn explicit_series_preserves_leading_zero_text_and_rejects_ambiguous_text() {
    let (document, request) = csv_fixture(
        b"Item001\nItem002\n".to_vec(),
        StructureOperation::Fill {
            table_id: 1,
            source: CellRange {
                anchor: CellAddress { row: 0, column: 0 },
                head: CellAddress { row: 1, column: 0 },
            },
            target: CellRange {
                anchor: CellAddress { row: 0, column: 0 },
                head: CellAddress { row: 3, column: 0 },
            },
            mode: crate::document::FillMode::Series,
        },
    );
    let prepared = prepare_structure(&request, &AtomicBool::new(false))
        .expect("explicit suffix series is unambiguous");
    let SourceDocument::Csv(after) = prepared.source_after.as_deref().unwrap() else {
        panic!("CSV source remains CSV");
    };
    assert_eq!(after.records[2].fields[0].value, "Item003");
    assert_eq!(after.records[3].fields[0].value, "Item004");

    let (_document, ambiguous) = csv_fixture(
        b"alpha\nbeta\n".to_vec(),
        StructureOperation::Fill {
            table_id: 1,
            source: CellRange {
                anchor: CellAddress { row: 0, column: 0 },
                head: CellAddress { row: 1, column: 0 },
            },
            target: CellRange {
                anchor: CellAddress { row: 0, column: 0 },
                head: CellAddress { row: 2, column: 0 },
            },
            mode: crate::document::FillMode::Series,
        },
    );
    assert!(prepare_structure(&ambiguous, &AtomicBool::new(false)).is_err());
    assert_eq!(document.revision, request.revision);
}

#[test]
// Verify that series fill advances dates and numeric text before trailing suffixes.
fn explicit_series_advances_calendar_dates_and_numeric_text_before_suffixes() {
    let fill = |input: &[u8]| {
        let (_document, request) = csv_fixture(
            input.to_vec(),
            StructureOperation::Fill {
                table_id: 1,
                source: CellRange {
                    anchor: CellAddress { row: 0, column: 0 },
                    head: CellAddress { row: 1, column: 0 },
                },
                target: CellRange {
                    anchor: CellAddress { row: 0, column: 0 },
                    head: CellAddress { row: 2, column: 0 },
                },
                mode: crate::document::FillMode::Series,
            },
        );
        prepare_structure(&request, &AtomicBool::new(false)).expect("explicit text series prepares")
    };
    let dates = fill(b"2026-11-29\n2026-11-30\n");
    let SourceDocument::Csv(dates) = dates.source_after.as_deref().unwrap() else {
        panic!("CSV source remains CSV");
    };
    assert_eq!(dates.records[2].fields[0].value, "2026-12-01");

    let decimals = fill(b"1.5\n2.0\n");
    let SourceDocument::Csv(decimals) = decimals.source_after.as_deref().unwrap() else {
        panic!("CSV source remains CSV");
    };
    assert_eq!(decimals.records[2].fields[0].value, "2.5");
}

#[test]
// Verify that invalid date seeds fail without changing the document or history.
fn invalid_iso_date_seeds_are_rejected_without_mutating_document_or_history() {
    let (mut document, request) = csv_fixture(
        b"2026-11-30\n2026-11-31\n".to_vec(),
        StructureOperation::Fill {
            table_id: 1,
            source: CellRange {
                anchor: CellAddress { row: 0, column: 0 },
                head: CellAddress { row: 1, column: 0 },
            },
            target: CellRange {
                anchor: CellAddress { row: 0, column: 0 },
                head: CellAddress { row: 2, column: 0 },
            },
            mode: crate::document::FillMode::Series,
        },
    );
    let revision = document.revision;
    let SourceDocument::Csv(before) = document.source.as_deref().unwrap() else {
        panic!("CSV source remains CSV");
    };
    let original_bytes = before.source_bytes().to_vec();
    let error = prepare_structure(&request, &AtomicBool::new(false))
        .expect_err("invalid calendar dates cannot fall back to a numeric suffix");
    assert!(
        error
            .resolve(crate::i18n::Language::Japanese)
            .contains("YYYY-MM-DD形式"),
        "{error}"
    );
    assert_eq!(document.revision, revision);
    assert!(document.changed_cell_snapshot().is_empty());
    assert!(
        document
            .undo()
            .expect("empty history remains safe")
            .is_none()
    );
    let SourceDocument::Csv(after) = document.source.as_deref().unwrap() else {
        panic!("CSV source remains CSV");
    };
    assert_eq!(after.source_bytes(), original_bytes);

    let prefix = prepare_structure(
        &csv_fixture(
            b"Date2026-11-30\nDate2026-11-31\n".to_vec(),
            StructureOperation::Fill {
                table_id: 1,
                source: CellRange {
                    anchor: CellAddress { row: 0, column: 0 },
                    head: CellAddress { row: 1, column: 0 },
                },
                target: CellRange {
                    anchor: CellAddress { row: 0, column: 0 },
                    head: CellAddress { row: 2, column: 0 },
                },
                mode: crate::document::FillMode::Series,
            },
        )
        .1,
        &AtomicBool::new(false),
    )
    .expect("ordinary prefix plus suffix series remains supported");
    let SourceDocument::Csv(prefix) = prefix.source_after.as_deref().unwrap() else {
        panic!("CSV source remains CSV");
    };
    assert_eq!(prefix.records[2].fields[0].value, "Date2026-11-32");
}

#[test]
// Verify that fill before the seed uses signed repeat offsets in the worker.
fn fill_before_seed_uses_signed_repeat_offsets_in_worker() {
    for target in [
        CellRange {
            anchor: CellAddress { row: 0, column: 1 },
            head: CellAddress { row: 1, column: 1 },
        },
        CellRange {
            anchor: CellAddress { row: 1, column: 0 },
            head: CellAddress { row: 1, column: 1 },
        },
    ] {
        let (_document, request) = csv_fixture(
            b"a,b\n,c\n".to_vec(),
            StructureOperation::Fill {
                table_id: 1,
                source: CellRange::single(CellAddress { row: 1, column: 1 }),
                target,
                mode: crate::document::FillMode::Copy,
            },
        );
        let prepared = prepare_structure(&request, &AtomicBool::new(false))
            .expect("upward or leftward fill does not underflow");
        let SourceDocument::Csv(after) = prepared.source_after.as_deref().unwrap() else {
            panic!("CSV source remains CSV");
        };
        if target.anchor.row == 0 {
            assert_eq!(after.records[0].fields[1].value, "c");
        } else {
            assert_eq!(after.records[1].fields[0].value, "c");
        }
    }
}

#[test]
// Verify that a copied formula keeps its original spacing when the engine makes no change.
fn copied_formula_with_unchanged_engine_formula_keeps_original_spacing() {
    let (_document, request) = csv_fixture(
        b"\"=SUM( 1 , 2 )\"\n\n".to_vec(),
        StructureOperation::Fill {
            table_id: 1,
            source: CellRange::single(CellAddress { row: 0, column: 0 }),
            target: CellRange {
                anchor: CellAddress { row: 0, column: 0 },
                head: CellAddress { row: 1, column: 0 },
            },
            mode: crate::document::FillMode::Copy,
        },
    );
    let prepared = prepare_structure(&request, &AtomicBool::new(false))
        .expect("constant formula copies safely");
    let copied = prepared.tables[0]
        .cell(CellAddress { row: 1, column: 0 })
        .expect("formula is copied");
    assert_eq!(copied.input.source_text(), "=SUM( 1 , 2 )");
    let SourceDocument::Csv(after) = prepared.source_after.as_deref().unwrap() else {
        panic!("CSV source remains CSV");
    };
    assert_eq!(after.records[1].fields[0].value, "=SUM( 1 , 2 )");
}

#[test]
// Verify that Markdown row sorting keeps extra raw cells.
fn markdown_row_reordering_keeps_surplus_raw_cells() {
    let original = b"# A heading that is much longer than thirty one characters\n\n| a | b |\n| --- | --- |\n| x | y | **retained** |\n| z | w |\n";
    for operation in [
        StructureOperation::Insert {
            table_id: 1,
            axis: StructureAxis::Rows,
            index: 1,
            count: 1,
        },
        StructureOperation::Move {
            table_id: 1,
            axis: StructureAxis::Rows,
            index: 1,
            count: 1,
            destination: 2,
        },
    ] {
        let parsed = MarkdownDocument::parse(original.to_vec()).expect("Markdown parses");
        let source = Arc::new(SourceDocument::Markdown(Arc::new(parsed)));
        let mut document = match source.as_ref() {
            SourceDocument::Markdown(markdown) => markdown.to_document(74, "fixture.md"),
            _ => unreachable!(),
        };
        document.set_source(Some(Arc::clone(&source)));
        let table = &document.tables[0];
        let view = document.view(table.id).unwrap();
        let request = StructureRequest {
            document_id: document.id,
            revision: document.revision,
            calculation_revision: document.calculation_revision,
            source: Some(source),
            operation: Some(operation),
            tables: vec![StructureTableMeta {
                id: table.id,
                name: table.name.clone(),
                rows: table.row_count(),
                columns: table.column_count(),
                string_cells: table.string_cells_shared(),
                ..StructureTableMeta::default()
            }],
            changes: Vec::new(),
            text_overlay_cells: Arc::new(Vec::new()),
            text_overlay_revision: 0,
            view_table_id: table.id,
            view_layout: Arc::clone(&view.layout),
            selection: view.selection,
            scroll: view.scroll,
            cross_sheet_references: false,
            clipboard_paste: None,
            prepared_selection_after: None,
        };
        let prepared = prepare_structure(&request, &AtomicBool::new(false))
            .expect("row operation keeps raw surplus cells");
        let SourceDocument::Markdown(after) = prepared.source_after.as_deref().unwrap() else {
            panic!("Markdown source remains Markdown");
        };
        let text = std::str::from_utf8(after.source_bytes()).unwrap();
        assert!(text.contains("| x | y | **retained** |"), "{text}");
        assert!(prepared.tables[0].name.len() > 31);
    }
}

#[test]
// Verify that Markdown column edits move surplus raw cells after the parsed table width.
fn markdown_column_transformations_move_raw_surplus_after_the_parsed_width() {
    let original = b"| a | b |\n| --- | --- |\n| x | y | **retained** |\n";
    let parsed = MarkdownDocument::parse(original.to_vec()).expect("original parses");
    assert_eq!(parsed.parser_width(0), Some(2));
    assert_eq!(parsed.display_width(0), Some(3));
    for (operation, expected_parser_width, expected_display_width) in [
        (
            StructureOperation::Insert {
                table_id: 1,
                axis: StructureAxis::Columns,
                index: 1,
                count: 1,
            },
            4,
            4,
        ),
        (
            StructureOperation::Delete {
                table_id: 1,
                axis: StructureAxis::Columns,
                index: 0,
                count: 1,
            },
            2,
            2,
        ),
    ] {
        let parsed = MarkdownDocument::parse(original.to_vec()).expect("Markdown parses");
        let source = Arc::new(SourceDocument::Markdown(Arc::new(parsed)));
        let document = match source.as_ref() {
            SourceDocument::Markdown(markdown) => markdown.to_document(77, "fixture.md"),
            _ => unreachable!(),
        };
        let table = &document.tables[0];
        let view = document.view(table.id).unwrap();
        let request = StructureRequest {
            document_id: document.id,
            revision: document.revision,
            calculation_revision: document.calculation_revision,
            source: Some(source),
            operation: Some(operation),
            tables: vec![StructureTableMeta {
                id: table.id,
                name: table.name.clone(),
                rows: table.row_count(),
                columns: table.column_count(),
                string_cells: table.string_cells_shared(),
                ..StructureTableMeta::default()
            }],
            changes: Vec::new(),
            text_overlay_cells: Arc::new(Vec::new()),
            text_overlay_revision: 0,
            view_table_id: table.id,
            view_layout: Arc::clone(&view.layout),
            selection: view.selection,
            scroll: view.scroll,
            cross_sheet_references: false,
            clipboard_paste: None,
            prepared_selection_after: None,
        };
        let prepared = prepare_structure(&request, &AtomicBool::new(false))
            .expect("column transform preserves raw surplus cells");
        let SourceDocument::Markdown(after) = prepared.source_after.as_deref().unwrap() else {
            panic!("Markdown source remains Markdown");
        };
        let text = std::str::from_utf8(after.source_bytes()).unwrap();
        assert!(text.contains("**retained**"), "{text}");
        assert_eq!(after.parser_width(0), Some(expected_parser_width), "{text}");
        assert_eq!(
            after.display_width(0),
            Some(expected_display_width),
            "{text}"
        );
        assert_eq!(
            after.tables[0].rows[1]
                .last()
                .map(|cell| cell.value.as_str()),
            Some("**retained**"),
            "{text}"
        );
    }
}

#[test]
// Verify that long Markdown table names do not block formula model setup.
fn long_markdown_table_name_does_not_block_formula_model_preparation() {
    let original = b"# A heading that is much longer than thirty one characters\n\n| value |\n| --- |\n| 1 |\n";
    let parsed = MarkdownDocument::parse(original.to_vec()).expect("Markdown parses");
    let source = Arc::new(SourceDocument::Markdown(Arc::new(parsed)));
    let mut document = match source.as_ref() {
        SourceDocument::Markdown(markdown) => markdown.to_document(75, "fixture.md"),
        _ => unreachable!(),
    };
    document.set_source(Some(Arc::clone(&source)));
    let table = &document.tables[0];
    let view = document.view(table.id).unwrap();
    let request = StructureRequest {
        document_id: document.id,
        revision: document.revision,
        calculation_revision: document.calculation_revision,
        source: Some(source),
        operation: Some(StructureOperation::Insert {
            table_id: table.id,
            axis: StructureAxis::Rows,
            index: 1,
            count: 1,
        }),
        tables: vec![StructureTableMeta {
            id: table.id,
            name: table.name.clone(),
            rows: table.row_count(),
            columns: table.column_count(),
            string_cells: table.string_cells_shared(),
            ..StructureTableMeta::default()
        }],
        changes: Vec::new(),
        text_overlay_cells: Arc::new(Vec::new()),
        text_overlay_revision: 0,
        view_table_id: table.id,
        view_layout: Arc::clone(&view.layout),
        selection: view.selection,
        scroll: view.scroll,
        cross_sheet_references: false,
        clipboard_paste: None,
        prepared_selection_after: None,
    };
    assert!(prepare_structure(&request, &AtomicBool::new(false)).is_ok());
}

#[test]
// Rebuilds Markdown calculation inputs from the source attached to a prepared root.
fn markdown_structure_snapshot_keeps_styled_numbers_calculable() {
    let markdown = MarkdownDocument::parse(
        b"| Value | Count |\n| --- | --- |\n| **5** | =COUNT(A2) |\n".to_vec(),
    )
    .expect("Markdown parses");
    let source = Arc::new(SourceDocument::Markdown(Arc::new(markdown.clone())));
    let mut document = markdown.to_document(76, "formula.md");
    document.set_source(Some(Arc::clone(&source)));
    let table_id = document.tables[0].id;
    let mut request = snapshot_structure_request(&document);
    request.operation = Some(StructureOperation::Insert {
        table_id,
        axis: StructureAxis::Rows,
        index: 2,
        count: 1,
    });

    let prepared = prepare_structure(&request, &AtomicBool::new(false))
        .expect("row insertion prepares on the worker");
    let snapshot = prepared.after_calculation;
    let stored_value = snapshot.tables[0]
        .cells
        .iter()
        .find(|(address, _)| *address == CellAddress { row: 1, column: 0 })
        .expect("styled source value is in the prepared snapshot");
    assert_eq!(stored_value.1.input, CellInput::Text("5".to_owned()));

    let worker = crate::calc::CalculationWorker::new();
    let token = crate::file_worker::JobToken {
        document_id: document.id,
        revision: request.calculation_revision,
        job_id: 10,
    };
    worker
        .initialize(token, (*snapshot).clone())
        .expect("initialize the prepared formula snapshot");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let Some(result) = worker.poll().into_iter().find(|result| result.done) {
            assert_eq!(result.token, token);
            assert!(result.error.is_none(), "{:?}", result.error);
            let entries = result.full_snapshot.expect("fresh formula results");
            assert!(entries.iter().any(|(id, address, status, value)| {
                *id == table_id
                    && *address == CellAddress { row: 1, column: 1 }
                    && *status == crate::document::CalcStatus::Fresh
                    && *value == Some(crate::document::CalcValue::Number(1.0))
            }));
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "calculation timed out"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
// Verify that large structure history spills both roots and restores them off the UI path.
fn large_structure_history_spills_and_restores_both_roots_off_the_ui_path() {
    let long_cell = "v".repeat(3 * 1024 * 1024);
    let original = format!("{long_cell}\nnext\n").into_bytes();
    let (mut document, request) = csv_fixture(
        original.clone(),
        StructureOperation::Insert {
            table_id: 1,
            axis: StructureAxis::Rows,
            index: 1,
            count: 1,
        },
    );
    let prepared = prepare_structure(&request, &AtomicBool::new(false))
        .expect("large history can be prepared");
    let history_file = prepared
        .history_file
        .as_ref()
        .expect("large history is evacuated before apply")
        .clone();
    let history_directory = history_file.directory.clone();
    let ((table_id, _), retired) = document
        .execute_prepared_structure(prepared)
        .expect("after root swaps in atomically");
    drop(retired);
    document.mark_saved(document.revision);

    let undo_request = document
        .structure_history_restore_request(false)
        .expect("spilled undo is prepared on a worker");
    let undo_root = prepare_structure_restore(&undo_request).expect("before root reads");
    let (target, retired) = document
        .apply_prepared_structure_restore(undo_root)
        .expect("before root swaps in atomically");
    drop(retired);
    assert_eq!(target.0, table_id);
    let SourceDocument::Csv(source) = document.source.as_deref().unwrap() else {
        panic!("undo restores source format");
    };
    assert_eq!(source.source_bytes(), original);
    document.mark_saved(document.revision);

    let redo_request = document
        .structure_history_restore_request(true)
        .expect("spilled redo is prepared on a worker");
    let redo_root = prepare_structure_restore(&redo_request).expect("after root reads");
    let (_target, retired) = document
        .apply_prepared_structure_restore(redo_root)
        .expect("after root swaps in atomically");
    drop(retired);
    let SourceDocument::Csv(source) = document.source.as_deref().unwrap() else {
        panic!("redo restores source format");
    };
    assert_eq!(source.records.len(), 3);
    assert_eq!(
        document.tables[0]
            .cell(CellAddress { row: 2, column: 0 })
            .unwrap()
            .input,
        crate::document::CellInput::Text("next".to_owned())
    );
    assert!(history_directory.exists());

    drop(undo_request);
    drop(redo_request);
    drop(history_file);
    drop(document);
    assert!(!history_directory.exists());
}

#[test]
// Verify that failed history restoration keeps the active root and undo entry.
fn failed_spilled_history_restore_keeps_current_root_and_undo_entry() {
    let long_cell = "v".repeat(3 * 1024 * 1024);
    let original = format!("{long_cell}\nnext\n").into_bytes();
    let (mut document, request) = csv_fixture(
        original,
        StructureOperation::Insert {
            table_id: 1,
            axis: StructureAxis::Rows,
            index: 1,
            count: 1,
        },
    );
    let prepared = prepare_structure(&request, &AtomicBool::new(false))
        .expect("large history can be prepared");
    let history_file = prepared
        .history_file
        .as_ref()
        .expect("large history uses the recovery file")
        .clone();
    document
        .execute_prepared_structure(prepared)
        .expect("prepared state swaps in");
    let revision = document.revision;
    let SourceDocument::Csv(source) = document.source.as_deref().expect("current source exists")
    else {
        panic!("source remains CSV");
    };
    let source_before = source.source_bytes().to_vec();
    let row_count = document.tables[0].row_count();
    let restore_request = document
        .structure_history_restore_request(false)
        .expect("undo entry remains available");
    fs::write(history_file.directory.join("before.json"), b"{broken")
        .expect("history corruption is prepared for the failure case");

    assert!(prepare_structure_restore(&restore_request).is_err());
    assert_eq!(document.revision, revision);
    assert_eq!(document.tables[0].row_count(), row_count);
    let SourceDocument::Csv(source_after) = document.source.as_deref().unwrap() else {
        panic!("failed restore keeps CSV source");
    };
    assert_eq!(source_after.source_bytes(), source_before);
    assert!(document.structure_history_restore_request(false).is_some());
}

#[test]
// Verify that spilled CSV history restores metadata needed by a later undo.
fn spilled_csv_structure_history_restores_edit_origin_metadata_for_later_undo() {
    let original = b"base\n".to_vec();
    let (mut document, mut request) = csv_fixture(
        original.clone(),
        StructureOperation::Insert {
            table_id: 1,
            axis: StructureAxis::Rows,
            index: 1,
            count: 1,
        },
    );
    // Read and validate the new row through the usual text editing worker.
    let insert = request.operation.replace(StructureOperation::SetTextCell {
        table_id: 1,
        address: CellAddress { row: 1, column: 0 },
        cell: Box::new(crate::document::Cell::text("x".repeat(5 * 1024 * 1024))),
    });
    let append =
        prepare_structure(&request, &AtomicBool::new(false)).expect("prepare the new CSV row");
    document
        .execute_prepared_structure(append)
        .expect("commit the new CSV row");
    request.operation = insert;
    request.source.clone_from(&document.source);
    request.text_overlay_cells = document.text_overlay_cells();
    request.text_overlay_revision = document.text_overlay_snapshot().revision;
    request.tables[0].rows = document.tables[0].row_count();
    request.tables[0].columns = document.tables[0].column_count();
    request.revision = document.revision;
    request.calculation_revision = document.calculation_revision;
    request.changes = document.changed_cell_snapshot();
    request.tables[0].string_cells = document.tables[0].string_cells_shared();

    let prepared = prepare_structure(&request, &AtomicBool::new(false))
        .expect("large structure history is prepared");
    assert!(
        prepared.history_file.is_some(),
        "history is spilled to disk"
    );
    let (_, retired) = document
        .execute_prepared_structure(prepared)
        .expect("prepared structural root is applied");
    drop(retired);
    let undo_request = document
        .structure_history_restore_request(false)
        .expect("structural undo uses the spilled before root");
    let restored = prepare_structure_restore(&undo_request).expect("CSV metadata restores");
    let (_, retired) = document
        .apply_prepared_structure_restore(restored)
        .expect("structural before root is swapped in");
    drop(retired);

    let append_undo = document
        .structure_history_restore_request(false)
        .expect("the large row append also uses spilled history");
    let original_root = prepare_structure_restore(&append_undo).expect("read the original row");
    document
        .apply_prepared_structure_restore(original_root)
        .expect("undo the row append");
    let SourceDocument::Csv(source) = document.source.as_deref().unwrap() else {
        panic!("CSV source remains CSV");
    };
    let changes = document
        .changed_cell_snapshot()
        .into_iter()
        .filter(|(table_id, _, _)| *table_id == 1)
        .map(|(_, address, cell)| CsvCellChange {
            row: address.row,
            column: address.column,
            value: cell.map(|cell| cell.input.source_text()),
        })
        .collect::<Vec<_>>();
    let after_cell_undo = source
        .apply_changes(&changes)
        .expect("restored origin metadata trims the undone append");
    assert_eq!(after_cell_undo.source_bytes(), original);
    assert_eq!(after_cell_undo.records.len(), 1);
    drop(undo_request);
}

#[test]
// Verify that new table names use the creation language without renaming older tables.
fn markdown_table_add_keeps_names_from_each_creation_language() {
    let markdown = MarkdownDocument::parse(b"| value |\n| --- |\n| cell |\n".to_vec())
        .expect("Markdown table parses");
    let mut document =
        markdown.to_document_with_language(92, "fixture.md", crate::i18n::Language::Japanese);
    let source = Arc::new(SourceDocument::Markdown(Arc::new(markdown)));
    document.set_source(Some(Arc::clone(&source)));
    assert_eq!(document.tables[0].name, "表1");

    let first_request = table_management_request(&document);
    let first_add = prepare_table_structure(
        &first_request,
        &TableAction::Add {
            new_table_id: 2,
            name: "表2".to_owned(),
        },
        &AtomicBool::new(false),
    )
    .expect("the Japanese table add prepares");
    let (_, retired) = document
        .execute_prepared_structure(first_add)
        .expect("the Japanese table add enters history");
    drop(retired);
    assert_eq!(document.tables[0].name, "表1");
    assert_eq!(document.tables[1].name, "表2");

    let second_request = table_management_request(&document);
    let second_add = prepare_table_structure(
        &second_request,
        &TableAction::Add {
            new_table_id: 3,
            name: "Table3".to_owned(),
        },
        &AtomicBool::new(false),
    )
    .expect("the English table add prepares");
    let (_, retired) = document
        .execute_prepared_structure(second_add)
        .expect("the English table add enters history");
    drop(retired);
    assert_eq!(
        document
            .tables
            .iter()
            .map(|table| table.name.as_str())
            .collect::<Vec<_>>(),
        ["表1", "表2", "Table3"]
    );
}

#[test]
// Verify that Markdown table edits keep surrounding source and local settings.
fn markdown_table_set_preserves_surrounding_source_views_and_local_settings() {
    let source_bytes = b"intro before\n\n## Alpha\n| Name |\n| --- |\n| one |\n\nmiddle paragraph\n\n## Beta\n| Name |\n| --- |\n| two |\n\ntail marker\n".to_vec();
    let markdown = MarkdownDocument::parse(source_bytes.clone()).expect("fixture parses");
    let source = Arc::new(SourceDocument::Markdown(Arc::new(markdown)));
    let SourceDocument::Markdown(markdown) = source.as_ref() else {
        unreachable!();
    };
    let mut document = markdown.to_document(91, "fixture.md");
    document.set_source(Some(Arc::clone(&source)));
    assert_eq!(
        document
            .tables
            .iter()
            .map(|table| table.id)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    let beta_selection = CellRange::single(CellAddress { row: 2, column: 0 });
    let beta_view = document.view_mut(2).expect("second table has a view");
    beta_view.set_selection(beta_selection);
    beta_view.scroll = ScrollPosition { x: 8.0, y: 12.0 };

    let duplicate_request = table_management_request(&document);
    let duplicate = prepare_table_structure(
        &duplicate_request,
        &TableAction::Duplicate {
            table_id: 1,
            new_table_id: 99,
            name: "Alpha".to_owned(),
        },
        &AtomicBool::new(false),
    )
    .expect("Markdown table duplicates on the worker");
    let SourceDocument::Markdown(duplicate_source) = duplicate.source_after.as_deref().unwrap()
    else {
        panic!("Markdown source format is retained");
    };
    let duplicate_text = std::str::from_utf8(duplicate_source.source_bytes()).unwrap();
    assert!(duplicate_text.starts_with("intro before\n\n## Alpha"));
    assert!(duplicate_text.contains("middle paragraph"));
    assert!(duplicate_text.ends_with("tail marker\n"));
    let (target, retired) = document
        .execute_prepared_structure(duplicate)
        .expect("duplicated table set enters shared history");
    drop(retired);
    assert_eq!(target.0, 99);
    assert_eq!(
        document
            .tables
            .iter()
            .map(|table| table.id)
            .collect::<Vec<_>>(),
        vec![1, 99, 2]
    );
    assert_eq!(document.view(2).unwrap().selection, beta_selection);
    assert_eq!(
        document.view(2).unwrap().scroll,
        ScrollPosition { x: 8.0, y: 12.0 }
    );
    let duplicate_selection = CellRange::single(CellAddress { row: 1, column: 0 });
    document
        .view_mut(99)
        .unwrap()
        .set_selection(duplicate_selection);

    let delete_request = table_management_request(&document);
    let delete = prepare_table_structure(
        &delete_request,
        &TableAction::Delete { table_id: 99 },
        &AtomicBool::new(false),
    )
    .expect("Markdown table deletion prepares on the worker");
    let (_, retired) = document
        .execute_prepared_structure(delete)
        .expect("table deletion enters shared history");
    drop(retired);
    assert!(matches!(
        document.undo().expect("deletion undo restores the table"),
        Some((99, _))
    ));
    assert_eq!(document.tables[1].id, 99);
    assert_eq!(document.view(99).unwrap().selection, duplicate_selection);
    assert_eq!(document.view(2).unwrap().selection, beta_selection);

    document
        .undo()
        .expect("duplicate undo restores original source");
    assert_eq!(
        document
            .tables
            .iter()
            .map(|table| table.id)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    let SourceDocument::Markdown(restored) = document.source.as_deref().unwrap() else {
        panic!("Markdown source remains available after undo");
    };
    assert_eq!(restored.source_bytes(), source_bytes);
    document.redo().expect("redo restores stable duplicate ID");
    assert_eq!(
        document
            .tables
            .iter()
            .map(|table| table.id)
            .collect::<Vec<_>>(),
        vec![1, 99, 2]
    );

    let one_table = MarkdownDocument::parse(b"| A |\n| --- |\n| x |\n".to_vec())
        .expect("one table fixture parses");
    assert!(matches!(
        one_table.delete_table(0),
        Err(crate::markdown_adapter::MarkdownError::LastTable)
    ));
}

#[test]
// Verify that recovery rejects non-finite values instead of writing them as null.
fn recovery_rejects_non_finite_values_instead_of_serializing_them_as_null() {
    let mut document = Document::new(72, "non-finite", &["Sheet1"]);
    document.tables[0].set_loaded_cell(
        CellAddress::default(),
        crate::document::Cell::number(f64::NAN),
    );
    assert!(validate_table_history(&document.tables[0]).is_err());
}

#[test]
// Verify that pasting coloured cells into a CSV table keeps the text but drops the formatting,
// so no edited cell carries colours, fonts, or fills that the grid would have to draw.
fn csv_paste_drops_unsupported_cell_formatting() {
    let selection = CellRange::single(CellAddress { row: 0, column: 0 });
    let (mut document, mut request) = paste_fixture("red", selection);
    if let Some(paste) = request.clipboard_paste.as_mut() {
        paste.html = Some(
            "<table><tr><td style=\"color:#ff0000;background-color:#00ff00\">red</td></tr></table>"
                .to_owned(),
        );
    }
    let prepared = prepare_structure(&request, &AtomicBool::new(false))
        .expect("the coloured paste prepares on the worker");
    document
        .execute_prepared_structure(prepared)
        .expect("the paste applies");

    let cell = document.tables[0]
        .cell(CellAddress { row: 0, column: 0 })
        .expect("the pasted cell exists");
    assert_eq!(cell.input.source_text(), "red");
    assert!(
        document.tables[0]
            .format(CellAddress { row: 0, column: 0 })
            .is_none()
    );
    assert!(matches!(
        document.source.as_deref(),
        Some(SourceDocument::Csv(_))
    ));
}
