use super::*;
use crate::csv_adapter::EncodingChoice;
use crate::document::{Cell, CellFormat, CellRange, Document};
use crate::xlsx_adapter::{XlsxCell, XlsxValue};

/// Builds the export document used to inspect prepared source values.
fn materialize_document(request: &ExportRequest) -> Result<Document, crate::i18n::Message> {
    materialize_document_cancellable(request, None)
}

#[test]
// Keeps literal formula-looking labels and their styles through Markdown link edits.
fn markdown_link_edits_keep_literal_values_and_inline_styles() {
    for source in [
        "| [\\'=x](https://example.test) |\n| --- |\n",
        "| `'=x` |\n| --- |\n",
    ] {
        let parsed = crate::markdown_adapter::MarkdownDocument::parse(source.as_bytes().to_vec())
            .expect("non-leading apostrophe source parses");
        assert_eq!(parsed.cell_display(0, 0, 0).unwrap().plain_text, "'=x");
    }

    let source = crate::markdown_adapter::MarkdownDocument::parse(
        b"| [=1+2](https://old.test) |\n| --- |\n".to_vec(),
    )
    .expect("formula-like link source parses");
    let plain = markdown_link_cell_source(&source, 0, CellAddress::default(), None)
        .expect("removing the link can preserve the literal value");
    assert_eq!(plain, "'=1+2");
    let changed = source
        .apply_changes(&[crate::markdown_adapter::MarkdownCellChange {
            table: 0,
            row: 0,
            column: 0,
            value: plain,
        }])
        .expect("literal source change reparses");
    assert_eq!(changed.cell_display(0, 0, 0).unwrap().plain_text, "=1+2");

    let source = crate::markdown_adapter::MarkdownDocument::parse(
        "| '=**語** |\n| --- |\n".as_bytes().to_vec(),
    )
    .expect("escaped rich-text source parses");
    assert_eq!(source.cell_display(0, 0, 0).unwrap().plain_text, "=語");
    let link = crate::document::CellHyperlink {
        target: "https://example.test".to_owned(),
        tooltip: None,
        location: false,
    };
    let linked_markup = markdown_link_cell_source(&source, 0, CellAddress::default(), Some(&link))
        .expect("rich text can accept a link");
    let linked = source
        .apply_changes(&[crate::markdown_adapter::MarkdownCellChange {
            table: 0,
            row: 0,
            column: 0,
            value: linked_markup,
        }])
        .expect("link markup reparses");
    assert_eq!(linked.cell_display(0, 0, 0).unwrap().plain_text, "=語");
    let unlinked_markup = markdown_link_cell_source(&linked, 0, CellAddress::default(), None)
        .expect("link can be removed from rich text");
    let unlinked = linked
        .apply_changes(&[crate::markdown_adapter::MarkdownCellChange {
            table: 0,
            row: 0,
            column: 0,
            value: unlinked_markup,
        }])
        .expect("unlinked rich text reparses");
    assert_eq!(unlinked.cell_display(0, 0, 0).unwrap().plain_text, "=語");
    assert!(
        unlinked
            .cell_display(0, 0, 0)
            .unwrap()
            .runs
            .iter()
            .any(|run| run.bold)
    );
}

// Build an export request for a source-backed test document.
fn request_for_source(
    source: ExportSource,
    format: ExportFormat,
    formulas: FormulaOutput,
    modifiers: ModifierOutput,
    markdown_header: MarkdownHeader,
    destination: &str,
) -> ExportRequest {
    let document = match &source {
        ExportSource::Csv(source) => source.to_document(1, "input.csv"),
        ExportSource::Markdown(source) => source.to_document(1, "input.md"),
        ExportSource::Xlsx(source) => source.to_document(1, "input.xlsx"),
        ExportSource::Html(source) => source.to_document(1, "input.html"),
    };
    ExportRequest {
        document_id: 1,
        revision: document.revision,
        options: ExportOptions {
            format,
            scope: ExportScope::AllTables,
            formulas,
            modifiers,
            markdown_header,
        },
        destination: PathBuf::from(destination),
        source_path: None,
        source: Some(source),
        source_report: None,
        tables: document
            .tables
            .iter()
            .map(|table| ExportTableMeta {
                id: table.id,
                name: table.name.clone(),
                rows: table.row_count(),
                columns: table.column_count(),
            })
            .collect(),
        changes: document.changed_cell_snapshot(),
        format_changes: Vec::new(),
        formula_results: None,
    }
}

#[test]
// Shows only destination file names in report settings, not private directory paths.
fn export_report_settings_uses_destination_file_names() {
    let request = request_for_source(
        ExportSource::Markdown(markdown_source("| Value |\n| --- |\n| kept |\n")),
        ExportFormat::Markdown,
        FormulaOutput::Formula,
        ModifierOutput::Convert,
        MarkdownHeader::UseFirstRow,
        "/private/secret/quarterly-report.md",
    );
    let prepared = prepare_export(&request, None).expect("export report is prepared");
    let english = prepared
        .report
        .settings_summary
        .resolve(crate::i18n::Language::English);

    assert!(english.contains("quarterly-report.md"), "{english}");
    assert!(!english.contains("/private/secret"), "{english}");
}

// Create a Markdown source used by export tests.
fn markdown_source(text: &str) -> Arc<MarkdownDocument> {
    Arc::new(MarkdownDocument::parse(text.as_bytes().to_vec()).expect("markdown source"))
}

#[test]
// Keeps multi-table CSV requests out of the worker path.
fn export_worker_requires_one_selected_table_for_csv() {
    let source = ExportSource::Markdown(markdown_source(
        "| Region | Revenue |\n| --- | ---: |\n| North | 18240 |\n",
    ));
    let mut csv_request = request_for_source(
        source,
        ExportFormat::Csv,
        FormulaOutput::Formula,
        ModifierOutput::Convert,
        MarkdownHeader::UseFirstRow,
        "/tmp/selected-table.csv",
    );
    let error = prepare_export_cancellable(&csv_request, None, &AtomicBool::new(false))
        .expect_err("CSV output requires one selected table");
    assert_eq!(error.key(), "status-export-csv-selected-table-required");

    csv_request.options.scope = ExportScope::Selected(csv_request.tables[0].id);
    let prepared = prepare_export_cancellable(&csv_request, None, &AtomicBool::new(false))
        .expect("the selected-table CSV route remains available");
    assert_eq!(prepared.artifacts.len(), 1);
}

