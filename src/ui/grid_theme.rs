use eframe::egui::{self, Color32};

/// Keeps grid colors consistent with the currently applied egui theme.
pub(super) struct GridThemePalette {
    pub(super) background: Color32,
    pub(super) header_background: Color32,
    pub(super) active_header_fill: Color32,
    pub(super) foreground: Color32,
    pub(super) grid: Color32,
    /// The line at the edge of frozen rows and columns: black, as in Excel, or near white
    /// on a dark theme.
    pub(super) frozen_boundary: Color32,
    pub(super) selection_fill: Color32,
    pub(super) selection: Color32,
    pub(super) error: Color32,
}

impl GridThemePalette {
    /// Uses calm grid lines while preserving the theme's strong selection accent.
    pub(super) fn from_visuals(visuals: &egui::Visuals) -> Self {
        let (background, header_background, grid) = if visuals.dark_mode {
            (
                Color32::from_rgb(27, 29, 32),
                Color32::from_rgb(36, 39, 43),
                Color32::from_rgb(55, 59, 65),
            )
        } else {
            (
                Color32::WHITE,
                Color32::from_rgb(245, 247, 250),
                Color32::from_rgb(225, 229, 235),
            )
        };
        Self {
            background,
            header_background,
            active_header_fill: visuals.selection.bg_fill.gamma_multiply(0.48),
            foreground: if visuals.dark_mode {
                Color32::from_rgb(226, 228, 232)
            } else {
                visuals.text_color()
            },
            grid,
            frozen_boundary: if visuals.dark_mode {
                Color32::from_rgb(226, 228, 232)
            } else {
                Color32::BLACK
            },
            selection_fill: visuals.selection.bg_fill.gamma_multiply(0.22),
            selection: visuals.selection.stroke.color,
            error: visuals.error_fg_color,
        }
    }
}

/// Reserves a slim, low-contrast scrollbar area so cells remain clickable.
pub(super) fn prepare_grid_scrollbars(ui: &mut egui::Ui) {
    let palette = GridThemePalette::from_visuals(ui.visuals());
    let spacing = ui.spacing_mut();
    spacing.scroll.floating = false;
    spacing.scroll.bar_width = 6.0;
    spacing.scroll.bar_inner_margin = 1.0;
    spacing.scroll.foreground_color = false;

    let visuals = ui.visuals_mut();
    visuals.extreme_bg_color = palette.background;
    visuals.widgets.inactive.bg_fill = palette.grid;
    visuals.widgets.hovered.bg_fill = palette.active_header_fill;
    visuals.widgets.active.bg_fill = palette.selection;
}
