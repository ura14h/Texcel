use super::*;

/// Marks generated HTML so tests can verify internal paste ownership.
pub(crate) fn mark_test_owned_html(html: &str) {
    let mark =
        read_internal_payload(html)
            .zip(payload_digest(html))
            .map(|(payload, payload_digest)| OwnedPayloadMark {
                owner_id: payload.owner_id,
                payload_digest,
            });
    set_owned_payload(mark);
}

/// Reads the private paste data using the owner embedded in this test payload.
pub(crate) fn read_owned_test_cells(text: &str, html: &str) -> Vec<Vec<ClipboardCell>> {
    let owner = read_internal_payload(html)
        .expect("the copy includes its source cells")
        .owner_id;
    let result = parse_clipboard_with_owner(text, Some(html), Some(&owner)).unwrap();
    assert_eq!(result.kind, ClipboardInputKind::Internal);
    result.ranges.into_iter().next().unwrap()
}

#[test]
// Verify that quoted TSV fields retain tabs and newlines inside their values.
fn parses_quoted_tsv_cells_and_keeps_embedded_newlines() {
    let parsed = parse_clipboard("alpha\t\"two\tparts\"\r\n\"line 1\nline 2\"\t3", None)
        .expect("quoted TSV is valid");
    assert_eq!(
        parsed.ranges[0][0][1].input,
        CellInput::Text("two\tparts".to_owned())
    );
    assert_eq!(
        parsed.ranges[0][1][0].input,
        CellInput::Text("line 1\nline 2".to_owned())
    );
    assert_eq!(parsed.ranges[0].len(), 2);
}

#[test]
// Verify that a single quoted field is parsed even without a delimiter.
fn parses_a_single_quoted_field_without_a_delimiter() {
    let parsed = parse_clipboard("\"say \"\"hello\"\"\"", None).expect("quoted field");
    assert_eq!(
        parsed.ranges[0][0][0].input,
        CellInput::Text("say \"hello\"".to_owned())
    );
}

#[test]
// Verify that blank rows remain present in plain one-column clipboard data.
fn plain_one_column_data_preserves_internal_blank_rows() {
    let parsed = parse_clipboard("上\n\n下", None).expect("plain rows parse");
    assert_eq!(parsed.ranges[0].len(), 3);
    assert_eq!(
        parsed.ranges[0][0][0].input,
        CellInput::Text("上".to_owned())
    );
    assert_eq!(parsed.ranges[0][1][0].input, CellInput::Empty);
    assert_eq!(
        parsed.ranges[0][2][0].input,
        CellInput::Text("下".to_owned())
    );
}

#[test]
// Verify that quoted multiline records stay aligned after plain one-column rows.
fn one_column_quoted_records_keep_embedded_newlines_after_plain_rows() {
    let parsed = parse_clipboard("a\n\"line 1\nline 2\"\nz", None)
        .expect("quoted record is parsed without a delimiter");
    assert_eq!(parsed.ranges[0].len(), 3);
    assert_eq!(
        parsed.ranges[0][0][0].input,
        CellInput::Text("a".to_owned())
    );
    assert_eq!(
        parsed.ranges[0][1][0].input,
        CellInput::Text("line 1\nline 2".to_owned())
    );
    assert_eq!(
        parsed.ranges[0][2][0].input,
        CellInput::Text("z".to_owned())
    );
}

#[test]
// Verify that delimiter detection ignores tabs inside quoted fields.
fn delimiter_detection_ignores_tabs_inside_quoted_fields() {
    let parsed = parse_clipboard("\"a\tb\",c", None).expect("CSV field can contain a tab");
    assert_eq!(parsed.ranges[0][0].len(), 2);
    assert_eq!(
        parsed.ranges[0][0][0].input,
        CellInput::Text("a\tb".to_owned())
    );
    assert_eq!(
        parsed.ranges[0][0][1].input,
        CellInput::Text("c".to_owned())
    );
}

#[test]
// Verify that internal clipboard data needs matching text and valid dimensions.
fn internal_payload_requires_matching_plain_text_and_valid_dimensions() {
    let matrix = vec![vec![ClipboardCell {
        input: CellInput::Number(42.0),
        style: CellStyle {
            bold: true,
            ..CellStyle::default()
        },
        hyperlink: None,
        rich_text: None,
        fresh_formula_value: None,
        formula_source: false,
        raw_markdown_source: false,
        markdown_plain_text: None,
        markdown_has_features: false,
    }]];
    let text = "42";
    let html = create_html_clipboard(
        std::slice::from_ref(&matrix),
        std::slice::from_ref(&matrix),
        &[],
        text,
    )
    .expect("HTML");
    let payload = read_internal_payload(&html).expect("payload");
    let parsed = parse_clipboard_with_owner(text, Some(&html), Some(&payload.owner_id))
        .expect("internal data");
    assert_eq!(parsed.kind, ClipboardInputKind::Internal);
    assert_eq!(parsed.ranges[0][0][0], matrix[0][0]);
    let changed = parse_clipboard_with_owner("changed", Some(&html), Some(&payload.owner_id))
        .expect("plain fallback");
    assert_eq!(changed.kind, ClipboardInputKind::Text);
    assert_eq!(
        changed.ranges[0][0][0].input,
        CellInput::Text("changed".to_owned())
    );
}

