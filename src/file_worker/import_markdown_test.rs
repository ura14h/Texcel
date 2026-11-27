use super::*;
use std::collections::BTreeMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[test]
// Converts HTML formatting and keeps the original file as an import source.
fn html_import_builds_named_markdown_table_without_changing_source() {
    let path = temporary_path("html-import", "html");
    let input = "<table><caption>Items</caption><tr><th align=\"right\">Name</th><th>Value</th></tr><tr><td align=\"center\"><code>raw</code><br>text</td><td><a href=\"https://one.example\">first</a> and <a href=\"https://two.example\"><strong>second</strong></a></td></tr></table>";
    std::fs::write(&path, input).expect("write HTML source");
    let original_identity = FileIdentity::read(&path).expect("read original identity");
    let worker = FileWorker::new();
    let token = JobToken {
        document_id: 8801,
        revision: 0,
        job_id: 1,
    };
    worker
        .import_html_with_language(token, path.clone(), crate::i18n::Language::English)
        .expect("queue HTML import");
    let result = wait_for_result(&worker);
    let FileResultKind::ImportedMarkdown {
        source,
        document,
        identity,
        report,
        input_format,
        ..
    } = result.kind
    else {
        panic!("HTML import returns a Markdown editing model");
    };

    let markdown = std::str::from_utf8(source.source_bytes()).expect("Markdown is UTF-8");
    assert_eq!(input_format, ImportedFormat::Html);
    assert_eq!(source.tables[0].name, "Items");
    assert!(markdown.starts_with("## Items\n\n"));
    assert!(
        markdown.contains("`raw`<br>text"),
        "code and breaks are represented: {markdown}"
    );
    assert!(markdown.contains("[first](https://one.example)"));
    assert!(markdown.contains("[**second**](https://two.example)"));
    assert!(
        markdown.contains("| :---: | --- |"),
        "body alignment is kept while unaligned columns stay default"
    );
    assert_eq!(
        document.tables[0]
            .cell(crate::document::CellAddress { row: 1, column: 0 })
            .map(|cell| cell.input.source_text()),
        Some("`raw`<br>text".to_owned())
    );
    assert!(report.entries.iter().any(|entry| {
        entry.message.as_ref()
            == Some(&crate::i18n::Message::new(
                "report-reason-html-markdown-alignment",
            ))
    }));
    assert_eq!(identity, original_identity);
    assert_eq!(
        std::fs::read(&path).expect("read source again"),
        input.as_bytes()
    );
    std::fs::remove_file(path).expect("remove HTML fixture");
}

#[test]
// Ignores header styles and reports mixed body alignment without a content warning.
fn html_alignment_uses_nonempty_body_cells_and_reports_mixed_columns() {
    let input = "<table><tr><th align=\"right\">Uniform</th><th>Mixed</th></tr><tr><td align=\"center\">a</td><td align=\"left\">x</td></tr><tr><td align=\"center\">b</td><td>y</td></tr></table>";
    let source = Arc::new(HtmlDocument::parse(input.as_bytes().to_vec()).expect("HTML parses"));
    let (markdown, _, report) = html_to_markdown(
        source,
        8815,
        "Alignment.html".to_owned(),
        crate::i18n::Language::English,
        &AtomicBool::new(false),
    )
    .expect("HTML imports as Markdown");

    let output = String::from_utf8_lossy(markdown.source_bytes());
    assert!(output.contains("| :---: | --- |"), "{output}");
    assert!(report.entries.iter().any(|entry| {
        entry.impact == crate::conversion_report::ConversionImpact::Formatting
            && entry.message.as_ref()
                == Some(&crate::i18n::Message::new(
                    "report-reason-markdown-column-alignment-mixed",
                ))
    }));
}

