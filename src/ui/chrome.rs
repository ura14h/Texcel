use super::*;

pub(super) const CHROME_EDGE_INSET: i8 = 8;
pub(super) const CHROME_CONTROL_HEIGHT: f32 = 32.0;
pub(super) const CHROME_TAB_HEIGHT: f32 = 30.0;
pub(super) const CHROME_STATUS_HEIGHT: f32 = 26.0;
pub(super) const FORMULA_BAR_CORNER_RADIUS: u8 = 2;
const CHROME_TEXT_PADDING: f32 = 12.0;

#[derive(Clone, Copy)]
pub(super) enum CommandIcon {
    Recalculate,
    Cancel,
    AddTable,
}

/// Draws connected sheet tabs with one selected cue and thin separators.
pub(super) fn sheet_tab_button(ui: &mut egui::Ui, selected: bool, label: &str) -> egui::Response {
    let text = if selected {
        egui::RichText::new(label).strong()
    } else {
        egui::RichText::new(label)
    };
    let width = (command_width(ui, label) + 4.0).max(56.0);
    let response = ui
        .scope(|ui| {
            let palette = GridThemePalette::from_visuals(ui.visuals());
            let base = if selected {
                palette.background
            } else {
                palette.header_background
            };
            set_sheet_button_fills(ui, base);
            ui.visuals_mut().selection.bg_fill = base;
            // No vertical padding, so the bold selected label does not make its tab taller.
            ui.spacing_mut().button_padding.y = 0.0;
            ui.add_sized(
                [width, CHROME_TAB_HEIGHT],
                egui::Button::selectable(selected, text)
                    .stroke(Stroke::NONE)
                    .corner_radius(0.0),
            )
        })
        .inner;
    let weak = ui.visuals().weak_text_color();
    let separator = Color32::from_rgba_unmultiplied(weak.r(), weak.g(), weak.b(), 88);
    ui.painter().vline(
        response.rect.right() - 0.5,
        response.rect.y_range().shrink(7.0),
        Stroke::new(1.0, separator),
    );
    response
}

/// Draws one borderless icon control in the sheet-tab band.
pub(super) fn sheet_band_icon_button(
    ui: &mut egui::Ui,
    label: &str,
    icon: CommandIcon,
    enabled: bool,
) -> egui::Response {
    let response = ui.add_enabled_ui(enabled, |ui| {
        ui.add_sized(
            [CHROME_TAB_HEIGHT, CHROME_TAB_HEIGHT],
            egui::Button::new(icon_atom(icon, 16.0))
                .frame_when_inactive(false)
                .corner_radius(0.0),
        )
    });
    let response = response.inner;
    response
        .accessible_name(label.to_owned())
        .on_hover_text(label.to_owned())
}

/// Opens the fixed table-list menu from a short text control.
pub(super) fn sheet_table_list_button(
    ui: &mut egui::Ui,
    visible_label: &str,
    label: &str,
    contents: impl FnOnce(&mut egui::Ui),
) -> egui::InnerResponse<Option<()>> {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = Vec2::new(CHROME_TEXT_PADDING, 0.0);
        ui.spacing_mut().interact_size.y = CHROME_TAB_HEIGHT;
        ui.set_min_height(CHROME_TAB_HEIGHT);
        let band_fill = GridThemePalette::from_visuals(ui.visuals()).header_background;
        set_sheet_button_fills(ui, band_fill);
        let mut response = ui.menu_button(visible_label, contents);
        response.response = response
            .response
            .accessible_name(label.to_owned())
            .on_hover_text(label.to_owned());
        response
    })
    .inner
}

/// Draws the table list button as a toggle for the Markdown minimap.
pub(super) fn sheet_minimap_button(
    ui: &mut egui::Ui,
    visible_label: &str,
    label: &str,
    open: bool,
) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = Vec2::new(CHROME_TEXT_PADDING, 0.0);
        ui.spacing_mut().interact_size.y = CHROME_TAB_HEIGHT;
        let band_fill = GridThemePalette::from_visuals(ui.visuals()).header_background;
        set_sheet_button_fills(ui, band_fill);
        ui.add(
            egui::Button::selectable(open, visible_label.to_owned())
                .min_size(Vec2::new(0.0, CHROME_TAB_HEIGHT)),
        )
        .accessible_name(label.to_owned())
        .on_hover_text(label.to_owned())
    })
    .inner
}

/// Gives sheet controls calm default fills with clear hover and press states.
fn set_sheet_button_fills(ui: &mut egui::Ui, base: Color32) {
    let dark = ui.visuals().dark_mode;
    let tint = if dark { Color32::WHITE } else { Color32::BLACK };
    let hover = base.lerp_to_gamma(tint, if dark { 0.12 } else { 0.06 });
    let pressed = base.lerp_to_gamma(tint, if dark { 0.20 } else { 0.12 });
    let visuals = ui.visuals_mut();
    visuals.widgets.inactive.weak_bg_fill = base;
    visuals.widgets.hovered.weak_bg_fill = hover;
    visuals.widgets.active.weak_bg_fill = pressed;
    visuals.widgets.open.weak_bg_fill = hover;
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::ZERO;
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::ZERO;
    visuals.widgets.active.corner_radius = egui::CornerRadius::ZERO;
    visuals.widgets.open.corner_radius = egui::CornerRadius::ZERO;
    // egui grows a hovered or pressed button by a pixel; tabs keep one fixed size instead.
    visuals.widgets.hovered.expansion = 0.0;
    visuals.widgets.active.expansion = 0.0;
    visuals.widgets.open.expansion = 0.0;
}

