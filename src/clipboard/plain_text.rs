use super::*;

/// Formats clipboard cells as a plain text matrix.
pub fn format_plain_text(matrix: &[Vec<ClipboardCell>], format: CopyFormat) -> String {
    format_plain_text_with_alignments(matrix, format, &[])
}

/// Formats Markdown clipboard tables with the selected columns' alignment markers.
pub fn format_plain_text_with_alignments(
    matrix: &[Vec<ClipboardCell>],
    format: CopyFormat,
    alignments: &[Option<crate::markdown_adapter::MarkdownTableAlignment>],
) -> String {
    match format {
        CopyFormat::DisplayTsv | CopyFormat::Rich => join_fields(matrix, '\t'),
        CopyFormat::Csv => join_fields(matrix, ','),
        CopyFormat::MarkdownTable => {
            let mut output = String::new();
            for (row_index, row) in matrix.iter().enumerate() {
                output.push('|');
                for cell in row {
                    output.push(' ');
                    output.push_str(&cell.plain_text().replace('|', "\\|").replace('\n', "<br>"));
                    output.push_str(" |");
                }
                output.push('\n');
                if row_index == 0 {
                    output.push('|');
                    for (column, _) in row.iter().enumerate() {
                        output.push(' ');
                        output.push_str(markdown_alignment_marker(
                            alignments.get(column).copied().flatten(),
                        ));
                        output.push_str(" |");
                    }
                    output.push('\n');
                }
            }
            output
        }
    }
}

/// Uses the same separator syntax as an ordinary Markdown table.
fn markdown_alignment_marker(
    alignment: Option<crate::markdown_adapter::MarkdownTableAlignment>,
) -> &'static str {
    use crate::markdown_adapter::MarkdownTableAlignment::{Center, Left, Right};
    match alignment {
        Some(Left) => ":---",
        Some(Center) => ":---:",
        Some(Right) => "---:",
        None => "---",
    }
}

