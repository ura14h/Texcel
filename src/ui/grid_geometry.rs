use super::*;

pub(super) const GRID_CELL_TEXT_VERTICAL_PADDING: f32 = 6.0;

// Map a pointer position to the cell address under it.
pub(super) fn grid_address_at_pointer(
    position: Pos2,
    geometry: GridPointerGeometry<'_>,
) -> Option<CellAddress> {
    if !geometry.viewport.contains(position)
        || position.x < geometry.body_min.x
        || position.y < geometry.body_min.y
    {
        return None;
    }
    let local_x = position.x - geometry.body_min.x;
    let local_y = position.y - geometry.body_min.y;
    let content_x = if local_x < geometry.frozen_width {
        local_x
    } else {
        local_x + geometry.scroll_x
    };
    let content_y = if local_y < geometry.frozen_height {
        local_y
    } else {
        local_y + geometry.scroll_y
    };
    let column = column_at_position(geometry.layout, geometry.columns, content_x);
    let row = geometry.row_layout.row_at_position(content_y);
    if geometry.layout.column_width_pixels(column) <= 0.0 || row >= geometry.rows {
        return None;
    }
    Some(CellAddress { row, column })
}

// Find the columns whose bounds overlap the visible viewport.
pub(super) fn visible_columns_in_pixels(
    layout: ScaledLayout<'_>,
    columns: usize,
    start: f32,
    end: f32,
) -> Vec<usize> {
    let mut visible = Vec::new();
    let mut position = start.max(0.0);
    if columns == 0 || layout.prefix_column_width_pixels(columns) as f32 <= position {
        return visible;
    }
    while position < end && columns > 0 {
        let mut column = column_at_position(layout, columns, position);
        if visible.last() == Some(&column) {
            position += 0.25;
            continue;
        }
        let mut next = layout.prefix_column_width_pixels(column + 1) as f32;
        // A start rounded to f32, such as the frozen width, can land on the end of the
        // previous column; the visible columns begin at the next one.
        while next <= position && column + 1 < columns {
            column += 1;
            next = layout.prefix_column_width_pixels(column + 1) as f32;
        }
        if next <= position {
            break;
        }
        visible.push(column);
        position = next;
    }
    visible
}

// Find the rows whose bounds overlap the visible viewport.
pub(super) fn visible_rows_in_pixels(
    row_layout: &GridRowLayout,
    start: f32,
    end: f32,
    upper_bound: usize,
) -> Vec<usize> {
    let mut visible = Vec::new();
    let mut position = start.max(0.0);
    let upper_bound = upper_bound.min(row_layout.row_count);
    while position < end && upper_bound > 0 {
        let row = row_layout.row_at_position(position);
        if row >= upper_bound {
            break;
        }
        if visible.last() != Some(&row) {
            visible.push(row);
        }
        let next = row_layout.prefix_height(row + 1);
        if next <= position {
            break;
        }
        position = next;
    }
    visible
}

