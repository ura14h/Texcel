//! Draws GitHub-flavored Markdown with egui: the Help window and the document minimap.

use super::*;

#[path = "markdown_view/parse.rs"]
mod parse;
pub(crate) use parse::{Block, Inline, parse_markdown};

#[cfg(test)]
#[path = "markdown_view_test.rs"]
mod tests;

/// Text size and spacing for one drawing of Markdown.
#[derive(Clone, Copy)]
pub(super) struct MarkdownViewStyle {
    /// Body text size; headings, gaps, and indents scale with it.
    pub(super) body_size: f32,
}

/// Where the drawn tables landed, in document order.
#[derive(Default)]
pub(super) struct MarkdownViewOutput {
    pub(super) table_rects: Vec<Rect>,
}

/// Draws parsed Markdown blocks top to bottom at the available width.
pub(super) fn show_markdown(
    ui: &mut egui::Ui,
    blocks: &[Block],
    style: MarkdownViewStyle,
) -> MarkdownViewOutput {
    let mut output = MarkdownViewOutput::default();
    ui.spacing_mut().item_spacing.y = style.body_size * 0.5;
    // Table rows and list markers follow the text size, not the size of buttons.
    ui.spacing_mut().interact_size.y = style.body_size * 1.3;
    for (index, block) in blocks.iter().enumerate() {
        show_block(ui, block, style, &mut output, index);
    }
    output
}

// Draw one block and record table positions.
fn show_block(
    ui: &mut egui::Ui,
    block: &Block,
    style: MarkdownViewStyle,
    output: &mut MarkdownViewOutput,
    index: usize,
) {
    let size = style.body_size;
    match block {
        Block::Heading(level, inline) => {
            let scale =
                [1.7, 1.4, 1.2, 1.05, 1.0, 0.95][usize::from(level.saturating_sub(1)).min(5)];
            ui.add_space(size * 0.4);
            let mut job = inline_job(ui, inline, size * scale, true);
            job.wrap.max_width = ui.available_width();
            ui.add(egui::Label::new(job).wrap());
            if *level <= 2 {
                ui.separator();
            }
        }
        Block::Paragraph(inline) => {
            let mut job = inline_job(ui, inline, size, false);
            job.wrap.max_width = ui.available_width();
            ui.add(egui::Label::new(job).wrap());
        }
        Block::List { start, items } => {
            for (number, item) in items.iter().enumerate() {
                let marker = match start {
                    Some(first) => format!("{}.", first + number as u64),
                    None => "•".to_owned(),
                };
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = size * 0.4;
                    let marker_width = size * 1.4;
                    ui.add_sized(
                        [marker_width, size * 1.3],
                        egui::Label::new(
                            egui::RichText::new(marker)
                                .size(size)
                                .family(grid_font_family(false)),
                        ),
                    );
                    ui.vertical(|ui| {
                        for (inner, block) in item.iter().enumerate() {
                            show_block(ui, block, style, output, index * 1000 + inner);
                        }
                    });
                });
            }
        }
        Block::Code(text) => {
            egui::Frame::new()
                .fill(ui.visuals().faint_bg_color)
                .inner_margin(egui::Margin::same((size * 0.5) as i8))
                .corner_radius(4)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        egui::RichText::new(text)
                            .size(size * 0.95)
                            .family(grid_mono_font_family(false)),
                    );
                });
        }
        Block::Quote(blocks) => {
            egui::Frame::new()
                .stroke(Stroke::NONE)
                .inner_margin(egui::Margin {
                    left: (size * 0.8) as i8,
                    ..egui::Margin::ZERO
                })
                .show(ui, |ui| {
                    let top = ui.cursor().top();
                    for (inner, block) in blocks.iter().enumerate() {
                        show_block(ui, block, style, output, index * 1000 + inner);
                    }
                    let left = ui.min_rect().left() - size * 0.6;
                    ui.painter().vline(
                        left,
                        top..=ui.min_rect().bottom(),
                        Stroke::new(2.0, ui.visuals().weak_text_color()),
                    );
                });
        }
        Block::Table { alignments, rows } => {
            // Columns keep their natural width when the table fits; otherwise the widest
            // columns wrap so the table stays in view.
            let columns = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
            let mut natural = vec![0.0_f32; columns];
            for (row_index, row) in rows.iter().enumerate() {
                for (column, cell) in row.iter().enumerate() {
                    let mut job = inline_job(ui, cell, size, row_index == 0);
                    job.wrap.max_width = f32::INFINITY;
                    let width = ui.painter().layout_job(job).size().x;
                    natural[column] = natural[column].max(width);
                }
            }
            let room = ui.available_width() - size * (columns as f32 - 1.0) - size * 0.6 - 1.0;
            let widths = fit_column_widths(&natural, room, size);
            let response = egui::Frame::new()
                .stroke(Stroke::new(
                    0.5,
                    ui.visuals().widgets.noninteractive.bg_stroke.color,
                ))
                .inner_margin(egui::Margin::same((size * 0.3) as i8))
                .show(ui, |ui| {
                    egui::Grid::new(("texcel_markdown_table", index))
                        .striped(true)
                        .min_col_width(0.0)
                        .spacing(Vec2::new(size, size * 0.3))
                        .show(ui, |ui| {
                            for (row_index, row) in rows.iter().enumerate() {
                                for (column, cell) in row.iter().enumerate() {
                                    let mut job = inline_job(ui, cell, size, row_index == 0);
                                    job.wrap.max_width = widths[column];
                                    job.halign = match alignments.get(column) {
                                        Some(pulldown_cmark::Alignment::Center) => {
                                            egui::Align::Center
                                        }
                                        Some(pulldown_cmark::Alignment::Right) => {
                                            egui::Align::RIGHT
                                        }
                                        _ => egui::Align::LEFT,
                                    };
                                    // A laid-out galley keeps its width; a label would
                                    // rewrap at the grid's wider available width.
                                    let galley = ui.painter().layout_job(job);
                                    ui.add(egui::Label::new(galley));
                                }
                                ui.end_row();
                            }
                        });
                });
            output.table_rects.push(response.response.rect);
        }
        Block::Rule => {
            ui.separator();
        }
    }
}

