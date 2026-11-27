//!  Defines export settings, preflight checks, and format-specific value conversion.
//!
//!  This module does not perform file I/O. FileWorker receives an immutable snapshot,
//!  scans and converts all entries and builds the report on a worker before returning output bytes.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use pulldown_cmark::{Alignment, Event, Options, Parser, Tag, TagEnd};

#[cfg(test)]
pub(crate) use self::prepare::prepare_export;
use crate::calc::{FormulaResultSnapshot, formula_source};
use crate::conversion_report::{ConversionClass, ConversionEntry, ExportReport, ImportReport};
use crate::csv_adapter::{CsvCellChange, CsvDocument};
use crate::document::{
    CalcStatus, CalcValue, Cell, CellAddress, CellFormat, CellHyperlink, CellInput, CellStyle,
    CellTextRun, Document, TableId,
};
use crate::html_adapter::HtmlDocument;
use crate::markdown_adapter::MarkdownDocument;
use crate::xlsx_adapter::{XlsxDocument, XlsxSheet, XlsxStyle, XlsxTextRun};

// The format used for the exported document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExportFormat {
    Csv,
    Tsv,
    Markdown,
}

impl ExportFormat {
    /// Returns the user-facing label for the export format.
    pub fn label(self) -> &'static str {
        match self {
            Self::Csv => "CSV",
            Self::Tsv => "TSV",
            Self::Markdown => "Markdown",
        }
    }

    /// Returns the file extension for the export format.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Tsv => "tsv",
            Self::Markdown => "md",
        }
    }

    /// Reports a one-table delimited format, CSV or TSV.
    pub fn is_delimited(self) -> bool {
        matches!(self, Self::Csv | Self::Tsv)
    }

    /// Reports whether a file extension belongs to this format.
    pub fn matches_path(self, path: &std::path::Path) -> bool {
        let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
            return false;
        };
        match self {
            Self::Csv => extension.eq_ignore_ascii_case("csv"),
            Self::Tsv => extension.eq_ignore_ascii_case("tsv"),
            Self::Markdown => {
                extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown")
            }
        }
    }

    /// Reports whether a CSV source's own delimiter can be kept for this format.
    /// TSV keeps only tabs; CSV keeps any other delimiter, such as a semicolon.
    pub fn keeps_csv_delimiter(self, delimiter: char) -> bool {
        match self {
            Self::Csv => delimiter != '\t',
            Self::Tsv => delimiter == '\t',
            Self::Markdown => false,
        }
    }

    /// Picks the delimited format whose files normally use this delimiter.
    pub fn for_csv_delimiter(delimiter: char) -> Self {
        if delimiter == '\t' {
            Self::Tsv
        } else {
            Self::Csv
        }
    }
}

// The tables to export. Exporting all CSV tables creates one file per table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExportScope {
    AllTables,
    Selected(TableId),
}

// The value written for formula cells.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FormulaOutput {
    Formula,
    LatestResult,
}

// Whether to convert text modifiers for the output format or keep plain text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModifierOutput {
    Convert,
    TextOnly,
}

// How to choose the Markdown table header row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkdownHeader {
    UseFirstRow,
    AddBlankHeader,
}

// Export settings chosen by the user.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExportOptions {
    pub format: ExportFormat,
    pub scope: ExportScope,
    pub formulas: FormulaOutput,
    pub modifiers: ModifierOutput,
    pub markdown_header: MarkdownHeader,
}

// Immutable input passed from the UI to the worker.
#[derive(Clone, Debug)]
pub enum ExportSource {
    Csv(Arc<CsvDocument>),
    Markdown(Arc<MarkdownDocument>),
    Xlsx(Arc<XlsxDocument>),
    Html(Arc<HtmlDocument>),
}

// Small table metadata. It does not include every cell in the logical range.
#[derive(Clone, Debug)]
pub struct ExportTableMeta {
    pub id: TableId,
    pub name: String,
    pub rows: usize,
    pub columns: usize,
}

// Immutable document version, changes, and output settings passed to the worker for preflight.
#[derive(Clone, Debug)]
pub struct ExportRequest {
    pub document_id: u64,
    pub revision: u64,
    pub options: ExportOptions,
    pub destination: PathBuf,
    pub source_path: Option<PathBuf>,
    pub source: Option<ExportSource>,
    /// Known omissions from the original XLSX import are carried into the output preflight.
    pub source_report: Option<Arc<ImportReport>>,
    pub tables: Vec<ExportTableMeta>,
    pub changes: Vec<(TableId, CellAddress, Option<Cell>)>,
    /// Imported formatting replaced for cells rewritten for Markdown output, such as HTML rich text.
    pub format_changes: Vec<(TableId, CellAddress, CellFormat)>,
    pub formula_results: Option<FormulaResultSnapshot>,
}