#[test]
// Verify that exporting an HTML cell to CSV writes visible text, not markup.
fn html_export_to_csv_uses_visible_cell_text() {
    let source = Arc::new(
        crate::html_adapter::HtmlDocument::parse(
            b"<table><tr><td><b>A&amp;B</b></td></tr></table>".to_vec(),
        )
        .expect("HTML source"),
    );
    let request = request_for_source(
        ExportSource::Html(source),
        ExportFormat::Csv,
        FormulaOutput::Formula,
        ModifierOutput::Convert,
        MarkdownHeader::UseFirstRow,
        "out.csv",
    );
    let prepared = prepare_export(&request, None).expect("CSV export");
    assert_eq!(prepared.artifacts[0].bytes, b"A&B\n");
    assert!(prepared.report.summary().converted >= 1);
    assert!(
        prepared
            .report
            .to_text(crate::i18n::Language::English)
            .contains("HTML table attributes")
    );
}

#[test]
// Verify that adjacent Markdown style runs merge and boundary spaces remain outside delimiters.
fn markdown_markup_merges_runs_and_keeps_boundary_spaces_outside_delimiters() {
    let adjacent = markdown_runs_as_markup(&[
        XlsxTextRun {
            text: "a".to_owned(),
            bold: true,
            ..XlsxTextRun::default()
        },
        XlsxTextRun {
            text: "b".to_owned(),
            bold: true,
            ..XlsxTextRun::default()
        },
    ]);
    assert_eq!(adjacent, "**ab**");

    let markup = markdown_runs_as_markup(&[
        XlsxTextRun {
            text: "a".to_owned(),
            ..XlsxTextRun::default()
        },
        XlsxTextRun {
            text: " b ".to_owned(),
            bold: true,
            ..XlsxTextRun::default()
        },
        XlsxTextRun {
            text: "c".to_owned(),
            ..XlsxTextRun::default()
        },
    ]);
    assert!(markup.contains("&#32;"));
    let mut strong_depth = 0;
    let mut parsed_text = String::new();
    let mut bold_text = String::new();
    for event in Parser::new_ext(&markup, Options::ENABLE_GFM) {
        match event {
            Event::Start(Tag::Strong) => strong_depth += 1,
            Event::End(TagEnd::Strong) => strong_depth -= 1,
            Event::Text(text) => {
                parsed_text.push_str(&text);
                if strong_depth > 0 {
                    bold_text.push_str(&text);
                }
            }
            _ => (),
        }
    }
    assert_eq!(parsed_text, "a b c");
    assert_eq!(bold_text, "b");
}

#[test]
// Verify that only rich text runs that fail a GFM round trip are dropped.
fn markdown_markup_drops_only_runs_that_fail_gfm_cell_round_trip() {
    let runs = [
        XlsxTextRun {
            text: "A".to_owned(),
            ..XlsxTextRun::default()
        },
        XlsxTextRun {
            text: "（字）".to_owned(),
            bold: true,
            ..XlsxTextRun::default()
        },
        XlsxTextRun {
            text: "B".to_owned(),
            ..XlsxTextRun::default()
        },
        XlsxTextRun {
            text: " safe".to_owned(),
            italic: true,
            ..XlsxTextRun::default()
        },
    ];
    let (markup, omitted, retry_limit_reached) = markdown_runs_as_markup_checked(&runs);
    assert!(omitted);
    assert!(!retry_limit_reached);
    assert!(markup.contains("&#32;"));
    let mut parsed_text = String::new();
    let mut bold_text = String::new();
    let mut italic_text = String::new();
    let mut bold_depth = 0;
    let mut italic_depth = 0;
    for event in Parser::new_ext(&markup, Options::ENABLE_GFM) {
        match event {
            Event::Start(Tag::Strong) => bold_depth += 1,
            Event::End(TagEnd::Strong) => bold_depth -= 1,
            Event::Start(Tag::Emphasis) => italic_depth += 1,
            Event::End(TagEnd::Emphasis) => italic_depth -= 1,
            Event::Text(text) => {
                parsed_text.push_str(&text);
                if bold_depth > 0 {
                    bold_text.push_str(&text);
                }
                if italic_depth > 0 {
                    italic_text.push_str(&text);
                }
            }
            _ => (),
        }
    }
    assert_eq!(parsed_text, "A（字）B safe");
    assert!(bold_text.is_empty(), "unsupported bold run was omitted");
    assert_eq!(italic_text, "safe", "the supported italic run remains");
    assert!(markdown_cell_markup_matches(&[
        XlsxTextRun {
            text: "A（字）B ".to_owned(),
            ..XlsxTextRun::default()
        },
        XlsxTextRun {
            text: "safe".to_owned(),
            italic: true,
            ..XlsxTextRun::default()
        },
    ]));

    let cell = Cell::text("A（字）B safe");
    let rich_text = runs
        .iter()
        .map(|run| crate::document::CellTextRun {
            text: run.text.clone(),
            bold: run.bold,
            italic: run.italic,
            strike: run.strike,
            ..crate::document::CellTextRun::default()
        })
        .collect();
    let table = ExportTable {
        order: 0,
        id: 1,
        name: "Text".to_owned(),
        rows: vec![vec![Cell::text("A（字）B safe")]],
        string_cells: BTreeSet::new(),
        formats: BTreeMap::from([(
            CellAddress { row: 0, column: 0 },
            CellFormat {
                rich_text: Some(rich_text),
                ..CellFormat::default()
            },
        )]),
    };
    let request = ExportRequest {
        document_id: 1,
        revision: 0,
        options: ExportOptions {
            format: ExportFormat::Markdown,
            scope: ExportScope::AllTables,
            formulas: FormulaOutput::Formula,
            modifiers: ModifierOutput::Convert,
            markdown_header: MarkdownHeader::UseFirstRow,
        },
        destination: PathBuf::from("round-trip.md"),
        source_path: None,
        source: None,
        source_report: None,
        tables: Vec::new(),
        changes: Vec::new(),
        format_changes: Vec::new(),
        formula_results: None,
    };
    let mut entries = Vec::new();
    let formula_index = FormulaResultIndex::new(None, None).expect("formula index builds");
    let context = ExportBuildContext {
        request: &request,
        cell_changes: HashMap::new(),
        formula_index: &formula_index,
        markdown_context: None,
        cancelled: None,
    };
    let output = markdown_markup(
        &context,
        None,
        &cell,
        "A（字）B safe",
        ModifierOutput::Convert,
        &mut entries,
        (&table, CellAddress { row: 0, column: 0 }),
    );
    assert!(output.contains("&#32;"));
    let mut text = String::new();
    let mut bold = String::new();
    let mut italic = String::new();
    let mut bold_depth = 0;
    let mut italic_depth = 0;
    for event in Parser::new_ext(&output, Options::ENABLE_GFM) {
        match event {
            Event::Start(Tag::Strong) => bold_depth += 1,
            Event::End(TagEnd::Strong) => bold_depth -= 1,
            Event::Start(Tag::Emphasis) => italic_depth += 1,
            Event::End(TagEnd::Emphasis) => italic_depth -= 1,
            Event::Text(value) => {
                text.push_str(&value);
                if bold_depth > 0 {
                    bold.push_str(&value);
                }
                if italic_depth > 0 {
                    italic.push_str(&value);
                }
            }
            _ => (),
        }
    }
    assert_eq!(text, "A（字）B safe");
    assert!(bold.is_empty());
    assert_eq!(italic, "safe");
    assert!(
        entries.iter().any(|entry| {
            entry.class == ConversionClass::Omitted && entry.reason.contains("GFM")
        })
    );
}

