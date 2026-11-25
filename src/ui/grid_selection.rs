use super::commands::AppCommand;
use super::*;

#[derive(Default)]
pub(super) struct GridOutcome {
    pub(super) table_id: u64,
    pub(super) clicked: Option<CellAddress>,
    pub(super) double_clicked: bool,
    pub(super) open_link: bool,
    pub(super) text_selection: Option<(CellAddress, std::ops::Range<usize>)>,
    pub(super) selected_range: Option<CellRange>,
    pub(super) formula_reference_shift_range: Option<CellRange>,
    pub(super) selection_active_cell: Option<CellAddress>,
    pub(super) preserve_additional_selections: bool,
    pub(super) add_selection_range: Option<CellRange>,
    pub(super) structure_action: Option<StructureAction>,
    pub(super) commands: Vec<AppCommand>,
    pub(super) fill_handle_started: Option<CellRange>,
    pub(super) fill_handle_finished: Option<CellAddress>,
    pub(super) fill_handle_cancelled: bool,
    /// Range the active fill-handle drag would fill if released now.
    pub(super) fill_handle_preview: Option<CellRange>,
    pub(super) focused: bool,
    pub(super) scroll: Option<crate::document::ScrollPosition>,
    pub(super) page_extent: Option<Vec2>,
    pub(super) selection_drag_started: Option<GridSelectionDrag>,
    pub(super) selection_drag_update: Option<CellAddress>,
    pub(super) selection_drag_finished: bool,
    pub(super) resized_layout: Option<(DocumentId, u64, LayoutTarget, LayoutValue)>,
    pub(super) resize_finished: bool,
    pub(super) auto_fit_requested: Option<(DocumentId, u64, usize)>,
    /// Screen rectangle of the active cell, used to place the IME candidate window.
    pub(super) active_cell_rect: Option<Rect>,
}

impl GridOutcome {
    // Records a Shift-click range for either the sheet or formula-reference picker.
    pub(super) fn record_shift_range(
        &mut self,
        range: CellRange,
        active_cell: CellAddress,
        formula_reference_mode: bool,
    ) {
        self.selected_range = Some(range);
        if formula_reference_mode {
            self.formula_reference_shift_range = Some(range);
        } else {
            self.selection_active_cell = Some(active_cell);
            self.preserve_additional_selections = true;
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct FillHandleDrag {
    pub(super) document_id: DocumentId,
    pub(super) table_id: u64,
    pub(super) source: CellRange,
    /// Last range the drag would fill, shown in the status line's cell count.
    pub(super) preview: Option<CellRange>,
}

#[derive(Clone, Copy)]
pub(super) struct GridSelectionDrag {
    pub(super) document_id: DocumentId,
    pub(super) table_id: u64,
    pub(super) anchor: CellAddress,
    pub(super) pointer_start: CellAddress,
    pub(super) extend: bool,
    pub(super) formula_reference_session: Option<u64>,
}

#[derive(Clone, Copy)]
pub(super) struct HeaderResizeDrag {
    pub(super) document_id: DocumentId,
    pub(super) table_id: u64,
    pub(super) column: usize,
    pub(super) pointer_start: f32,
    pub(super) width_start: f32,
}

// Build row or column menu actions for the selected cell.
pub(super) fn structure_axis_context_menu(
    ui: &mut egui::Ui,
    axis: StructureAxis,
    index: usize,
    size: usize,
    enabled: bool,
    reason: Option<&str>,
    action: &mut Option<StructureAction>,
) {
    visual::prepare_context_menu(ui);
    let (insert_label, insert, delete, duplicate, up, down) = match axis {
        StructureAxis::Rows => (
            "structure-insert-above",
            StructureAction::InsertRows,
            StructureAction::DeleteRows,
            StructureAction::DuplicateRows,
            StructureAction::MoveRowsUp,
            StructureAction::MoveRowsDown,
        ),
        StructureAxis::Columns => (
            "structure-insert-left",
            StructureAction::InsertColumns,
            StructureAction::DeleteColumns,
            StructureAction::DuplicateColumns,
            StructureAction::MoveColumnsLeft,
            StructureAction::MoveColumnsRight,
        ),
    };
    if let Some(reason) = reason {
        ui.label(reason);
    }
    ui.add_enabled_ui(enabled, |ui| {
        if ui.button(context_text(ui.ctx(), insert_label)).clicked() {
            *action = Some(insert);
            ui.close();
        }
        if ui.button(context_text(ui.ctx(), "action-delete")).clicked() {
            *action = Some(delete);
            ui.close();
        }
        if ui
            .button(context_text(ui.ctx(), "action-duplicate"))
            .clicked()
        {
            *action = Some(duplicate);
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(
                index > 0,
                egui::Button::new(context_text(ui.ctx(), "structure-move-previous")),
            )
            .clicked()
        {
            *action = Some(up);
            ui.close();
        }
        if ui
            .add_enabled(
                index + 1 < size,
                egui::Button::new(context_text(ui.ctx(), "structure-move-next")),
            )
            .clicked()
        {
            *action = Some(down);
            ui.close();
        }
    });
}

#[derive(Clone)]
pub(super) struct GridPaneZone {
    pub(super) rows: std::ops::Range<usize>,
    pub(super) columns: std::ops::Range<usize>,
    pub(super) clip_rect: Rect,
    pub(super) frozen_rows: bool,
    pub(super) frozen_columns: bool,
}

/// Moves an index by `delta`, staying within `0..count`.
pub(super) fn step_index(index: usize, delta: isize, count: usize) -> usize {
    let last = count.saturating_sub(1);
    index.saturating_add_signed(delta).min(last)
}

// Choose the next cell address for grid navigation.
pub(super) fn grid_navigation_target(
    current: CellAddress,
    row_delta: i32,
    column_delta: i32,
    row_count: usize,
    column_count: usize,
) -> CellAddress {
    CellAddress {
        row: if row_delta == 0 {
            current.row
        } else {
            step_index(current.row, row_delta as isize, row_count)
        },
        column: if column_delta == 0 {
            current.column
        } else {
            step_index(current.column, column_delta as isize, column_count)
        },
    }
}
