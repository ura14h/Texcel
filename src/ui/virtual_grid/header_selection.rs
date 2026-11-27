use super::*;

/// Row or column header press that a drag extends, kept in egui memory between frames.
#[derive(Clone, Copy, PartialEq)]
struct HeaderDrag {
    document_id: DocumentId,
    table_id: u64,
    axis: StructureAxis,
    anchor: usize,
}

impl GridViewportFrame<'_> {
    /// Turns a click, Shift+click, Ctrl/Cmd+click, drag, or right-click on one row or column
    /// header into a selection of whole rows or columns, as in Excel.
    pub(super) fn update_header_selection(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        axis: StructureAxis,
        index: usize,
        outcome: &mut GridOutcome,
    ) {
        let drag_id = Id::unique(egui::IdSalt::new((
            "texcel_header_drag",
            self.document_id,
            self.table_id,
        )));
        let active_index = self.axis_index(axis, self.active_cell);
        let modifiers = ui.input(|input| input.modifiers);
        if response.drag_started_by(egui::PointerButton::Primary) && !modifiers.command {
            let anchor = if modifiers.shift { active_index } else { index };
            ui.data_mut(|data| {
                data.insert_temp(
                    drag_id,
                    HeaderDrag {
                        document_id: self.document_id,
                        table_id: self.table_id,
                        axis,
                        anchor,
                    },
                )
            });
            self.select_whole(axis, anchor, index, modifiers.shift, outcome);
        } else if response.dragged_by(egui::PointerButton::Primary)
            && let Some(drag) = ui
                .data(|data| data.get_temp::<HeaderDrag>(drag_id))
                .filter(|drag| drag.axis == axis)
            && let Some(pointer) = ui.input(|input| input.pointer.interact_pos())
        {
            let head = self.header_index_at(axis, pointer);
            self.select_whole(
                axis,
                drag.anchor,
                head,
                drag.anchor == active_index,
                outcome,
            );
        } else if response.clicked() {
            let whole = self.whole_axis_range(axis, index, index);
            if modifiers.command {
                outcome.add_selection_range = Some(whole);
            } else if modifiers.shift {
                self.select_whole(axis, active_index, index, true, outcome);
            } else {
                outcome.selected_range = Some(whole);
            }
        } else if response.secondary_clicked() && !self.whole_axis_selected(axis, index) {
            // A right-click inside the selected rows or columns keeps them for the menu.
            outcome.selected_range = Some(self.whole_axis_range(axis, index, index));
        }
        if response.drag_stopped() {
            ui.data_mut(|data| data.remove::<HeaderDrag>(drag_id));
        }
        if response.clicked() || response.secondary_clicked() || response.drag_started() {
            response.request_focus();
        }
    }

    // Select whole rows or columns from `anchor` to `head`, keeping the active cell if asked.
    fn select_whole(
        &self,
        axis: StructureAxis,
        anchor: usize,
        head: usize,
        keep_active: bool,
        outcome: &mut GridOutcome,
    ) {
        let range = self.whole_axis_range(axis, anchor, head);
        outcome.selected_range = Some(range);
        if keep_active {
            outcome.selection_active_cell = Some(self.active_cell);
        }
    }

    // Build the range covering whole rows or columns between two header indexes.
    fn whole_axis_range(&self, axis: StructureAxis, anchor: usize, head: usize) -> CellRange {
        match axis {
            StructureAxis::Columns => CellRange {
                anchor: CellAddress {
                    row: 0,
                    column: anchor,
                },
                head: CellAddress {
                    row: self.row_count.saturating_sub(1),
                    column: head,
                },
            },
            StructureAxis::Rows => CellRange {
                anchor: CellAddress {
                    row: anchor,
                    column: 0,
                },
                head: CellAddress {
                    row: head,
                    column: self.column_count.saturating_sub(1),
                },
            },
        }
    }

    // Report whether a header lies inside a selected range that spans whole rows or columns.
    fn whole_axis_selected(&self, axis: StructureAxis, index: usize) -> bool {
        self.selected_ranges.iter().any(|(start, end)| match axis {
            StructureAxis::Columns => {
                start.row == 0
                    && end.row + 1 >= self.row_count
                    && (start.column..=end.column).contains(&index)
            }
            StructureAxis::Rows => {
                start.column == 0
                    && end.column + 1 >= self.column_count
                    && (start.row..=end.row).contains(&index)
            }
        })
    }

    // Read a cell's position along one axis.
    fn axis_index(&self, axis: StructureAxis, address: CellAddress) -> usize {
        match axis {
            StructureAxis::Columns => address.column,
            StructureAxis::Rows => address.row,
        }
    }

    // Find the row or column under the pointer, also past the headers while dragging.
    fn header_index_at(&self, axis: StructureAxis, pointer: Pos2) -> usize {
        match axis {
            StructureAxis::Columns => {
                let left = self.viewport_origin.x + self.row_header_width;
                let scroll = if self.frozen_column_count > 0 && pointer.x < self.frozen_x {
                    0.0
                } else {
                    self.effective_scroll_x
                };
                column_at_position(self.grid.layout, MAX_COLUMNS, pointer.x - left + scroll)
            }
            StructureAxis::Rows => {
                let top = self.viewport_origin.y + self.header_height;
                let scroll = if self.frozen_row_count > 0 && pointer.y < self.frozen_y {
                    0.0
                } else {
                    self.effective_scroll_y
                };
                self.row_layout
                    .row_at_position((pointer.y - top + scroll).max(0.0))
            }
        }
    }
}