#[test]
// Verify that stored formula result types are checked before clipboard restore.
fn internal_formula_result_type_is_validated_before_restore() {
    let mut cell = ClipboardCell::from(&Cell::number(7.0));
    cell.fresh_formula_value = Some(CellInput::Number(7.0));
    assert!(validate_matrix(&[vec![cell]]).is_err());
}

#[test]
// Preserve text-formula metadata and its fresh value in the owned clipboard payload.
fn internal_text_formula_payload_keeps_its_fresh_result() {
    let cell = Cell::text("=1+1");
    let result = crate::document::CalcResult {
        revision: 3,
        status: CalcStatus::Fresh,
        value: Some(CalcValue::Number(2.0)),
    };
    let copied = ClipboardCell::from_cell_with_formula_context(&cell, true, Some(&result));
    let ranges = vec![vec![vec![copied]]];
    let html = create_html_clipboard(&ranges, &ranges, &[], "=1+1").expect("valid owned payload");
    let owner = read_internal_payload(&html)
        .expect("read generated payload")
        .owner_id;
    let restored = parse_clipboard_with_owner("=1+1", Some(&html), Some(&owner))
        .expect("restore owned payload");
    let restored_cell = &restored.ranges[0][0][0];
    assert!(restored_cell.formula_source);
    assert_eq!(
        restored_cell.fresh_formula_value,
        Some(CellInput::Number(2.0))
    );
}

#[test]
// Verify that internal clipboard data is accepted only from its current owner.
fn internal_payload_requires_the_current_clipboard_owner_id() {
    let matrix = vec![vec![ClipboardCell::from(&Cell::number(7.0))]];
    let matrix_ref = std::slice::from_ref(&matrix);
    let html = create_html_clipboard(matrix_ref, matrix_ref, &[], "7").expect("HTML");
    let payload = read_internal_payload(&html).expect("payload");
    let external = parse_clipboard_with_owner("7", Some(&html), Some("another-copy"))
        .expect("unowned HTML falls back to text");
    assert_eq!(external.kind, ClipboardInputKind::Html);
    assert_eq!(
        external.ranges[0][0][0].input,
        CellInput::Text("7".to_owned())
    );
    let owned = parse_clipboard_with_owner("7", Some(&html), Some(&payload.owner_id))
        .expect("owned HTML restores typed data");
    assert_eq!(owned.kind, ClipboardInputKind::Internal);
    assert_eq!(owned.ranges[0][0][0].input, CellInput::Number(7.0));
}

#[test]
// Verify that plain clipboard text cannot restore private cell metadata.
fn plain_text_alone_has_no_internal_payload_to_restore() {
    let plain_only = parse_clipboard("7", None).expect("plain fallback parses");
    assert_eq!(plain_only.kind, ClipboardInputKind::Text);
    assert_eq!(
        plain_only.ranges[0][0][0].input,
        CellInput::Text("7".to_owned())
    );
}

#[test]
// Verify that external HTML tables preserve supported styles and decode escaped text.
fn parses_external_html_table_styles_and_escaped_text() {
    let html =
        "<table><tr><td style=\"background-color:#ffeeaa\"><b>あ &amp; い</b></td></tr></table>";
    let parsed = parse_clipboard("あ & い", Some(html)).expect("html table");
    let cell = &parsed.ranges[0][0][0];
    assert_eq!(cell.input, CellInput::Text("あ & い".to_owned()));
    assert_eq!(cell.style.fill_color.as_deref(), Some("FFFFEEAA"));
    assert!(cell.rich_text.as_ref().is_some_and(|runs| runs[0].bold));
}

#[test]
// Verify that HTML headers and data cells keep their own style and rich text.
fn html_header_and_data_cells_keep_separate_style_and_rich_state() {
    let html = "<table><tr><th style=\"background-color:#ffeeaa\"><b>header</b></th><td>data</td></tr></table>";
    let parsed = parse_clipboard("header\tdata", Some(html)).expect("mixed HTML row");
    assert_eq!(parsed.kind, ClipboardInputKind::Html);
    assert_eq!(parsed.ranges[0][0].len(), 2);
    let header = &parsed.ranges[0][0][0];
    assert_eq!(header.input, CellInput::Text("header".to_owned()));
    assert_eq!(header.style.fill_color.as_deref(), Some("FFFFEEAA"));
    assert!(header.rich_text.as_ref().is_some_and(|runs| runs[0].bold));
    assert_eq!(
        parsed.ranges[0][0][1].input,
        CellInput::Text("data".to_owned())
    );
}

