use std::io::{Cursor, Write};

use zip::{ZipWriter, write::SimpleFileOptions};

use super::*;

/// Wraps a sheet body in a minimal `content.xml` with the usual namespace prefixes.
fn content(automatic_styles: &str, body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="o" xmlns:style="s" xmlns:text="t" xmlns:table="ta" xmlns:fo="f" xmlns:xlink="x" xmlns:calcext="c">
<office:automatic-styles>{automatic_styles}</office:automatic-styles>
<office:body><office:spreadsheet>{body}</office:spreadsheet></office:body>
</office:document-content>"#
    )
}

/// Builds an ODS package from explicit parts, in the given order.
fn package(parts: &[(&str, &str)]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, text) in parts {
        writer
            .start_file(*name, SimpleFileOptions::default())
            .expect("start ZIP entry");
        writer.write_all(text.as_bytes()).expect("write ZIP entry");
    }
    writer.finish().expect("finish ZIP").into_inner()
}

/// Builds a spreadsheet package with one `content.xml`.
fn ods(content: &str) -> Vec<u8> {
    package(&[
        ("mimetype", "application/vnd.oasis.opendocument.spreadsheet"),
        ("content.xml", content),
    ])
}

/// Reads the checked-in sample workbook.
fn sample() -> OdsImport {
    parse_ods(include_bytes!("../samples/workbook.ods")).expect("parse the sample ODS")
}

/// Looks up one sample cell by sheet index and A1 reference.
fn cell<'a>(import: &'a OdsImport, sheet: usize, reference: &str) -> &'a XlsxCell {
    let address = crate::xlsx_adapter::parse_cell_reference(reference).expect("valid reference");
    import.workbook.sheets[sheet]
        .cells
        .get(&address)
        .unwrap_or_else(|| panic!("cell {reference} exists"))
}

/// Says whether the report has an entry with the given message key.
fn reports(entries: &[ConversionEntry], key: &str) -> bool {
    entries.iter().any(|entry| {
        entry
            .message
            .as_ref()
            .is_some_and(|message| message.key() == key)
    })
}

#[test]
/// Reads sheet names, typed values, and formulas without expanding the repeated empty rows.
fn sample_values_and_formulas() {
    let import = sample();
    let names = import
        .workbook
        .sheets
        .iter()
        .map(|sheet| sheet.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, ["売上", "型と書式"]);
    assert_eq!(import.workbook.sheets[0].cells.len(), 14);
    assert_eq!(cell(&import, 0, "B2").value, XlsxValue::Number(120.0));
    let total = cell(&import, 0, "D4");
    assert_eq!(total.formula.as_deref(), Some("SUM(D2:D3)"));
    assert_eq!(total.value, XlsxValue::Number(5200.0));
    assert!(
        cell(&import, 0, "A1").style.bold,
        "named parent style is bold"
    );

    assert_eq!(
        cell(&import, 1, "A1").value,
        XlsxValue::String("00123".to_owned())
    );
    assert_eq!(cell(&import, 1, "B1").value, XlsxValue::Boolean(true));
    let date = cell(&import, 1, "A2");
    assert_eq!(date.value, XlsxValue::Number(45292.0));
    assert!(
        date.style.number_format.is_some(),
        "date display is reported"
    );
    assert_eq!(cell(&import, 1, "B2").value, XlsxValue::Number(0.25));
    // Another sheet's reference is kept for the shared import to replace with its value.
    assert_eq!(cell(&import, 1, "A6").formula.as_deref(), Some("'売上'!D4"));
    // The `;` inside the string stays; the argument separators become commas.
    assert_eq!(
        cell(&import, 1, "B6").formula.as_deref(),
        Some("IF(A1=\"a;b\",1,2)")
    );
}

#[test]
/// Keeps spans, links, paragraphs, and explicit spaces from cell text.
fn sample_text_runs_links_and_spacing() {
    let import = sample();
    let XlsxValue::RichText(runs) = &cell(&import, 1, "A3").value else {
        panic!("a partly italic cell keeps its runs");
    };
    assert_eq!(runs.len(), 2);
    assert!(runs[0].italic && runs[0].text == "重要");
    assert!(!runs[1].italic && runs[1].text == "な連絡");
    let link = cell(&import, 1, "B3");
    assert_eq!(link.value, XlsxValue::String("Example".to_owned()));
    assert_eq!(
        link.hyperlink.as_ref().map(|link| link.target.as_str()),
        Some("https://example.com/")
    );
    assert_eq!(
        cell(&import, 1, "A4").value,
        XlsxValue::String("1行目\n2行目".to_owned())
    );
    assert_eq!(
        cell(&import, 1, "B4").value,
        XlsxValue::String("a  b".to_owned())
    );
    // The annotation's own paragraph does not leak into the cell text.
    assert_eq!(
        cell(&import, 1, "A7").value,
        XlsxValue::String("背景色".to_owned())
    );
}

