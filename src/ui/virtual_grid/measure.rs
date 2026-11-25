use super::*;

/// Measures the row-number gutter from the largest row number on screen.
pub(super) fn row_header_width(
    ctx: &egui::Context,
    row_layout: &GridRowLayout,
    scroll_y: f32,
    viewport_height: f32,
    frozen_rows: usize,
    scale: f32,
) -> f32 {
    let header_height = grid_header_height(scale);
    let frozen_height = row_layout.prefix_height(frozen_rows);
    let pinned_rows = visible_rows_in_pixels(row_layout, 0.0, frozen_height, frozen_rows);
    let scrolling_rows = visible_rows_in_pixels(
        row_layout,
        frozen_height + scroll_y,
        frozen_height + scroll_y + (viewport_height - header_height - frozen_height).max(0.0),
        MAX_ROWS,
    );
    let max_row = pinned_rows
        .into_iter()
        .chain(scrolling_rows)
        .max()
        .unwrap_or_default();
    let label = (max_row + 1).to_string();
    let font_id = FontId::proportional(grid_font_size(scale));
    let label_width = ctx.fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(label, font_id, Color32::WHITE)
            .size()
            .x
    });
    (ROW_HEADER_WIDTH * scale).max(label_width.ceil() + 16.0 * scale)
}

/// Measures visible rows and adjusts the scroll offset after wrapped heights change.
pub(super) fn adjust_visible_row_heights(
    ui: &egui::Ui,
    grid: GridTable<'_>,
    display: GridDisplay,
    row_layout: &mut GridRowLayout,
    scroll: crate::document::ScrollPosition,
    viewport_size: Vec2,
    markdown_cell_displays: &BTreeMap<MarkdownCellDisplayKey, CachedMarkdownCellDisplay>,
) -> (crate::document::ScrollPosition, f32) {
    let GridTable {
        document,
        table,
        layout,
        ..
    } = grid;
    let row_count = MAX_ROWS;
    let column_count = MAX_COLUMNS;
    let frozen = layout.frozen();
    let frozen_row_count = frozen.rows.min(row_count);
    let frozen_column_count = frozen.columns.min(column_count);
    let frozen_height = row_layout.prefix_height(frozen_row_count);
    let frozen_width = layout.prefix_column_width_pixels(frozen_column_count) as f32;
    let header_width = row_header_width(
        ui.ctx(),
        row_layout,
        scroll.y,
        viewport_size.y,
        frozen_row_count,
        display.scale,
    );
    let header_height = grid_header_height(display.scale);
    let body_width = (viewport_size.x - header_width).max(0.0);
    let pinned_measure_columns =
        visible_columns_in_pixels(layout, column_count, 0.0, frozen_width.min(body_width))
            .into_iter()
            .filter(|column| *column < frozen_column_count)
            .collect::<Vec<_>>();
    let scrolling_measure_columns = visible_columns_in_pixels(
        layout,
        column_count,
        scroll.x + frozen_width,
        scroll.x + body_width,
    )
    .into_iter()
    .filter(|column| *column >= frozen_column_count)
    .collect::<Vec<_>>();
    let mut measure_column_ranges = Vec::new();
    for columns in [&pinned_measure_columns, &scrolling_measure_columns] {
        if let Some((&first, &last)) = columns.first().zip(columns.last()) {
            measure_column_ranges.push(first..last.saturating_add(1));
        }
    }
    if measure_column_ranges.is_empty() {
        measure_column_ranges.push(0..0);
    }

    let measure_start = scroll.y + frozen_height;
    let measure_end = scroll.y + viewport_size.y - header_height;
    let first_visible_row = row_layout.row_at_position(measure_start);
    let mut measure_rows =
        visible_rows_in_pixels(row_layout, measure_start, measure_end, row_count);
    measure_rows.extend(
        visible_rows_in_pixels(
            row_layout,
            0.0,
            frozen_height.min((viewport_size.y - header_height).max(0.0)),
            frozen_row_count,
        )
        .into_iter()
        .filter(|row| *row < frozen_row_count),
    );
    measure_rows.sort_unstable();
    measure_rows.dedup();
    let mut adjusted_scroll = scroll;
    for row in measure_rows {
        let (fingerprint, wrapped_cells) = wrapped_row_cells_for_ranges(
            document,
            table,
            layout,
            row,
            &measure_column_ranges,
            display.wrap_text,
        );
        let (display_fingerprint, display_height) = measure_display_row_height(
            ui.ctx(),
            grid,
            display,
            row,
            &measure_column_ranges,
            markdown_cell_displays,
        );
        let mut combined_fingerprint = DefaultHasher::new();
        fingerprint.hash(&mut combined_fingerprint);
        display_fingerprint.hash(&mut combined_fingerprint);
        let fingerprint = combined_fingerprint.finish();
        let height = row_layout
            .measured_height(row, document.revision, fingerprint)
            .unwrap_or_else(|| {
                measure_wrapped_cells(
                    ui.ctx(),
                    &wrapped_cells,
                    display.wrap_text,
                    display.show_invisibles,
                    display.mode == CellDisplayMode::Source,
                    display.scale,
                )
                .max(display_height)
            });
        let delta = row_layout.update_row(row, document.revision, fingerprint, height);
        if row >= frozen_row_count && row < first_visible_row {
            adjusted_scroll.y += delta;
        }
    }
    (adjusted_scroll, header_width)
}
