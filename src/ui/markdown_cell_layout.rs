use super::*;

// Build the styled text layout used to draw a Markdown cell.
pub(crate) fn markdown_cell_layout_job(
    display: &MarkdownCellDisplay,
    format: CellTextFormat<'_>,
    markdown_source: Option<&crate::markdown_adapter::MarkdownDocument>,
    language: crate::i18n::Language,
) -> (egui::text::LayoutJob, Color32) {
    let CellTextFormat {
        style,
        color: fallback_color,
        width,
        show_invisibles,
        scale,
    } = format;
    let base_color = fallback_color;
    let font_size = GRID_TEXT_SIZE * scale;
    let mut job = egui::text::LayoutJob {
        halign: match style.alignment {
            Some(HorizontalAlignment::Center) => egui::Align::Center,
            Some(HorizontalAlignment::Right) => egui::Align::Max,
            _ => egui::Align::Min,
        },
        ..Default::default()
    };
    job.wrap.max_width = if style.wrap_text {
        (width - 10.0 * scale).max(1.0)
    } else {
        f32::INFINITY
    };
    job.wrap.max_rows = 256;

    let display_limit = MAX_GRID_TEXT_BYTES - 3;
    let mut remaining = display_limit;
    for run in &display.runs {
        if remaining == 0 {
            break;
        }
        let run_text = run.text_for_language(markdown_source, language);
        let preview = text_preview(run_text.as_deref().unwrap_or_default(), remaining);
        remaining = remaining.saturating_sub(preview.len());
        append_markdown_layout_text(
            &mut job,
            preview,
            base_color,
            font_size,
            MarkdownInlineStyle {
                bold: run.bold,
                italic: run.italic,
                strike: run.strike,
                code: run.code,
                link: run.link,
            },
            show_invisibles,
        );
    }
    if display.rendered_bytes > display_limit {
        append_markdown_layout_text(
            &mut job,
            "…",
            base_color,
            font_size,
            MarkdownInlineStyle::default(),
            show_invisibles,
        );
    }
    (job, base_color)
}