#[test]
// Keeps only uniform XLSX body alignment and reports mixed columns as formatting loss.
fn xlsx_alignment_ignores_header_and_reports_mixed_body_columns() {
    let mut cells = BTreeMap::new();
    let add_cell =
        |cells: &mut BTreeMap<crate::document::CellAddress, crate::xlsx_adapter::XlsxCell>,
         row,
         column,
         value: &str,
         alignment: Option<&str>| {
            cells.insert(
                crate::document::CellAddress { row, column },
                crate::xlsx_adapter::XlsxCell {
                    value: crate::xlsx_adapter::XlsxValue::String(value.to_owned()),
                    formula: None,
                    style: crate::xlsx_adapter::XlsxStyle {
                        horizontal_alignment: alignment.map(str::to_owned),
                        ..crate::xlsx_adapter::XlsxStyle::default()
                    },
                    hyperlink: None,
                },
            );
        };
    add_cell(&mut cells, 0, 0, "Uniform", Some("right"));
    add_cell(&mut cells, 0, 1, "Mixed", Some("center"));
    add_cell(&mut cells, 1, 0, "a", Some("center"));
    add_cell(&mut cells, 1, 1, "x", Some("left"));
    add_cell(&mut cells, 2, 0, "b", Some("center"));
    add_cell(&mut cells, 2, 1, "y", None);
    let source = Arc::new(XlsxDocument {
        date_1904: false,
        sheets: vec![crate::xlsx_adapter::XlsxSheet {
            table_id: 8816,
            name: "Sheet".to_owned(),
            cells,
            columns: Vec::new(),
            rows: Vec::new(),
            merges: Vec::new(),
            frozen_rows: 0,
            frozen_columns: 0,
            freeze_top_left: "A1".to_owned(),
            rule_losses: Vec::new(),
        }],
    });
    let (markdown, _, report) = workbook_to_markdown(
        source,
        8816,
        "Alignment.xlsx".to_owned(),
        "XLSX",
        crate::i18n::Language::English,
        Vec::new(),
        &AtomicBool::new(false),
    )
    .expect("XLSX imports as Markdown");

    let output = String::from_utf8_lossy(markdown.source_bytes());
    assert!(output.contains("| :---: | --- |"), "{output}");
    assert!(report.entries.iter().any(|entry| {
        entry.impact == crate::conversion_report::ConversionImpact::Formatting
            && entry.message.as_ref()
                == Some(&crate::i18n::Message::new(
                    "report-reason-markdown-column-alignment-mixed",
                ))
    }));
}

#[test]
// Keeps HTML entity text literal instead of parsing it as new HTML markup.
fn html_import_does_not_reparse_decoded_markup_text() {
    let path = temporary_path("html-literal-markup", "html");
    let input = "<table><tr><th>Tag</th><th>Script</th></tr><tr><td>&lt;b&gt;A&lt;/b&gt;</td><td>&lt;script&gt;x&lt;/script&gt;</td></tr></table>";
    std::fs::write(&path, input).expect("write HTML source");
    let worker = FileWorker::new();
    let token = JobToken {
        document_id: 8810,
        revision: 0,
        job_id: 10,
    };
    worker
        .import_html_with_language(token, path.clone(), crate::i18n::Language::English)
        .expect("queue HTML import");
    let result = wait_for_result(&worker);
    let FileResultKind::ImportedMarkdown { source, .. } = result.kind else {
        panic!("HTML import returns a Markdown document");
    };
    let text_at = |column| {
        let cell = &source.tables[0].rows[1][column];
        source
            .cell_display_text_for_search(0, 1, column, &cell.value, crate::i18n::Language::English)
            .expect("the imported Markdown cell has cached display text")
    };
    assert_eq!(text_at(0), "<b>A</b>");
    assert_eq!(text_at(1), "<script>x</script>");
    assert!(
        source
            .cell_display(0, 1, 0)
            .unwrap()
            .runs
            .iter()
            .all(|run| !run.bold)
    );
    assert!(
        source
            .cell_display(0, 1, 1)
            .unwrap()
            .runs
            .iter()
            .all(|run| !run.italic)
    );
    std::fs::remove_file(path).expect("remove HTML source");
}

