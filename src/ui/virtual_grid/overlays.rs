use super::*;

impl GridViewportFrame<'_> {
    /// Describes this frame's cell layout for pointer hit testing.
    fn pointer_geometry(&self) -> GridPointerGeometry<'_> {
        GridPointerGeometry {
            viewport: self.viewport_rect,
            body_min: self.body_min,
            scroll_x: self.effective_scroll_x,
            scroll_y: self.effective_scroll_y,
            frozen_width: self.frozen_width,
            frozen_height: self.frozen_height,
            layout: self.grid.layout,
            row_layout: self.row_layout,
            rows: self.row_count,
            columns: self.column_count,
        }
    }

    /// Finds the range an active fill-handle drag would fill if released now.
    /// Cells outside the source get the outline before the drag is released.
    pub(super) fn fill_handle_preview(&self, grid_ui: &egui::Ui) -> Option<CellRange> {
        let drag = self.input.fill_handle_drag?;
        if drag.document_id != self.document_id || drag.table_id != self.table_id {
            return None;
        }
        let position = grid_ui.input(|input| input.pointer.interact_pos())?;
        let endpoint = grid_address_at_pointer(position, self.pointer_geometry())?;
        fill_handle_target(drag.source, endpoint)
    }

    /// Draws frozen-pane guides and finishes an active fill-handle drag.
    pub(super) fn draw_frozen_guides_and_fill(
        &mut self,
        grid_ui: &mut egui::Ui,
        painter: &egui::Painter,
        outcome: &mut GridOutcome,
    ) {
        if self.grid.layout.frozen().columns > 0 {
            let x = self.viewport_origin.x
                + self.row_header_width
                + self
                    .grid
                    .layout
                    .prefix_column_width_pixels(self.grid.layout.frozen().columns)
                    as f32;
            painter.line_segment(
                [
                    Pos2::new(x, self.viewport_origin.y),
                    Pos2::new(x, self.viewport_rect.max.y),
                ],
                Stroke::new(1.25, self.palette.grid),
            );
        }
        if self.grid.layout.frozen().rows > 0 {
            let y = self.viewport_origin.y
                + HEADER_HEIGHT
                + self
                    .row_layout
                    .prefix_height(self.grid.layout.frozen().rows);
            painter.line_segment(
                [
                    Pos2::new(self.viewport_origin.x, y),
                    Pos2::new(self.viewport_rect.max.x, y),
                ],
                Stroke::new(1.25, self.palette.grid),
            );
        }
        if let Some(drag) = self.input.fill_handle_drag {
            let (released, cancelled, pointer_position) = grid_ui.input(|input| {
                (
                    input.pointer.button_released(egui::PointerButton::Primary),
                    input.key_pressed(Key::Escape),
                    input.pointer.interact_pos(),
                )
            });
            if drag.document_id != self.document_id
                || drag.table_id != self.table_id
                || self.input.structure_block_reason.is_some()
                || cancelled
            {
                outcome.fill_handle_cancelled = true;
            } else if released {
                let endpoint = pointer_position.and_then(|position| {
                    grid_address_at_pointer(position, self.pointer_geometry())
                });
                if let Some(endpoint) = endpoint {
                    outcome.fill_handle_finished = Some(endpoint);
                } else {
                    outcome.fill_handle_cancelled = true;
                }
            }
        }
    }
}
