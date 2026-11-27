use super::*;
use crate::markdown_adapter::MarkdownTableAlignment;

#[cfg(test)]
#[path = "headers_test.rs"]
mod tests;

/// Gap between a column header's edge and its alignment colon.
const ALIGNMENT_MARK_INSET: f32 = 5.0;

/// Says which header edges show a colon, matching the Markdown separator row.
fn alignment_marks(alignment: Option<MarkdownTableAlignment>) -> (bool, bool) {
    match alignment {
        None => (false, false),
        Some(MarkdownTableAlignment::Left) => (true, false),
        Some(MarkdownTableAlignment::Center) => (true, true),
        Some(MarkdownTableAlignment::Right) => (false, true),
    }
}

/// Picks the header label color: strong inside the saved range or the selection, weak outside.
fn header_text_color(selected: bool, saved: bool, strong: Color32, weak: Color32) -> Color32 {
    if selected || saved { strong } else { weak }
}

impl GridViewportFrame<'_> {
    /// Draws the visible column headers and records their selection actions.
    pub(super) fn draw_column_headers(
        &mut self,
        grid_ui: &mut egui::Ui,
        painter: &egui::Painter,
        outcome: &mut GridOutcome,
    ) {
        let last_visible_frozen_column = self
            .visible_columns
            .iter()
            .copied()
            .filter(|column| *column < self.frozen_column_count)
            .max();
        for column in &self.visible_columns {
            let width = self.grid.layout.column_width_pixels(*column) as f32;
            if width <= 0.0 {
                continue;
            }
            let x = if *column < self.frozen_column_count {
                self.viewport_origin.x
                    + self.row_header_width
                    + self.grid.layout.prefix_column_width_pixels(*column) as f32
            } else {
                self.viewport_origin.x
                    + self.row_header_width
                    + self.grid.layout.prefix_column_width_pixels(*column) as f32
                    - self.effective_scroll_x
            };
            let rect = Rect::from_min_size(
                Pos2::new(x, self.viewport_origin.y),
                Vec2::new(width, self.header_height),
            );
            let header_clip = if *column < self.frozen_column_count {
                Rect::from_min_max(
                    Pos2::new(self.body_min.x, self.viewport_rect.min.y),
                    Pos2::new(self.frozen_x, self.viewport_rect.min.y + self.header_height),
                )
            } else {
                Rect::from_min_max(
                    Pos2::new(self.frozen_x, self.viewport_rect.min.y),
                    Pos2::new(
                        self.viewport_rect.max.x,
                        self.viewport_rect.min.y + self.header_height,
                    ),
                )
            };
            let header_painter = painter.with_clip_rect(header_clip);
            let active_column = *column == self.active_cell.column;
            let selected_column = self
                .selected_ranges
                .iter()
                .any(|(start, end)| *column >= start.column && *column <= end.column);
            header_painter.rect_filled(
                rect,
                0.0,
                if active_column {
                    self.palette.active_header_fill
                } else if selected_column {
                    self.palette.selection_fill
                } else {
                    self.palette.header_background
                },
            );
            header_painter.rect_stroke(
                rect,
                0.0,
                Stroke::new(0.75, self.palette.grid),
                egui::StrokeKind::Inside,
            );
            let header_text_color = header_text_color(
                active_column || selected_column,
                *column <= self.extent_end.column,
                self.palette.foreground,
                self.visuals.weak_text_color(),
            );
            header_painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                column_name(*column),
                FontId::proportional(grid_font_size(self.grid.layout.scale())),
                header_text_color,
            );
            // Show the Markdown column alignment with colons at the header's edges.
            let alignment = self
                .grid
                .markdown_source
                .zip(self.grid.markdown_table_index)
                .and_then(|(source, table)| source.column_alignment(table, *column));
            let (left_mark, right_mark) = alignment_marks(alignment);
            for (shown, anchor, position) in [
                (
                    left_mark,
                    Align2::LEFT_CENTER,
                    rect.left_center()
                        + Vec2::new(ALIGNMENT_MARK_INSET * self.grid.layout.scale(), 0.0),
                ),
                (
                    right_mark,
                    Align2::RIGHT_CENTER,
                    rect.right_center()
                        - Vec2::new(ALIGNMENT_MARK_INSET * self.grid.layout.scale(), 0.0),
                ),
            ] {
                if shown {
                    header_painter.text(
                        position,
                        anchor,
                        ":",
                        FontId::proportional(grid_font_size(self.grid.layout.scale())),
                        header_text_color,
                    );
                }
            }
            let click_rect = Rect::from_min_max(
                Pos2::new((rect.left() + 4.0).min(rect.right()), rect.top()),
                Pos2::new((rect.right() - 4.0).max(rect.left()), rect.bottom()),
            );
            let click_clip = if self.frozen_column_count > 0 && *column >= self.frozen_column_count
            {
                Rect::from_min_max(
                    Pos2::new(self.frozen_x + 4.0, header_clip.min.y),
                    header_clip.max,
                )
            } else {
                header_clip
            };
            let response = grid_ui.interact(
                click_rect.intersect(click_clip),
                Id::unique(egui::IdSalt::new((
                    "grid_column_header",
                    self.document_id,
                    self.table_id,
                    *column,
                ))),
                Sense::click_and_drag(),
            );
            self.update_header_selection(
                grid_ui,
                &response,
                StructureAxis::Columns,
                *column,
                outcome,
            );
            outcome.focused |= response.has_focus();
            super::super::grid_focus::lock_grid_navigation(grid_ui, &response);
            response.context_menu(|menu| {
                structure_axis_context_menu(
                    menu,
                    StructureAxis::Columns,
                    *column,
                    self.column_count,
                    self.input.structure_block_reason.is_none(),
                    self.input.structure_block_reason,
                    &mut outcome.structure_action,
                );
            });
            let resize_rect = Rect::from_center_size(
                Pos2::new(rect.right(), rect.center().y),
                Vec2::new(8.0, self.header_height),
            );
            let resize_clip = if Some(*column) == last_visible_frozen_column {
                Rect::from_min_max(
                    Pos2::new(self.frozen_x - 4.0, header_clip.min.y),
                    Pos2::new(self.frozen_x + 4.0, header_clip.max.y),
                )
                .intersect(self.viewport_rect)
            } else {
                header_clip
            };
            let resize_rect = resize_rect.intersect(resize_clip);
            let resize = grid_ui.interact(
                resize_rect,
                Id::unique(egui::IdSalt::new((
                    "grid_column_resize",
                    self.document_id,
                    self.table_id,
                    *column,
                ))),
                Sense::click_and_drag(),
            );
            update_header_resize(
                grid_ui,
                &resize,
                self.state.header_resize_drag,
                HeaderResizeTarget {
                    document_id: self.document_id,
                    table_id: self.table_id,
                    column: *column,
                    start_width: self.grid.layout.column_width_pixels(*column) as f32,
                    scale: self.grid.layout.scale(),
                    character_width: self.grid.layout.character_width(),
                },
                outcome,
            );
        }
    }

    /// Draws the visible row headers and records their selection actions.
    pub(super) fn draw_row_headers(
        &mut self,
        grid_ui: &mut egui::Ui,
        painter: &egui::Painter,
        outcome: &mut GridOutcome,
    ) {
        for row in &self.visible_rows {
            let row_height = self.row_layout.row_height(*row);
            let y = if *row < self.frozen_row_count {
                self.viewport_origin.y + self.header_height + self.row_layout.prefix_height(*row)
            } else {
                self.viewport_origin.y + self.header_height + self.row_layout.prefix_height(*row)
                    - self.effective_scroll_y
            };
            let row_header = Rect::from_min_size(
                Pos2::new(self.viewport_origin.x, y),
                Vec2::new(self.row_header_width, row_height),
            );
            let header_clip = if *row < self.frozen_row_count {
                Rect::from_min_max(
                    Pos2::new(self.viewport_rect.min.x, self.body_min.y),
                    Pos2::new(
                        self.viewport_rect.min.x + self.row_header_width,
                        self.frozen_y,
                    ),
                )
            } else {
                Rect::from_min_max(
                    Pos2::new(self.viewport_rect.min.x, self.frozen_y),
                    Pos2::new(
                        self.viewport_rect.min.x + self.row_header_width,
                        self.viewport_rect.max.y,
                    ),
                )
            };
            let header_painter = painter.with_clip_rect(header_clip);
            let active_row = *row == self.active_cell.row;
            let selected_row = self
                .selected_ranges
                .iter()
                .any(|(start, end)| *row >= start.row && *row <= end.row);
            header_painter.rect_filled(
                row_header,
                0.0,
                if active_row {
                    self.palette.active_header_fill
                } else if selected_row {
                    self.palette.selection_fill
                } else {
                    self.palette.header_background
                },
            );
            header_painter.rect_stroke(
                row_header,
                0.0,
                Stroke::new(0.75, self.palette.grid),
                egui::StrokeKind::Inside,
            );
            header_painter.text(
                row_header.right_center() - Vec2::new(8.0 * self.grid.layout.scale(), 0.0),
                Align2::RIGHT_CENTER,
                (*row + 1).to_string(),
                FontId::proportional(grid_font_size(self.grid.layout.scale())),
                header_text_color(
                    active_row || selected_row,
                    *row <= self.extent_end.row,
                    self.palette.foreground,
                    self.visuals.weak_text_color(),
                ),
            );
            // Row headers have no resize edge, so the whole header selects the row.
            let response = grid_ui.interact(
                row_header.intersect(header_clip),
                Id::unique(egui::IdSalt::new((
                    "grid_row_header",
                    self.document_id,
                    self.table_id,
                    *row,
                ))),
                Sense::click_and_drag(),
            );
            self.update_header_selection(grid_ui, &response, StructureAxis::Rows, *row, outcome);
            outcome.focused |= response.has_focus();
            super::super::grid_focus::lock_grid_navigation(grid_ui, &response);
            response.context_menu(|menu| {
                structure_axis_context_menu(
                    menu,
                    StructureAxis::Rows,
                    *row,
                    self.row_count,
                    self.input.structure_block_reason.is_none(),
                    self.input.structure_block_reason,
                    &mut outcome.structure_action,
                );
            });
        }
    }
}

