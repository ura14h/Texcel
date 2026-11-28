use super::*;
use std::borrow::Cow;

#[test]
// Verify that worker display uses full-document links and excludes raw HTML.
fn worker_inline_display_uses_full_document_links_and_omits_html() {
    let bytes = b"[ref]: https://example.test/path\n\n| value |\n| --- |\n| **bold** [link][ref]<br>![alt](image.png)<script>bad</script> |\n".to_vec();
    let markdown = MarkdownDocument::parse(bytes).expect("Markdown parses");
    let display = markdown
        .cell_display(0, 1, 0)
        .expect("table cell display is available");
    assert!(display.differs_from_source);
    assert!(display.runs.iter().any(|run| run.bold));
    assert!(display.runs.iter().any(|run| run.link));
    let rendered = display
        .runs
        .iter()
        .filter_map(|run| run.text_for_language(Some(&markdown), crate::i18n::Language::Japanese))
        .map(Cow::into_owned)
        .collect::<String>();
    assert!(rendered.contains("bold link\nalt [画像]"), "{rendered}");
    assert!(!rendered.contains("<script>"), "{rendered}");
    let english = display
        .runs
        .iter()
        .filter_map(|run| run.text_for_language(Some(&markdown), crate::i18n::Language::English))
        .map(Cow::into_owned)
        .collect::<String>();
    assert!(english.contains("alt [Image]"), "{english}");
}

/// Keeps generated Markdown table names in the language used when each document is created.
#[test]
fn generated_markdown_table_names_use_the_creation_language() {
    let markdown = MarkdownDocument::parse(b"| value |\n| --- |\n| cell |\n".to_vec())
        .expect("Markdown table parses");
    let japanese =
        markdown.to_document_with_language(101, "notes.md", crate::i18n::Language::Japanese);
    let english =
        markdown.to_document_with_language(102, "notes.md", crate::i18n::Language::English);
    assert_eq!(japanese.tables[0].name, "表1");
    assert_eq!(english.tables[0].name, "Table1");

    let titled =
        MarkdownDocument::parse(b"# Existing name\n\n| value |\n| --- |\n| cell |\n".to_vec())
            .expect("titled Markdown table parses")
            .to_document_with_language(103, "notes.md", crate::i18n::Language::Japanese);
    assert_eq!(titled.tables[0].name, "Existing name");
}

/// Opens a BOM-marked table and keeps its original byte offsets for edits.
#[test]
fn markdown_bom_is_ignored_for_parsing_and_preserved_on_save() {
    let original = b"\xEF\xBB\xBF| A | B |\n| --- | --- |\n| x | y |".to_vec();
    let markdown = MarkdownDocument::parse(original.clone()).expect("BOM table parses");
    assert_eq!(markdown.tables.len(), 1);
    assert_eq!(markdown.source_bytes(), original);

    let saved = markdown
        .apply_changes(&[MarkdownCellChange {
            table: 0,
            row: 1,
            column: 1,
            value: "changed".to_owned(),
        }])
        .expect("cell edit preserves source offsets");
    assert!(saved.source_bytes().starts_with(b"\xEF\xBB\xBF"));
    assert!(saved.source_bytes().ends_with(b"| x | changed |"));
    let reopened =
        MarkdownDocument::parse(saved.source_bytes().to_vec()).expect("edited BOM table reloads");
    assert_eq!(reopened.tables[0].rows[1][1].value, "changed");
}

/// Keeps valid Markdown indentation when a BOM shifts parser ranges.
#[test]
fn bom_markdown_with_one_to_three_leading_spaces_keeps_tables_editable() {
    for indentation in 1..=3 {
        let spaces = " ".repeat(indentation);
        let source =
            format!("\u{feff}{spaces}# Heading\n\n{spaces}| A |\n{spaces}| --- |\n{spaces}| x |\n");
        let markdown =
            MarkdownDocument::parse(source.as_bytes().to_vec()).expect("indented BOM table parses");
        assert_eq!(markdown.tables.len(), 1, "indentation: {indentation}");
        assert_eq!(markdown.tables[0].name, "Heading");
        let saved = markdown
            .apply_changes(&[MarkdownCellChange {
                table: 0,
                row: 1,
                column: 0,
                value: "changed".to_owned(),
            }])
            .expect("offsets address the original indented source");
        let saved_text = std::str::from_utf8(saved.source_bytes()).expect("UTF-8 source");
        assert!(saved_text.starts_with('\u{feff}'));
        assert!(saved_text.contains(&format!("{spaces}| changed |")));
    }
}