#[test]
// Reports nested content as omitted and verifies that only the outer cell remains.
fn html_import_report_matches_the_nested_table_that_was_kept() {
    let input = "<table><tr><td>outer<table><tr><td>inner</td></tr></table></td></tr></table>";
    let source =
        Arc::new(HtmlDocument::parse(input.as_bytes().to_vec()).expect("nested HTML parses"));
    let (markdown, _, report) = html_to_markdown(
        source,
        8812,
        "Nested".to_owned(),
        crate::i18n::Language::English,
        &AtomicBool::new(false),
    )
    .expect("nested HTML converts to one Markdown table");

    assert_eq!(markdown.tables.len(), 1);
    assert_eq!(markdown.tables[0].rows[0][0].value, "outer");
    assert!(!String::from_utf8_lossy(markdown.source_bytes()).contains("inner"));
    assert!(report.entries.iter().any(|entry| {
        entry.class == ConversionClass::Omitted
            && entry.message.as_ref()
                == Some(&crate::i18n::Message::new(
                    "report-reason-html-nested-table-omitted",
                ))
    }));
    assert!(report.summary().omitted >= 1);
    assert!(
        report
            .to_text(crate::i18n::Language::English)
            .contains("outer HTML table was kept")
    );
}

#[test]
// Keeps an unsupported HTML style diagnostic when importing its display text.
fn html_import_reports_unsupported_styles_after_text_conversion() {
    let input = "<table><tr><td><span style=\"color:red\">red</span></td></tr></table>";
    let source = Arc::new(HtmlDocument::parse(input.as_bytes().to_vec()).expect("HTML parses"));
    let (markdown, _, report) = html_to_markdown(
        source,
        8814,
        "Styled".to_owned(),
        crate::i18n::Language::English,
        &AtomicBool::new(false),
    )
    .expect("HTML converts into a Markdown document");

    assert!(
        markdown
            .source_bytes()
            .windows(3)
            .any(|bytes| bytes == b"red")
    );
    assert!(report.entries.iter().any(|entry| {
        entry.class == ConversionClass::Converted
            && entry.message.as_ref()
                == Some(&crate::i18n::Message::new("report-reason-html-markup-text"))
    }));
}

