use super::*;

// Keeps irregular separator bytes unless the selected column alignment changes.
#[test]
fn alignment_and_cell_edits_keep_unselected_separator_bytes() {
    let original = include_bytes!("../../samples/column-alignment.md").to_vec();
    let source = MarkdownDocument::parse(original.clone()).expect("the sample parses");
    let edited_cell = source
        .apply_changes(&[MarkdownCellChange {
            table: 0,
            row: 1,
            column: 2,
            value: "13".to_owned(),
        }])
        .expect("the cell edit keeps the source table");
    let mut expected_cell = original.clone();
    let value_start = expected_cell
        .windows(2)
        .position(|window| window == b"12")
        .expect("the sample contains one count of twelve");
    expected_cell[value_start + 1] = b'3';
    assert_eq!(edited_cell.source_bytes(), expected_cell);

    let aligned = source
        .set_column_alignment(0, 1..=1, MarkdownTableAlignment::Left)
        .expect("the selected column alignment changes");
    let original_text = std::str::from_utf8(&original).expect("the sample is UTF-8");
    let expected_alignment = original_text.replace("|:---:|", "|:---|");
    assert_eq!(aligned.source_bytes(), expected_alignment.as_bytes());
}

// Keeps a virtual column alignment until a value expands the saved table width.
#[test]
fn virtual_alignment_is_written_with_a_value_and_returns_to_memory_on_clear() {
    let original = b"| A | B |\n| --- | --- |\n| x | y |\n";
    let source = MarkdownDocument::parse(original.to_vec()).expect("the table parses");
    let aligned = source
        .set_column_alignment(0, 2..=2, MarkdownTableAlignment::Right)
        .expect("a virtual column keeps its alignment in memory");
    assert_eq!(aligned.source_bytes(), original);
    assert_eq!(
        aligned.column_alignment(0, 2),
        Some(MarkdownTableAlignment::Right)
    );

    let value = MarkdownCellChange {
        table: 0,
        row: 1,
        column: 2,
        value: "far".to_owned(),
    };
    let expanded = aligned
        .apply_changes(std::slice::from_ref(&value))
        .expect("the value grows the saved table");
    let text = std::str::from_utf8(expanded.source_bytes()).expect("the result is UTF-8");
    assert!(
        text.lines()
            .nth(1)
            .is_some_and(|line| line.contains("---:"))
    );
    assert!(text.lines().nth(2).is_some_and(|line| line.contains("far")));
    assert_eq!(
        expanded.column_alignment(0, 2),
        Some(MarkdownTableAlignment::Right)
    );

    let cleared = expanded
        .apply_changes(&[MarkdownCellChange {
            value: String::new(),
            ..value
        }])
        .expect("clearing the new value shrinks to the open-time extent");
    assert_eq!(cleared.source_bytes(), original);
    assert_eq!(
        cleared.column_alignment(0, 2),
        Some(MarkdownTableAlignment::Right),
        "the alignment remains temporary while the document is open"
    );
}

// Keeps a distant virtual alignment when changing a lower column.
#[test]
fn updating_a_lower_alignment_does_not_truncate_virtual_columns() {
    let source = MarkdownDocument::parse(b"| A | B |\n| --- | --- |\n| x | y |\n".to_vec())
        .expect("the two-column table parses")
        .set_column_alignment(0, 5..=5, MarkdownTableAlignment::Right)
        .expect("the far column receives a temporary alignment")
        .set_column_alignment(0, 0..=0, MarkdownTableAlignment::Center)
        .expect("the first column alignment changes");

    assert_eq!(
        source.column_alignment(0, 0),
        Some(MarkdownTableAlignment::Center)
    );
    assert_eq!(
        source.column_alignment(0, 5),
        Some(MarkdownTableAlignment::Right),
        "a short assignment keeps the prior virtual column"
    );
}

// Keeps another table's temporary alignment through source reconstruction.
#[test]
fn structure_changes_preserve_virtual_alignments_on_other_tables() {
    let source = MarkdownDocument::parse(
        b"| A | B |\n| --- | --- |\n| x | y |\n\n| C | D |\n| --- | --- |\n| z | w |\n".to_vec(),
    )
    .expect("both source tables parse")
    .set_column_alignment(1, 4..=4, MarkdownTableAlignment::Center)
    .expect("the second table keeps a temporary alignment");
    let document = source.to_document(45, "alignment.md");
    let table = &document.tables[0];
    let operation = crate::document::StructureOperation::Insert {
        table_id: table.id,
        axis: crate::document::StructureAxis::Columns,
        index: 1,
        count: 1,
    };
    let plan = crate::document::StructurePlan::build(table, operation)
        .expect("the first table column can be inserted");

    let updated = source
        .apply_structure(0, table.id, &plan, &Default::default())
        .expect("the first table source is rebuilt");
    assert_eq!(
        updated.column_alignment(1, 4),
        Some(MarkdownTableAlignment::Center)
    );
}

