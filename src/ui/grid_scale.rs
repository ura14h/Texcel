//! Sheet zoom for the grid: sizes are kept at 100% and multiplied only where the grid is drawn.
use super::*;

/// The smallest sheet zoom the user can choose.
pub(super) const MIN_SHEET_ZOOM: f32 = 0.10;
/// The largest sheet zoom the user can choose.
pub(super) const MAX_SHEET_ZOOM: f32 = 4.0;

/// A table layout seen at the sheet zoom.
///
/// Its pixel methods return zoomed sizes and shadow the layout's own; every other
/// method reaches the stored layout through `Deref`, so stored widths stay at 100%.
#[derive(Clone, Copy)]
pub(super) struct ScaledLayout<'a> {
    layout: &'a TableLayout,
    scale: f32,
}

impl<'a> ScaledLayout<'a> {
    /// Views a stored layout at one grid scale.
    pub(super) fn new(layout: &'a TableLayout, scale: f32) -> Self {
        Self { layout, scale }
    }

    /// Returns the factor applied to every grid size.
    pub(super) fn scale(&self) -> f32 {
        self.scale
    }

    /// Returns one column's zoomed width in points.
    pub(super) fn column_width_pixels(&self, index: usize) -> f64 {
        self.layout.column_width_pixels(index) * f64::from(self.scale)
    }

    /// Returns the zoomed width of the columns before an index.
    pub(super) fn prefix_column_width_pixels(&self, columns: usize) -> f64 {
        self.layout.prefix_column_width_pixels(columns) * f64::from(self.scale)
    }
}

impl std::ops::Deref for ScaledLayout<'_> {
    type Target = TableLayout;

    fn deref(&self) -> &TableLayout {
        self.layout
    }
}

/// One window's sheet zoom, 1.0 for 100%; a new window starts at 100%.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SheetZoom(pub(super) f32);

impl Default for SheetZoom {
    fn default() -> Self {
        Self(1.0)
    }
}

/// Keeps a requested sheet zoom inside the supported range, in whole percent.
pub(super) fn clamp_sheet_zoom(zoom: f32) -> f32 {
    if !zoom.is_finite() {
        return 1.0;
    }
    ((zoom * 100.0).round() / 100.0).clamp(MIN_SHEET_ZOOM, MAX_SHEET_ZOOM)
}

/// Steps a sheet zoom to the next or previous multiple of 10%.
pub(super) fn step_sheet_zoom(zoom: f32, zoom_in: bool) -> f32 {
    let tens = (zoom * 10.0).round_ties_even();
    let tens = if (zoom * 10.0 - tens).abs() < 0.001 {
        if zoom_in { tens + 1.0 } else { tens - 1.0 }
    } else if zoom_in {
        (zoom * 10.0).ceil()
    } else {
        (zoom * 10.0).floor()
    };
    clamp_sheet_zoom(tens / 10.0)
}

/// Height of the column-letter band at a grid scale.
pub(super) fn grid_header_height(scale: f32) -> f32 {
    HEADER_HEIGHT * scale
}

/// Size of header and grid text at a grid scale.
pub(super) fn grid_font_size(scale: f32) -> f32 {
    visual::BODY_SIZE * scale
}

#[cfg(test)]
#[path = "grid_scale_test.rs"]
mod tests;