#[test]
// Reports fallback outcomes and preserves a leading apostrophe in rich Markdown text.
fn xlsx_import_reports_empty_fallbacks_and_keeps_rich_formula_text_literal() {
    let mut cells = BTreeMap::new();
    for (column, value) in [
        (
            0,
            crate::xlsx_adapter::XlsxValue::String("Value".to_owned()),
        ),
        (
            1,
            crate::xlsx_adapter::XlsxValue::String("Count".to_owned()),
        ),
        (2, crate::xlsx_adapter::XlsxValue::String("Pipe".to_owned())),
        (
            3,
            crate::xlsx_adapter::XlsxValue::String("External".to_owned()),
        ),
        (
            4,
            crate::xlsx_adapter::XlsxValue::String("Invalid".to_owned()),
        ),
        (
            5,
            crate::xlsx_adapter::XlsxValue::String("Literal".to_owned()),
        ),
    ] {
        cells.insert(
            crate::document::CellAddress { row: 0, column },
            crate::xlsx_adapter::XlsxCell {
                value,
                formula: None,
                style: crate::xlsx_adapter::XlsxStyle::default(),
                hyperlink: None,
            },
        );
    }
    let bold = crate::xlsx_adapter::XlsxStyle {
        bold: true,
        ..Default::default()
    };
    for (address, value, formula, style) in [
        (
            crate::document::CellAddress { row: 1, column: 0 },
            crate::xlsx_adapter::XlsxValue::Number(5.0),
            None,
            bold,
        ),
        (
            crate::document::CellAddress { row: 1, column: 1 },
            crate::xlsx_adapter::XlsxValue::Number(1.0),
            Some("COUNT(A2)"),
            crate::xlsx_adapter::XlsxStyle::default(),
        ),
        (
            crate::document::CellAddress { row: 1, column: 2 },
            crate::xlsx_adapter::XlsxValue::String("a|b".to_owned()),
            Some("\"a|b\""),
            crate::xlsx_adapter::XlsxStyle::default(),
        ),
        (
            crate::document::CellAddress { row: 1, column: 3 },
            crate::xlsx_adapter::XlsxValue::Empty,
            Some("Other!A1"),
            crate::xlsx_adapter::XlsxStyle::default(),
        ),
        (
            crate::document::CellAddress { row: 1, column: 4 },
            crate::xlsx_adapter::XlsxValue::Empty,
            Some("SUM("),
            crate::xlsx_adapter::XlsxStyle::default(),
        ),
        (
            crate::document::CellAddress { row: 1, column: 5 },
            crate::xlsx_adapter::XlsxValue::RichText(vec![crate::xlsx_adapter::XlsxTextRun {
                text: "'=1+1".to_owned(),
                bold: true,
                ..crate::xlsx_adapter::XlsxTextRun::default()
            }]),
            None,
            crate::xlsx_adapter::XlsxStyle::default(),
        ),
    ] {
        cells.insert(
            address,
            crate::xlsx_adapter::XlsxCell {
                value,
                formula: formula.map(str::to_owned),
                style,
                hyperlink: None,
            },
        );
    }
    let workbook = Arc::new(XlsxDocument {
        date_1904: false,
        sheets: vec![crate::xlsx_adapter::XlsxSheet {
            table_id: 32,
            name: "Main".to_owned(),
            cells,
            columns: Vec::new(),
            rows: Vec::new(),
            merges: Vec::new(),
            frozen_rows: 0,
            frozen_columns: 0,
            freeze_top_left: "A1".to_owned(),
            rule_losses: Vec::new(),
        }],
    });
    let cancelled = AtomicBool::new(false);
    let (source, document, report) = workbook_to_markdown(
        workbook,
        8811,
        "Main".to_owned(),
        "XLSX",
        crate::i18n::Language::English,
        Vec::new(),
        &cancelled,
    )
    .expect("XLSX becomes a Markdown document");

    assert_eq!(source.tables[0].rows[1][0].value, "**5**");
    assert_eq!(source.tables[0].rows[1][2].value, "=\"a\\|b\"");
    assert_eq!(source.tables[0].rows[1][5].value, "**''=1+1**");
    assert!(report.entries.iter().any(|entry| {
        entry.message.as_ref()
            == Some(&crate::i18n::Message::new(
                "report-reason-xlsx-external-formula-empty",
            ))
    }));
    assert!(
        report.entries.iter().any(|entry| {
            entry.message.as_ref()
                == Some(&crate::i18n::Message::new(
                    "report-reason-xlsx-invalid-formula-empty",
                ))
        }),
        "unexpected import report: {:?}",
        report.entries
    );
    assert!(!report.entries.iter().any(|entry| {
        entry.message.as_ref() == Some(&crate::i18n::Message::new("report-reason-formula-csv-text"))
    }));
    let calculation = crate::calc::CalculationSnapshot::from_document(&document, false);
    assert_eq!(
        source
            .cell_display_text_for_search(
                0,
                1,
                5,
                &source.tables[0].rows[1][5].value,
                crate::i18n::Language::English,
            )
            .expect("rich literal display resolves"),
        "'=1+1"
    );
    let cell_input = |row, column| {
        &calculation.tables[0]
            .cells
            .iter()
            .find(|(address, _)| *address == crate::document::CellAddress { row, column })
            .expect("calculation cell exists")
            .1
            .input
    };
    assert_eq!(
        cell_input(1, 0),
        &crate::document::CellInput::Text("5".to_owned())
    );
    assert_eq!(
        cell_input(1, 2),
        &crate::document::CellInput::Text("=\"a|b\"".to_owned())
    );
    assert_eq!(
        cell_input(1, 5),
        &crate::document::CellInput::Text("''=1+1".to_owned())
    );
    assert_eq!(
        crate::document::literal_text_value(cell_input(1, 5).source_text().as_str()),
        "'=1+1"
    );
}