/// Gives narrow columns their natural width and splits the remaining room evenly among
/// the wider ones, never below `minimum`.
pub(super) fn fit_column_widths(natural: &[f32], room: f32, minimum: f32) -> Vec<f32> {
    let mut order = (0..natural.len()).collect::<Vec<_>>();
    order.sort_by(|left, right| natural[*left].total_cmp(&natural[*right]));
    let mut widths = vec![0.0; natural.len()];
    let mut remaining = room.max(0.0);
    for (placed, column) in order.iter().enumerate() {
        let share = remaining / (natural.len() - placed) as f32;
        let width = natural[*column].min(share).max(minimum);
        widths[*column] = width;
        remaining -= width;
    }
    widths
}

// Lay out inline runs with their marks at one text size.
fn inline_job(ui: &egui::Ui, inline: &Inline, size: f32, bold: bool) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let text_color = ui.visuals().text_color();
    for span in inline {
        let family = if span.code {
            grid_mono_font_family(bold || span.bold)
        } else {
            grid_font_family(bold || span.bold)
        };
        let color = if span.link {
            ui.visuals().hyperlink_color
        } else {
            text_color
        };
        let mut format = egui::TextFormat {
            font_id: FontId::new(if span.code { size * 0.95 } else { size }, family),
            color,
            italics: span.italic,
            ..Default::default()
        };
        if span.code {
            format.background = ui.visuals().faint_bg_color;
        }
        if span.strike {
            format.strikethrough = Stroke::new(1.0, color);
        }
        if span.link {
            format.underline = Stroke::new(1.0, color);
        }
        job.append(&span.text, 0.0, format);
    }
    job
}
