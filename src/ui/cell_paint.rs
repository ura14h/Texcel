use super::*;

#[cfg(test)]
#[path = "cell_paint_test.rs"]
mod tests;

/// How the grid lays out one cell's text: table wrapping and horizontal alignment.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct CellTextStyle {
    pub(crate) wrap_text: bool,
    pub(crate) alignment: Option<HorizontalAlignment>,
}

/// The horizontal placement of grid text inside a cell.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HorizontalAlignment {
    Left,
    Center,
    Right,
}

/// One cell's text and how to colour and decorate it.
pub(super) struct CellTextPaint<'a> {
    pub(super) cell: Option<&'a Cell>,
    pub(super) text: &'a str,
    /// The editor text shown instead of the stored source while another editor previews it.
    pub(super) source_override: Option<&'a str>,
    pub(super) style: &'a CellTextStyle,
    /// The text colour used when the cell style does not set one.
    pub(super) color: Color32,
    /// The cell's fill, used to blend the markers for text that runs past the edge.
    pub(super) background: Color32,
    pub(super) bold: bool,
    /// Present when the text is Markdown that live editing should show formatted.
    pub(super) markdown: Option<CellMarkdown<'a>>,
}

/// The parsed Markdown of one cell and the document it came from.
#[derive(Clone, Copy)]
pub(super) struct CellMarkdown<'a> {
    pub(super) display: Option<&'a MarkdownCellDisplay>,
    pub(super) source: Option<&'a crate::markdown_adapter::MarkdownDocument>,
}

// Draw one cell’s text and its supported inline styling.
pub(super) fn paint_cell_text(
    painter: &egui::Painter,
    rect: Rect,
    paint: CellTextPaint<'_>,
    display: GridDisplay,
) {
    let CellTextPaint {
        cell,
        text,
        source_override,
        style,
        color: fallback_color,
        background: cell_background,
        bold,
        markdown,
    } = paint;
    let display_mode = display.mode;
    let show_invisibles = display.show_invisibles;
    let scale = display.scale;
    let is_markdown = markdown.is_some();
    let markdown_display = markdown.and_then(|markdown| markdown.display);
    let markdown_source = markdown.and_then(|markdown| markdown.source);
    let (mut job, base_color) = if is_markdown
        && display_mode != CellDisplayMode::Source
        && cell.is_some_and(|cell| matches!(cell.input, CellInput::Text(_)))
    {
        let raw_source = source_override.unwrap_or_else(|| {
            cell.and_then(|cell| match &cell.input {
                CellInput::Text(source) => Some(source.as_str()),
                _ => None,
            })
            .unwrap_or(text)
        });
        let literal = cell.and_then(|cell| match &cell.input {
            CellInput::Text(source) => {
                let value = crate::document::literal_text_value(source);
                (value != source).then_some(value)
            }
            _ => None,
        });
        let escaped_display = literal.map(|_| MarkdownCellDisplay::parse_fragment(raw_source));
        let display = escaped_display.as_ref().or(markdown_display);
        display.map_or_else(
            || {
                cell_layout_job(
                    text,
                    CellTextFormat {
                        style,
                        color: fallback_color,
                        width: rect.width(),
                        show_invisibles,
                        scale,
                    },
                )
            },
            |display| {
                markdown_cell_layout_job(
                    display,
                    CellTextFormat {
                        style,
                        color: fallback_color,
                        width: rect.width(),
                        show_invisibles,
                        scale,
                    },
                    markdown_source,
                    crate::i18n::context_language(painter.ctx()),
                )
            },
        )
    } else {
        cell_layout_job(
            text,
            CellTextFormat {
                style,
                color: fallback_color,
                width: rect.width(),
                show_invisibles,
                scale,
            },
        )
    };
    let has_explicit_lines = job.text.contains('\n');
    if has_explicit_lines {
        job.wrap.max_rows = 256;
    }
    if display_mode == CellDisplayMode::Source {
        apply_grid_monospace(&mut job);
    }
    if bold {
        apply_grid_bold(&mut job);
    }
    constrain_cell_text_layout(
        painter.ctx(),
        rect,
        style,
        has_explicit_lines,
        scale,
        &mut job,
    );
    let galley = painter.ctx().fonts_mut(|fonts| fonts.layout_job(job));
    let position = cell_text_position(rect, galley.rect, style, scale);
    let painter = painter.with_clip_rect(rect.shrink(3.0 * scale));
    painter.galley(position, galley.clone(), base_color);
    if !style.wrap_text && has_explicit_lines {
        paint_horizontal_overflow_markers(
            &painter,
            rect.shrink(3.0 * scale),
            position,
            &galley,
            cell_background,
            base_color,
        );
    }
}