#[test]
// Verify that failed Markdown formatting retries are bounded and reported.
fn markdown_run_retries_are_bounded_and_report_plain_text_fallback() {
    let mut runs = vec![
        XlsxTextRun {
            text: "A".to_owned(),
            ..XlsxTextRun::default()
        },
        XlsxTextRun {
            text: "（字）".to_owned(),
            bold: true,
            ..XlsxTextRun::default()
        },
        XlsxTextRun {
            text: "B".to_owned(),
            ..XlsxTextRun::default()
        },
    ];
    runs.extend((0..40).map(|index| XlsxTextRun {
        text: format!("v{index}"),
        bold: index % 2 == 0,
        italic: index % 2 != 0,
        ..XlsxTextRun::default()
    }));
    let original = runs.iter().map(|run| run.text.as_str()).collect::<String>();
    let (markup, omitted, retry_limit_reached) = markdown_runs_as_markup_checked(&runs);
    assert!(omitted);
    assert!(retry_limit_reached);
    assert_eq!(markup, escape_markdown_text(&original));
    assert!(markdown_cell_markup_matches(&[XlsxTextRun {
        text: original,
        ..XlsxTextRun::default()
    }]));

    let table = ExportTable {
        order: 0,
        id: 1,
        name: "Long runs".to_owned(),
        rows: vec![vec![Cell::text("text")]],
        string_cells: BTreeSet::new(),
        formats: BTreeMap::new(),
    };
    let mut entries = Vec::new();
    report_markdown_run_omission(
        &mut entries,
        &table,
        CellAddress { row: 0, column: 0 },
        retry_limit_reached,
    );
    assert!(entries.iter().any(|entry| {
        entry.class == ConversionClass::Omitted && entry.reason.contains("セルの原文")
    }));
}

#[test]
// Verify that internal links are omitted and external link tooltips become titles.
fn internal_markdown_link_is_omitted_and_external_tooltip_becomes_title() {
    let table = ExportTable {
        order: 0,
        id: 1,
        name: "Links".to_owned(),
        rows: vec![vec![Cell::text("text")]],
        string_cells: BTreeSet::new(),
        formats: BTreeMap::new(),
    };
    let address = CellAddress { row: 0, column: 0 };
    let mut entries = Vec::new();
    let internal = markdown_link_markup(
        &crate::document::CellHyperlink {
            target: "#Other!A1".to_owned(),
            tooltip: None,
            location: true,
        },
        "plain",
        &mut entries,
        &table,
        address,
    );
    assert_eq!(internal, "plain");
    assert!(
        entries
            .iter()
            .any(|entry| entry.class == ConversionClass::Omitted)
    );

    let external = markdown_link_markup(
        &crate::document::CellHyperlink {
            target: "https://example.test/path".to_owned(),
            tooltip: Some("open this".to_owned()),
            location: false,
        },
        "**value**",
        &mut entries,
        &table,
        address,
    );
    assert_eq!(
        external,
        "[**value**](https://example.test/path \"open this\")"
    );
}

#[test]
// Verify that Markdown formulas report styles and links that XLSX cannot retain.
fn markdown_formula_source_reports_dropped_cell_style_and_link() {
    let address = CellAddress { row: 0, column: 0 };
    let source = Arc::new(XlsxDocument {
        date_1904: false,
        sheets: vec![XlsxSheet {
            table_id: 1,
            name: "Formula".to_owned(),
            cells: BTreeMap::from([(
                address,
                XlsxCell {
                    value: XlsxValue::Number(3.0),
                    formula: Some("SUM(A1:A2)".to_owned()),
                    style: XlsxStyle {
                        bold: true,
                        ..XlsxStyle::default()
                    },
                    hyperlink: Some(crate::document::CellHyperlink {
                        target: "https://example.test/formula".to_owned(),
                        tooltip: None,
                        location: false,
                    }),
                },
            )]),
            columns: Vec::new(),
            rows: Vec::new(),
            merges: Vec::new(),
            frozen_rows: 0,
            frozen_columns: 0,
            freeze_top_left: String::new(),

            rule_losses: Vec::new(),
        }],
    });
    let request = request_for_source(
        ExportSource::Xlsx(source),
        ExportFormat::Markdown,
        FormulaOutput::Formula,
        ModifierOutput::Convert,
        MarkdownHeader::UseFirstRow,
        "formula.md",
    );
    let prepared = prepare_export(&request, None).expect("Markdown export");
    let output = String::from_utf8(prepared.artifacts[0].bytes.clone()).unwrap();
    assert!(output.contains("SUM(A1:A2)"), "{output}");
    assert!(!output.contains("https://example.test/formula"));
    assert!(prepared.report.entries.iter().any(|entry| {
        entry.class == ConversionClass::Omitted && entry.reason.contains("文字書式")
    }));
    assert!(prepared.report.entries.iter().any(|entry| {
        entry.class == ConversionClass::Omitted && entry.reason.contains("セルリンク")
    }));
}

