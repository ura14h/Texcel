use super::*;

/// Paints one cell's fill and boundary behind its text.
pub(super) fn paint_cell_base(
    painter: &egui::Painter,
    rect: Rect,
    row: usize,
    selected: bool,
    active: bool,
    palette: &GridThemePalette,
) -> Color32 {
    let fill = if row == 0 {
        palette.header_background
    } else {
        palette.background
    };
    painter.rect_filled(rect, 0.0, fill);
    if selected && !active {
        painter.rect_filled(rect, 0.0, palette.selection_fill);
    }
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(0.7, palette.grid),
        egui::StrokeKind::Inside,
    );
    fill
}

/// Paints the selection outline and its small fill handle over one cell segment.
/// The outline edges may belong to a fill-handle preview that extends past the selection.
pub(super) fn paint_selection_overlay(
    painter: &egui::Painter,
    rect: Rect,
    selected_edges: [bool; 4],
    fill_handle: Option<Rect>,
    palette: &GridThemePalette,
) {
    if selected_edges.contains(&true) {
        let rect = rect.shrink(0.75);
        let stroke = Stroke::new(2.0, palette.selection);
        let [top, bottom, left, right] = selected_edges;
        if top {
            painter.line_segment([rect.left_top(), rect.right_top()], stroke);
        }
        if bottom {
            painter.line_segment([rect.left_bottom(), rect.right_bottom()], stroke);
        }
        if left {
            painter.line_segment([rect.left_top(), rect.left_bottom()], stroke);
        }
        if right {
            painter.line_segment([rect.right_top(), rect.right_bottom()], stroke);
        }
    }
    if let Some(handle) = fill_handle {
        painter.rect_filled(handle, 1.0, palette.selection);
        painter.rect_stroke(
            handle,
            1.0,
            Stroke::new(1.0, palette.header_background),
            egui::StrokeKind::Inside,
        );
    }
}
