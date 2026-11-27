use super::*;

#[test]
// Reads headings, marks, nested lists, code, tables, and `<br>` breaks from GFM text.
fn parse_markdown_builds_blocks_for_the_supported_syntax() {
    let blocks = parse_markdown(
        "# Title\n\nSome **bold** and `code`.\n\n- one\n- two\n  1. inner\n\n```\nx = 1\n```\n\n| A | B |\n| :-- | --: |\n| a<br>b | 2 |\n\n---\n",
    );
    assert!(matches!(&blocks[0], Block::Heading(1, inline) if inline[0].text == "Title"));
    let Block::Paragraph(paragraph) = &blocks[1] else {
        panic!("a paragraph follows the heading");
    };
    assert!(
        paragraph
            .iter()
            .any(|span| span.bold && span.text == "bold")
    );
    assert!(
        paragraph
            .iter()
            .any(|span| span.code && span.text == "code")
    );
    let Block::List { start: None, items } = &blocks[2] else {
        panic!("a bullet list follows");
    };
    assert_eq!(items.len(), 2);
    assert!(
        items[1]
            .iter()
            .any(|block| matches!(block, Block::List { start: Some(1), .. }))
    );
    assert_eq!(blocks[3], Block::Code("x = 1".to_owned()));
    let Block::Table { alignments, rows } = &blocks[4] else {
        panic!("a table follows");
    };
    assert_eq!(alignments.len(), 2);
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[1][0]
            .iter()
            .map(|span| span.text.as_str())
            .collect::<String>(),
        "a\nb"
    );
    assert_eq!(blocks[5], Block::Rule);
}

#[test]
// Wraps long table cells so the table stays within the width it is drawn at.
fn long_table_cells_wrap_within_the_available_width() {
    let ctx = egui::Context::default();
    install_texcel_grid_fonts(&ctx, crate::settings::FontPreset::Auto);
    let sentence = "a long sentence that keeps going ".repeat(12);
    let blocks = parse_markdown(&format!(
        "| File | Notes |\n| --- | --- |\n| `tables.md` | {sentence} |\n"
    ));
    let mut table = None;
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.set_max_width(200.0);
        let drawn = show_markdown(ui, &blocks, MarkdownViewStyle { body_size: 6.0 });
        table = drawn.table_rects.first().copied();
    });
    output.textures_delta.clear();
    let table = table.expect("the table is drawn");
    assert!(table.width() <= 200.5, "the table fits: {table:?}");
}