#[test]
// Verify that cleared imported rich text stays cleared in text exports.
fn clearing_imported_rich_text_does_not_reappear_in_csv_or_markdown_export() {
    let address = CellAddress { row: 0, column: 0 };
    let source = Arc::new(XlsxDocument {
        date_1904: false,
        sheets: vec![XlsxSheet {
            table_id: 1,
            name: "Cleared".to_owned(),
            cells: BTreeMap::from([(
                address,
                XlsxCell {
                    value: XlsxValue::RichText(vec![XlsxTextRun {
                        text: "stale rich content".to_owned(),
                        bold: true,
                        ..XlsxTextRun::default()
                    }]),
                    formula: None,
                    style: XlsxStyle::default(),
                    hyperlink: None,
                },
            )]),
            columns: Vec::new(),
            rows: Vec::new(),
            merges: Vec::new(),
            frozen_rows: 0,
            frozen_columns: 0,
            freeze_top_left: String::new(),

            rule_losses: Vec::new(),
        }],
    });
    let mut document = source.to_document(1, "clear.xlsx");
    assert!(
        document
            .clear_range(1, CellRange::single(address))
            .expect("clear rich-text cell")
    );

    for (format, destination) in [
        (ExportFormat::Csv, "clear.csv"),
        (ExportFormat::Markdown, "clear.md"),
    ] {
        let mut request = request_for_source(
            ExportSource::Xlsx(Arc::clone(&source)),
            format,
            FormulaOutput::Formula,
            ModifierOutput::Convert,
            MarkdownHeader::UseFirstRow,
            destination,
        );
        request.revision = document.revision;
        request.changes = document.changed_cell_snapshot();
        let prepared = prepare_export(&request, None).expect("export cleared cell");
        let output = String::from_utf8(prepared.artifacts[0].bytes.clone()).unwrap();
        assert!(!output.contains("stale rich content"), "{output}");
    }
}

#[test]
// Verify that dirty cell edits overlay the loaded Markdown source in export.
fn dirty_snapshot_overlays_loaded_markdown_cells() {
    let source =
        Arc::new(MarkdownDocument::parse(b"| h |\n| --- |\n| a |\n".to_vec()).expect("markdown"));
    let request = ExportRequest {
        document_id: 1,
        revision: 1,
        options: ExportOptions {
            format: ExportFormat::Csv,
            scope: ExportScope::AllTables,
            formulas: FormulaOutput::Formula,
            modifiers: ModifierOutput::TextOnly,
            markdown_header: MarkdownHeader::UseFirstRow,
        },
        destination: PathBuf::from("out.csv"),
        source_path: None,
        source: Some(ExportSource::Markdown(source)),
        source_report: None,
        tables: vec![ExportTableMeta {
            id: 1,
            name: "Items".to_owned(),
            rows: 3,
            columns: 1,
        }],
        changes: vec![(1, CellAddress { row: 2, column: 0 }, Some(Cell::text("b")))],
        format_changes: Vec::new(),
        formula_results: None,
    };
    let doc = materialize_document(&request).expect("document");
    assert_eq!(
        doc.tables[0]
            .cell(CellAddress { row: 2, column: 0 })
            .unwrap()
            .input
            .source_text(),
        "b"
    );
}

#[test]
// Verify that same-format CSV export keeps empty records and untouched source bytes.
fn same_format_csv_export_keeps_zero_field_records_and_source_bytes() {
    let source_bytes = b"a,b\n\nx,y\n".to_vec();
    let source = Arc::new(
        CsvDocument::parse(
            source_bytes.clone(),
            crate::csv_adapter::EncodingChoice::Auto,
            ',',
        )
        .expect("csv"),
    );
    let document = source.to_document(1, "input.csv");
    let request = ExportRequest {
        document_id: 1,
        revision: document.revision,
        options: ExportOptions {
            format: ExportFormat::Csv,
            scope: ExportScope::AllTables,
            formulas: FormulaOutput::Formula,
            modifiers: ModifierOutput::Convert,
            markdown_header: MarkdownHeader::UseFirstRow,
        },
        destination: PathBuf::from("out.csv"),
        source_path: None,
        source: Some(ExportSource::Csv(Arc::clone(&source))),
        source_report: None,
        tables: vec![ExportTableMeta {
            id: 1,
            name: "CSV".to_owned(),
            rows: 3,
            columns: 2,
        }],
        changes: document.changed_cell_snapshot(),
        format_changes: Vec::new(),
        formula_results: None,
    };
    let prepared = prepare_export(&request, None).expect("preflight");
    assert_eq!(prepared.artifacts[0].bytes, source_bytes);
    assert!(!prepared.report.requires_confirmation());
}

#[test]
// Verify that CSV formula results update only the target record.
fn same_format_csv_result_export_changes_formula_and_preserves_other_records() {
    let source = Arc::new(
        CsvDocument::parse(
            b"head,=1+1\r\nkeep,row\r\nalso,untouched\r\n".to_vec(),
            EncodingChoice::Auto,
            ',',
        )
        .expect("CSV source"),
    );
    let mut request = request_for_source(
        ExportSource::Csv(source),
        ExportFormat::Csv,
        FormulaOutput::LatestResult,
        ModifierOutput::TextOnly,
        MarkdownHeader::UseFirstRow,
        "out.csv",
    );
    request.changes = vec![(
        1,
        CellAddress { row: 1, column: 0 },
        Some(Cell::text("changed")),
    )];
    request.formula_results = Some(Arc::new(crate::calc::FormulaResultRoot::new(
        0,
        vec![(
            1,
            CellAddress { row: 0, column: 1 },
            CalcStatus::Fresh,
            Some(CalcValue::Number(2.0)),
        )],
    )));

    let prepared = prepare_export(&request, request.formula_results.as_ref())
        .expect("latest-result CSV preflight");
    assert_eq!(
        prepared.artifacts[0].bytes,
        b"head,2\r\nchanged,row\r\nalso,untouched\r\n"
    );
}

#[test]
// Keeps formula results literal when Markdown syntax characters are exported as values.
fn markdown_result_export_round_trips_formula_text_without_inline_markup() {
    let source = markdown_source("| formula |\n| --- |\n| =1+1 |\n| =1+2 |\n| =1+3 |\n| =1+4 |\n");
    let mut request = request_for_source(
        ExportSource::Markdown(source),
        ExportFormat::Markdown,
        FormulaOutput::LatestResult,
        ModifierOutput::Convert,
        MarkdownHeader::UseFirstRow,
        "results.md",
    );
    let table_id = request.tables[0].id;
    request.formula_results = Some(Arc::new(crate::calc::FormulaResultRoot::new(
        0,
        [
            ("**literal**", 1),
            ("[x](url)", 2),
            ("<br>", 3),
            ("=literal", 4),
        ]
        .into_iter()
        .map(|(value, row)| {
            (
                table_id,
                CellAddress { row, column: 0 },
                CalcStatus::Fresh,
                Some(CalcValue::Text(value.to_owned())),
            )
        })
        .collect(),
    )));

    let prepared = prepare_export(&request, request.formula_results.as_ref())
        .expect("literal formula results can be exported");
    let updated =
        crate::markdown_adapter::MarkdownDocument::parse(prepared.artifacts[0].bytes.clone())
            .expect("result Markdown parses again");
    for (row, expected) in ["**literal**", "[x](url)", "<br>", "=literal"]
        .into_iter()
        .enumerate()
    {
        let display = updated
            .cell_display(0, row + 1, 0)
            .expect("formula result cell has display text");
        assert_eq!(display.plain_text, expected);
        assert!(display.runs.iter().all(|run| !run.bold && !run.link));
    }
}

