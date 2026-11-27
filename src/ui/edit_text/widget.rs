use super::*;

/// Draws the requested editor and keeps its shared text behavior.
pub(super) fn show_cell_edit_widget(
    ui: &mut egui::Ui,
    text: &mut String,
    id: Id,
    widget: CellEditWidget,
) -> egui::text_edit::TextEditOutput {
    match widget {
        CellEditWidget::Singleline { width, frame_none } => {
            let mut layouter = line_break_marker_layouter(None);
            let has_break = has_line_break(text);
            let mut editor = egui::TextEdit::singleline(text)
                .id(id)
                .return_key(None)
                .desired_width(width);
            if has_break {
                editor = editor.layouter(&mut layouter);
            }
            if frame_none {
                editor.frame(egui::Frame::NONE).show(ui)
            } else {
                editor.show(ui)
            }
        }
        CellEditWidget::FormulaBarSingleline { width, height } => {
            ui.scope(|ui| {
                let radius = egui::CornerRadius::same(FORMULA_BAR_CORNER_RADIUS);
                let visuals = ui.visuals_mut();
                visuals.widgets.noninteractive.corner_radius = radius;
                visuals.widgets.inactive.corner_radius = radius;
                visuals.widgets.hovered.corner_radius = radius;
                visuals.widgets.active.corner_radius = radius;
                visuals.widgets.open.corner_radius = radius;
                let mut layouter = line_break_marker_layouter(Some(FontId::monospace(14.0)));
                let has_break = has_line_break(text);
                let mut editor = egui::TextEdit::singleline(text)
                    .id(id)
                    .return_key(None)
                    .desired_width(width)
                    .font(FontId::monospace(14.0))
                    .margin(egui::Margin::symmetric(8, 0))
                    .align(Align2::LEFT_CENTER)
                    .min_size(Vec2::new(width, height));
                if has_break {
                    editor = editor.layouter(&mut layouter);
                }
                editor.show(ui)
            })
            .inner
        }
        CellEditWidget::Multiline {
            width,
            rows,
            lock_focus,
        } => egui::TextEdit::multiline(text)
            .id(id)
            .desired_width(width)
            .desired_rows(rows)
            .lock_focus(lock_focus)
            .show(ui),
    }
}

// Report whether a one-line editor holds a raw line-ending character.
fn has_line_break(text: &str) -> bool {
    text.contains(['\n', '\r'])
}

/// Replaces line-ending characters with `↲` one for one, so caret positions stay the same.
/// A one-line egui editor would otherwise draw a missing-glyph box for them.
pub(super) fn line_break_markers(text: &str) -> String {
    text.chars()
        .map(|character| match character {
            '\n' | '\r' => '↲',
            other => other,
        })
        .collect()
}

// Lay out one-line editor text like egui does, but with visible line-break markers.
fn line_break_marker_layouter(
    font: Option<FontId>,
) -> impl FnMut(&egui::Ui, &dyn egui::TextBuffer, f32) -> std::sync::Arc<egui::Galley> {
    move |ui, text, _wrap_width| {
        let font_id = font
            .clone()
            .unwrap_or_else(|| egui::TextStyle::Body.resolve(ui.style()));
        let color = ui
            .visuals()
            .override_text_color
            .unwrap_or_else(|| ui.visuals().widgets.inactive.text_color());
        let line_height =
            ui.fonts_mut(|fonts| fonts.row_height(&font_id)) + ui.spacing().extra_text_line_spacing;
        let mut job = egui::text::LayoutJob::simple_singleline(
            line_break_markers(text.as_str()),
            font_id,
            color,
        );
        job.keep_trailing_whitespace = true;
        for section in &mut job.sections {
            section.format.line_height = Some(line_height);
        }
        ui.fonts_mut(|fonts| fonts.layout_job(job))
    }
}

#[cfg(test)]
#[path = "widget_test.rs"]
mod tests;