// Finished artifact passed to the safe save worker after preflight.
#[derive(Debug)]
pub struct ExportArtifact {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    // Whether the destination was absent during preflight or was chosen in the save dialog.
    pub expected_identity: Option<crate::file_worker::FileIdentity>,
    pub replace_existing: bool,
}

// Output bytes and the preflight report for the same document version and settings.
#[derive(Debug)]
pub struct PreparedExport {
    pub report: Arc<ExportReport>,
    pub artifacts: Vec<ExportArtifact>,
}

#[derive(Clone, Default)]
struct MarkdownCellContent {
    runs: Vec<XlsxTextRun>,
    has_link: bool,
    has_reference_link: bool,
    links: Vec<(usize, usize, crate::document::CellHyperlink)>,
    whole_cell_link: Option<crate::document::CellHyperlink>,
    has_image: bool,
    has_code: bool,
    has_html: bool,
}

impl MarkdownCellContent {
    /// Returns the plain text stored in this output cell.
    fn plain_text(&self) -> String {
        self.runs.iter().map(|run| run.text.as_str()).collect()
    }
}

#[derive(Default)]
struct MarkdownExportContext {
    cells: BTreeMap<(TableId, CellAddress), MarkdownCellContent>,
    alignments: BTreeMap<TableId, Vec<Alignment>>,
}

struct FormulaResultIndex<'a> {
    snapshot: Option<&'a FormulaResultSnapshot>,
}

struct ExportBuildContext<'context, 'results> {
    request: &'context ExportRequest,
    cell_changes: HashMap<(TableId, CellAddress), Option<&'context Cell>>,
    formula_index: &'context FormulaResultIndex<'results>,
    markdown_context: Option<&'context MarkdownExportContext>,
    cancelled: Option<&'context AtomicBool>,
}

impl<'a> FormulaResultIndex<'a> {
    /// Creates output content for one Markdown cell.
    fn new(
        snapshot: Option<&'a FormulaResultSnapshot>,
        cancelled: Option<&AtomicBool>,
    ) -> Result<Self, crate::i18n::Message> {
        check_cancelled(cancelled)?;
        Ok(Self { snapshot })
    }

    /// Returns the calculation value for a cell when available.
    fn value(&self, table_id: TableId, address: CellAddress) -> Option<&'a CalcValue> {
        self.snapshot
            .and_then(|snapshot| snapshot.get(table_id, address))
            .filter(|(status, _)| matches!(status, CalcStatus::Fresh | CalcStatus::Error))
            .and_then(|(_, value)| value)
    }
}

#[derive(Default)]
struct MarkdownCellBuilder {
    content: MarkdownCellContent,
    bold_depth: usize,
    italic_depth: usize,
    strike_depth: usize,
    code_depth: usize,
    active_links: Vec<(usize, crate::document::CellHyperlink)>,
}

impl MarkdownCellBuilder {
    /// Adds a text run to the current Markdown cell.
    fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.push_styled_text(text, false);
    }

    /// Adds text that came from an inline code span.
    fn push_code(&mut self, text: &str) {
        self.content.has_code = true;
        self.push_styled_text(text, true);
    }

    // Adds text with the styles that are active at this point in the source.
    fn push_styled_text(&mut self, text: &str, code: bool) {
        if text.is_empty() {
            return;
        }
        let run = XlsxTextRun {
            text: text.to_owned(),
            bold: self.bold_depth > 0,
            italic: self.italic_depth > 0,
            strike: self.strike_depth > 0,
            code: code || self.code_depth > 0,
            hyperlink: self.active_links.last().map(|(_, link)| link.clone()),
            font_name: None,
            font_size: None,
            font_color: None,
        };
        if let Some(previous) = self.content.runs.last_mut()
            && previous.bold == run.bold
            && previous.italic == run.italic
            && previous.strike == run.strike
            && previous.code == run.code
            && previous.hyperlink == run.hyperlink
        {
            previous.text.push_str(text);
        } else {
            self.content.runs.push(run);
        }
    }

    /// Builds the final Markdown cell content.
    fn finish(mut self) -> MarkdownCellContent {
        let character_count = self.content.plain_text().chars().count();
        self.content.whole_cell_link = (self.content.links.len() == 1)
            .then(|| self.content.links[0].clone())
            .filter(|(start, end, _)| *start == 0 && *end == character_count)
            .map(|(_, _, link)| link);
        self.content
    }
}

/// Stops export work when the user has canceled.
fn check_cancelled(cancelled: Option<&AtomicBool>) -> Result<(), crate::i18n::Message> {
    if cancelled.is_some_and(|cancelled| cancelled.load(Ordering::Acquire)) {
        Err(crate::i18n::Message::new("status-export-cancelled"))
    } else {
        Ok(())
    }
}
#[derive(Clone)]
struct ExportTable {
    order: usize,
    id: TableId,
    name: String,
    rows: Vec<Vec<Cell>>,
    string_cells: BTreeSet<CellAddress>,
    /// Imported formatting for the placed cells, kept apart from the cell values.
    formats: BTreeMap<CellAddress, CellFormat>,
}

