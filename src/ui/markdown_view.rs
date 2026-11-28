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
    /// Leaves each table as an empty box for the caller to label, instead of drawing cells.
    pub(super) table_placeholders: bool,
}

/// Where the drawn tables landed, in document order.
#[derive(Default)]
pub(super) struct MarkdownViewOutput {
    pub(super) table_rects: Vec<Rect>,
    /// Each top-level heading's block index and where it was drawn.
    pub(super) heading_rects: Vec<(usize, Rect)>,
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
        let top = ui.cursor().top();
        show_block(ui, block, style, &mut output, index);
        if matches!(block, Block::Heading(..)) {
            let rect = Rect::from_x_y_ranges(ui.max_rect().x_range(), top..=ui.cursor().top());
            output.heading_rects.push((index, rect));
        }
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
        Block::Table { rows, .. } if style.table_placeholders => {
            // The box grows with the row count up to a few rows, so tables keep their shape.
            let height = size * 1.4 * rows.len().clamp(2, 8) as f32;
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
            output.table_rects.push(rect);
        }
        Block::Table { alignments, rows } => {
            let rect = show_table(ui, alignments, rows, size, index);
            output.table_rects.push(rect);
        }
        Block::Rule => {
            ui.separator();
        }
    }
}

/// Draws one table with each column as wide as its widest cell, aligned as the separator
/// row says, and wraps the widest columns when the table would not fit.
fn show_table(
    ui: &mut egui::Ui,
    alignments: &[pulldown_cmark::Alignment],
    rows: &[Vec<Inline>],
    size: f32,
    index: usize,
) -> Rect {
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
    // Many columns shrink the gaps too, so even a very wide table stays in view.
    let frame = size * 0.6 + 1.0;
    let gap = size.min((ui.available_width() - frame).max(0.0) / (columns as f32 * 2.0));
    let room = ui.available_width() - gap * (columns as f32 - 1.0) - frame;
    let widths = fit_column_widths(&natural, room, gap);
    let aligns = (0..columns)
        .map(|column| match alignments.get(column) {
            Some(pulldown_cmark::Alignment::Center) => egui::Align::Center,
            Some(pulldown_cmark::Alignment::Right) => egui::Align::RIGHT,
            _ => egui::Align::LEFT,
        })
        .collect::<Vec<_>>();
    // Wrap each cell at its column width, then size each column to its widest cell.
    let galleys = rows
        .iter()
        .enumerate()
        .map(|(row_index, row)| {
            row.iter()
                .enumerate()
                .map(|(column, cell)| {
                    let mut job = inline_job(ui, cell, size, row_index == 0);
                    job.wrap.max_width = widths[column];
                    job.halign = aligns[column];
                    ui.painter().layout_job(job)
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut column_widths = vec![0.0_f32; columns];
    for row in &galleys {
        for (column, galley) in row.iter().enumerate() {
            // A glyph wider than a very narrow column is clipped instead of widening it.
            column_widths[column] = column_widths[column]
                .max(galley.size().x)
                .min(widths[column]);
        }
    }
    let text_color = ui.visuals().text_color();
    egui::Frame::new()
        .stroke(Stroke::new(
            0.5,
            ui.visuals().widgets.noninteractive.bg_stroke.color,
        ))
        .inner_margin(egui::Margin::same((size * 0.3) as i8))
        .show(ui, |ui| {
            egui::Grid::new(("texcel_markdown_table", index))
                .striped(true)
                .min_col_width(0.0)
                .spacing(Vec2::new(gap, size * 0.3))
                .show(ui, |ui| {
                    for row in galleys {
                        for (column, galley) in row.into_iter().enumerate() {
                            let (rect, response) = ui.allocate_exact_size(
                                Vec2::new(column_widths[column], galley.size().y),
                                Sense::hover(),
                            );
                            let text = galley.text().to_owned();
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(egui::Role::Label, true, &text)
                            });
                            // A right- or center-aligned galley is drawn from that edge.
                            let anchor = match aligns[column] {
                                egui::Align::Center => rect.center_top(),
                                egui::Align::Max => rect.right_top(),
                                egui::Align::Min => rect.left_top(),
                            };
                            ui.painter()
                                .with_clip_rect(rect.intersect(ui.clip_rect()))
                                .galley(anchor, galley, text_color);
                        }
                        ui.end_row();
                    }
                });
        })
        .response
        .rect
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