#[test]
/// Records merges, hidden rows and columns inside the content, and dropped elements.
fn sample_structure_losses() {
    let import = sample();
    assert_eq!(import.workbook.sheets[1].merges, ["A5:B5"]);
    let hidden_columns = import.workbook.sheets[0]
        .columns
        .iter()
        .map(|column| column.index)
        .collect::<Vec<_>>();
    assert_eq!(hidden_columns, [4]);
    let hidden_rows = import.workbook.sheets[1]
        .rows
        .iter()
        .map(|row| row.index)
        .collect::<Vec<_>>();
    assert_eq!(hidden_rows, [7]);
    assert!(reports(
        &import.entries,
        "report-reason-ods-unsupported-element"
    ));
    let fill = &cell(&import, 1, "A7").style;
    assert_eq!(fill.fill_color.as_deref(), Some("#ffff00"));
    assert_eq!(
        cell(&import, 1, "B7").style.horizontal_alignment.as_deref(),
        Some("right")
    );
}

#[test]
/// Rewrites OpenFormula references and separators, and refuses what has no A1 form.
fn open_formula_conversion() {
    let cases = [
        ("of:=[.A1]+[.$B$2]", Some("A1+$B$2")),
        ("of:=SUM([.A1:.B3];1)", Some("SUM(A1:B3,1)")),
        ("of:=[$'My ''Q'' Sheet'.A1]", Some("'My ''Q'' Sheet'!A1")),
        ("of:=[$Data.A1:.A9]", Some("'Data'!A1:A9")),
        ("of:=SUM([.A:.A])", Some("SUM(A:A)")),
        (
            "of:=IF(ISERROR([.A1]);#DIV/0!;{1;2|3;4})",
            Some("IF(ISERROR(A1),#DIV/0!,{1,2;3,4})"),
        ),
        (
            "of:=COM.MICROSOFT.CONCAT([.A1];\"x\")",
            Some("CONCAT(A1,\"x\")"),
        ),
        ("of:=[.#REF!]", Some("#REF!")),
        ("msoxl:=SUM(A1:A2)", Some("SUM(A1:A2)")),
        ("of:=[.A1:.B2]~[.C3]", None),
        ("of:=[.A1:.B2]![.B1:.C2]", None),
        ("of:=['file:///x.ods'#$Sheet1.A1]", None),
        ("of:=[$A.A1:$B.B2]", None),
        ("foreign:=A1", None),
        ("of:=\"unterminated", None),
    ];
    for (raw, expected) in cases {
        assert_eq!(convert_open_formula(raw).as_deref(), expected, "{raw}");
    }
}

#[test]
/// A formula with no A1 form keeps its saved value and is reported at its cell.
fn unconvertible_formula_keeps_saved_value() {
    let bytes = ods(&content(
        "",
        r#"<table:table table:name="S"><table:table-row>
<table:table-cell table:formula="of:=[.A2]~[.A3]" office:value-type="float" office:value="9"><text:p>9</text:p></table:table-cell>
</table:table-row></table:table>"#,
    ));
    let import = parse_ods(&bytes).expect("parse");
    let cell = &import.workbook.sheets[0].cells[&CellAddress::default()];
    assert_eq!(cell.formula, None);
    assert_eq!(cell.value, XlsxValue::Number(9.0));
    let entry = import
        .entries
        .iter()
        .find(|entry| entry.range.as_deref() == Some("A1"))
        .expect("the formula is reported at A1");
    assert_eq!(
        entry.message.as_ref().map(Message::key),
        Some("report-reason-ods-formula-value")
    );
}

#[test]
/// Rejects broken, foreign, encrypted, and oversized input with a message instead of a panic.
fn invalid_packages_fail_with_messages() {
    let key = |bytes: &[u8]| {
        parse_ods(bytes)
            .expect_err("the input is rejected")
            .key()
            .to_owned()
    };
    assert_eq!(key(b"not a zip"), "status-ods-archive-invalid");
    assert_eq!(
        key(&package(&[
            ("mimetype", "application/vnd.oasis.opendocument.text"),
            ("content.xml", &content("", "")),
        ])),
        "status-ods-not-spreadsheet"
    );
    assert_eq!(
        key(&package(&[
            ("META-INF/manifest.xml", "<manifest:encryption-data/>"),
            ("content.xml", "")
        ])),
        "status-ods-encrypted"
    );
    assert_eq!(
        key(&package(&[("mimetype", "")])),
        "status-ods-not-spreadsheet"
    );
    assert_eq!(
        key(&package(&[("styles.xml", "")])),
        "status-ods-content-missing"
    );
    assert_eq!(
        key(&ods("<office:body><table:table")),
        "status-ods-xml-invalid"
    );
    assert_eq!(key(&ods(&content("", ""))), "status-ods-no-sheets");
    // One value repeated down every row exceeds the cell limit before anything is expanded.
    let repeated = content(
        "",
        r#"<table:table table:name="S"><table:table-row table:number-rows-repeated="1048576">
<table:table-cell office:value-type="float" office:value="1"><text:p>1</text:p></table:table-cell>
</table:table-row></table:table>"#,
    );
    assert_eq!(key(&ods(&repeated)), "status-ods-too-many-cells");
}

#[test]
/// Converts ODF dates and durations into day serials.
fn date_and_time_serials() {
    assert_eq!(values::date_serial("2024-01-01"), Some(45292.0));
    assert_eq!(values::date_serial("2024-01-01T12:00:00"), Some(45292.5));
    assert_eq!(values::date_serial("not a date"), None);
    assert_eq!(values::time_serial("PT12H00M00S"), Some(0.5));
    assert_eq!(values::time_serial("P1DT6H"), Some(1.25));
    assert_eq!(values::time_serial("-PT6H"), Some(-0.25));
    assert_eq!(values::time_serial("PT1S2H"), None);
}