// Measure the rendered height of one row in the current view.
pub(super) fn measure_display_row_height(
    ctx: &egui::Context,
    grid: GridTable<'_>,
    display: GridDisplay,
    row: usize,
    visible_column_ranges: &[std::ops::Range<usize>],
    markdown_cell_displays: &BTreeMap<MarkdownCellDisplayKey, CachedMarkdownCellDisplay>,
) -> (u64, f32) {
    let GridTable {
        document,
        table,
        layout,
        is_markdown,
        markdown_source,
        markdown_table_index,
    } = grid;
    let GridDisplay {
        mode,
        show_invisibles,
        wrap_text,
        font_layout_generation,
        scale,
    } = display;
    let mut hasher = DefaultHasher::new();
    row.hash(&mut hasher);
    document.revision.hash(&mut hasher);
    document.calculation_revision.hash(&mut hasher);
    is_markdown.hash(&mut hasher);
    crate::i18n::context_language(ctx).hash(&mut hasher);
    mode.hash(&mut hasher);
    wrap_text.hash(&mut hasher);
    font_layout_generation.hash(&mut hasher);
    show_invisibles.hash(&mut hasher);
    scale.to_bits().hash(&mut hasher);
    let mut height = ROW_HEIGHT * scale;
    for columns in visible_column_ranges {
        columns.start.hash(&mut hasher);
        columns.end.hash(&mut hasher);
        for (address, cell) in table.stored_cells_in_row_columns(row, columns.clone()) {
            let calculation = document.calculation_result_for(table.id, address);
            let is_formula = crate::structure::is_formula_cell(table, address, cell);
            let rendered_text = cell_display_text(
                cell,
                calculation.as_ref(),
                mode,
                is_markdown && !is_formula,
                is_formula,
            );
            let rendered_text = bounded_grid_text(rendered_text);
            let cell_revision = document.cell_input_revision(table.id, address);
            let source_display = if is_markdown && !is_formula && mode != CellDisplayMode::Source {
                markdown_table_index
                    .and_then(|table_index| {
                        markdown_source.and_then(|source| {
                            if document.cell_input_is_dirty(table.id, address) {
                                None
                            } else {
                                source.cell_display(table_index, address.row, address.column)
                            }
                        })
                    })
                    .or_else(|| {
                        markdown_cell_displays
                            .get(&(document.id, table.id, address))
                            .filter(|cached| {
                                cached.cell_input_revision == Some(cell_revision)
                                    && cached.edit_session_id.is_none()
                                    && cached.raw_text.is_none()
                            })
                            .map(|cached| &cached.display)
                    })
            } else {
                None
            };
            let has_markdown_break =
                source_display.is_some_and(|display| display.rendered_lines > 1);
            let explicit_break = rendered_text.contains('\n');
            if !explicit_break && !has_markdown_break && !wrap_text {
                continue;
            }

            address.column.hash(&mut hasher);
            mode.hash(&mut hasher);
            rendered_text.as_ref().hash(&mut hasher);
            wrap_text.hash(&mut hasher);
            let cell_style = CellTextStyle {
                wrap_text,
                alignment: None,
            };
            let width = layout.column_width_pixels(address.column) as f32;
            width.to_bits().hash(&mut hasher);
            if let Some(display) = source_display {
                display.rendered_lines.hash(&mut hasher);
                display.rendered_bytes.hash(&mut hasher);
                for run in &display.runs {
                    run.text.hash(&mut hasher);
                    run.source_range.hash(&mut hasher);
                    run.bold.hash(&mut hasher);
                    run.italic.hash(&mut hasher);
                    run.strike.hash(&mut hasher);
                    run.code.hash(&mut hasher);
                    run.link.hash(&mut hasher);
                    run.localized_image.hash(&mut hasher);
                }
            }

            let (mut job, _) = if is_markdown
                && !is_formula
                && mode != CellDisplayMode::Source
                && matches!(cell.input, CellInput::Text(_))
            {
                if let Some(display) = source_display {
                    markdown_cell_layout_job(
                        display,
                        CellTextFormat {
                            style: &cell_style,
                            color: Color32::BLACK,
                            width,
                            show_invisibles,
                            scale,
                        },
                        markdown_source,
                        crate::i18n::context_language(ctx),
                    )
                } else {
                    cell_layout_job(
                        &rendered_text,
                        CellTextFormat {
                            style: &cell_style,
                            color: Color32::BLACK,
                            width,
                            show_invisibles,
                            scale,
                        },
                    )
                }
            } else {
                cell_layout_job(
                    &rendered_text,
                    CellTextFormat {
                        style: &cell_style,
                        color: Color32::BLACK,
                        width,
                        show_invisibles,
                        scale,
                    },
                )
            };
            if mode == CellDisplayMode::Source {
                apply_grid_monospace(&mut job);
            }
            job.wrap.max_rows = 256;
            let line_height = super::cell_paint::cell_text_line_height(ctx, &job);
            let galley = ctx.fonts_mut(|fonts| fonts.layout_job(job));
            height = height.max(
                (measured_galley_height(&galley, line_height)
                    + GRID_CELL_TEXT_VERTICAL_PADDING * scale)
                    .min(MAX_WRAPPED_ROW_HEIGHT * scale),
            );
        }
    }
    (hasher.finish(), height)
}

// Keeps fractional row heights so painting can show the same measured lines.
fn measured_galley_height(galley: &egui::text::Galley, line_height: f32) -> f32 {
    let rows_height = galley.rows.len() as f32 * line_height;
    galley.size().y.max(rows_height)
}