impl ExportTable {
    /// Returns the imported formatting stored for one cell, if any.
    fn format(&self, address: CellAddress) -> Option<&CellFormat> {
        self.formats.get(&address)
    }

    /// Returns the imported rich text runs for one cell, if any.
    fn rich_text(&self, address: CellAddress) -> Option<&Vec<CellTextRun>> {
        self.format(address)?.rich_text.as_ref()
    }

    /// Returns the imported link for one cell, if any.
    fn hyperlink(&self, address: CellAddress) -> Option<&CellHyperlink> {
        self.format(address)?.hyperlink.as_ref()
    }

    /// Returns a copy of the imported style for one cell, or the default style.
    fn style(&self, address: CellAddress) -> CellStyle {
        self.format(address)
            .map(|format| format.style.clone())
            .unwrap_or_default()
    }
}

mod build;
mod csv_markdown;
mod formula;
mod helpers;
mod markdown_alignment;

/// Escapes plain CSV text for a literal Markdown table cell.
pub(crate) fn escape_plain_markdown_cell(value: &str) -> String {
    helpers::escape_table_cell(&helpers::escape_markdown_text(value))
}

/// Serializes supported pasted text formatting into Markdown cell source.
pub(crate) fn markdown_cell_source_for_paste(
    cell: &Cell,
    format: Option<&CellFormat>,
    string_cell: bool,
) -> (String, bool) {
    runs::markdown_cell_source_for_paste(cell, format, string_cell)
}

/// Serializes one inline-code value only when the existing Markdown parser verifies its value.
pub(crate) fn markdown_code_markup(value: &str) -> Option<String> {
    let run = XlsxTextRun {
        text: value.to_owned(),
        code: true,
        ..XlsxTextRun::default()
    };
    let (markup, omitted, retry_limit_reached) =
        runs::markdown_runs_as_markup_checked(std::slice::from_ref(&run));
    (!omitted
        && !retry_limit_reached
        && runs::markdown_cell_markup_matches(std::slice::from_ref(&run)))
    .then_some(markup)
}

/// Rebuilds one Markdown cell while changing its first inline link.
pub(crate) fn markdown_link_cell_source(
    markdown: &MarkdownDocument,
    table: usize,
    address: CellAddress,
    hyperlink: Option<&crate::document::CellHyperlink>,
) -> Option<String> {
    let display = markdown.cell_display(table, address.row, address.column)?;
    if display.runs.iter().any(|run| run.localized_image) {
        return None;
    }
    let run_text = display
        .runs
        .iter()
        .filter_map(|run| markdown.display_run_text(run))
        .collect::<String>();
    if run_text != display.plain_text {
        return None;
    }
    let old_target = display.first_link_target.as_deref();
    let runs = display
        .runs
        .iter()
        .map(|run| {
            let replace_link =
                old_target.is_none_or(|target| run.link_target.as_deref() == Some(target));
            Some(XlsxTextRun {
                text: markdown.display_run_text(run)?.to_owned(),
                bold: run.bold,
                italic: run.italic,
                strike: run.strike,
                code: run.code,
                hyperlink: if replace_link {
                    hyperlink.cloned()
                } else {
                    run.link_target
                        .as_ref()
                        .map(|target| crate::document::CellHyperlink {
                            target: target.clone(),
                            tooltip: run.link_tooltip.clone(),
                            location: false,
                        })
                },
                font_name: None,
                font_size: None,
                font_color: None,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    if runs.is_empty() || display.plain_text.is_empty() {
        return None;
    }
    let (markup, omitted, retry_limit_reached) = runs::markdown_runs_as_markup_checked(&runs);
    let literal_markup = hyperlink.is_none()
        && (markup.starts_with('=') || crate::document::literal_text_value(&markup) != markup);
    let markup = if literal_markup {
        format!("'{markup}")
    } else {
        markup
    };
    (!omitted && !retry_limit_reached && runs::markdown_cell_markup_matches(&runs))
        .then_some(markup)
}
mod markdown_source;
mod markup;
mod materialize;
mod preflight;
mod report_entries;
use report_entries::collect_table_entries;
mod prepare;
mod reports;
mod runs;
mod selection;
mod values;

use self::build::*;
use self::csv_markdown::*;
use self::formula::*;
use self::helpers::*;
use self::markdown_alignment::*;
use self::markdown_source::*;
use self::markup::*;
use self::materialize::*;
use self::preflight::*;
pub(crate) use self::prepare::prepare_export_cancellable;
use self::reports::*;
use self::runs::*;
use self::selection::*;
use self::values::*;

#[cfg(test)]
#[path = "export_test.rs"]
mod tests;
