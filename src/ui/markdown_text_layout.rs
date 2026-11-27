use super::*;

#[cfg(test)]
#[path = "markdown_text_layout_test.rs"]
mod tests;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct MarkdownInlineStyle {
    pub(super) bold: bool,
    pub(super) italic: bool,
    pub(super) strike: bool,
    pub(super) code: bool,
    pub(super) link: bool,
}

/// Adds Markdown source text to the cell layout job.
pub(super) fn append_markdown_layout_text(
    job: &mut egui::text::LayoutJob,
    text: &str,
    color: Color32,
    font_size: f32,
    style: MarkdownInlineStyle,
    show_invisibles: bool,
) {
    let color = if style.link {
        Color32::from_rgb(5, 99, 193)
    } else {
        color
    };
    let mut format = egui::TextFormat {
        font_id: FontId::new(
            font_size,
            if style.code {
                grid_mono_font_family(style.bold)
            } else {
                grid_font_family(style.bold)
            },
        ),
        color,
        italics: style.italic,
        strikethrough: if style.strike {
            Stroke::new(1.0, color)
        } else {
            Stroke::NONE
        },
        ..Default::default()
    };
    if style.code {
        format.background = inline_code_background(color);
    }
    if style.link {
        format.underline = Stroke::new(1.0, color);
    }
    let text = visualize_invisibles(text, show_invisibles);
    job.append(text.as_ref(), 0.0, format);
}

/// Keeps inline code behind both light and dark grid text colors.
fn inline_code_background(foreground: Color32) -> Color32 {
    let brightness = (0.299 * f32::from(foreground.r())
        + 0.587 * f32::from(foreground.g())
        + 0.114 * f32::from(foreground.b()))
        / 255.0;
    if brightness > 0.5 {
        Color32::from_gray(48)
    } else {
        Color32::from_gray(235)
    }
}

/// Adds display markers for spaces and line endings.
pub(super) fn visualize_invisibles(text: &str, enabled: bool) -> std::borrow::Cow<'_, str> {
    if !enabled {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut visible = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            ' ' => visible.push('·'),
            '\t' => visible.push('⇥'),
            '\n' => visible.push_str("↲\n"),
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                visible.push_str("↲\n");
            }
            '\u{00a0}' => visible.push('⍽'),
            '\u{200b}' => visible.push('⟂'),
            other => visible.push(other),
        }
    }
    std::borrow::Cow::Owned(visible)
}

/// Uses the grid monospace font for code text.
pub(crate) fn apply_grid_monospace(job: &mut egui::text::LayoutJob) {
    for section in &mut job.sections {
        section.format.font_id.family = if section.format.font_id.family == grid_font_family(true) {
            grid_mono_font_family(true)
        } else {
            grid_mono_font_family(false)
        };
    }
}

/// Returns the font family for general grid text.
pub(crate) fn grid_font_family(bold: bool) -> FontFamily {
    FontFamily::Name(if bold {
        "texcel-grid-bold".into()
    } else {
        "texcel-grid-regular".into()
    })
}

/// Returns the font family for monospaced grid text.
pub(crate) fn grid_mono_font_family(bold: bool) -> FontFamily {
    FontFamily::Name(if bold {
        "texcel-grid-mono-bold".into()
    } else {
        "texcel-grid-mono-regular".into()
    })
}

