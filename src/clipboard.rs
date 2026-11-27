//!  The OS clipboard boundary and conversions to text formats for table data.

use serde::{Deserialize, Serialize};

use crate::document::{
    CalcStatus, CalcValue, Cell, CellAddress, CellFormat, CellHyperlink, CellInput, CellStyle,
    CellTextRun,
};

pub const MAX_CLIPBOARD_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_CLIPBOARD_CELLS: usize = 250_000;
mod ownership;
use ownership::*;
mod plain_text;
use plain_text::*;
pub use plain_text::{
    format_ranges_plain_text, format_ranges_plain_text_with_alignments, transpose,
};
mod html;
use html::*;
#[path = "clipboard/output_html.rs"]
mod output_html;
pub use output_html::create_html_clipboard;
#[cfg(test)]
pub(crate) use plain_text::{format_plain_text, format_plain_text_with_alignments};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CopyFormat {
    #[default]
    DisplayTsv,
    MarkdownTable,
    Csv,
    Rich,
}

/// One paste request's choices. The default is the normal paste.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PasteOptions {
    /// Reads only plain text, ignoring Texcel data and HTML.
    pub text_only: bool,
    /// Pastes formula results as values and keeps text that starts with `=` as text.
    pub values_only: bool,
    /// Drops Markdown formatting and links from the pasted cells.
    pub strip_formatting: bool,
    pub transpose: bool,
    pub skip_blank: bool,
}

impl PasteOptions {
    /// Pastes plain text read like typed input.
    pub const TEXT: Self = Self {
        text_only: true,
        values_only: false,
        strip_formatting: true,
        transpose: false,
        skip_blank: false,
    };
}

/// Clipboard-safe cell state: exact input and formatting, without stale calculation caches.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ClipboardCell {
    pub input: CellInput,
    pub style: CellStyle,
    pub hyperlink: Option<CellHyperlink>,
    pub rich_text: Option<Vec<CellTextRun>>,
    #[serde(default)]
    pub fresh_formula_value: Option<CellInput>,
    #[serde(default)]
    pub formula_source: bool,
    /// Keeps Texcel-owned Markdown source cells from being escaped as external plain text.
    #[serde(default)]
    pub raw_markdown_source: bool,
    /// Stores the rendered text independently from the original Markdown source.
    #[serde(default)]
    pub markdown_plain_text: Option<String>,
    /// Marks source cells whose Markdown carries supported inline formatting.
    #[serde(default)]
    pub markdown_has_features: bool,
}

impl From<&Cell> for ClipboardCell {
    /// Converts a document cell into a clipboard cell with no pasted formatting.
    fn from(cell: &Cell) -> Self {
        Self {
            input: cell.input.clone(),
            style: CellStyle::default(),
            hyperlink: None,
            rich_text: None,
            fresh_formula_value: None,
            formula_source: matches!(cell.input, CellInput::Formula(_)),
            raw_markdown_source: false,
            markdown_plain_text: None,
            markdown_has_features: false,
        }
    }
}

impl ClipboardCell {
    /// Builds clipboard data with table-specific formula interpretation.
    pub fn from_cell_with_formula_context(
        cell: &Cell,
        formula_source: bool,
        calculation: Option<&crate::document::CalcResult>,
    ) -> Self {
        let mut copied = Self::from(cell);
        copied.formula_source = formula_source;
        if formula_source
            && let Some(result) = calculation.filter(|result| result.status == CalcStatus::Fresh)
        {
            copied.fresh_formula_value = result.value.as_ref().and_then(calc_value_input);
        }
        copied
    }

    /// Converts clipboard data into a document cell, leaving its formatting behind.
    pub fn into_cell(self) -> Cell {
        Cell {
            input: self.input,
            imported_result: None,
            calculation: None,
        }
    }

    /// Drops the pasted character style and rich text, keeping the value and link.
    pub(crate) fn clear_pasted_style(&mut self) {
        self.style = CellStyle::default();
        self.rich_text = None;
    }

    /// Returns the pasted formatting carried by this clipboard cell.
    pub(crate) fn format(&self) -> CellFormat {
        CellFormat {
            style: self.style.clone(),
            hyperlink: self.hyperlink.clone(),
            rich_text: self.rich_text.clone(),
        }
    }

    /// Returns the cell content used for plain text copy.
    fn plain_text(&self) -> String {
        self.input.source_text()
    }
}

