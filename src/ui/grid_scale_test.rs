use super::*;

// Zoomed pixel sizes scale while the stored width units stay unchanged.
#[test]
fn scaled_layout_multiplies_only_pixel_sizes() {
    let mut layout = TableLayout::default();
    layout.apply_value(
        LayoutTarget::Column(1),
        LayoutValue::Column(Some(ColumnLayout {
            width: Some(20.0),
            ..Default::default()
        })),
    );
    let scaled = ScaledLayout::new(&layout, 2.0, crate::document::REFERENCE_CHARACTER_WIDTH);
    assert_eq!(
        scaled.column_width_pixels(1),
        layout.column_width_pixels(1) * 2.0
    );
    assert_eq!(
        scaled.prefix_column_width_pixels(3),
        layout.prefix_column_width_pixels(3) * 2.0
    );
    assert_eq!(scaled.column(1).and_then(|column| column.width), Some(20.0));
}

// Zoom in and out move to the neighbouring multiple of 10% and stop at the limits.
#[test]
fn sheet_zoom_steps_by_ten_percent() {
    assert_eq!(step_sheet_zoom(1.0, true), 1.1);
    assert_eq!(step_sheet_zoom(1.0, false), 0.9);
    assert_eq!(step_sheet_zoom(0.75, true), 0.8);
    assert_eq!(step_sheet_zoom(0.75, false), 0.7);
    assert_eq!(step_sheet_zoom(MIN_SHEET_ZOOM, false), MIN_SHEET_ZOOM);
    assert_eq!(step_sheet_zoom(MAX_SHEET_ZOOM, true), MAX_SHEET_ZOOM);
}

// Keeps chosen zooms to whole percents between 10% and 400%.
#[test]
fn sheet_zoom_is_clamped_to_whole_percents() {
    assert_eq!(clamp_sheet_zoom(0.05), MIN_SHEET_ZOOM);
    assert_eq!(clamp_sheet_zoom(5.0), MAX_SHEET_ZOOM);
    assert_eq!(clamp_sheet_zoom(1.234), 1.23);
    assert_eq!(clamp_sheet_zoom(f32::NAN), 1.0);
}

// The column width unit is one half-width character of the bundled monospace font.
#[test]
fn grid_character_width_measures_the_bundled_monospace_font() {
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
    output.textures_delta.clear();
    assert_eq!(
        grid_character_width(&ctx),
        crate::document::REFERENCE_CHARACTER_WIDTH,
        "the reference width is used before the grid fonts exist"
    );
    install_texcel_grid_fonts(&ctx, crate::settings::FontPreset::NotoSansJp);
    let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
    output.textures_delta.clear();
    assert_eq!(grid_character_width(&ctx), 7.0);
}
