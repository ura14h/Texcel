use super::*;

mod roundtrip;
pub(super) use roundtrip::{
    markdown_cell_markup_matches, markdown_runs_as_markup_checked, report_markdown_run_omission,
};

/// Builds markup for Markdown runs in the target format.
pub(super) fn markdown_runs_as_markup(runs: &[XlsxTextRun]) -> String {
    let merged = merge_markdown_runs(runs);
    let mut output = String::new();
    for run in merged {
        let start = run
            .text
            .char_indices()
            .find(|(_, character)| !character.is_whitespace())
            .map_or(run.text.len(), |(index, _)| index);
        let end = run
            .text
            .char_indices()
            .rev()
            .find(|(_, character)| !character.is_whitespace())
            .map_or(start, |(index, character)| index + character.len_utf8());
        if start == end {
            output.push_str(&escape_markdown_text(&run.text));
            continue;
        }
        output.push_str(&escape_markdown_text(&run.text[..start]));
        let mut text = if run.code {
            markdown_code_markup(&run.text[start..end])
        } else {
            escape_markdown_text(&run.text[start..end])
        };
        if !run.code && run.strike {
            text = format!("~~{text}~~");
        }
        if !run.code && run.italic {
            text = format!("*{text}*");
        }
        if !run.code && run.bold {
            text = format!("**{text}**");
        }
        if let Some(link) = &run.hyperlink
            && !link.location
        {
            text = markdown_run_link_markup(link, &text);
        }
        output.push_str(&text);
        output.push_str(&escape_markdown_text(&run.text[end..]));
    }
    output
}

/// Writes pasted text formatting as inline Markdown source.
pub(super) fn markdown_cell_source_for_paste(
    cell: &Cell,
    format: Option<&CellFormat>,
    string_cell: bool,
) -> (String, bool) {
    let CellInput::Text(input) = &cell.input else {
        return (cell.source_text_for_text_file(string_cell), false);
    };
    if input.is_empty() {
        return (String::new(), false);
    }
    // A formula is written as typed; Markdown escapes would change what the engine reads.
    if crate::calc::formula_source(&cell.input, string_cell).is_some() {
        return (cell.source_text_for_text_file(string_cell), false);
    }
    let stored = cell.source_text_for_text_file(string_cell);
    let text = crate::document::literal_text_value(&stored);
    let default_format = CellFormat::default();
    let format = format.unwrap_or(&default_format);
    let mut runs = format.rich_text.as_ref().map_or_else(
        || {
            vec![XlsxTextRun {
                text: text.to_owned(),
                bold: format.style.bold,
                italic: format.style.italic,
                strike: format.style.strike,
                code: false,
                hyperlink: None,
                font_name: None,
                font_size: None,
                font_color: None,
            }]
        },
        |source_runs| {
            source_runs
                .iter()
                .map(|run| XlsxTextRun {
                    text: run.text.clone(),
                    bold: run.bold || format.style.bold,
                    italic: run.italic || format.style.italic,
                    strike: run.strike || format.style.strike,
                    code: run.code,
                    hyperlink: run.hyperlink.clone(),
                    font_name: None,
                    font_size: None,
                    font_color: None,
                })
                .collect()
        },
    );
    let run_text = runs.iter().map(|run| run.text.as_str()).collect::<String>();
    if run_text == text
        && let Some(marker) = stored.strip_suffix(text)
        && !marker.is_empty()
        && let Some(first) = runs.first_mut()
    {
        first.text.insert_str(0, marker);
    }
    let (mut markup, omitted, _) = markdown_runs_as_markup_checked(&runs);
    if let Some(link) = &format.hyperlink
        && !link.location
    {
        let target = link
            .target
            .replace('(', "%28")
            .replace(')', "%29")
            .replace(' ', "%20");
        markup = match link.tooltip.as_deref().filter(|title| !title.is_empty()) {
            Some(title) => {
                let title = title.replace('\\', "\\\\").replace('"', "\\\"");
                format!("[{markup}]({target} \"{title}\")")
            }
            None => format!("[{markup}]({target})"),
        };
    }
    (markup, omitted)
}

/// Combines adjacent runs with matching text styles.
pub(super) fn merge_markdown_runs(runs: &[XlsxTextRun]) -> Vec<XlsxTextRun> {
    let mut merged = Vec::<XlsxTextRun>::new();
    for run in runs {
        if let Some(previous) = merged.last_mut()
            && previous.bold == run.bold
            && previous.italic == run.italic
            && previous.strike == run.strike
            && previous.code == run.code
            && previous.hyperlink == run.hyperlink
        {
            previous.text.push_str(&run.text);
        } else {
            merged.push(run.clone());
        }
    }
    merged
}

// Wraps code runs with enough backticks to keep their text intact.
fn markdown_code_markup(text: &str) -> String {
    text.split('\n')
        .map(|part| {
            if part.is_empty() {
                return String::new();
            }
            let longest = part
                .split(|character| character != '`')
                .map(str::len)
                .max()
                .unwrap_or_default();
            let delimiter = "`".repeat(longest.saturating_add(1).max(1));
            let padding = if part.starts_with(' ')
                || part.ends_with(' ')
                || part.starts_with('`')
                || part.ends_with('`')
            {
                " "
            } else {
                ""
            };
            format!("{delimiter}{padding}{part}{padding}{delimiter}")
        })
        .collect::<Vec<_>>()
        .join("<br>")
}

// Escapes a link target and title before writing Markdown link syntax.
fn markdown_run_link_markup(link: &crate::document::CellHyperlink, content: &str) -> String {
    let target = link
        .target
        .replace('(', "%28")
        .replace(')', "%29")
        .replace(' ', "%20");
    match link.tooltip.as_deref() {
        Some(title) if !title.is_empty() => {
            let title = title.replace('\\', "\\\\").replace('"', "\\\"");
            format!("[{content}]({target} \"{title}\")")
        }
        _ => format!("[{content}]({target})"),
    }
}