#[test]
// Verify that malformed HTML cell closing falls back safely.
fn mismatched_html_cell_close_falls_back_without_panicking() {
    let parsed = parse_clipboard("fallback", Some("<table><tr><td>unsupported</th></table>"))
        .expect("malformed HTML falls back to text");
    assert_eq!(parsed.kind, ClipboardInputKind::Text);
    assert_eq!(
        parsed.ranges[0][0][0].input,
        CellInput::Text("fallback".to_owned())
    );
}

#[test]
// Verify that large HTML tables retain every parsed cell.
fn parses_large_html_tables_without_losing_cells() {
    let mut html = String::from("<table><tr>");
    for index in 0..2_000 {
        html.push_str(&format!("<td>{index}</td>"));
    }
    html.push_str("</tr></table>");
    let matrix = parse_html_table(&html).expect("HTML cells parse");
    assert_eq!(matrix[0].len(), 2_000);
    assert_eq!(matrix[0][1_999].input, CellInput::Text("1999".to_owned()));
}

#[test]
// Verify that malformed multibyte color values do not panic.
fn malformed_multibyte_color_does_not_panic() {
    assert_eq!(css_color("漢abcde"), None);
    assert_eq!(css_color("FF336699"), Some("#336699".to_owned()));
}

#[test]
// Private clipboard data is bound to the payload this process wrote, even when the OS wraps
// the HTML in its own header on the way back (macOS adds a <head> with a charset).
fn internal_payload_owner_is_bound_to_the_written_payload() {
    let cell = vec![vec![ClipboardCell::from(&Cell::text("7"))]];
    let html = create_html_clipboard(
        std::slice::from_ref(&cell),
        std::slice::from_ref(&cell),
        &[],
        "7",
    )
    .expect("clipboard data encodes");
    let mark = OwnedPayloadMark {
        owner_id: "owner-1".to_owned(),
        payload_digest: payload_digest(&html).expect("the HTML carries a payload"),
    };
    let wrapped = format!(
        "<html><head><meta http-equiv=\"content-type\" content=\"text/html; charset=utf-8\"></head><body>{html}</body></html>"
    );
    assert_eq!(
        owned_html_owner_id_for_mark(&wrapped, &mark).as_deref(),
        Some("owner-1")
    );
    let other = create_html_clipboard(
        std::slice::from_ref(&cell),
        std::slice::from_ref(&cell),
        &[],
        "7",
    )
    .expect("clipboard data encodes");
    assert_eq!(owned_html_owner_id_for_mark(&other, &mark), None);
    assert_eq!(
        owned_html_owner_id_for_mark("<table><tr><td>7</td></tr></table>", &mark),
        None
    );
}

#[test]
// Verify that copied CSV and Markdown escape cell content reversibly.
fn csv_and_markdown_output_escape_content() {
    let matrix = vec![vec![
        text_cell("a,b".to_owned()),
        text_cell("c|d".to_owned()),
    ]];
    assert_eq!(format_plain_text(&matrix, CopyFormat::Csv), "\"a,b\",c|d");
    assert_eq!(
        format_plain_text(&matrix, CopyFormat::MarkdownTable),
        "| a,b | c\\|d |\n| --- | --- |\n"
    );
}

#[test]
// Keeps selected Markdown column alignment in the copied separator row.
fn markdown_copy_writes_each_selected_column_alignment() {
    let matrix = vec![vec![
        text_cell("left".to_owned()),
        text_cell("center".to_owned()),
        text_cell("right".to_owned()),
    ]];
    let alignments = [
        Some(crate::markdown_adapter::MarkdownTableAlignment::Left),
        Some(crate::markdown_adapter::MarkdownTableAlignment::Center),
        Some(crate::markdown_adapter::MarkdownTableAlignment::Right),
    ];
    assert_eq!(
        format_plain_text_with_alignments(&matrix, CopyFormat::MarkdownTable, &alignments),
        "| left | center | right |\n| :--- | :---: | ---: |\n"
    );
}

#[test]
// Verify that copying disjoint selections excludes cells between ranges.
fn disjoint_copy_concatenates_ranges_without_copying_the_gaps() {
    let first = vec![vec![text_cell("first".to_owned())]];
    let second = vec![vec![text_cell("second".to_owned())]];
    assert_eq!(
        format_ranges_plain_text(&[first, second], CopyFormat::DisplayTsv),
        "first\n\nsecond"
    );
}

#[test]
fn windows_row_breaks_leave_line_breaks_inside_quoted_cells() {
    assert_eq!(
        plain_text::row_breaks_as_crlf("a\t\"b\nc\"\nd\t\"say \"\"hi\"\"\"\n"),
        "a\t\"b\nc\"\r\nd\t\"say \"\"hi\"\"\"\r\n"
    );
    let parsed = parse_plain_text(&plain_text::row_breaks_as_crlf("a\t\"b\nc\"\nd\te"))
        .expect("CRLF rows parse");
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0][1].input, CellInput::Text("b\nc".to_owned()));
}