#[test]
// Verify that XLSX merges collapse to empty followers and report lost formatting.
fn xlsx_import_collapses_merges_and_reports_unsupported_style() {
    let mut cells = BTreeMap::new();
    let address = |row, column| crate::document::CellAddress { row, column };
    for (cell_address, value, style) in [
        (
            address(0, 0),
            crate::xlsx_adapter::XlsxValue::String("Heading".to_owned()),
            crate::xlsx_adapter::XlsxStyle::default(),
        ),
        (
            address(0, 1),
            crate::xlsx_adapter::XlsxValue::String("covered value".to_owned()),
            crate::xlsx_adapter::XlsxStyle::default(),
        ),
        (
            address(1, 0),
            crate::xlsx_adapter::XlsxValue::String("body".to_owned()),
            crate::xlsx_adapter::XlsxStyle {
                fill_color: Some("#ffeecc".to_owned()),
                ..crate::xlsx_adapter::XlsxStyle::default()
            },
        ),
    ] {
        cells.insert(
            cell_address,
            crate::xlsx_adapter::XlsxCell {
                value,
                formula: None,
                style,
                hyperlink: None,
            },
        );
    }
    let source = Arc::new(XlsxDocument {
        date_1904: false,
        sheets: vec![crate::xlsx_adapter::XlsxSheet {
            table_id: 1,
            name: "Main".to_owned(),
            cells,
            columns: Vec::new(),
            rows: Vec::new(),
            merges: vec!["A1:B1".to_owned()],
            frozen_rows: 0,
            frozen_columns: 0,
            freeze_top_left: "A1".to_owned(),
            rule_losses: Vec::new(),
        }],
    });

    let (markdown, document, report) = workbook_to_markdown(
        source,
        8821,
        "Main".to_owned(),
        "XLSX",
        crate::i18n::Language::English,
        Vec::new(),
        &AtomicBool::new(false),
    )
    .expect("XLSX data becomes one Markdown source");

    assert_eq!(markdown.tables[0].rows[0][0].value, "Heading");
    assert_eq!(markdown.tables[0].rows[0][1].value, "");
    assert!(
        document.tables[0]
            .cell(address(0, 1))
            .is_none_or(|cell| cell.input.source_text().is_empty())
    );
    assert!(report.entries.iter().any(|entry| {
        entry.message.as_ref() == Some(&crate::i18n::Message::new("report-reason-merge-collapsed"))
    }));
    assert!(report.entries.iter().any(|entry| {
        entry.message.as_ref()
            == Some(&crate::i18n::Message::new(
                "report-reason-cell-style-unsupported",
            ))
    }));
}

#[test]
// Normalizes same-sheet formulas and keeps external values and literal text safe.
fn xlsx_import_uses_markdown_formula_scope_and_literal_markers() {
    let path = temporary_path("xlsx-import", "xlsx");
    // Build the fixture directly with umya-spreadsheet; Texcel itself never writes XLSX.
    let mut workbook = umya_spreadsheet::new_file();
    workbook
        .set_sheet_name(0, "Main")
        .expect("rename the first sheet");
    let sheet = workbook
        .sheet_by_name_mut("Main")
        .expect("the renamed sheet exists");
    sheet.cell_value_mut("A1").set_value_string("Item");
    sheet.cell_value_mut("B1").set_value_string("Value");
    sheet.cell_value_mut("A2").set_value_string("=literal");
    sheet
        .cell_value_mut("B2")
        .set_formula("Main!A1")
        .set_formula_result_number(11.0);
    sheet
        .cell_value_mut("C2")
        .set_formula("Other!A1")
        .set_formula_result_number(77.0);
    sheet
        .cell_value_mut("D2")
        .set_formula("\"!\"")
        .set_formula_result_string("!");
    let mut bytes = std::io::Cursor::new(Vec::new());
    umya_spreadsheet::writer::xlsx::write_writer(&workbook, &mut bytes)
        .expect("write XLSX fixture");
    std::fs::write(&path, bytes.into_inner()).expect("write XLSX source");
    let original = std::fs::read(&path).expect("read source bytes");
    let original_identity = FileIdentity::read(&path).expect("read original identity");
    let worker = FileWorker::new();
    let token = JobToken {
        document_id: 8802,
        revision: 0,
        job_id: 2,
    };
    worker
        .import_workbook_with_language(
            token,
            path.clone(),
            WorkbookFormat::Xlsx,
            crate::i18n::Language::English,
        )
        .expect("queue XLSX import");
    let result = wait_for_result(&worker);
    let FileResultKind::ImportedMarkdown {
        source,
        identity,
        report,
        input_format,
        ..
    } = result.kind
    else {
        panic!("XLSX import returns a Markdown editing model");
    };

    let table = &source.tables[0];
    assert_eq!(table.name, "Main");
    assert_eq!(table.rows[1][0].value, "'=literal");
    assert_eq!(table.rows[1][1].value, "=A1");
    assert_eq!(table.rows[1][2].value, "77");
    assert_eq!(table.rows[1][3].value, "=\"!\"");
    assert_eq!(input_format, ImportedFormat::Xlsx);
    assert!(report.entries.iter().any(|entry| {
        entry.message.as_ref()
            == Some(&crate::i18n::Message::new(
                "report-reason-xlsx-external-formula-value",
            ))
    }));
    assert_eq!(identity, original_identity);
    assert_eq!(std::fs::read(&path).expect("read source again"), original);
    std::fs::remove_file(path).expect("remove XLSX fixture");
}