/// Keeps BOM-shifted ranges aligned for multi-column headers and literal markers.
#[test]
fn bom_markdown_keeps_header_columns_and_literal_display_ranges() {
    let source = "\u{feff}| A | B |\n| --- | --- |\n| '=1+1 | `code` |\n";
    let markdown = MarkdownDocument::parse(source.as_bytes().to_vec())
        .expect("the BOM table with two columns parses");
    assert_eq!(markdown.tables[0].rows[0].len(), 2);
    assert_eq!(markdown.tables[0].rows[0][0].value, "A");
    assert_eq!(markdown.tables[0].rows[0][1].value, "B");
    assert_eq!(markdown.tables[0].rows[1][0].value, "'=1+1");
    assert_eq!(markdown.tables[0].rows[1][1].value, "`code`");

    let display = markdown
        .cell_display(0, 1, 0)
        .expect("the literal cell has display metadata");
    let marker = display
        .literal_marker_range
        .as_ref()
        .expect("the marker range includes the original BOM offset");
    assert_eq!(&source[marker.clone()], "'");
    assert!(display.literal_marker_consumed);
    assert_eq!(display.plain_text, "=1+1");
}

#[test]
// Verify that preview fallback parses heading-like cell text inside a table.
fn search_display_fallback_reparses_heading_like_text_in_table_context() {
    let mut markdown = MarkdownDocument::parse(b"| value |\n| --- |\n| # heading |\n".to_vec())
        .expect("Markdown table parses");
    markdown.displays.clear();
    let rendered = markdown
        .cell_display_text_for_search(0, 1, 0, "# heading", crate::i18n::Language::English)
        .expect("fallback reparses through the full-document table parser");
    assert_eq!(rendered, "# heading");
}