/// Adds Texcel grid fonts and fallbacks to egui.
pub(super) fn install_texcel_grid_fonts(ctx: &egui::Context, preset: FontPreset) {
    let mut definitions = egui::FontDefinitions::default();
    definitions.font_data.insert(
        "texcel-noto-sans-cjk-regular".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../../assets/fonts/NotoSansCJKjp-Regular.otf"
        ))),
    );
    definitions.font_data.insert(
        "texcel-noto-sans-cjk-bold".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../../assets/fonts/NotoSansCJKjp-Bold.otf"
        ))),
    );
    definitions.font_data.insert(
        "texcel-noto-sans-mono-cjk-regular".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../../assets/fonts/NotoSansMonoCJKjp-Regular.otf"
        ))),
    );
    definitions.font_data.insert(
        "texcel-noto-sans-mono-cjk-bold".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../../assets/fonts/NotoSansMonoCJKjp-Bold.otf"
        ))),
    );
    let proportional_fallback = definitions
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let monospace_fallback = definitions
        .families
        .get(&FontFamily::Monospace)
        .cloned()
        .unwrap_or_default();
    let proportional_regular = font_fallback_order(
        preset,
        &proportional_fallback,
        "texcel-noto-sans-cjk-regular",
        false,
    );
    let proportional_bold = font_fallback_order(
        preset,
        &proportional_fallback,
        "texcel-noto-sans-cjk-bold",
        true,
    );
    let monospace_regular = font_fallback_order(
        preset,
        &monospace_fallback,
        "texcel-noto-sans-mono-cjk-regular",
        false,
    );
    let monospace_bold = font_fallback_order(
        preset,
        &monospace_fallback,
        "texcel-noto-sans-mono-cjk-bold",
        true,
    );

    definitions
        .families
        .insert(FontFamily::Proportional, proportional_regular.clone());
    definitions
        .families
        .insert(FontFamily::Monospace, monospace_regular.clone());
    definitions
        .families
        .insert(grid_font_family(false), proportional_regular);
    definitions
        .families
        .insert(grid_font_family(true), proportional_bold);
    definitions
        .families
        .insert(grid_mono_font_family(false), monospace_regular);
    definitions
        .families
        .insert(grid_mono_font_family(true), monospace_bold);
    ctx.set_fonts(definitions);
}

/// Resolves the automatic preset from the current UI language.
pub(super) fn resolved_font_preset(preset: FontPreset, language: Language) -> FontPreset {
    match preset {
        FontPreset::Auto => match language {
            Language::English => FontPreset::Default,
            Language::Japanese => FontPreset::NotoSansJp,
        },
        explicit => explicit,
    }
}

/// Returns the font fallback order used by the grid.
pub(super) fn font_fallback_order(
    preset: FontPreset,
    fallback: &[String],
    noto: &str,
    bold: bool,
) -> Vec<String> {
    match preset {
        FontPreset::Auto => fallback
            .iter()
            .cloned()
            .chain(std::iter::once(noto.to_owned()))
            .collect(),
        FontPreset::Default if !bold => fallback
            .iter()
            .cloned()
            .chain(std::iter::once(noto.to_owned()))
            .collect(),
        FontPreset::Default => std::iter::once(noto.to_owned())
            .chain(fallback.iter().cloned())
            .collect(),
        FontPreset::NotoSansJp => std::iter::once(noto.to_owned())
            .chain(fallback.iter().cloned())
            .collect(),
    }
}

/// How a cell's text is laid out: its style, default colour, column width, and invisible marks.
#[derive(Clone, Copy)]
pub(crate) struct CellTextFormat<'a> {
    pub(crate) style: &'a CellTextStyle,
    /// The colour used when the cell style does not set one.
    pub(crate) color: Color32,
    pub(crate) width: f32,
    pub(crate) show_invisibles: bool,
    /// Grid scale applied to the font size and the text inset; 1.0 outside the grid.
    pub(crate) scale: f32,
}

/// Size of grid text in points. Edited cells carry no font size of their own.
pub(super) const GRID_TEXT_SIZE: f32 = 14.0;

/// Builds the text layout for one plain grid cell.
pub(crate) fn cell_layout_job(
    text: &str,
    format: CellTextFormat<'_>,
) -> (egui::text::LayoutJob, Color32) {
    let CellTextFormat {
        style,
        color,
        width,
        show_invisibles,
        scale,
    } = format;
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
    job.wrap.max_rows = if style.wrap_text { 256 } else { 1 };
    let text_format = egui::TextFormat {
        font_id: FontId::new(GRID_TEXT_SIZE * scale, grid_font_family(false)),
        color,
        ..Default::default()
    };
    let visible_text = visualize_invisibles(text, show_invisibles);
    job.append(visible_text.as_ref(), 0.0, text_format);
    (job, color)
}
