use super::*;

#[cfg(test)]
#[path = "cells_test.rs"]
mod tests;

/// Right-aligns numbers and text that the calculation rules read as a number.
fn semantic_numeric_input(cell: &Cell) -> bool {
    match &cell.input {
        CellInput::Number(_) => true,
        CellInput::Text(text) => crate::numeric_text::parse_ascii_decimal(text).is_some(),
        _ => false,
    }
}

impl GridViewportFrame<'_> {
    /// Draws visible cells through the frozen panes and records cell interactions.
    pub(super) fn draw_visible_cells(
        &mut self,
        grid_ui: &mut egui::Ui,
        painter: &egui::Painter,
        outcome: &mut GridOutcome,
    ) {
        let visible_reference_overlays = visible_formula_reference_overlays(
            self.input.formula_reference_overlays,
            self.row_count,
            self.column_count,
        );
        for (row, column) in &self.render_cells {
            let cell_address = CellAddress {
                row: *row,
                column: *column,
            };
            // Each cell lies in exactly one frozen or scrolling pane.
            let Some((zone_index, zone)) = self
                .pane_zones
                .iter()
                .enumerate()
                .find(|(_, zone)| zone.rows.contains(row) && zone.columns.contains(column))
            else {
                continue;
            };
            let cell = self.grid.table.cell(cell_address);
            let is_formula = cell.is_some_and(|cell| {
                crate::structure::is_formula_cell(self.grid.table, cell_address, cell)
            });
            let calculation = self
                .grid
                .document
                .calculation_result_for(self.table_id, cell_address);
            let selected_range = self.selected_ranges.iter().find(|(start, end)| {
                cell_address.row >= start.row
                    && cell_address.row <= end.row
                    && cell_address.column >= start.column
                    && cell_address.column <= end.column
            });
            let selected = selected_range.is_some();
            {
                let x = self.viewport_origin.x
                    + self.row_header_width
                    + self.grid.layout.prefix_column_width_pixels(*column) as f32
                    - if zone.frozen_columns {
                        0.0
                    } else {
                        self.effective_scroll_x
                    };
                let y =
                    self.viewport_origin.y + HEADER_HEIGHT + self.row_layout.prefix_height(*row)
                        - if zone.frozen_rows {
                            0.0
                        } else {
                            self.effective_scroll_y
                        };
                let width = self.grid.layout.column_width_pixels(*column) as f32;
                let height = self.row_layout.row_height(*row);
                let rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, height));
                // While the fill handle is dragged, outline the range it would fill.
                let outline_range = match self.fill_preview {
                    Some((start, end)) => (cell_address.row >= start.row
                        && cell_address.row <= end.row
                        && cell_address.column >= start.column
                        && cell_address.column <= end.column)
                        .then_some((start, end)),
                    None => selected_range.copied(),
                };
                let selected_edges = outline_range.map_or([false; 4], |(start, end)| {
                    [
                        cell_address.row == start.row,
                        cell_address.row == end.row,
                        cell_address.column == start.column,
                        cell_address.column == end.column,
                    ]
                });
                let visible_rect = rect.intersect(zone.clip_rect);
                if visible_rect.width() <= 0.0 || visible_rect.height() <= 0.0 {
                    continue;
                }
                let edit_preview = self.state.edit.is_editing()
                    && self.state.edit.table_id == Some(self.table_id)
                    && self.state.edit.address == Some(cell_address)
                    && matches!(
                        self.state.edit.target,
                        Some(
                            crate::document::EditTarget::FormulaBar
                                | crate::document::EditTarget::DetailPanel
                        )
                    );
                let language = crate::i18n::context_language(grid_ui.ctx());
                let column_header = self
                    .header_row
                    .filter(|header_row| *header_row != cell_address.row)
                    .map(|header_row| {
                        grid_accessibility_cell_text(
                            language,
                            self.grid,
                            CellAddress {
                                row: header_row,
                                column: cell_address.column,
                            },
                            self.display.mode,
                        )
                    })
                    .filter(|value| !value.is_empty())
                    .map(|value| text_preview(&value, 128).to_owned())
                    .unwrap_or_default();
                // Name the row by its first cell, but only when the header row names that column.
                let row_header = self
                    .header_row
                    .filter(|header_row| *header_row != cell_address.row && cell_address.column > 0)
                    .filter(|header_row| {
                        let first_column_header = grid_accessibility_cell_text(
                            language,
                            self.grid,
                            CellAddress {
                                row: *header_row,
                                column: 0,
                            },
                            self.display.mode,
                        );
                        !first_column_header.is_empty()
                    })
                    .map(|_| {
                        grid_accessibility_cell_text(
                            language,
                            self.grid,
                            CellAddress {
                                row: cell_address.row,
                                column: 0,
                            },
                            self.display.mode,
                        )
                    })
                    .filter(|value| !value.is_empty())
                    .map(|value| text_preview(&value, 128).to_owned())
                    .unwrap_or_default();
                let markdown_display = if is_formula {
                    None
                } else if edit_preview {
                    markdown_display_for_preview(
                        self.grid,
                        cell_address,
                        self.state.edit.session_id,
                        &self.state.edit.buffer,
                        self.state.markdown_cell_displays,
                    )
                } else {
                    cell.and_then(|cell| {
                        markdown_display_for_cell(
                            self.grid,
                            cell_address,
                            cell,
                            self.state.markdown_cell_displays,
                            self.state.markdown_display_requests,
                        )
                    })
                };
                draw_grid_cell_segment(
                    grid_ui,
                    &painter.with_clip_rect(zone.clip_rect),
                    SegmentGrid {
                        grid: self.grid,
                        display: self.display,
                        input: &self.input,
                        selection: self.selection,
                        active_cell: self.active_cell,
                        edit: self.state.edit,
                        request_editor_focus: self.state.request_editor_focus,
                        request_grid_focus: self.state.request_grid_focus,
                    },
                    CellSegment {
                        rect,
                        visible_rect,
                        address: cell_address,
                        cell,
                        calculation: calculation.as_ref(),
                        is_formula,
                        semantic_numeric_input: cell.is_some_and(semantic_numeric_input),
                        selected,
                        selected_edges,
                        index: zone_index,
                        column_header: &column_header,
                        row_header: &row_header,
                        // Live editing shows the header row in bold; source editing shows plain text.
                        header_bold: self.display.mode != CellDisplayMode::Source
                            && self.header_row == Some(cell_address.row),
                        markdown_display,
                        reference_overlays: &visible_reference_overlays,
                    },
                    outcome,
                );
            }
        }
    }
}
