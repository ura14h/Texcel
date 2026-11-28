//!  Connects GFM Markdown tables to offsets in the original document.
//!
//!  pulldown-cmark identifies table syntax and code block boundaries. Only original cell ranges are editable.
//!  Saving replaces changed ranges from the end, without rebuilding prose or bytes outside tables.

use std::ops::Range;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

/// The GFM extensions every reader of a Markdown file uses, so they all find the same tables.
pub(crate) const MARKDOWN_OPTIONS: Options = Options::ENABLE_TABLES
    .union(Options::ENABLE_STRIKETHROUGH)
    .union(Options::ENABLE_TASKLISTS)
    .union(Options::ENABLE_GFM);

/// Splits a leading byte-order mark off Markdown text, returning the parsed body and the
/// byte offset of that body in the text.
pub(crate) fn markdown_body(text: &str) -> (&str, usize) {
    let offset = usize::from(text.starts_with('\u{feff}')) * '\u{feff}'.len_utf8();
    (text.get(offset..).unwrap_or(text), offset)
}

use crate::document::{Cell, CellAddress, CellInput, Document, DocumentId, StructurePlan};

mod alignment;
mod display;
mod document;
mod editing;
mod parser;
mod structure;
mod syntax;
mod table_names;
mod view;

use display::*;
pub use display::{MarkdownCellDisplay, MarkdownDisplayRun};
use syntax::*;

pub(crate) const MAX_MARKDOWN_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const MAX_MARKDOWN_ROWS: usize = 1_048_576;
pub(crate) const MAX_MARKDOWN_COLUMNS: usize = 16_384;
pub(crate) const MAX_MARKDOWN_CELLS: usize = 500_000;

// One table cell and its replacement range, excluding outer syntax spacing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarkdownCell {
    pub value: String,
    pub source_range: Range<usize>,
    // Whether the cell exists in the row or GFM added it to display a missing column.
    pub has_source: bool,
}

// A GFM table name, its rows including the header, and its document range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarkdownTable {
    pub name: String,
    pub(crate) name_generated: bool,
    pub rows: Vec<Vec<MarkdownCell>>,
    /// Explicit GFM separator alignment by column; missing entries use app defaults.
    pub alignments: Vec<Option<MarkdownTableAlignment>>,
    pub source_range: Range<usize>,
    /// The heading deleted with the table: the one that names only this table, directly above it.
    pub(crate) heading_range: Option<Range<usize>>,
    parser_width: usize,
    row_syntax: Vec<MarkdownRowSyntax>,
    separator_line: String,
    separator_range: Range<usize>,
    newline: String,
}

impl MarkdownTable {
    /// Returns the source range from the header through the separator and body rows.
    pub(crate) fn replacement_range(&self) -> Range<usize> {
        let start = self
            .row_syntax
            .first()
            .map_or(self.source_range.start, |row| row.range.start);
        let end = self
            .row_syntax
            .last()
            .map_or(self.separator_range.end, |row| row.range.end)
            .max(self.separator_range.end);
        start..end
    }
}

/// Alignment encoded in one Markdown table separator cell.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkdownTableAlignment {
    Left,
    Center,
    Right,
}

// A Markdown cell in the document to patch when saving.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarkdownCellChange {
    pub table: usize,
    pub row: usize,
    pub column: usize,
    pub value: String,
}

// Why Markdown input could not be read or a cell could not be safely replaced.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MarkdownError {
    InvalidUtf8,
    ResourceLimit(crate::i18n::ResourceLimit),
    OutputMismatch,
    LastTable,
    CellNotFound {
        table: usize,
        row: usize,
        column: usize,
    },
}

impl std::fmt::Display for MarkdownError {
    // Show the Markdown import or export failure in a short message.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = crate::i18n::markdown_error_message(self.clone());
        formatter.write_str(&message.resolve(crate::i18n::Language::English))
    }
}

impl std::error::Error for MarkdownError {}

// A Markdown document with GFM table positions and its original bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarkdownDocument {
    bytes: Vec<u8>,
    pub tables: Vec<MarkdownTable>,
    displays: Vec<Vec<Vec<MarkdownCellDisplay>>>,
    /// Open-time table dimensions stay stable while save results refresh byte offsets.
    source_extents: Vec<(usize, usize)>,
    parsed_row_count: usize,
    parsed_cell_count: usize,
    parsed_max_column_count: usize,
}

#[derive(Default)]
struct TableBuilder {
    start: usize,
    alignments: Vec<Option<MarkdownTableAlignment>>,
    rows: Vec<Vec<MarkdownCell>>,
    parser_width: usize,
    current_row: Option<Vec<MarkdownCell>>,
    current_row_start: Option<usize>,
    row_syntax: Vec<MarkdownRowSyntax>,
}

#[cfg(test)]
#[path = "markdown_adapter_test.rs"]
mod tests;