#[test]
// Removes only the persisted formula marker and keeps any apostrophe in the value.
fn markdown_display_removes_one_marker_before_an_equals_sign() {
    let markdown = MarkdownDocument::parse(
        b"| escaped | actual apostrophe |\n| --- | --- |\n| '=1+1 | ''=1+1 |\n".to_vec(),
    )
    .expect("Markdown table parses");
    assert_eq!(
        markdown
            .cell_display_text_for_search(
                0,
                1,
                0,
                &markdown.tables[0].rows[1][0].value,
                crate::i18n::Language::English,
            )
            .expect("escaped formula-like value has display text"),
        "=1+1"
    );
    assert_eq!(
        markdown
            .cell_display_text_for_search(
                0,
                1,
                1,
                &markdown.tables[0].rows[1][1].value,
                crate::i18n::Language::English,
            )
            .expect("literal apostrophe value has display text"),
        "'=1+1"
    );
    let fragment = MarkdownCellDisplay::parse_fragment("''=1+1");
    assert_eq!(
        fragment
            .runs
            .iter()
            .filter_map(|run| run.text_for_language(None, crate::i18n::Language::English))
            .map(|text| text.into_owned())
            .collect::<String>(),
        "'=1+1"
    );

    let source = b"| styled | code | entity | escaped | code equals |\n| --- | --- | --- | --- | --- |\n| **''=1+1** | `''=1+1` | &#39;'=1+1 | \\'=1+1 | '`=1+1` |\n";
    let markdown = MarkdownDocument::parse(source.to_vec()).expect("styled table parses");
    for (column, expected) in [
        (0, "'=1+1"),
        (1, "''=1+1"),
        (2, "''=1+1"),
        (3, "'=1+1"),
        (4, "'=1+1"),
    ] {
        assert_eq!(
            markdown
                .cell_display_text_for_search(
                    0,
                    1,
                    column,
                    &markdown.tables[0].rows[1][column].value,
                    crate::i18n::Language::English,
                )
                .expect("full document display resolves"),
            expected
        );
    }

    for (fragment, expected) in [
        ("**''=1+1**", "'=1+1"),
        ("**''=1+1** tail", "'=1+1 tail"),
        ("`''=1+1`", "''=1+1"),
        ("&#39;'=1+1", "''=1+1"),
        ("\\'=1+1", "'=1+1"),
        ("'\\==A1", "'==A1"),
        ("'`=1+1`", "'=1+1"),
    ] {
        let display = MarkdownCellDisplay::parse_fragment(fragment);
        let text = display
            .runs
            .iter()
            .filter_map(|run| run.text_for_language(None, crate::i18n::Language::English))
            .map(|text| text.into_owned())
            .collect::<String>();
        assert_eq!(text, expected, "fragment: {fragment}");
    }

    let edge_cases = MarkdownDocument::parse(
        br#"| Styled tail | Escaped first equals |
| --- | --- |
| **''=1+1** tail | '\==A1 |
"#
        .to_vec(),
    )
    .expect("marker edge cases parse");
    for (column, expected) in [(0, "'=1+1 tail"), (1, "'==A1")] {
        assert_eq!(
            edge_cases
                .cell_display_text_for_search(
                    0,
                    1,
                    column,
                    &edge_cases.tables[0].rows[1][column].value,
                    crate::i18n::Language::English,
                )
                .expect("edge case display resolves"),
            expected
        );
    }

    let decorated =
        MarkdownCellDisplay::parse_fragment("**bold** [link](https://example.test) `code`");
    assert!(decorated.runs.iter().any(|run| run.bold));
    assert!(decorated.runs.iter().any(|run| run.link));
    assert!(decorated.runs.iter().any(|run| run.code));
    assert!(
        decorated
            .runs
            .iter()
            .filter(|run| !run.localized_image)
            .all(|run| run.text.is_some())
    );
}

#[test]
// Verify that Markdown table discovery ignores fenced examples and keeps section names.
fn sample_tables_exclude_fenced_examples_and_keep_section_names() {
    let markdown = MarkdownDocument::parse(include_bytes!("../samples/tables.md").to_vec())
        .expect("Markdown sample is UTF-8");
    assert_eq!(markdown.tables.len(), 2);
    assert_eq!(markdown.tables[0].name, "商品");
    assert_eq!(markdown.tables[1].name, "注記");
    assert_eq!(markdown.tables[0].rows[0][0].value, "商品");
    assert_eq!(markdown.tables[0].rows[1][0].value, "**日本語 ABC**");
    assert_eq!(markdown.tables[1].rows[3][1].value, "A\\|B");
    assert_eq!(markdown.tables[1].rows[4][1].value, "1行目<br>2行目");
}

#[test]
// Verify that cell edits preserve surrounding text, outer pipes, and fenced code.
fn editing_cells_preserves_surrounding_text_outer_pipes_and_fenced_code() {
    let source = include_bytes!("../samples/tables.md").to_vec();
    let markdown = MarkdownDocument::parse(source.clone()).expect("sample parses");
    let updated = markdown
        .apply_changes(&[
            MarkdownCellChange {
                table: 0,
                row: 1,
                column: 0,
                value: "**日本語|ABC**".to_owned(),
            },
            MarkdownCellChange {
                table: 1,
                row: 3,
                column: 1,
                value: "X|Y".to_owned(),
            },
            MarkdownCellChange {
                table: 1,
                row: 4,
                column: 1,
                value: "上\r\n下".to_owned(),
            },
        ])
        .expect("cell edits can be patched");
    let output = std::str::from_utf8(updated.source_bytes()).expect("output is UTF-8");
    assert!(output.starts_with("# Texcel 表示サンプル\n\nこの文章はセル編集で変化しない。"));
    assert!(output.contains("| **日本語\\|ABC** | 1200 | 3 | =B2*C2 |"));
    assert!(output.contains("| 区切り | X\\|Y |"));
    assert!(output.contains("| 改行 | 上<br>下 |"));
    assert!(output.contains("```markdown\n| 偽の表 | 編集対象外 |"));
    assert_eq!(updated.tables.len(), 2);
}