#[test]
// Keeps one BOM before a table when its first row is inserted, moved, copied, or deleted.
fn row_structure_changes_keep_the_bom_as_document_prefix() {
    let bytes = "\u{feff}| A |\n| --- |\n| first |\n| second |\n"
        .as_bytes()
        .to_vec();
    let operations = [
        crate::document::StructureOperation::Insert {
            table_id: 1,
            axis: crate::document::StructureAxis::Rows,
            index: 0,
            count: 1,
        },
        crate::document::StructureOperation::Delete {
            table_id: 1,
            axis: crate::document::StructureAxis::Rows,
            index: 0,
            count: 1,
        },
        crate::document::StructureOperation::Duplicate {
            table_id: 1,
            axis: crate::document::StructureAxis::Rows,
            index: 0,
            count: 1,
        },
        crate::document::StructureOperation::Move {
            table_id: 1,
            axis: crate::document::StructureAxis::Rows,
            index: 0,
            count: 1,
            destination: 2,
        },
    ];

    for operation in operations {
        let source = MarkdownDocument::parse(bytes.clone()).expect("BOM table parses");
        let document = source.to_document(1, "bom.md");
        let table = &document.tables[0];
        let plan = crate::document::StructurePlan::build(table, operation)
            .expect("row operation is in range");
        let updated = source
            .apply_structure(0, table.id, &plan, &Default::default())
            .expect("row operation rebuilds the table");

        assert!(updated.source_bytes().starts_with(b"\xEF\xBB\xBF"));
        assert_eq!(
            std::str::from_utf8(updated.source_bytes())
                .unwrap()
                .matches('\u{feff}')
                .count(),
            1
        );
        assert_eq!(
            MarkdownDocument::parse(updated.source_bytes().to_vec())
                .expect("rebuilt BOM table reopens")
                .tables
                .len(),
            1
        );
    }
}

// Moves saved and temporary column alignments with sheet structure changes.
#[test]
fn column_structure_moves_duplicates_and_deletes_alignments() {
    let source = MarkdownDocument::parse(
        b"| A | B | C |\n| :--- | :---: | ---: |\n| x | y | z |\n".to_vec(),
    )
    .expect("the three-column table parses")
    .set_column_alignment(0, 3..=3, MarkdownTableAlignment::Right)
    .expect("the virtual D column keeps a temporary alignment");

    let inserted = apply_column_structure(
        &source,
        crate::document::StructureOperation::Insert {
            table_id: 1,
            axis: crate::document::StructureAxis::Columns,
            index: 1,
            count: 1,
        },
    );
    assert_eq!(inserted.column_alignment(0, 1), None);
    assert_eq!(
        inserted.column_alignment(0, 2),
        Some(MarkdownTableAlignment::Center)
    );
    assert_eq!(
        inserted.column_alignment(0, 4),
        Some(MarkdownTableAlignment::Right),
        "the virtual alignment shifts past an inserted column"
    );

    let duplicated = apply_column_structure(
        &source,
        crate::document::StructureOperation::Duplicate {
            table_id: 1,
            axis: crate::document::StructureAxis::Columns,
            index: 1,
            count: 1,
        },
    );
    assert_eq!(
        duplicated.column_alignment(0, 1),
        Some(MarkdownTableAlignment::Center)
    );
    assert_eq!(
        duplicated.column_alignment(0, 2),
        Some(MarkdownTableAlignment::Center),
        "a duplicated column copies its alignment"
    );
    assert_eq!(
        duplicated.column_alignment(0, 4),
        Some(MarkdownTableAlignment::Right),
        "the virtual alignment shifts past the duplicate"
    );

    let moved = apply_column_structure(
        &source,
        crate::document::StructureOperation::Move {
            table_id: 1,
            axis: crate::document::StructureAxis::Columns,
            index: 1,
            count: 1,
            destination: 2,
        },
    );
    assert_eq!(
        moved.column_alignment(0, 1),
        Some(MarkdownTableAlignment::Right)
    );
    assert_eq!(
        moved.column_alignment(0, 2),
        Some(MarkdownTableAlignment::Center)
    );
    assert_eq!(
        moved.column_alignment(0, 3),
        Some(MarkdownTableAlignment::Right),
        "the virtual D column stays outside the moved table columns"
    );

    let deleted = apply_column_structure(
        &source,
        crate::document::StructureOperation::Delete {
            table_id: 1,
            axis: crate::document::StructureAxis::Columns,
            index: 1,
            count: 1,
        },
    );
    assert_eq!(
        deleted.column_alignment(0, 1),
        Some(MarkdownTableAlignment::Right)
    );
    assert_eq!(
        deleted.column_alignment(0, 2),
        Some(MarkdownTableAlignment::Right),
        "the virtual alignment shifts left when a column is deleted"
    );
}

/// Applies a real sparse-table column plan to the source adapter.
fn apply_column_structure(
    source: &MarkdownDocument,
    operation: crate::document::StructureOperation,
) -> MarkdownDocument {
    let document = source.to_document(1, "alignment.md");
    let table = &document.tables[0];
    let plan = crate::document::StructurePlan::build(table, operation)
        .expect("the column operation is inside the sheet");
    source
        .apply_structure(0, table.id, &plan, &Default::default())
        .expect("the source table follows the column mapping")
}

// Aligns separate column ranges in one edit and leaves the columns between them alone.
#[test]
fn separate_column_ranges_align_together() {
    let source =
        MarkdownDocument::parse(b"| a | b | c | d |\n| --- | --- | --- | --- |\n".to_vec())
            .expect("the table parses");
    let aligned = source
        .set_column_alignment_ranges(0, &[0..=0, 2..=3], MarkdownTableAlignment::Right)
        .expect("separate ranges align");
    assert_eq!(
        std::str::from_utf8(aligned.source_bytes()).expect("UTF-8"),
        "| a | b | c | d |\n| ---: | --- | ---: | ---: |\n"
    );
}