#[test]
// Marks calculated strings beginning with equals as literal in both CSV source routes.
fn csv_formula_result_export_marks_equals_text_for_csv_and_markdown_sources() {
    let csv_source = Arc::new(
        CsvDocument::parse(b"=1+1\n".to_vec(), EncodingChoice::Auto, ',')
            .expect("CSV source parses"),
    );
    let markdown_source = markdown_source("| formula |\n| --- |\n| =1+1 |\n");

    for (source, formula_row, expected_bytes) in [
        (ExportSource::Csv(csv_source), 0, b"'=literal\n".as_slice()),
        (
            ExportSource::Markdown(markdown_source),
            1,
            b"formula\n'=literal\n".as_slice(),
        ),
    ] {
        let mut request = request_for_source(
            source,
            ExportFormat::Csv,
            FormulaOutput::LatestResult,
            ModifierOutput::Convert,
            MarkdownHeader::UseFirstRow,
            "result.csv",
        );
        let table_id = request.tables[0].id;
        request.formula_results = Some(Arc::new(crate::calc::FormulaResultRoot::new(
            0,
            vec![(
                table_id,
                CellAddress {
                    row: formula_row,
                    column: 0,
                },
                CalcStatus::Fresh,
                Some(CalcValue::Text("=literal".to_owned())),
            )],
        )));

        let prepared = prepare_export(&request, request.formula_results.as_ref())
            .expect("formula result CSV can be exported");
        assert!(
            prepared.report.can_export(),
            "{}",
            prepared.report.to_text(crate::i18n::Language::English)
        );
        assert_eq!(prepared.artifacts[0].bytes, expected_bytes);
        let reopened = CsvDocument::parse(
            prepared.artifacts[0].bytes.clone(),
            EncodingChoice::Auto,
            ',',
        )
        .expect("result CSV parses again");
        let document = reopened.to_document(8, "results.csv");
        let stored = &document.tables[0]
            .cell(CellAddress {
                row: formula_row,
                column: 0,
            })
            .expect("result cell remains present")
            .input;
        let CellInput::Text(stored) = stored else {
            panic!("the calculated text must not become a formula")
        };
        assert_eq!(crate::document::literal_text_value(stored), "=literal");
    }
}

#[test]
// Verify that export preflight leaves the document dirty until save succeeds.
fn successful_export_preflight_does_not_mark_dirty_document_saved() {
    let mut document = Document::new(1, "new", &["Sheet1"]);
    document
        .set_cell(
            1,
            CellAddress { row: 0, column: 0 },
            Some(Cell::text("value")),
        )
        .expect("cell edit");
    let revision = document.revision;
    let request = ExportRequest {
        document_id: 1,
        revision,
        options: ExportOptions {
            format: ExportFormat::Csv,
            scope: ExportScope::AllTables,
            formulas: FormulaOutput::Formula,
            modifiers: ModifierOutput::Convert,
            markdown_header: MarkdownHeader::UseFirstRow,
        },
        destination: PathBuf::from("out.csv"),
        source_path: None,
        source: None,
        source_report: None,
        tables: vec![ExportTableMeta {
            id: 1,
            name: "Sheet1".to_owned(),
            rows: 1,
            columns: 1,
        }],
        changes: document.changed_cell_snapshot(),
        format_changes: Vec::new(),
        formula_results: None,
    };
    let prepared = prepare_export(&request, None).expect("preflight");
    assert_eq!(prepared.artifacts[0].bytes, b"value\n");
    assert!(document.is_dirty());
    assert_eq!(document.revision, revision);
    assert_eq!(document.saved_revision, 0);
}

#[test]
// Verify that an unmatched latest formula result cannot be exported.
fn latest_formula_result_without_matching_snapshot_blocks_output() {
    let mut document = Document::new(1, "new", &["Sheet1"]);
    document
        .set_cell(
            1,
            CellAddress { row: 0, column: 0 },
            Some(Cell::formula("=1+1", None)),
        )
        .expect("formula edit");
    let request = ExportRequest {
        document_id: 1,
        revision: document.revision,
        options: ExportOptions {
            format: ExportFormat::Csv,
            scope: ExportScope::AllTables,
            formulas: FormulaOutput::LatestResult,
            modifiers: ModifierOutput::Convert,
            markdown_header: MarkdownHeader::UseFirstRow,
        },
        destination: PathBuf::from("out.csv"),
        source_path: None,
        source: None,
        source_report: None,
        tables: vec![ExportTableMeta {
            id: 1,
            name: "Sheet1".to_owned(),
            rows: 1,
            columns: 1,
        }],
        changes: document.changed_cell_snapshot(),
        format_changes: Vec::new(),
        formula_results: None,
    };
    let revision_before = document.revision;
    let saved_before = document.saved_revision;
    let prepared =
        prepare_export(&request, None).expect("report is returned for unprocessable formulas");
    assert!(!prepared.report.can_export());
    assert!(prepared.artifacts.is_empty());
    assert!(document.is_dirty());
    assert_eq!(document.revision, revision_before);
    assert_eq!(document.saved_revision, saved_before);
}

#[test]
// Verify that formula source skips Markdown markup and retains its typed result.
fn formula_source_bypasses_markdown_markup_and_keeps_result_type() {
    let source = markdown_source("| value |\n| --- |\n| =CONCATENATE(\"**x**\") |\n");
    let csv_request = request_for_source(
        ExportSource::Markdown(Arc::clone(&source)),
        ExportFormat::Csv,
        FormulaOutput::Formula,
        ModifierOutput::Convert,
        MarkdownHeader::UseFirstRow,
        "out.csv",
    );
    let prepared = prepare_export(&csv_request, None).expect("formula CSV preflight");
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(prepared.artifacts[0].bytes.as_slice());
    let formula = reader
        .records()
        .nth(1)
        .expect("formula row")
        .expect("formula CSV row");
    assert_eq!(formula.get(0), Some("=CONCATENATE(\"**x**\")"));

    let formula = "=CONCATENATE(\"**x**\")";
    let markdown_request = request_for_source(
        ExportSource::Markdown(markdown_source(
            "| value |\n| --- |\n| =CONCATENATE(\"**x**\") |\n",
        )),
        ExportFormat::Markdown,
        FormulaOutput::Formula,
        ModifierOutput::Convert,
        MarkdownHeader::AddBlankHeader,
        "formula.md",
    );
    let prepared = prepare_export(&markdown_request, None).expect("formula Markdown fallback");
    let reopened = MarkdownDocument::parse(prepared.artifacts[0].bytes.clone())
        .expect("read formula Markdown");
    assert_eq!(reopened.tables[0].rows[2][0].value, formula);
}