#[test]
/// Saves an expanded cell and trims it again without changing the opened source extent.
fn markdown_sheet_extent_trims_repeated_save_after_sparse_clear() {
    let original = b"# Data\n\n| Name | Value |\n| --- | ---: |\n| A | keep |\n\nafter\n".to_vec();
    let opened = MarkdownDocument::parse(original.clone()).expect("source table parses");
    let expanded = opened
        .apply_changes(&[MarkdownCellChange {
            table: 0,
            row: 7,
            column: 5,
            value: "F8 value".to_owned(),
        }])
        .expect("a sparse F8 edit extends the table");
    assert_eq!(expanded.tables[0].rows.len(), 8);
    assert!(expanded.tables[0].rows.iter().all(|row| row.len() == 6));
    assert_eq!(expanded.tables[0].rows[7][5].value, "F8 value");

    let cleared = expanded
        .apply_changes(&[MarkdownCellChange {
            table: 0,
            row: 7,
            column: 5,
            value: String::new(),
        }])
        .expect("clearing the last sparse value shrinks the saved range");
    assert_eq!(cleared.source_bytes(), original);
    assert_eq!(cleared.tables[0].rows.len(), 2);
    assert_eq!(cleared.tables[0].rows[0].len(), 2);

    let reopened = MarkdownDocument::parse(expanded.source_bytes().to_vec())
        .expect("the expanded file can be reopened");
    let reopened_clear = reopened
        .apply_changes(&[MarkdownCellChange {
            table: 0,
            row: 7,
            column: 5,
            value: String::new(),
        }])
        .expect("the reopened file keeps its authored dimensions");
    assert_eq!(reopened_clear.tables[0].rows.len(), 8);
    assert_eq!(reopened_clear.tables[0].rows[0].len(), 6);
    assert_eq!(reopened_clear.tables[0].rows[7][5].value, "");
}

#[test]
/// Keeps a new row's empty first cell when the source omits outer pipes.
fn markdown_sparse_f8_edit_keeps_the_first_cell_and_reloads() {
    let original = b"Name | Value\n---- | -----\nA | keep\n";
    let opened = MarkdownDocument::parse(original.to_vec()).expect("source table parses");
    let saved = opened
        .apply_changes(&[MarkdownCellChange {
            table: 0,
            row: 7,
            column: 5,
            value: "F8 value".to_owned(),
        }])
        .expect("the sparse F8 value is serialized");
    let saved_bytes = saved.source_bytes();
    let reopened = MarkdownDocument::parse(saved_bytes.to_vec()).expect("saved table reloads");

    assert_eq!(reopened.tables[0].rows.len(), 8);
    assert!(reopened.tables[0].rows.iter().all(|row| row.len() == 6));
    assert!(reopened.tables[0].rows[7][0].value.is_empty());
    assert_eq!(reopened.tables[0].rows[7][5].value, "F8 value");
}

#[test]
// Verify that Markdown input with invalid UTF-8 is rejected.
fn markdown_must_be_valid_utf8() {
    assert_eq!(
        MarkdownDocument::parse(vec![0xFF]),
        Err(MarkdownError::InvalidUtf8)
    );
}