/// The column whose header edge is being dragged, and its width when the drag began.
struct HeaderResizeTarget {
    document_id: DocumentId,
    table_id: u64,
    column: usize,
    start_width: f32,
    /// Sheet zoom of the drag, so the stored width stays at 100%.
    scale: f32,
    /// Width of one column width unit at 100%.
    character_width: f64,
}

// Applies one column header drag as a local display layout value.
fn update_header_resize(
    ui: &egui::Ui,
    response: &egui::Response,
    active_drag: &mut Option<HeaderResizeDrag>,
    header: HeaderResizeTarget,
    outcome: &mut GridOutcome,
) {
    let HeaderResizeTarget {
        document_id,
        table_id,
        column,
        start_width,
        scale,
        character_width,
    } = header;
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    if response.double_clicked() {
        outcome.auto_fit_requested = Some((document_id, table_id, column));
    }
    if response.drag_started_by(egui::PointerButton::Primary)
        && let Some(pointer) = ui.input(|input| input.pointer.press_origin())
    {
        *active_drag = Some(HeaderResizeDrag {
            document_id,
            table_id,
            column,
            pointer_start: pointer.x,
            width_start: start_width,
        });
    }
    let matching_drag = active_drag.is_some_and(|drag| {
        drag.document_id == document_id && drag.table_id == table_id && drag.column == column
    });
    if matching_drag
        && (response.dragged() || response.drag_stopped_by(egui::PointerButton::Primary))
        && let (Some(drag), Some(pointer)) = (
            *active_drag,
            ui.input(|input| input.pointer.interact_pos().or(input.pointer.latest_pos())),
        )
    {
        let pixels = (drag.width_start + pointer.x - drag.pointer_start).max(1.0) / scale;
        // Convert pixels to half-width monospace characters, as stored in the layout.
        let width =
            crate::document::column_width_units_for_pixels(f64::from(pixels), character_width);
        outcome.resized_layout = Some((
            document_id,
            table_id,
            LayoutTarget::Column(column),
            LayoutValue::Column(Some(ColumnLayout { width: Some(width) })),
        ));
    }
    if matching_drag && response.drag_stopped_by(egui::PointerButton::Primary) {
        *active_drag = None;
        outcome.resize_finished = true;
    }
}
