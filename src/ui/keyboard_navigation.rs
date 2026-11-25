use super::*;

impl TexcelApp {
    /// Moves one visible viewport page along the requested grid axis.
    pub(super) fn move_page(&mut self, vertical: bool, direction: isize, extend: bool) {
        let target = self.workspace.active_document().and_then(|document| {
            let table = document.active_table()?;
            let view = document.view(table.id)?;
            let layout = ScaledLayout::new(view.layout.as_ref(), self.grid_scale());
            let active = view.active_cell();
            let current = if extend && view.selection.anchor == active {
                view.selection.head
            } else {
                active
            };
            let extent = if vertical {
                self.grid_page_extent.y
            } else {
                self.grid_page_extent.x
            }
            .max(1.0);
            let mut target = current;
            if vertical {
                let rows = self.grid_row_layouts.get(&(document.id, table.id))?;
                let position =
                    (rows.prefix_height(current.row) + extent * direction as f32).max(0.0);
                target.row = rows.row_at_position(position);
            } else {
                let prefix = layout.prefix_column_width_pixels(current.column) as f32;
                let position = (prefix + extent * direction as f32).max(0.0);
                target.column = column_at_position(layout, MAX_COLUMNS, position);
            }
            crate::document::TableViewState::normalized_active_address(
                CellRange::single(target),
                target,
                MAX_ROWS,
                MAX_COLUMNS,
            )
            .filter(|next| *next != current)
        });
        if let Some(target) = target {
            self.select_boundary(target, extend);
        }
    }

    /// Moves to the first or last visible column on the active row.
    pub(super) fn move_row_boundary(&mut self, last: bool, extend: bool) {
        let target = self.workspace.active_document().and_then(|document| {
            let table = document.active_table()?;
            let view = document.view(table.id)?;
            let active = view.active_cell();
            let current = if extend && view.selection.anchor == active {
                view.selection.head
            } else {
                active
            };
            let column = if last { MAX_COLUMNS - 1 } else { 0 };
            Some(CellAddress {
                row: current.row,
                column,
            })
        });
        if let Some(target) = target {
            self.select_boundary(target, extend);
        }
    }

    /// Selects a whole row or column while keeping the input cell inside it.
    pub(super) fn select_active_axis(&mut self, rows: bool) {
        let Some((document_id, table_id, selection, active)) =
            self.workspace.active_document().and_then(|document| {
                let table = document.active_table()?;
                let view = document.view(table.id)?;
                let active = view.active_cell();
                let range = if rows {
                    CellRange {
                        anchor: CellAddress {
                            row: active.row,
                            column: 0,
                        },
                        head: CellAddress {
                            row: active.row,
                            column: MAX_COLUMNS - 1,
                        },
                    }
                } else {
                    CellRange {
                        anchor: CellAddress {
                            row: 0,
                            column: active.column,
                        },
                        head: CellAddress {
                            row: MAX_ROWS - 1,
                            column: active.column,
                        },
                    }
                };
                Some((document.id, table.id, range, active))
            })
        else {
            return;
        };
        if let Some(view) = self
            .workspace
            .documents
            .get_mut(&document_id)
            .and_then(|document| document.view_mut(table_id))
        {
            view.set_selection_with_active(selection, active);
        }
        self.request_grid_focus = true;
        self.request_scroll_to_selection = true;
    }

    /// Cycles the active cell in the primary selected rectangle without shrinking it.
    pub(super) fn cycle_selection_cell(&mut self, reverse: bool) -> bool {
        let target = self.workspace.active_document().and_then(|document| {
            let table = document.active_table()?;
            let view = document.view(table.id)?;
            let (start, end) = view.selection.normalized();
            if start == end {
                return None;
            }
            let active = view.active_cell();
            let direction = if reverse { -1 } else { 1 };
            let next_row = step_index(active.row, direction, MAX_ROWS);
            let row = if next_row != active.row && next_row >= start.row && next_row <= end.row {
                next_row
            } else {
                let next_column = step_index(active.column, direction, MAX_COLUMNS);
                if next_column != active.column
                    && next_column >= start.column
                    && next_column <= end.column
                {
                    let row = if reverse { end.row } else { start.row };
                    return Some((
                        table.id,
                        CellAddress {
                            row,
                            column: next_column,
                        },
                    ));
                }
                let row = if reverse { end.row } else { start.row };
                let column = if reverse { end.column } else { start.column };
                return Some((table.id, CellAddress { row, column }));
            };
            Some((
                table.id,
                CellAddress {
                    row,
                    column: active.column,
                },
            ))
        });
        let Some((table_id, address)) = target else {
            return false;
        };
        if let Some(document) = self.workspace.active_document_mut()
            && let Some(view) = document.view_mut(table_id)
        {
            view.active_cell = Some(address);
        }
        self.request_grid_focus = true;
        self.request_scroll_to_selection = true;
        true
    }
}