#[test]
// Verify that an editable padded cell remains within its table.
fn padded_missing_cell_is_editable_without_leaving_the_table() {
    let source = b"| A | B |\n| --- | --- |\n| x |\n".to_vec();
    let markdown = MarkdownDocument::parse(source).expect("ragged GFM table parses");
    assert_eq!(markdown.tables[0].rows[1].len(), 2);
    assert!(!markdown.tables[0].rows[1][1].has_source);

    let updated = markdown
        .apply_changes(&[MarkdownCellChange {
            table: 0,
            row: 1,
            column: 1,
            value: "filled".to_owned(),
        }])
        .expect("a GFM-padded cell is inserted into its source row");
    assert_eq!(updated.tables.len(), 1);
    assert_eq!(updated.tables[0].rows[1].len(), 2);
    assert_eq!(updated.tables[0].rows[1][1].value, "filled");
    assert!(
        std::str::from_utf8(updated.source_bytes())
            .expect("output remains UTF-8")
            .contains("| x | filled |")
    );
}

#[test]
// Verify that changing alignment updates only the selected separator marker.
fn separator_alignment_survives_edits_and_column_structure() {
    let source = b"before\n\n| A | B |\n| --- | ---: |\n| x | y |\n\nafter\n".to_vec();
    let markdown = MarkdownDocument::parse(source).expect("table parses");
    assert_eq!(markdown.column_alignment(0, 0), None);
    assert_eq!(
        markdown.column_alignment(0, 1),
        Some(MarkdownTableAlignment::Right)
    );
    let updated = markdown
        .set_column_alignment(0, 0..=0, MarkdownTableAlignment::Center)
        .expect("alignment updates on the source adapter");
    let output = std::str::from_utf8(updated.source_bytes()).expect("UTF-8 output");
    assert!(output.contains("| :---: | ---: |"));
    assert!(output.starts_with("before\n"));
    assert!(output.ends_with("after\n"));
    assert_eq!(
        updated.column_alignment(0, 0),
        Some(MarkdownTableAlignment::Center)
    );
    let explicit_left = markdown
        .set_column_alignment(0, 1..=1, MarkdownTableAlignment::Left)
        .expect("explicit left alignment is encoded in the separator");
    assert_eq!(
        explicit_left.column_alignment(0, 1),
        Some(MarkdownTableAlignment::Left)
    );
    assert_ne!(
        explicit_left.column_alignment(0, 0),
        Some(MarkdownTableAlignment::Left)
    );

    let document = markdown.to_document(44, "alignment.md");
    let table = &document.tables[0];
    let insert = crate::document::StructurePlan::build(
        table,
        crate::document::StructureOperation::Insert {
            table_id: table.id,
            axis: crate::document::StructureAxis::Columns,
            index: 1,
            count: 1,
        },
    )
    .expect("column insertion maps the source columns");
    let transformed = markdown
        .apply_structure(0, table.id, &insert, &Default::default())
        .expect("the inserted separator retains prior column markers");
    assert_eq!(transformed.column_alignment(0, 0), None);
    assert_eq!(transformed.column_alignment(0, 1), None);
    assert_eq!(
        transformed.column_alignment(0, 2),
        Some(MarkdownTableAlignment::Right)
    );
    let transformed_document = transformed.to_document(44, "alignment.md");
    let transformed_table = &transformed_document.tables[0];
    let delete = crate::document::StructurePlan::build(
        transformed_table,
        crate::document::StructureOperation::Delete {
            table_id: transformed_table.id,
            axis: crate::document::StructureAxis::Columns,
            index: 1,
            count: 1,
        },
    )
    .expect("column deletion maps the restored source columns");
    let restored = transformed
        .apply_structure(0, transformed_table.id, &delete, &Default::default())
        .expect("the structure inverse restores the separator marker");
    assert_eq!(
        restored.column_alignment(0, 1),
        Some(MarkdownTableAlignment::Right)
    );
    let values = |document: &MarkdownDocument| {
        document.tables[0]
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| cell.value.clone())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(values(&updated), values(&markdown));
}

