use super::*;
use crate::document::CalcValue;

#[test]
// Show a Markdown text formula's fresh value in both grid text and accessibility.
fn markdown_text_formula_uses_the_calculation_result_for_view_and_accessibility() {
    let mut document = Document::new(721, "formula markdown", &["Sheet1"]);
    let table_id = document.tables[0].id;
    let formula = CellAddress { row: 0, column: 0 };
    let literal = CellAddress { row: 0, column: 1 };
    document.tables[0].set_loaded_cell(formula, Cell::text("=1+1"));
    document.tables[0].set_loaded_cell(literal, Cell::text("=1+1"));
    document
        .set_string_cell_local(table_id, literal, true)
        .expect("keep one formula-looking value as text");
    let revision = document.calculation_revision;
    assert!(document.apply_calculation_results(
        revision,
        [(
            table_id,
            formula,
            CalcStatus::Fresh,
            Some(CalcValue::Number(2.0)),
        )]
    ));

    let table = &document.tables[0];
    assert_eq!(
        grid_accessibility_cell_text(
            Language::English,
            GridTable {
                document: &document,
                table,
                layout: ScaledLayout::new(&table.layout, 1.0),
                is_markdown: true,
                markdown_source: None,
                markdown_table_index: None,
            },
            formula,
            CellDisplayMode::View,
        ),
        "2"
    );
    assert_eq!(
        grid_accessibility_cell_text(
            Language::English,
            GridTable {
                document: &document,
                table,
                layout: ScaledLayout::new(&table.layout, 1.0),
                is_markdown: true,
                markdown_source: None,
                markdown_table_index: None,
            },
            literal,
            CellDisplayMode::View,
        ),
        "=1+1"
    );
}

#[test]
// Keeps an actual leading apostrophe after removing the persisted literal marker once.
fn markdown_grid_accessibility_keeps_a_literal_apostrophe() {
    let source = Arc::new(
        crate::markdown_adapter::MarkdownDocument::parse(
            b"| Value |\n| --- |\n| ''=1+1 |\n".to_vec(),
        )
        .expect("Markdown source parses"),
    );
    let mut document = source.to_document(722, "literal markdown");
    document.set_source(Some(Arc::new(crate::source::SourceDocument::Markdown(
        Arc::clone(&source),
    ))));
    let table = &document.tables[0];
    let value = table.cell(CellAddress { row: 1, column: 0 }).unwrap();

    assert_eq!(
        cell_display_text(value, None, CellDisplayMode::View, true, false),
        "'=1+1"
    );
    assert_eq!(
        grid_accessibility_cell_text(
            Language::English,
            GridTable {
                document: &document,
                table,
                layout: ScaledLayout::new(&table.layout, 1.0),
                is_markdown: true,
                markdown_source: Some(&source),
                markdown_table_index: Some(0),
            },
            CellAddress { row: 1, column: 0 },
            CellDisplayMode::View,
        ),
        "'=1+1"
    );
}
