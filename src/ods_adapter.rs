//!  Reads OpenDocument spreadsheets (.ods) into the sheet model shared with XLSX import.
//!
//!  Only `content.xml` and `styles.xml` are read, with the existing ZIP and XML readers.
//!  Formulas are rewritten from OpenFormula into the A1 syntax the calculation engine parses,
//!  and everything Markdown cannot keep is reported instead of preserved.

use std::collections::BTreeMap;

use crate::conversion_report::{ConversionClass, ConversionEntry};
use crate::document::{CellAddress, CellHyperlink, MAX_COLUMNS, MAX_ROWS};
use crate::i18n::Message;
use crate::xlsx_adapter::{
    XlsxCell, XlsxColumn, XlsxDocument, XlsxRow, XlsxSheet, XlsxStyle, XlsxTextRun, XlsxValue,
};

mod cell;
mod cell_text;
mod formula;
mod package;
mod parse;
mod style;
mod values;

pub(crate) use self::formula::convert_open_formula;

/// Most cells with content one workbook may hold, matching the XLSX and text limits.
const MAX_CELLS: usize = 500_000;

/// The imported sheets plus the losses found while reading the file.
#[derive(Debug)]
pub(crate) struct OdsImport {
    pub(crate) workbook: XlsxDocument,
    pub(crate) entries: Vec<ConversionEntry>,
}

/// Parses an ODS file, sending each sheet's rows to `progress`; `Ok(None)` means cancelled.
pub(crate) fn parse_ods_with_progress(
    bytes: &[u8],
    progress: impl FnMut(usize, &str, usize, &[(usize, String)]) -> bool,
) -> Result<Option<OdsImport>, Message> {
    let parts = package::read_package(bytes)?;
    let styles = style::StyleSheet::parse(&parts.styles, &parts.content)?;
    parse::parse_content(&parts.content, &styles, progress)
}

/// Parses an ODS file without progress reporting.
#[cfg(test)]
pub(crate) fn parse_ods(bytes: &[u8]) -> Result<OdsImport, Message> {
    match parse_ods_with_progress(bytes, |_, _, _, _| true)? {
        Some(import) => Ok(import),
        None => unreachable!("the progress callback always continues"),
    }
}

/// Builds one report line for content the ODS import leaves out.
fn omitted_entry(sheet: Option<(usize, &str)>, element: &str) -> ConversionEntry {
    ConversionEntry::new(
        ConversionClass::Omitted,
        sheet.map(|(index, _)| index + 1),
        sheet.map(|(_, name)| name.to_owned()),
        None,
        "This ODS element is not carried over.",
    )
    .with_message(
        Message::new("report-reason-ods-unsupported-element").with_text("element", element),
    )
}

#[cfg(test)]
#[path = "ods_adapter_test.rs"]
mod tests;
