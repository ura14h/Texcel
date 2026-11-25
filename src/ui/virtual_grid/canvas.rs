use super::*;

/// Where the grid is scrolled to and where it has been asked to scroll next.
pub(super) struct CanvasScroll {
    pub(super) offset: Vec2,
    /// Extra distance a scroll request has asked for beyond the current offset.
    pub(super) pending_ahead: Vec2,
    /// A cell that must become reachable, such as the selection after a jump.
    pub(super) target: Option<CellAddress>,
}

// Grows the scroll canvas from saved content and the current viewport without storing blank cells.
pub(super) fn canvas_size(
    extent_end: CellAddress,
    row_layout: &GridRowLayout,
    layout: ScaledLayout<'_>,
    viewport: Vec2,
    row_header_width: f32,
    scroll: CanvasScroll,
) -> Vec2 {
    let header_height = grid_header_height(layout.scale());
    let CanvasScroll {
        offset: scroll,
        pending_ahead: pending_scroll_ahead,
        target: scroll_target,
    } = scroll;
    let used_size = Vec2::new(
        row_header_width + layout.prefix_column_width_pixels(extent_end.column + 1) as f32,
        header_height + row_layout.prefix_height(extent_end.row + 1),
    );
    let sheet_size = Vec2::new(
        row_header_width + layout.prefix_column_width_pixels(MAX_COLUMNS) as f32,
        header_height + row_layout.prefix_height(MAX_ROWS),
    );
    let target_size = scroll_target.map_or(Vec2::ZERO, |address| {
        Vec2::new(
            row_header_width + layout.prefix_column_width_pixels(address.column + 1) as f32,
            header_height + row_layout.prefix_height(address.row + 1),
        )
    });
    let dynamic_size = Vec2::new(
        used_size
            .x
            .max(scroll.x + viewport.x + pending_scroll_ahead.x)
            + viewport.x,
        used_size
            .y
            .max(scroll.y + viewport.y + pending_scroll_ahead.y)
            + viewport.y,
    );
    let requested_size = target_size + viewport;
    Vec2::new(
        dynamic_size.x.max(requested_size.x).min(sheet_size.x),
        dynamic_size.y.max(requested_size.y).min(sheet_size.y),
    )
}