#[test]
// Keeps formula meaning through Markdown table escaping and reload.
fn markdown_formula_pipe_escape_round_trips_as_formula_data() {
    let source = markdown_source("| Formula |\n| --- |\n| =\"a\\|b\" |\n");
    let mut request = request_for_source(
        ExportSource::Markdown(Arc::clone(&source)),
        ExportFormat::Markdown,
        FormulaOutput::Formula,
        ModifierOutput::Convert,
        MarkdownHeader::UseFirstRow,
        "formula-pipe.md",
    );
    request.options.scope = ExportScope::Selected(request.tables[0].id);
    let prepared = prepare_export(&request, None).expect("Markdown formula export");
    let reopened = Arc::new(
        MarkdownDocument::parse(prepared.artifacts[0].bytes.clone())
            .expect("reopened Markdown table parses"),
    );
    let mut document = reopened.to_document(28, "Formula");
    document.set_source(Some(Arc::new(crate::source::SourceDocument::Markdown(
        Arc::clone(&reopened),
    ))));
    let snapshot = crate::calc::CalculationSnapshot::from_document(&document, false);
    let formula = snapshot.tables[0]
        .cells
        .iter()
        .find(|(address, _)| *address == CellAddress { row: 1, column: 0 })
        .expect("formula calculation cell exists");
    assert_eq!(formula.1.input, CellInput::Text("=\"a|b\"".to_owned()));

    let worker = crate::calc::CalculationWorker::new();
    let token = crate::file_worker::JobToken {
        document_id: document.id,
        revision: document.calculation_revision,
        job_id: 1,
    };
    worker
        .initialize(token, snapshot)
        .expect("enqueue reloaded formula");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let Some(result) = worker.poll().into_iter().next() {
            assert_eq!(result.token, token);
            assert!(result.error.is_none(), "{:?}", result.error);
            let values = result.full_snapshot.expect("formula snapshot");
            assert_eq!(
                values.get(1, CellAddress { row: 1, column: 0 }),
                Some((CalcStatus::Fresh, Some(&CalcValue::Text("a|b".to_owned()))))
            );
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "calculation completes"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
// Verify that Markdown cell export uses inline display text and line breaks.
fn markdown_cell_export_uses_inline_display_text_and_breaks() {
    let source = markdown_source(
        "| link | code | heading | bullet | break | rule |\n| --- | --- | --- | --- | --- | --- |\n| [show](https://example.test) | `value` | # heading | - bullet | one<br>two | ---- |\n",
    );
    let request = request_for_source(
        ExportSource::Markdown(source),
        ExportFormat::Csv,
        FormulaOutput::Formula,
        ModifierOutput::TextOnly,
        MarkdownHeader::UseFirstRow,
        "out.csv",
    );
    let prepared = prepare_export(&request, None).expect("preflight");
    assert!(
        prepared
            .report
            .to_text(crate::i18n::Language::Japanese)
            .contains("リンク")
    );
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_reader(prepared.artifacts[0].bytes.as_slice());
    let row = reader
        .records()
        .nth(1)
        .expect("data row")
        .expect("valid CSV");
    assert_eq!(row.get(0), Some("show"));
    assert_eq!(row.get(1), Some("value"));
    assert_eq!(row.get(2), Some("# heading"));
    assert_eq!(row.get(3), Some("- bullet"));
    assert_eq!(row.get(4), Some("one\ntwo"));
    assert_eq!(row.get(5), Some("----"));
}

#[test]
// Verify that same-format Markdown export keeps table alignment and link definitions.
fn same_markdown_export_preserves_source_alignment_and_reference_definitions() {
    let bytes = b"# Report\n\n| left | right |\n| :--- | ---: |\n| [show][id] | z |\n\n[id]: https://example.test\n";
    let source = Arc::new(MarkdownDocument::parse(bytes.to_vec()).expect("markdown source"));
    let mut request = request_for_source(
        ExportSource::Markdown(Arc::clone(&source)),
        ExportFormat::Markdown,
        FormulaOutput::Formula,
        ModifierOutput::Convert,
        MarkdownHeader::UseFirstRow,
        "out.md",
    );
    let prepared = prepare_export(&request, None).expect("same-format preflight");
    assert_eq!(prepared.artifacts[0].bytes, bytes);

    request.options.modifiers = ModifierOutput::TextOnly;
    let prepared = prepare_export(&request, None).expect("fallback preflight");
    assert!(
        prepared
            .report
            .to_text(crate::i18n::Language::Japanese)
            .contains("リンク定義")
    );
    let output = std::str::from_utf8(&prepared.artifacts[0].bytes).expect("Markdown output");
    assert!(output.contains("show"));
}

#[test]
// Verify that Markdown export follows a surviving table's stable ID after deletion.
fn selected_markdown_export_uses_the_remaining_stable_table_id() {
    let source = markdown_source(
        "| gone | old |\n| :--- | ---: |\n| first | removed |\n\n| keep | right |\n| --- | ---: |\n| row | old |\n",
    );
    let source = Arc::new(source.delete_table(0).expect("delete first source table"));
    let mut request = request_for_source(
        ExportSource::Markdown(source),
        ExportFormat::Markdown,
        FormulaOutput::Formula,
        ModifierOutput::Convert,
        MarkdownHeader::UseFirstRow,
        "remaining.md",
    );
    request.options.scope = ExportScope::Selected(2);
    request.tables[0].id = 2;
    request.changes.push((
        2,
        CellAddress { row: 1, column: 1 },
        Some(Cell::text("edited")),
    ));

    let prepared = prepare_export(&request, None).expect("remaining table export");
    assert!(prepared.report.can_export(), "stable table ID resolves");
    let output = String::from_utf8(prepared.artifacts[0].bytes.clone()).expect("UTF-8 Markdown");
    assert!(output.contains("| keep | right |"));
    assert!(
        output.contains("| --- | ---: |"),
        "right alignment is retained"
    );
    assert!(
        output.contains("| row | edited |"),
        "stable-ID edit is applied"
    );
    assert!(!output.contains("removed"), "deleted table does not return");
}

#[test]
// Verify that alignment loss is reported only for the selected non-Markdown table.
fn markdown_alignment_loss_report_matches_the_selected_table() {
    let source = markdown_source(
        "| aligned | left |\n| :--- | ---: |\n| a | b |\n\n| plain | header |\n| --- | --- |\n| x | y |\n",
    );
    let mut request = request_for_source(
        ExportSource::Markdown(source),
        ExportFormat::Csv,
        FormulaOutput::Formula,
        ModifierOutput::Convert,
        MarkdownHeader::UseFirstRow,
        "selected.csv",
    );
    request.options.scope = ExportScope::Selected(2);
    let plain = prepare_export(&request, None).expect("plain table CSV preflight");
    assert!(!plain.report.entries.iter().any(|entry| {
        entry.message.as_ref().is_some_and(|message| {
            message
                .resolve(crate::i18n::Language::English)
                .contains("column alignment")
        })
    }));

    request.options.scope = ExportScope::Selected(1);
    let aligned = prepare_export(&request, None).expect("aligned table CSV preflight");
    assert!(aligned.report.entries.iter().any(|entry| {
        entry.class == ConversionClass::Omitted
            && entry.sheet_order == Some(1)
            && entry.message.as_ref().is_some_and(|message| {
                message
                    .resolve(crate::i18n::Language::Japanese)
                    .contains("Markdown列配置")
            })
    }));

    request.options.format = ExportFormat::Markdown;
    let same_format = prepare_export(&request, None).expect("same-format Markdown preflight");
    assert!(!same_format.report.entries.iter().any(|entry| {
        entry.message.as_ref().is_some_and(|message| {
            message
                .resolve(crate::i18n::Language::English)
                .contains("column alignment")
        })
    }));
}

#[test]
// Reports each omitted Markdown table with its own current order and name.
fn selected_csv_report_names_unselected_tables_by_display_order() {
    let source = markdown_source(
        "| first |\n| --- |\n| a |\n\n| shared |\n| --- |\n| b |\n\n| shared |\n| --- |\n| c |\n",
    );
    let mut request = request_for_source(
        ExportSource::Markdown(source),
        ExportFormat::Csv,
        FormulaOutput::Formula,
        ModifierOutput::Convert,
        MarkdownHeader::UseFirstRow,
        "selected.csv",
    );
    for (table, id, name) in request
        .tables
        .iter_mut()
        .zip([41, 82, 123])
        .zip(["First", "Shared", "Shared"])
        .map(|((table, id), name)| (table, id, name))
    {
        table.id = id;
        table.name = name.to_owned();
    }
    request.options.scope = ExportScope::Selected(82);

    let prepared = prepare_export(&request, None).expect("selected CSV report is prepared");
    let omitted = prepared
        .report
        .entries
        .iter()
        .filter(|entry| {
            entry.class == ConversionClass::Omitted
                && entry
                    .message
                    .as_ref()
                    .is_some_and(|message| message.key() == "report-reason-unselected-tables")
        })
        .map(|entry| (entry.sheet_order, entry.sheet_name.as_deref()))
        .collect::<Vec<_>>();
    assert_eq!(
        omitted,
        [(Some(1), Some("First")), (Some(3), Some("Shared"))]
    );
    assert!(!omitted.contains(&(Some(2), Some("Shared"))));
}

#[test]
// Verify that CSV text containing ampersands and pipes round-trips through Markdown.
fn csv_to_markdown_escapes_ampersands_and_pipes_reversibly() {
    let source = Arc::new(
        CsvDocument::parse(b"A&B|C\n".to_vec(), EncodingChoice::Auto, ',').expect("CSV source"),
    );
    let request = request_for_source(
        ExportSource::Csv(source),
        ExportFormat::Markdown,
        FormulaOutput::Formula,
        ModifierOutput::Convert,
        MarkdownHeader::UseFirstRow,
        "out.md",
    );
    let prepared = prepare_export(&request, None).expect("Markdown preflight");
    let markdown = std::str::from_utf8(&prepared.artifacts[0].bytes).expect("Markdown output");
    assert!(markdown.contains("A&amp;B\\|C"));
    let reopened = Arc::new(
        MarkdownDocument::parse(prepared.artifacts[0].bytes.clone()).expect("read Markdown output"),
    );
    let csv_request = request_for_source(
        ExportSource::Markdown(reopened),
        ExportFormat::Csv,
        FormulaOutput::Formula,
        ModifierOutput::TextOnly,
        MarkdownHeader::UseFirstRow,
        "roundtrip.csv",
    );
    let csv = prepare_export(&csv_request, None).expect("CSV preflight");
    assert_eq!(csv.artifacts[0].bytes, b"A&B|C\n");
}

#[test]
// Verify that a blank Markdown header is emitted once.
fn blank_markdown_header_is_emitted_once() {
    let source = Arc::new(
        CsvDocument::parse(b"value\n".to_vec(), EncodingChoice::Auto, ',').expect("CSV source"),
    );
    let request = request_for_source(
        ExportSource::Csv(source),
        ExportFormat::Markdown,
        FormulaOutput::Formula,
        ModifierOutput::Convert,
        MarkdownHeader::AddBlankHeader,
        "out.md",
    );
    let prepared = prepare_export(&request, None).expect("preflight");
    let output = std::str::from_utf8(&prepared.artifacts[0].bytes).expect("Markdown output");
    assert_eq!(output.lines().count(), 3);
    assert_eq!(output, "|  |\n| --- |\n| value |\n");
}

#[test]
// Verify that a blank header does not inherit the first data cell’s inline formatting.
fn blank_markdown_header_does_not_reuse_first_cell_inline_context() {
    let source = markdown_source(
        "| **heading** | [show](https://example.test) |\n| --- | --- |\n| value | next |\n",
    );
    let request = request_for_source(
        ExportSource::Markdown(source),
        ExportFormat::Markdown,
        FormulaOutput::Formula,
        ModifierOutput::TextOnly,
        MarkdownHeader::AddBlankHeader,
        "out.md",
    );
    let prepared = prepare_export(&request, None).expect("Markdown preflight");
    let output = std::str::from_utf8(&prepared.artifacts[0].bytes).expect("Markdown output");
    assert_eq!(output.lines().next(), Some("|  |  |"));
    assert_eq!(output.lines().nth(2), Some("| heading | show |"));
}

#[test]
// Verify that duplicate Markdown headings produce unique CSV paths and sheet names.
fn duplicate_markdown_headings_get_unique_csv_paths() {
    let source = markdown_source(
        "# Report\n\n| value |\n| --- |\n| one |\n\n# Report\n\n| value |\n| --- |\n| two |\n\n# 2026/11 [summary]\n\n| value |\n| --- |\n| three |\n\n# Report-1\n\n| value |\n| --- |\n| four |\n",
    );
    let csv_request = request_for_source(
        ExportSource::Markdown(Arc::clone(&source)),
        ExportFormat::Csv,
        FormulaOutput::Formula,
        ModifierOutput::TextOnly,
        MarkdownHeader::UseFirstRow,
        "/tmp/export.csv",
    );
    let csv = prepare_export(&csv_request, None).expect("multi-CSV preflight");
    assert!(csv.report.can_export());
    assert_eq!(csv.artifacts.len(), 4);
    assert_eq!(
        csv.artifacts
            .iter()
            .take(2)
            .map(|artifact| artifact
                .path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned())
            .collect::<Vec<_>>(),
        vec!["export-Report-1.csv", "export-Report-2.csv"]
    );
    assert_eq!(
        csv.artifacts[2].path.file_name().unwrap().to_string_lossy(),
        "export-2026_11__summary_.csv"
    );
    assert_eq!(
        csv.artifacts[3].path.file_name().unwrap().to_string_lossy(),
        "export-Report-1-4.csv"
    );
    assert!(
        csv.report
            .entries
            .iter()
            .any(|entry| { entry.sheet_order == Some(2) && entry.reason.contains("表番号 2") })
    );
    assert!(
        csv.report
            .entries
            .iter()
            .any(|entry| { entry.sheet_order == Some(4) && entry.reason.contains("衝突") })
    );
}

#[test]
// Verify that export preflight checks cancellation during conversion.
fn export_preflight_checks_cancellation_between_conversion_steps() {
    let source = markdown_source("| h |\n| --- |\n| v |\n");
    let request = request_for_source(
        ExportSource::Markdown(source),
        ExportFormat::Csv,
        FormulaOutput::Formula,
        ModifierOutput::Convert,
        MarkdownHeader::UseFirstRow,
        "out.csv",
    );
    let cancelled = AtomicBool::new(true);
    assert!(
        prepare_export_cancellable(&request, None, &cancelled)
            .expect_err("cancelled preflight")
            .resolve(crate::i18n::Language::Japanese)
            .contains("キャンセル")
    );
}

#[test]
// Writes Markdown `<br>` line breaks as real line breaks in CSV and TSV output.
fn markdown_line_break_tags_become_newlines_in_csv_and_tsv() {
    for (destination, modifiers, expected) in [
        ("breaks.csv", ModifierOutput::Convert, "value\n\"a\nb\"\n"),
        ("breaks.csv", ModifierOutput::TextOnly, "value\n\"a\nb\"\n"),
        ("breaks.tsv", ModifierOutput::Convert, "value\n\"a\nb\"\n"),
    ] {
        let format = if destination.ends_with(".tsv") {
            ExportFormat::Tsv
        } else {
            ExportFormat::Csv
        };
        let mut request = request_for_source(
            ExportSource::Markdown(markdown_source("| value |\n| --- |\n| a<br>b |\n")),
            format,
            FormulaOutput::Formula,
            modifiers,
            MarkdownHeader::UseFirstRow,
            destination,
        );
        request.options.scope = ExportScope::Selected(request.tables[0].id);
        let prepared = prepare_export_cancellable(&request, None, &AtomicBool::new(false))
            .expect("the line break exports");
        assert_eq!(
            std::str::from_utf8(&prepared.artifacts[0].bytes).expect("UTF-8 output"),
            expected,
            "{destination} {modifiers:?}"
        );
    }
}

#[test]
// Writes TSV with tabs, and rewrites a CSV source only when its delimiter does not fit.
fn tsv_output_uses_tabs_and_keeps_only_a_matching_csv_delimiter() {
    let mut markdown = request_for_source(
        ExportSource::Markdown(markdown_source("| a | b |\n| --- | --- |\n| 1 | x,y |\n")),
        ExportFormat::Tsv,
        FormulaOutput::Formula,
        ModifierOutput::Convert,
        MarkdownHeader::UseFirstRow,
        "table.tsv",
    );
    markdown.options.scope = ExportScope::Selected(markdown.tables[0].id);
    let prepared = prepare_export(&markdown, None).expect("Markdown exports as TSV");
    assert_eq!(prepared.artifacts[0].bytes, b"a\tb\n1\tx,y\n");

    for (bytes, delimiter, format, destination, expected) in [
        (
            &b"a,b\n1,2\n"[..],
            ',',
            ExportFormat::Tsv,
            "out.tsv",
            &b"a\tb\n1\t2\n"[..],
        ),
        (
            &b"a\tb\n1\t2\n"[..],
            '\t',
            ExportFormat::Csv,
            "out.csv",
            &b"a,b\n1,2\n"[..],
        ),
        (
            &b"a\tb\n1\t2\n"[..],
            '\t',
            ExportFormat::Tsv,
            "out.tsv",
            &b"a\tb\n1\t2\n"[..],
        ),
        (
            &b"a;b\n1;2\n"[..],
            ';',
            ExportFormat::Csv,
            "out.csv",
            &b"a;b\n1;2\n"[..],
        ),
    ] {
        let source = Arc::new(
            CsvDocument::parse(
                bytes.to_vec(),
                crate::csv_adapter::EncodingChoice::Auto,
                delimiter,
            )
            .expect("csv"),
        );
        let mut request = request_for_source(
            ExportSource::Csv(source),
            format,
            FormulaOutput::Formula,
            ModifierOutput::Convert,
            MarkdownHeader::UseFirstRow,
            destination,
        );
        request.options.scope = ExportScope::Selected(request.tables[0].id);
        let prepared = prepare_export(&request, None).expect("CSV source exports");
        assert_eq!(
            prepared.artifacts[0].bytes, expected,
            "{delimiter:?} source as {format:?}"
        );
    }
    assert!(ExportFormat::Tsv.matches_path(std::path::Path::new("x.TSV")));
    assert!(!ExportFormat::Csv.matches_path(std::path::Path::new("x.tsv")));
}