// Rebuilds a pipe-free table with an empty expanded header cell.
#[test]
fn sparse_edit_expands_a_pipe_free_table_with_an_empty_header_cell() {
    let original = b"A|B\n---|---\nx|y\n";
    let markdown = MarkdownDocument::parse(original.to_vec()).expect("the table parses");
    let updated = markdown
        .apply_changes(&[MarkdownCellChange {
            table: 0,
            row: 1,
            column: 2,
            value: "z".to_owned(),
        }])
        .expect("a sparse edit expands the source table");
    let output = std::str::from_utf8(updated.source_bytes()).expect("output remains UTF-8");
    let reopened = MarkdownDocument::parse(updated.source_bytes().to_vec())
        .expect("the expanded table remains valid Markdown");

    assert_eq!(output, "A|B||\n---|---|---\nx|y|z\n");
    assert_eq!(reopened.tables[0].parser_width, 3);
    assert_eq!(reopened.tables[0].rows[0][2].value, "");
    assert_eq!(reopened.tables[0].rows[1][2].value, "z");
}

#[test]
// Verify that editing a padded row preserves existing spacing and width.
fn editing_a_padded_short_row_preserves_existing_cell_spacing_and_width() {
    let markdown =
        MarkdownDocument::parse(b"| A | B | C |\n| --- | --- | --- |\n|  x  |\n".to_vec())
            .expect("three-column table with a short row parses");
    let updated = markdown
        .apply_changes(&[MarkdownCellChange {
            table: 0,
            row: 1,
            column: 1,
            value: "n".to_owned(),
        }])
        .expect("the second cell is added without materializing the third");
    let output = std::str::from_utf8(updated.source_bytes()).expect("output is UTF-8");
    // The untouched cell keeps its bytes; the written cell follows the one-space rule.
    assert!(output.contains("|  x  | n |"));
    assert_eq!(updated.tables[0].rows[1][0].value, "x");
    assert_eq!(updated.tables[0].rows[1][1].value, "n");
    assert!(!updated.tables[0].rows[1][2].has_source);
}

#[test]
// Verify that clearing edge cells keeps coordinates when outer pipes are absent.
fn clearing_edge_cells_without_outer_pipes_preserves_table_coordinates() {
    let source = b"A|B\n---|---\nx|y\n".to_vec();
    let markdown = MarkdownDocument::parse(source).expect("table without outer pipes parses");
    let updated = markdown
        .apply_changes(&[
            MarkdownCellChange {
                table: 0,
                row: 1,
                column: 0,
                value: String::new(),
            },
            MarkdownCellChange {
                table: 0,
                row: 1,
                column: 1,
                value: String::new(),
            },
        ])
        .expect("empty edge cells are represented with safe row boundaries");
    assert_eq!(updated.tables.len(), 1);
    assert_eq!(updated.tables[0].rows[1].len(), 2);
    assert_eq!(updated.tables[0].rows[1][0].value, "");
    assert_eq!(updated.tables[0].rows[1][1].value, "");
}

#[test]
// Verify that a trailing backslash does not escape a compact table delimiter.
fn trailing_backslash_cannot_escape_a_compact_table_pipe() {
    let markdown = MarkdownDocument::parse(b"| A | B |\n| --- | --- |\n|x|y|\n".to_vec())
        .expect("compact table parses");
    let updated = markdown
        .apply_changes(&[MarkdownCellChange {
            table: 0,
            row: 1,
            column: 1,
            value: "tail\\".to_owned(),
        }])
        .expect("trailing backslash is escaped before the column delimiter");
    assert_eq!(updated.tables.len(), 1);
    assert_eq!(updated.tables[0].rows[1].len(), 2);
    assert_eq!(updated.tables[0].rows[1][1].value, "tail\\");
    assert!(
        std::str::from_utf8(updated.source_bytes())
            .expect("output remains UTF-8")
            .contains("|x|tail\\ |")
    );
}