#[test]
// Avoids treating an exclamation mark inside a string as a sheet reference.
fn formula_import_uses_ast_for_reference_scope() {
    let names = vec!["Main".to_owned(), "Other".to_owned()];
    let address = crate::document::CellAddress { row: 0, column: 1 };
    assert_eq!(
        normalize_import_formula("Main!A1", &names, 0, address),
        FormulaImport::Local("A1".to_owned())
    );
    assert_eq!(
        normalize_import_formula("\"!\"", &names, 0, address),
        FormulaImport::Local("\"!\"".to_owned())
    );
    assert_eq!(
        normalize_import_formula("Other!A1", &names, 0, address),
        FormulaImport::External
    );
}

#[test]
// Reopens Markdown through the production loader and recalculates styled values.
fn markdown_load_recalculates_a_styled_number_as_a_number() {
    let path = temporary_path("markdown-styled-number", "md");
    let input = "| Value | Count |\n| --- | --- |\n| **5** | =COUNT(A2) |\n";
    std::fs::write(&path, input).expect("write Markdown source");
    let worker = FileWorker::new();
    let token = JobToken {
        document_id: 8813,
        revision: 0,
        job_id: 13,
    };
    worker
        .load_markdown_with_language(token, path.clone(), crate::i18n::Language::English)
        .expect("queue Markdown load");
    let result = wait_for_result(&worker);
    let FileResultKind::LoadedMarkdown {
        calculation,
        document,
        ..
    } = result.kind
    else {
        panic!("the Markdown loader returns an editable document");
    };
    let value = calculation.tables[0]
        .cells
        .iter()
        .find(|(address, _)| *address == crate::document::CellAddress { row: 1, column: 0 })
        .expect("styled number exists");
    assert_eq!(
        value.1.input,
        crate::document::CellInput::Text("5".to_owned())
    );

    let calculator = crate::calc::CalculationWorker::new();
    let token = JobToken {
        document_id: document.id,
        revision: document.calculation_revision,
        job_id: 14,
    };
    calculator
        .initialize(token, calculation)
        .expect("enqueue reopened calculation");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(result) = calculator.poll().into_iter().next() {
            assert_eq!(result.token, token);
            assert!(result.error.is_none(), "{:?}", result.error);
            let values = result.full_snapshot.expect("calculation snapshot");
            assert_eq!(
                values.get(1, crate::document::CellAddress { row: 1, column: 1 }),
                Some((
                    crate::document::CalcStatus::Fresh,
                    Some(&crate::document::CalcValue::Number(1.0))
                ))
            );
            break;
        }
        assert!(Instant::now() < deadline, "calculation completes");
        std::thread::sleep(Duration::from_millis(5));
    }
    std::fs::remove_file(path).expect("remove Markdown source");
}

fn temporary_path(label: &str, extension: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock is after the epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("texcel-{label}-{unique}.{extension}"))
}

fn wait_for_result(worker: &FileWorker) -> FileResult {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(result) = worker.poll().into_iter().next() {
            return result;
        }
        assert!(Instant::now() < deadline, "file import completes");
        std::thread::sleep(Duration::from_millis(5));
    }
}