/// Formats multiple selections as plain text blocks.
pub fn format_ranges_plain_text(ranges: &[Vec<Vec<ClipboardCell>>], format: CopyFormat) -> String {
    ranges
        .iter()
        .map(|range| format_plain_text(range, format))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Formats several copied ranges with the matching per-range column alignments.
pub fn format_ranges_plain_text_with_alignments(
    ranges: &[Vec<Vec<ClipboardCell>>],
    format: CopyFormat,
    alignments: &[Vec<Option<crate::markdown_adapter::MarkdownTableAlignment>>],
) -> String {
    ranges
        .iter()
        .enumerate()
        .map(|(index, range)| {
            format_plain_text_with_alignments(
                range,
                format,
                alignments.get(index).map_or(&[], Vec::as_slice),
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Transposes a clipboard matrix.
pub fn transpose(matrix: Vec<Vec<ClipboardCell>>) -> Vec<Vec<ClipboardCell>> {
    let rows = matrix.len();
    let columns = matrix.iter().map(Vec::len).max().unwrap_or(0);
    let mut transposed = vec![Vec::with_capacity(rows); columns];
    for row in matrix {
        for (column, values) in transposed.iter_mut().enumerate() {
            values.push(row.get(column).cloned().unwrap_or_else(blank_cell));
        }
    }
    transposed
}

/// Parses plain clipboard text into a cell matrix.
pub(super) fn parse_plain_text(text: &str) -> Result<Vec<Vec<ClipboardCell>>, ClipboardError> {
    let has_unquoted_tab = contains_unquoted_delimiter(text, '\t');
    let has_unquoted_comma = contains_unquoted_delimiter(text, ',');
    let delimiter = if has_unquoted_tab {
        '\t'
    } else if has_unquoted_comma {
        ','
    } else {
        '\0'
    };
    let rows = parse_quoted_rows(text, delimiter)?;
    if rows.is_empty() {
        return Ok(vec![vec![blank_cell()]]);
    }
    Ok(rows
        .into_iter()
        .map(|row| row.into_iter().map(text_cell).collect())
        .collect())
}

/// Checks whether a line contains a delimiter outside quotes.
pub(super) fn contains_unquoted_delimiter(text: &str, delimiter: char) -> bool {
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
            }
            '"' => quoted = !quoted,
            value if value == delimiter && !quoted => return true,
            _ => (),
        }
    }
    false
}

/// Parses rows and fields while honoring quoted delimiters.
pub(super) fn parse_quoted_rows(
    text: &str,
    delimiter: char,
) -> Result<Vec<Vec<String>>, ClipboardError> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut chars = text.chars().peekable();
    let mut quoted = false;
    let mut field_started = false;
    let mut cell_count = 0_usize;
    while let Some(character) = chars.next() {
        if quoted {
            match character {
                '"' if chars.peek() == Some(&'"') => {
                    field.push('"');
                    chars.next();
                }
                '"' => quoted = false,
                _ => field.push(character),
            }
        } else if character == '"' && !field_started && field.is_empty() {
            quoted = true;
            field_started = true;
        } else if delimiter != '\0' && character == delimiter {
            cell_count = cell_count.saturating_add(1);
            row.push(std::mem::take(&mut field));
            if cell_count > MAX_CLIPBOARD_CELLS {
                return Err(ClipboardError(crate::i18n::Message::new(
                    "status-clipboard-cell-limit",
                )));
            }
            field_started = false;
        } else if matches!(character, '\r' | '\n') {
            if character == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            cell_count = cell_count.saturating_add(1);
            row.push(std::mem::take(&mut field));
            rows.push(std::mem::take(&mut row));
            if cell_count > MAX_CLIPBOARD_CELLS || rows.len() > MAX_CLIPBOARD_CELLS {
                return Err(ClipboardError(crate::i18n::Message::new(
                    "status-clipboard-cell-limit",
                )));
            }
            field_started = false;
        } else {
            field.push(character);
            field_started = true;
        }
    }
    if quoted {
        return Err(ClipboardError(crate::i18n::Message::new(
            "status-clipboard-unclosed-quote",
        )));
    }
    if !field.is_empty() || !row.is_empty() || field_started {
        cell_count = cell_count.saturating_add(1);
        row.push(field);
        rows.push(row);
        if cell_count > MAX_CLIPBOARD_CELLS || rows.len() > MAX_CLIPBOARD_CELLS {
            return Err(ClipboardError(crate::i18n::Message::new(
                "status-clipboard-cell-limit",
            )));
        }
    }
    Ok(rows)
}

/// Converts a cell matrix into its plain text form.
pub(super) fn matrix_plain_text(matrix: &[Vec<ClipboardCell>]) -> String {
    matrix
        .iter()
        .map(|row| {
            row.iter()
                .map(ClipboardCell::plain_text)
                .collect::<Vec<_>>()
                .join("\t")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Normalizes line endings before clipboard parsing.
pub(super) fn normalize_clipboard_text(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Uses CRLF between rows on Windows, where other apps expect it. Line breaks inside
/// quoted fields stay LF, as Excel writes them.
pub(super) fn native_line_endings(text: &str) -> std::borrow::Cow<'_, str> {
    if cfg!(windows) {
        std::borrow::Cow::Owned(row_breaks_as_crlf(text))
    } else {
        std::borrow::Cow::Borrowed(text)
    }
}

/// Replaces each LF outside double quotes with CRLF.
pub(super) fn row_breaks_as_crlf(text: &str) -> String {
    let mut output = String::with_capacity(text.len() + text.len() / 16);
    let mut quoted = false;
    for character in text.chars() {
        match character {
            '"' => quoted = !quoted,
            '\n' if !quoted => output.push('\r'),
            _ => (),
        }
        output.push(character);
    }
    output
}

/// Builds a text clipboard cell.
pub(super) fn text_cell(text: String) -> ClipboardCell {
    ClipboardCell {
        input: if text.is_empty() {
            CellInput::Empty
        } else {
            CellInput::Text(text)
        },
        style: CellStyle::default(),
        hyperlink: None,
        rich_text: None,
        fresh_formula_value: None,
        formula_source: false,
        raw_markdown_source: false,
        markdown_plain_text: None,
        markdown_has_features: false,
    }
}

/// Builds an empty clipboard cell.
pub(super) fn blank_cell() -> ClipboardCell {
    text_cell(String::new())
}

/// Joins fields using the requested delimiter.
pub(super) fn join_fields(matrix: &[Vec<ClipboardCell>], delimiter: char) -> String {
    matrix
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| {
                    let value = cell.plain_text();
                    if value.contains(delimiter)
                        || value.contains('"')
                        || value.contains('\n')
                        || value.contains('\r')
                    {
                        format!("\"{}\"", value.replace('"', "\"\""))
                    } else {
                        value
                    }
                })
                .collect::<Vec<_>>()
                .join(&delimiter.to_string())
        })
        .collect::<Vec<_>>()
        .join("\n")
}
