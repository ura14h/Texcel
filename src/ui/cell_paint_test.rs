use super::*;

#[test]
// Marks a long right-aligned identifier as clipped without changing its source text.
fn long_cell_text_keeps_its_value_and_draws_an_overflow_mark() {
    let ctx = egui::Context::default();
    let original = "00123456789012345678";
    let mut job = egui::text::LayoutJob::default();
    job.append(original, 0.0, egui::TextFormat::default());
    let style = CellTextStyle {
        alignment: Some(HorizontalAlignment::Right),
        ..CellTextStyle::default()
    };
    let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(48.0, 24.0));
    let mut galley = None;
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        constrain_cell_text_layout(ui.ctx(), bounds, &style, false, 1.0, &mut job);
        galley = Some(ui.ctx().fonts_mut(|fonts| fonts.layout_job(job.clone())));
    });
    output.textures_delta.clear();
    let galley = galley.expect("the frame lays out the cell text");
    assert!(galley.elided);
    assert_eq!(galley.job.text, original);
    assert_eq!(
        galley
            .rows
            .iter()
            .flat_map(|row| row.row.glyphs.iter())
            .last()
            .map(|glyph| glyph.chr),
        Some('…')
    );
}

#[test]
// Marks a long Markdown line and keeps its explicit second line.
fn nowrap_multiline_text_marks_width_overflow_without_dropping_lines() {
    let ctx = egui::Context::default();
    let original = "**a very long first line**<br>x";
    let style = CellTextStyle {
        wrap_text: false,
        ..CellTextStyle::default()
    };
    let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(52.0, 40.0));
    let display = crate::markdown_adapter::MarkdownCellDisplay::parse_fragment(original);
    let (mut job, _) = crate::ui::markdown_cell_layout_job(
        &display,
        CellTextFormat {
            style: &style,
            color: Color32::BLACK,
            width: bounds.width(),
            show_invisibles: false,
            scale: 1.0,
        },
        None,
        crate::i18n::Language::English,
    );
    assert_eq!(job.text, "a very long first line\nx");
    job.wrap.max_rows = 256;
    let mut galley = None;
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        constrain_cell_text_layout(
            ui.ctx(),
            bounds,
            &style,
            job.text.contains('\n'),
            1.0,
            &mut job,
        );
        galley = Some(ui.ctx().fonts_mut(|fonts| fonts.layout_job(job.clone())));
    });
    output.textures_delta.clear();
    let galley = galley.expect("the frame lays out the cell text");

    assert_eq!(galley.job.text, "a very long first line\nx");
    assert_eq!(galley.rows.len(), 2);
    assert_eq!(
        galley.rows[1]
            .row
            .glyphs
            .iter()
            .map(|glyph| glyph.chr)
            .collect::<String>(),
        "x"
    );
    let position = cell_text_position(bounds, galley.rect, &style, 1.0);
    assert!(galley.rows[0].row.size.x > bounds.shrink(3.0).width());
    assert_eq!(
        clipped_row_edges(bounds.shrink(3.0), position, &galley),
        vec![(0, false, true)]
    );
    let mut marker_output = ctx.run_ui(egui::RawInput::default(), |ui| {
        paint_horizontal_overflow_markers(
            ui.painter(),
            bounds.shrink(3.0),
            position,
            &galley,
            Color32::WHITE,
            Color32::BLACK,
        );
    });
    let marker_count = marker_output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
            _ => None,
        })
        .filter(|text| *text == "…")
        .count();
    assert_eq!(marker_count, 1);
    marker_output.textures_delta.clear();
}

#[test]
// Keeps Markdown alignment in the painter instead of applying it twice to the galley.
fn markdown_galley_uses_the_single_cell_paint_alignment() {
    let style = CellTextStyle {
        alignment: Some(HorizontalAlignment::Right),
        ..CellTextStyle::default()
    };
    let display = crate::markdown_adapter::MarkdownCellDisplay::parse_fragment("Revenue");
    let (job, _) = crate::ui::markdown_cell_layout_job(
        &display,
        CellTextFormat {
            style: &style,
            color: Color32::BLACK,
            width: 112.0,
            show_invisibles: false,
            scale: 1.0,
        },
        None,
        crate::i18n::Language::English,
    );
    assert_eq!(job.halign, egui::Align::Max);

    let cell = Rect::from_min_size(Pos2::ZERO, Vec2::new(112.0, 40.0));
    let right_galley = Rect::from_min_max(Pos2::new(-42.0, 0.0), Pos2::new(0.0, 16.0));
    let right = cell_text_position(cell, right_galley, &style, 1.0);
    assert_eq!(right.x, cell.right() - 6.0);
    assert_eq!(right.y, cell.center().y - right_galley.center().y);

    let center_style = CellTextStyle {
        alignment: Some(HorizontalAlignment::Center),
        ..CellTextStyle::default()
    };
    let center_galley = Rect::from_min_max(Pos2::new(-21.0, 0.0), Pos2::new(21.0, 16.0));
    let center = cell_text_position(cell, center_galley, &center_style, 1.0);
    assert_eq!(center.x, cell.center().x - center_galley.center().x);
}