/// Converts a calculation result into typed cell input.
fn calc_value_input(value: &CalcValue) -> Option<CellInput> {
    match value {
        CalcValue::Text(value) => {
            Some(CellInput::Text(crate::document::encode_literal_text(value)))
        }
        CalcValue::Number(value) if value.is_finite() => Some(CellInput::Number(*value)),
        CalcValue::Boolean(value) => Some(CellInput::Boolean(*value)),
        CalcValue::Error(value) => Some(CellInput::Error(value.clone())),
        CalcValue::Number(_) => None,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClipboardInputKind {
    Internal,
    Html,
    Text,
}

#[derive(Clone, Debug)]
pub struct ParsedClipboard {
    pub ranges: Vec<Vec<Vec<ClipboardCell>>>,
    pub kind: ClipboardInputKind,
    /// Top-left source cell of each copied range; only Texcel data carries it.
    pub origins: Vec<CellAddress>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipboardError(pub crate::i18n::Message);

impl std::fmt::Display for ClipboardError {
    /// Formats the clipboard error for display.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.resolve(crate::i18n::Language::English))
    }
}

/// Writes HTML plus plain text through the single native clipboard adapter.
pub fn write_system_clipboard(html: &str, text: &str) -> Result<(), ClipboardError> {
    set_owned_payload(None);
    let text = native_line_endings(text);
    let mut clipboard = arboard::Clipboard::new().map_err(|error| {
        ClipboardError(
            crate::i18n::Message::new("status-clipboard-open-failed").with_text("detail", error),
        )
    })?;
    clipboard
        .set_html(html.to_owned(), Some(text.into_owned()))
        .map_err(|error| {
            ClipboardError(
                crate::i18n::Message::new("status-clipboard-copy-failed")
                    .with_text("detail", error),
            )
        })?;
    let mark =
        read_internal_payload(html)
            .zip(payload_digest(html))
            .map(|(payload, payload_digest)| OwnedPayloadMark {
                owner_id: payload.owner_id,
                payload_digest,
            });
    set_owned_payload(mark);
    Ok(())
}

/// Writes text to the operating system clipboard.
pub fn write_system_text(text: &str) -> Result<(), ClipboardError> {
    set_owned_payload(None);
    let text = native_line_endings(text);
    let mut clipboard = arboard::Clipboard::new().map_err(|error| {
        ClipboardError(
            crate::i18n::Message::new("status-clipboard-open-failed").with_text("detail", error),
        )
    })?;
    clipboard.set_text(text.into_owned()).map_err(|error| {
        ClipboardError(
            crate::i18n::Message::new("status-clipboard-copy-failed").with_text("detail", error),
        )
    })?;
    set_owned_payload(None);
    Ok(())
}

/// Returns the owner ID of the clipboard data this process wrote last, if any.
pub fn owned_payload_owner_id() -> Option<String> {
    current_owned_payload().map(|mark| mark.owner_id)
}

/// Returns the owner ID when `html` is exactly what this process wrote last.
pub fn owner_id_of_html(html: &str) -> Option<String> {
    owned_html_owner_id(html)
}

/// Reads plain text for a one-time paste command.
pub fn read_system_text() -> Result<String, ClipboardError> {
    let mut clipboard = arboard::Clipboard::new().map_err(|error| {
        ClipboardError(
            crate::i18n::Message::new("status-clipboard-open-failed").with_text("detail", error),
        )
    })?;
    let text = clipboard.get_text().map_err(|error| {
        ClipboardError(
            crate::i18n::Message::new("status-clipboard-read-failed").with_text("detail", error),
        )
    })?;
    if text.len() > MAX_CLIPBOARD_BYTES {
        return Err(ClipboardError(crate::i18n::Message::new(
            "status-paste-clipboard-limit-exceeded",
        )));
    }
    Ok(text)
}

/// Returns native HTML only when its plain-text representation matches the paste event.
pub fn read_system_html_if_matching(expected_text: &str) -> Option<String> {
    let mut clipboard = arboard::Clipboard::new().ok()?;
    let actual_text = clipboard.get_text().ok()?;
    if normalize_clipboard_text(&actual_text) != normalize_clipboard_text(expected_text) {
        return None;
    }
    clipboard
        .get()
        .html()
        .ok()
        .filter(|html| html.len() <= MAX_CLIPBOARD_BYTES)
}

/// Reads an app payload only when both native clipboard representations still agree.
pub fn parse_clipboard(
    plain_text: &str,
    html: Option<&str>,
) -> Result<ParsedClipboard, ClipboardError> {
    let owner_id = html.and_then(owned_html_owner_id);
    parse_clipboard_with_owner(plain_text, html, owner_id.as_deref())
}

/// Parses clipboard data while checking its owner and formats.
pub fn parse_clipboard_with_owner(
    plain_text: &str,
    html: Option<&str>,
    expected_owner_id: Option<&str>,
) -> Result<ParsedClipboard, ClipboardError> {
    if plain_text.len() > MAX_CLIPBOARD_BYTES {
        return Err(ClipboardError(crate::i18n::Message::new(
            "status-clipboard-paste-limit",
        )));
    }
    if let Some(html) = html.filter(|html| html.len() <= MAX_CLIPBOARD_BYTES) {
        if let Some(payload) = read_internal_payload(html)
            && payload.version == 1
            && valid_owner_id(&payload.owner_id)
            && expected_owner_id.is_some_and(|owner_id| owner_id == payload.owner_id)
            && normalize_clipboard_text(&payload.text) == normalize_clipboard_text(plain_text)
            && validate_ranges(&payload.ranges).is_ok()
        {
            let origins = if payload.origins.len() == payload.ranges.len() {
                payload
                    .origins
                    .iter()
                    .map(|[row, column]| CellAddress {
                        row: *row,
                        column: *column,
                    })
                    .collect()
            } else {
                Vec::new()
            };
            return Ok(ParsedClipboard {
                ranges: payload.ranges,
                kind: ClipboardInputKind::Internal,
                origins,
            });
        }
        if let Some(cells) = parse_html_table(html)
            && matrix_plain_text(&cells) == normalize_clipboard_text(plain_text)
        {
            return Ok(ParsedClipboard {
                ranges: vec![cells],
                kind: ClipboardInputKind::Html,
                origins: Vec::new(),
            });
        }
    }
    let matrix = parse_plain_text(plain_text)?;
    validate_matrix(&matrix)?;
    Ok(ParsedClipboard {
        ranges: vec![matrix],
        kind: ClipboardInputKind::Text,
        origins: Vec::new(),
    })
}

// Check that clipboard ranges are nonempty and within the supported cell limit.
fn validate_ranges(ranges: &[Vec<Vec<ClipboardCell>>]) -> Result<(), ClipboardError> {
    if ranges.is_empty() || ranges.iter().any(Vec::is_empty) {
        return Err(ClipboardError(crate::i18n::Message::new(
            "status-clipboard-empty-ranges",
        )));
    }
    let count = ranges
        .iter()
        .flat_map(|range| range.iter())
        .map(Vec::len)
        .fold(0_usize, usize::saturating_add);
    if count > MAX_CLIPBOARD_CELLS {
        return Err(ClipboardError(crate::i18n::Message::new(
            "status-clipboard-cell-limit",
        )));
    }
    if ranges.iter().any(|range| validate_matrix(range).is_err()) {
        return Err(ClipboardError(crate::i18n::Message::new(
            "status-clipboard-invalid-ranges",
        )));
    }
    Ok(())
}

/// Rejects clipboard matrices that exceed supported dimensions.
fn validate_matrix(matrix: &[Vec<ClipboardCell>]) -> Result<(), ClipboardError> {
    if matrix.is_empty() || matrix.iter().any(Vec::is_empty) {
        return Err(ClipboardError(crate::i18n::Message::new(
            "status-paste-empty-range",
        )));
    }
    let columns = matrix[0].len();
    if matrix.iter().any(|row| row.len() != columns)
        || matrix.len().saturating_mul(columns) > MAX_CLIPBOARD_CELLS
    {
        return Err(ClipboardError(crate::i18n::Message::new(
            "status-paste-invalid-dimensions",
        )));
    }
    if matrix.iter().flatten().any(|cell| {
        matches!(cell.input, CellInput::Number(value) if !value.is_finite())
            || cell.rich_text.as_ref().is_some_and(|runs| {
                runs.iter().map(|run| run.text.as_str()).collect::<String>()
                    != cell.input.source_text()
                    || runs.iter().any(|run| {
                        run.font_size
                            .is_some_and(|size| !size.is_finite() || size <= 0.0)
                    })
            })
            || cell
                .style
                .font_size
                .is_some_and(|size| !size.is_finite() || size <= 0.0)
            || cell.fresh_formula_value.as_ref().is_some_and(|value| {
                !(cell.formula_source || matches!(cell.input, CellInput::Formula(_)))
                    || !matches!(
                        value,
                        CellInput::Text(_)
                            | CellInput::Number(_)
                            | CellInput::Boolean(_)
                            | CellInput::Error(_)
                    )
                    || matches!(value, CellInput::Number(number) if !number.is_finite())
            })
    }) {
        return Err(ClipboardError(crate::i18n::Message::new(
            "status-paste-invalid-cell",
        )));
    }
    Ok(())
}

#[cfg(test)]
impl ClipboardCell {
    /// Builds clipboard data using the current calculation result.
    pub fn from_cell(cell: &Cell, calculation_revision: u64) -> Self {
        let calculation = cell
            .calculation
            .as_ref()
            .filter(|result| result.revision == calculation_revision);
        Self::from_cell_with_calculation(cell, calculation)
    }
    /// Builds clipboard data using an indexed document calculation result.
    pub fn from_cell_with_calculation(
        cell: &Cell,
        calculation: Option<&crate::document::CalcResult>,
    ) -> Self {
        Self::from_cell_with_formula_context(
            cell,
            matches!(cell.input, CellInput::Formula(_)),
            calculation,
        )
    }
}

#[cfg(test)]
#[path = "clipboard_test.rs"]
pub(crate) mod tests;
