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

#[test]
// Keeps narrow columns at their natural width and wraps only the widest ones.
fn table_columns_shrink_only_when_they_do_not_fit() {
    assert_eq!(fit_column_widths(&[40.0, 80.0], 300.0, 4.0), [40.0, 80.0]);
    assert_eq!(fit_column_widths(&[40.0, 500.0], 300.0, 4.0), [40.0, 260.0]);
    assert_eq!(
        fit_column_widths(&[400.0, 500.0], 300.0, 4.0),
        [150.0, 150.0]
    );
    assert_eq!(fit_column_widths(&[10.0, 10.0], 0.0, 4.0), [4.0, 4.0]);
}

/// Draws Markdown at one width and returns where each text run ended up on screen.
fn drawn_text(markdown: &str, width: f32) -> (Vec<(String, Rect)>, Rect) {
    let ctx = egui::Context::default();
    install_texcel_grid_fonts(&ctx, crate::settings::FontPreset::Auto);
    let blocks = parse_markdown(markdown);
    let mut table = Rect::NOTHING;
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.set_max_width(width);
        let drawn = show_markdown(ui, &blocks, MarkdownViewStyle { body_size: 6.0 });
        table = drawn.table_rects.first().copied().unwrap_or(Rect::NOTHING);
    });
    output.textures_delta.clear();
    let texts = output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) => Some((
                text.galley.text().to_owned(),
                text.galley.rect.translate(text.pos.to_vec2()),
            )),
            _ => None,
        })
        .collect();
    (texts, table)
}

#[test]
// Lines right-aligned cells up on the right edge of their column.
fn right_aligned_cells_share_their_right_edge() {
    let (texts, _) = drawn_text(
        "| Amount | Note |\n| ---: | --- |\n| 1 | a |\n| 12345 | b |\n",
        300.0,
    );
    let right = |wanted: &str| {
        texts
            .iter()
            .find(|(text, _)| text == wanted)
            .map(|(_, rect)| rect.right())
            .expect("the cell is drawn")
    };
    assert!((right("1") - right("12345")).abs() < 0.5);
    assert!((right("Amount") - right("12345")).abs() < 0.5);
}

#[test]
// Keeps a table with very many columns inside the width it is drawn at.
fn many_columns_stay_within_the_available_width() {
    let header = (0..40)
        .map(|column| format!("C{column}"))
        .collect::<Vec<_>>();
    let markdown = format!(
        "| {} |\n|{}\n",
        header.join(" | "),
        " ---: |".repeat(header.len())
    );
    let (_, table) = drawn_text(&markdown, 200.0);
    assert!(table.width() <= 200.5, "the table fits: {table:?}");
}