/// Keeps the display mode visible and switchable at the end of the sheet band.
pub(super) fn mode_tab_button(
    ui: &mut egui::Ui,
    selected: bool,
    visible_label: &str,
    action_label: &str,
) -> egui::Response {
    let width = mode_tab_width(ui, visible_label);
    ui.scope(|ui| {
        // No vertical padding: the default would make the band taller than the tabs, and a
        // hovered tab would then not fill the band's height.
        ui.spacing_mut().button_padding = Vec2::new(CHROME_TEXT_PADDING, 0.0);
        ui.add(
            egui::Button::selectable(selected, visible_label.to_owned())
                .frame_when_inactive(false)
                .min_size(Vec2::new(width, CHROME_TAB_HEIGHT)),
        )
    })
    .inner
    .accessible_name(action_label.to_owned())
}

/// Draws one fixed-size status action with a translated name and hover text.
pub(super) fn status_action_button(
    ui: &mut egui::Ui,
    label: &str,
    icon: CommandIcon,
    enabled: bool,
) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = CHROME_STATUS_HEIGHT - 8.0;
        ui.spacing_mut().button_padding = Vec2::ZERO;
        ui.add_enabled(
            enabled,
            egui::Button::new(icon_atom(icon, 13.0))
                .frame_when_inactive(false)
                .min_size(Vec2::new(20.0, CHROME_STATUS_HEIGHT - 8.0)),
        )
    })
    .inner
    .accessible_name(label.to_owned())
    .on_hover_text(label.to_owned())
}

/// Measures a label and keeps the sheet-band controls readable in both languages.
pub(super) fn command_width(ui: &egui::Ui, label: &str) -> f32 {
    let font = ui
        .style()
        .text_styles
        .get(&egui::TextStyle::Button)
        .cloned()
        .unwrap_or_else(|| egui::FontId::proportional(14.0));
    ui.painter()
        .layout_no_wrap(label.to_owned(), font, ui.visuals().text_color())
        .size()
        .x
        + CHROME_TEXT_PADDING * 2.0
}

/// Reserves the text width used by the mode control.
pub(super) fn mode_tab_width(ui: &egui::Ui, label: &str) -> f32 {
    command_width(ui, label)
}

/// Creates a line icon at the requested size.
fn icon_atom(icon: CommandIcon, size: f32) -> egui::Atom<'static> {
    egui::Atom::paint(Vec2::splat(size), move |ui, args| {
        paint_command_icon(ui, args.rect, args.fallback_text_color, icon);
    })
}

/// Draws the sheet and status icons with the same stroke and active text color.
fn paint_command_icon(ui: &egui::Ui, rect: egui::Rect, color: Color32, icon: CommandIcon) {
    let painter = ui.painter();
    let stroke = Stroke::new(1.6, color);
    let rect = rect.shrink(1.0);

    match icon {
        CommandIcon::Recalculate => paint_history_icon(painter, rect, stroke, true),
        // Shows a stop square inside a ring, distinct from closing a tab.
        CommandIcon::Cancel => {
            painter.circle_stroke(
                rect.center(),
                rect.width().min(rect.height()) * 0.38,
                stroke,
            );
            painter.rect_filled(
                egui::Rect::from_center_size(rect.center(), Vec2::splat(4.0)),
                0.5,
                color,
            );
        }
        CommandIcon::AddTable => {
            let center = rect.center();
            painter.line_segment(
                [
                    egui::pos2(rect.left() + 2.0, center.y),
                    egui::pos2(rect.right() - 2.0, center.y),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(center.x, rect.top() + 2.0),
                    egui::pos2(center.x, rect.bottom() - 2.0),
                ],
                stroke,
            );
        }
    }
}

/// Draws the undo and redo arrows as mirrored line paths.
fn paint_history_icon(painter: &egui::Painter, rect: egui::Rect, stroke: Stroke, redo: bool) {
    let center_y = rect.center().y;
    if redo {
        let tip = egui::pos2(rect.right() - 2.0, center_y);
        painter.add(egui::Shape::line(
            vec![
                tip,
                egui::pos2(tip.x - 3.0, center_y - 3.0),
                tip,
                egui::pos2(tip.x - 3.0, center_y + 3.0),
            ],
            stroke,
        ));
        painter.add(egui::Shape::line(
            vec![
                tip,
                egui::pos2(rect.left() + 3.0, center_y),
                egui::pos2(rect.left() + 1.0, center_y + 2.0),
                egui::pos2(rect.left() + 1.0, rect.bottom() - 2.0),
            ],
            stroke,
        ));
    } else {
        let tip = egui::pos2(rect.left() + 2.0, center_y);
        painter.add(egui::Shape::line(
            vec![
                tip,
                egui::pos2(tip.x + 3.0, center_y - 3.0),
                tip,
                egui::pos2(tip.x + 3.0, center_y + 3.0),
            ],
            stroke,
        ));
        painter.add(egui::Shape::line(
            vec![
                tip,
                egui::pos2(rect.right() - 3.0, center_y),
                egui::pos2(rect.right() - 1.0, center_y + 2.0),
                egui::pos2(rect.right() - 1.0, rect.bottom() - 2.0),
            ],
            stroke,
        ));
    }
}
