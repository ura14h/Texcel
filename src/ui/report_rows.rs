use super::{DocumentId, visual};
use crate::conversion_report::ConversionGroup;
use crate::i18n::{Language, Message, context_text};
use eframe::egui;

/// Shows a short row that fits the report's fixed-height list.
pub(super) fn group_summary(group: &ConversionGroup, language: Language, import: bool) -> String {
    format!(
        "[{}] {} ×{}",
        group.class.label(language),
        group_location(group, language, import),
        group.count
    )
}

/// Keeps the full location visible when one report group is selected.
pub(super) fn group_location(group: &ConversionGroup, language: Language, import: bool) -> String {
    match (&group.sheet_order, &group.sheet_name) {
        (Some(order), Some(name)) => Message::new(if import {
            "report-location-sheet-name"
        } else {
            "report-location-table-name"
        })
        .with_number("order", *order as i64)
        .with_text("name", name)
        .resolve(language),
        (Some(order), None) => Message::new(if import {
            "report-location-sheet-order"
        } else {
            "report-location-table-order"
        })
        .with_number("order", *order as i64)
        .resolve(language),
        (_, Some(name)) => Message::new("report-location-name")
            .with_text("name", name)
            .resolve(language),
        _ => Message::new(if import {
            "report-location-workbook"
        } else {
            "report-location-document"
        })
        .resolve(language),
    }
}

/// Resolves a group's full reason only in its selected detail section.
pub(super) fn group_reason(group: &ConversionGroup, language: Language) -> String {
    group
        .message
        .as_ref()
        .map_or_else(|| group.reason.clone(), |message| message.resolve(language))
}

/// The text inset at the left of each report list row and its column headings.
pub(super) const REPORT_ROW_INSET: f32 = 8.0;

/// Lays out the import report as result and location columns beside the selected details.
pub(super) fn import_report_panes(
    ui: &mut egui::Ui,
    document_id: DocumentId,
    groups: &[ConversionGroup],
    selected: &mut usize,
    height: f32,
    language: Language,
) {
    let gap = visual::SPACE_16;
    let list_width = ((ui.available_width() - gap) * 0.5).max(200.0);
    let result_heading = context_text(ui.ctx(), "report-column-result");
    // The result column is as wide as its widest label, so locations line up after it.
    let result_width = groups
        .iter()
        .map(|group| group.class.label(language))
        .chain(std::iter::once(result_heading.clone()))
        .map(|label| text_width(ui, &label))
        .fold(0.0_f32, f32::max)
        + gap;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        ui.allocate_ui_with_layout(
            egui::vec2(list_width, height),
            egui::Layout::top_down_justified(egui::Align::Min),
            |ui| {
                ui.set_min_size(egui::vec2(list_width, height));
                let location_heading = context_text(ui.ctx(), "report-column-location");
                column_headings(
                    ui,
                    &[
                        (REPORT_ROW_INSET, result_heading.as_str()),
                        (REPORT_ROW_INSET + result_width, location_heading.as_str()),
                    ],
                );
                egui::ScrollArea::vertical()
                    .id_salt(("import_report_groups", document_id))
                    .max_height(ui.available_height())
                    .auto_shrink([false, false])
                    .show_rows(ui, visual::BUTTON_HEIGHT, groups.len(), |ui, rows| {
                        for index in rows {
                            if report_group_row(
                                ui,
                                &groups[index],
                                *selected == index,
                                result_width,
                                language,
                            )
                            .clicked()
                            {
                                *selected = index;
                            }
                        }
                    });
            },
        );
        let detail_width = ui.available_width();
        ui.allocate_ui_with_layout(
            egui::vec2(detail_width, height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_min_size(egui::vec2(detail_width, height));
                ui.set_max_width(detail_width);
                let detail_heading = context_text(ui.ctx(), "report-column-detail");
                column_headings(ui, &[(0.0, detail_heading.as_str())]);
                let Some(group) = groups.get(*selected) else {
                    ui.weak(context_text(ui.ctx(), "report-no-items"));
                    return;
                };
                // Every detail line starts at the pane's left edge, also when it wraps.
                let left =
                    |text: egui::WidgetText| egui::Label::new(text).halign(egui::Align::Min).wrap();
                ui.add(left(
                    egui::RichText::new(group_location(group, language, true))
                        .strong()
                        .into(),
                ));
                ui.add(left(group_reason(group, language).into()));
                if group.ranges.is_empty() {
                    return;
                }
                ui.add_space(visual::SPACE_8);
                ui.add(left(
                    Message::new("report-range-count")
                        .with_number("count", group.ranges.len() as i64)
                        .resolve(language)
                        .into(),
                ));
                // Ranges scroll inside the remaining height so the pane never grows.
                egui::ScrollArea::vertical()
                    .id_salt(("import_report_ranges", document_id, *selected))
                    .max_height(ui.available_height().max(20.0))
                    .auto_shrink([false, true])
                    .show_rows(ui, 20.0, group.ranges.len(), |ui, rows| {
                        for index in rows {
                            ui.add(left(group.ranges[index].as_str().into()));
                        }
                    });
            },
        );
    });
}

/// Draws one selectable list row with the result and its location in separate columns.
fn report_group_row(
    ui: &mut egui::Ui,
    group: &ConversionGroup,
    selected: bool,
    result_width: f32,
    language: Language,
) -> egui::Response {
    let size = egui::vec2(ui.available_width(), visual::BUTTON_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    // Screen readers hear the whole row as before: result, location, and count.
    let summary = group_summary(group, language, true);
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, &summary));
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let visuals = ui.style().interact_selectable(&response, selected);
    if selected || response.hovered() {
        ui.painter()
            .rect_filled(rect, visuals.corner_radius, visuals.weak_bg_fill);
    }
    let font = egui::FontId::proportional(visual::BODY_SIZE);
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    let result_left = rect.left() + REPORT_ROW_INSET;
    painter.text(
        egui::pos2(result_left, rect.center().y),
        egui::Align2::LEFT_CENTER,
        group.class.label(language),
        font.clone(),
        visuals.text_color(),
    );
    painter.text(
        egui::pos2(result_left + result_width, rect.center().y),
        egui::Align2::LEFT_CENTER,
        format!("{} ×{}", group_location(group, language, true), group.count),
        font,
        visuals.text_color(),
    );
    response
}

/// Measures one line of body text for column widths.
pub(super) fn text_width(ui: &egui::Ui, text: &str) -> f32 {
    ui.painter()
        .layout_no_wrap(
            text.to_owned(),
            egui::FontId::proportional(visual::SECTION_TITLE_SIZE),
            ui.visuals().text_color(),
        )
        .size()
        .x
}

/// Places column headings on one row of shared height, each starting at its column's offset.
pub(super) fn column_headings(ui: &mut egui::Ui, headings: &[(f32, &str)]) {
    let height = ui.fonts_mut(|fonts| {
        fonts.row_height(&egui::FontId::proportional(visual::SECTION_TITLE_SIZE))
    }) + visual::SPACE_8;
    let (row, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::hover(),
    );
    for (offset, heading) in headings {
        let rect = egui::Rect::from_min_max(row.min + egui::vec2(*offset, 0.0), row.max);
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
            |ui| visual::section_title(ui, heading),
        );
    }
}