#[test]
// Verify that Markdown preview rows arrive only after a table closes.
fn markdown_progress_cancels_after_a_confirmed_table_row() {
    let source = "| A | B |\n| --- | --- |\n| 1 | 2 |\n| 3 | 4 |\n\n| C | D |\n| --- | --- |\n";
    let mut preview_rows = Vec::new();
    let result =
        MarkdownDocument::parse_with_progress(source.as_bytes().to_vec(), |table, row, cells| {
            preview_rows.push((
                table,
                row,
                cells
                    .iter()
                    .map(|cell| cell.value.clone())
                    .collect::<Vec<_>>(),
            ));
            table == 0 && row < 1
        })
        .expect("the Markdown tables parse");

    assert!(result.is_none());
    assert_eq!(preview_rows.len(), 2);
    assert_eq!(
        preview_rows[0],
        (0, 0, vec!["A".to_owned(), "B".to_owned()])
    );
    assert_eq!(
        preview_rows[1],
        (0, 1, vec!["1".to_owned(), "2".to_owned()])
    );
}

#[test]
// Checks that deleting a table also removes its name heading and keeps one blank line.
fn delete_table_removes_its_name_heading() {
    let source =
        "Intro\n\n## Sales\n\n| A |\n| --- |\n| 1 |\n\n## Costs\n| B |\n| --- |\n\nOutro\n";
    let document = MarkdownDocument::parse(source.as_bytes().to_vec()).unwrap();
    assert_eq!(document.tables[0].name, "Sales");

    let first_removed = document.delete_table(0).unwrap();
    assert_eq!(
        std::str::from_utf8(first_removed.source_bytes()).unwrap(),
        "Intro\n\n## Costs\n| B |\n| --- |\n\nOutro\n"
    );
    let second_removed = document.delete_table(1).unwrap();
    assert_eq!(
        std::str::from_utf8(second_removed.source_bytes()).unwrap(),
        "Intro\n\n## Sales\n\n| A |\n| --- |\n| 1 |\n\nOutro\n"
    );
}

#[test]
// Checks that a heading separated from the table by other text is kept.
fn delete_table_keeps_a_heading_that_is_not_its_name() {
    let source = "## Notes\n\nSome text.\n\n| A |\n| --- |\n\n| B |\n| --- |\n";
    let document = MarkdownDocument::parse(source.as_bytes().to_vec()).unwrap();
    let removed = document.delete_table(0).unwrap();
    assert_eq!(
        std::str::from_utf8(removed.source_bytes()).unwrap(),
        "## Notes\n\nSome text.\n\n| B |\n| --- |\n"
    );
}

#[test]
// Writes every changed cell with one space on each padded side, also into empty cells.
fn written_cells_follow_one_padding_rule() {
    let write = |source: &[u8], row: usize, column: usize, value: &str| {
        let markdown = MarkdownDocument::parse(source.to_vec()).expect("table parses");
        let updated = markdown
            .apply_changes(&[MarkdownCellChange {
                table: 0,
                row,
                column,
                value: value.to_owned(),
            }])
            .expect("the cell is written");
        String::from_utf8(updated.source_bytes().to_vec()).expect("output is UTF-8")
    };
    // An empty `|  |` cell, as in a new document, gets the value between single spaces.
    assert_eq!(
        write(b"|  |\n| --- |\n|  |\n", 0, 0, "Name"),
        "| Name |\n| --- |\n|  |\n"
    );
    assert_eq!(
        write(b"| a |  |\n| --- | --- |\n", 0, 1, "120"),
        "| a | 120 |\n| --- | --- |\n"
    );
    // A column-aligned cell does not keep its alignment spaces after a rewrite.
    assert_eq!(
        write(
            b"| name   | v |\n| ------ | - |\n| ab     | 1 |\n",
            1,
            0,
            "abcdef"
        ),
        "| name   | v |\n| ------ | - |\n| abcdef | 1 |\n"
    );
    assert_eq!(
        write(
            b"| name   | v |\n| ------ | - |\n| ab     | 1 |\n",
            1,
            0,
            "x"
        ),
        "| name   | v |\n| ------ | - |\n| x | 1 |\n"
    );
    // A compact row stays compact.
    assert_eq!(
        write(b"|a|b|\n|-|-|\n|c|d|\n", 1, 1, "e"),
        "|a|b|\n|-|-|\n|c|e|\n"
    );
}