/// Marks text that cannot fit inside the visible cell instead of clipping it silently.
fn constrain_cell_text_layout(
    ctx: &egui::Context,
    rect: Rect,
    style: &CellTextStyle,
    has_explicit_lines: bool,
    scale: f32,
    job: &mut egui::text::LayoutJob,
) {
    if !style.wrap_text && !has_explicit_lines {
        job.wrap =
            egui::text::TextWrapping::truncate_at_width((rect.width() - 12.0 * scale).max(1.0));
        return;
    }

    let line_height = cell_text_line_height(ctx, job);
    if line_height > 0.0 {
        let inner_height = (rect.height()
            - super::grid_geometry::GRID_CELL_TEXT_VERTICAL_PADDING * scale)
            .max(line_height);
        let visible_rows = (inner_height / line_height).floor() as usize;
        job.wrap.max_rows = if style.wrap_text {
            job.wrap.max_rows.min(visible_rows.max(1))
        } else {
            visible_rows.max(1)
        };
    }
    job.wrap.overflow_character = Some('…');
}

// Uses the same font row height for painting and row measurement.
pub(super) fn cell_text_line_height(ctx: &egui::Context, job: &egui::text::LayoutJob) -> f32 {
    ctx.fonts_mut(|fonts| {
        job.sections
            .iter()
            .map(|section| fonts.row_height(&section.format.font_id))
            .fold(0.0, f32::max)
    })
}

// Adds a visible marker where an unwrapped line crosses a cell edge.
fn paint_horizontal_overflow_markers(
    painter: &egui::Painter,
    content_rect: Rect,
    text_position: Pos2,
    galley: &egui::text::Galley,
    cell_background: Color32,
    fallback_color: Color32,
) {
    for (row_index, left_clipped, right_clipped) in
        clipped_row_edges(content_rect, text_position, galley)
    {
        let placed_row = &galley.rows[row_index];
        let row_rect = Rect::from_min_size(
            text_position + placed_row.pos.to_vec2(),
            placed_row.row.size,
        );
        if left_clipped {
            paint_one_overflow_marker(
                painter,
                content_rect,
                row_rect,
                row_edge_format(galley, row_index, true),
                true,
                cell_background,
                fallback_color,
            );
        }
        if right_clipped {
            paint_one_overflow_marker(
                painter,
                content_rect,
                row_rect,
                row_edge_format(galley, row_index, false),
                false,
                cell_background,
                fallback_color,
            );
        }
    }
}

// Finds which visible rows cross either horizontal cell edge.
fn clipped_row_edges(
    content_rect: Rect,
    text_position: Pos2,
    galley: &egui::text::Galley,
) -> Vec<(usize, bool, bool)> {
    galley
        .rows
        .iter()
        .enumerate()
        .filter_map(|(index, placed_row)| {
            let row_rect = Rect::from_min_size(
                text_position + placed_row.pos.to_vec2(),
                placed_row.row.size,
            );
            let left_clipped = row_rect.left() < content_rect.left();
            let right_clipped = row_rect.right() > content_rect.right();
            (left_clipped || right_clipped).then_some((index, left_clipped, right_clipped))
        })
        .collect()
}

// Covers clipped glyphs at one edge and paints an ellipsis in their place.
fn paint_one_overflow_marker(
    painter: &egui::Painter,
    content_rect: Rect,
    row_rect: Rect,
    format: Option<egui::TextFormat>,
    at_left: bool,
    cell_background: Color32,
    fallback_color: Color32,
) {
    let format = format.unwrap_or_else(|| egui::TextFormat {
        color: fallback_color,
        ..Default::default()
    });
    let marker_job = egui::text::LayoutJob::simple_format("…".to_owned(), format.clone());
    let marker = painter
        .ctx()
        .fonts_mut(|fonts| fonts.layout_job(marker_job));
    let marker_width = marker.rect.width().min(content_rect.width());
    let x = if at_left {
        content_rect.left()
    } else {
        content_rect.right() - marker_width
    };
    let y = row_rect.center().y - marker.rect.center().y;
    let patch = Rect::from_min_max(
        Pos2::new(x, row_rect.top()),
        Pos2::new(x + marker_width, row_rect.bottom()),
    );
    painter.rect_filled(patch, 0.0, cell_background);
    painter.galley(Pos2::new(x, y), marker, fallback_color);
}

