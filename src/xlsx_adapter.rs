//!  Converts XLSX files into Texcel sparse intermediate data at the umya-spreadsheet boundary.
//!
//!  The library Workbook is not kept as edit state. Saving builds a new one from the intermediate model,
//!  and explicitly transfers only supported styles and layout.

use std::collections::BTreeMap;
use std::io::Cursor;

use serde::{Deserialize, Serialize};
use umya_spreadsheet::{EnumTrait, Font, PaneStateValues};

use crate::document::{
    CalcResult, CalcStatus, CalcValue, Cell, CellAddress, CellBorder, CellHyperlink, CellInput,
    CellRange, CellStyle, CellTextRun, ColumnLayout, Document, FrozenPanes, LayoutTarget,
    LayoutValue, MergedRange, TableLayout,
};

// Why XLSX conversion failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XlsxError(pub crate::i18n::Message);

impl std::fmt::Display for XlsxError {
    // Show the XLSX conversion failure in a short message.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0.resolve(crate::i18n::Language::English))
    }
}

impl std::error::Error for XlsxError {}

impl From<umya_spreadsheet::XlsxError> for XlsxError {
    // Convert the XLSX library error into Texcel’s public error type.
    fn from(error: umya_spreadsheet::XlsxError) -> Self {
        Self(crate::i18n::Message::new("status-xlsx-library-error").with_text("detail", error))
    }
}

/// Holds the imported XLSX values, formulas, styles, and layout; ODS import fills the same model.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct XlsxDocument {
    pub date_1904: bool,
    pub sheets: Vec<XlsxSheet>,
}

// An intermediate sheet with its order, cells, dimensions, frozen panes, and merges.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct XlsxSheet {
    pub table_id: u64,
    pub name: String,
    #[serde(with = "crate::document::map_entries")]
    pub cells: BTreeMap<CellAddress, XlsxCell>,
    pub columns: Vec<XlsxColumn>,
    pub rows: Vec<XlsxRow>,
    pub merges: Vec<String>,
    pub frozen_rows: u32,
    pub frozen_columns: u32,
    pub freeze_top_left: String,
    /// Filters, input rules, and conditional formats that the Markdown result cannot keep.
    pub rule_losses: Vec<XlsxRuleLoss>,
}

// Stores cell type and formula result separately. Formula text is stored without the leading `=`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct XlsxCell {
    pub value: XlsxValue,
    pub formula: Option<String>,
    pub style: XlsxStyle,
    pub hyperlink: Option<CellHyperlink>,
}

// A typed value supported by XLSX.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum XlsxValue {
    Empty,
    String(String),
    RichText(Vec<XlsxTextRun>),
    Number(f64),
    Boolean(bool),
    Error(String),
}

// Text and basic character attributes kept in a partially formatted run.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct XlsxTextRun {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    #[serde(default)]
    pub code: bool,
    #[serde(default)]
    pub hyperlink: Option<CellHyperlink>,
    pub font_name: Option<String>,
    pub font_size: Option<f64>,
    pub font_color: Option<String>,
}

// Cell styles currently imported by this adapter.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct XlsxStyle {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub font_name: Option<String>,
    pub font_size: Option<f64>,
    pub font_color: Option<String>,
    pub fill_color: Option<String>,
    pub number_format: Option<String>,
    pub wrap_text: bool,
    pub horizontal_alignment: Option<String>,
    pub vertical_alignment: Option<String>,
    pub top_border: Option<XlsxBorder>,
    pub bottom_border: Option<XlsxBorder>,
    pub left_border: Option<XlsxBorder>,
    pub right_border: Option<XlsxBorder>,
}

/// A supported border side with its OOXML line style and optional color.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct XlsxBorder {
    pub style: String,
    pub color: Option<String>,
}

// Column width and hidden state.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct XlsxColumn {
    pub index: u32,
    pub width: f64,
    pub hidden: bool,
}

// Row height and hidden state.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct XlsxRow {
    pub index: u32,
    pub height: f64,
    pub height_present: bool,
    pub custom_height: bool,
    pub custom_height_present: bool,
    pub hidden: bool,
}

mod document;
mod parse;
mod references;
mod rule_scan;
mod style;
mod values;

use self::references::*;
use self::rule_scan::scan_sheet_rules;
pub use self::rule_scan::{XlsxRuleKind, XlsxRuleLoss};
use self::style::*;
use self::values::*;

pub(crate) use self::parse::emit_preview_rows;
pub(crate) use self::references::{cell_reference, parse_cell_reference};
pub(crate) use self::style::xlsx_style;

#[cfg(test)]
#[path = "xlsx_adapter_test.rs"]
pub(crate) mod tests;