// Uses the visible row's first or last source character to keep its text style.
fn row_edge_format(
    galley: &egui::text::Galley,
    row_index: usize,
    at_left: bool,
) -> Option<egui::TextFormat> {
    if galley.job.sections.is_empty() {
        return None;
    }
    let mut byte_offset = 0;
    for (index, line) in galley.job.text.split('\n').enumerate() {
        if index == row_index {
            let within_line = if at_left {
                0
            } else {
                line.char_indices().last().map_or(0, |(offset, _)| offset)
            };
            return Some(
                galley
                    .job
                    .format_at_byte(egui::text::ByteIndex(byte_offset + within_line))
                    .clone(),
            );
        }
        byte_offset += line.len() + 1;
    }
    None
}

/// Places a measured cell galley using the column's horizontal alignment, centred vertically.
fn cell_text_position(rect: Rect, galley_rect: Rect, style: &CellTextStyle, scale: f32) -> Pos2 {
    let x = match style.alignment {
        Some(HorizontalAlignment::Center) => rect.center().x - galley_rect.center().x,
        Some(HorizontalAlignment::Right) => rect.right() - 6.0 * scale - galley_rect.right(),
        _ => rect.left() + 6.0 * scale - galley_rect.left(),
    };
    let y = rect.center().y - galley_rect.center().y;
    Pos2::new(x, y)
}

// Choose the display text shown for a cell and its calculation result.
pub(super) fn cell_display_text<'a>(
    cell: &'a Cell,
    calculation: Option<&'a crate::document::CalcResult>,
    mode: CellDisplayMode,
    is_markdown: bool,
    is_formula: bool,
) -> std::borrow::Cow<'a, str> {
    let source = cell_input_source(&cell.input);
    if mode == CellDisplayMode::Source {
        return source;
    }
    if is_markdown && !is_formula && matches!(cell.input, CellInput::Text(_)) {
        if let CellInput::Text(source) = &cell.input {
            return std::borrow::Cow::Borrowed(crate::document::literal_text_value(source));
        }
        return source;
    }
    cell_view_text(cell, calculation)
}

// Build the accessible value and metadata for one visible cell.
pub(super) fn grid_cell_accessibility_value(
    language: crate::i18n::Language,
    fallback_text: &str,
    display_mode: CellDisplayMode,
    is_markdown: bool,
    markdown_display: Option<&MarkdownCellDisplay>,
    markdown_source: Option<&crate::markdown_adapter::MarkdownDocument>,
) -> String {
    if !is_markdown || display_mode == CellDisplayMode::Source {
        return fallback_text.to_owned();
    }
    markdown_display.map_or_else(
        || fallback_text.to_owned(),
        |display| markdown_accessibility_text(display, markdown_source, language),
    )
}

// Describe a cell’s content for accessibility without drawing it.
pub(super) fn grid_accessibility_cell_text(
    language: crate::i18n::Language,
    grid: GridTable<'_>,
    address: CellAddress,
    display_mode: CellDisplayMode,
) -> String {
    let GridTable {
        document,
        table,
        is_markdown,
        markdown_source,
        markdown_table_index,
        ..
    } = grid;
    let Some(cell) = table.cell(address) else {
        return String::new();
    };
    let is_formula = crate::structure::is_formula_cell(table, address, cell);
    let calculation = document.calculation_result_for(table.id, address);
    let text = cell_display_text(
        cell,
        calculation.as_ref(),
        display_mode,
        is_markdown && !is_formula,
        is_formula,
    );
    if !is_markdown || is_formula || display_mode == CellDisplayMode::Source {
        return text.into_owned();
    }
    if let CellInput::Text(source_text) = &cell.input {
        let literal = crate::document::literal_text_value(source_text);
        let display = markdown_source.and_then(|source| {
            source.cell_display_for_source(
                markdown_table_index?,
                address.row,
                address.column,
                source_text,
            )
        });
        let escaped_display =
            (literal != source_text).then(|| MarkdownCellDisplay::parse_fragment(source_text));
        if let Some(display) = escaped_display.as_ref().or(display) {
            return markdown_accessibility_text(display, markdown_source, language);
        }
    }
    text.into_owned()
}

// Provide plain text for an inline Markdown cell display.
pub(super) fn markdown_accessibility_text(
    display: &MarkdownCellDisplay,
    markdown_source: Option<&crate::markdown_adapter::MarkdownDocument>,
    language: crate::i18n::Language,
) -> String {
    let display_limit = MAX_GRID_TEXT_BYTES - 3;
    let mut rendered = String::with_capacity(display.rendered_bytes.min(display_limit + 3));
    let mut remaining = display_limit;
    for run in &display.runs {
        if remaining == 0 {
            break;
        }
        let text = run.text_for_language(markdown_source, language);
        let preview = text_preview(text.as_deref().unwrap_or_default(), remaining);
        rendered.push_str(preview);
        remaining = remaining.saturating_sub(preview.len());
    }
    if display.rendered_bytes > display_limit {
        rendered.push('…');
    }
    rendered
}
