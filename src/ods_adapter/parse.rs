use std::collections::BTreeSet;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use super::cell::{CellState, convert_cell, start_text_element};
use super::cell_text::CellText;
use super::style::{StyleSheet, attributes, xml_error};
use super::*;

/// Elements whose content Markdown cannot hold; they are reported and their content skipped.
const REPORTED_ELEMENTS: &[&str] = &[
    "office:annotation",
    "table:content-validations",
    "table:named-expressions",
    "table:dde-links",
    "table:tracked-changes",
    "table:data-pilot-tables",
    "calcext:conditional-formats",
    "table:scenario",
    "table:table-source",
    "table:shapes",
];
/// Elements skipped without a report because they hold nothing a user sees in cells.
const SILENT_ELEMENTS: &[&str] = &["office:forms", "table:database-ranges"];

/// Reads every sheet of `content.xml`; `Ok(None)` means the progress callback cancelled.
pub(super) fn parse_content(
    content: &[u8],
    styles: &StyleSheet,
    mut progress: impl FnMut(usize, &str, usize, &[(usize, String)]) -> bool,
) -> Result<Option<OdsImport>, Message> {
    let mut reader = Reader::from_reader(content);
    let mut walker = Walker {
        styles,
        sheets: Vec::new(),
        entries: Vec::new(),
        cell_total: 0,
        skip_depth: 0,
        reported: BTreeSet::new(),
        sheet: None,
        row: None,
        cell: None,
    };
    loop {
        let event = reader
            .read_event()
            .map_err(|error| xml_error("content.xml", &error))?;
        let finished = match event {
            Event::Eof => break,
            Event::Start(element) => {
                walker.start(&element, true)?;
                None
            }
            Event::Empty(element) => {
                walker.start(&element, false)?;
                walker.end(element.name().as_ref())?
            }
            Event::End(element) => walker.end(element.name().as_ref())?,
            Event::Text(text) => {
                let text = text
                    .xml10_content()
                    .map_err(|error| xml_error("content.xml", &error))?;
                walker.text(&text);
                None
            }
            Event::CData(text) => {
                walker.text(&String::from_utf8_lossy(&text));
                None
            }
            Event::GeneralRef(reference) => {
                let character = match reference
                    .resolve_char_ref()
                    .map_err(|error| xml_error("content.xml", &error))?
                {
                    Some(character) => character.to_string(),
                    None => {
                        let entity = reference
                            .decode()
                            .map_err(|error| xml_error("content.xml", &error))?;
                        quick_xml::escape::resolve_xml_entity(&entity)
                            .unwrap_or_default()
                            .to_owned()
                    }
                };
                walker.text(&character);
                None
            }
            _ => None,
        };
        if let Some(index) = finished {
            let sheet = &walker.sheets[index];
            if !crate::xlsx_adapter::emit_preview_rows(
                index,
                &sheet.name,
                &sheet.cells,
                &mut progress,
            ) {
                return Ok(None);
            }
        }
    }
    if walker.sheets.is_empty() {
        return Err(Message::new("status-ods-no-sheets"));
    }
    Ok(Some(OdsImport {
        workbook: XlsxDocument {
            date_1904: false,
            sheets: walker.sheets,
        },
        entries: walker.entries,
    }))
}

/// Walk state for the current sheet, row, and cell.
struct Walker<'a> {
    styles: &'a StyleSheet,
    sheets: Vec<XlsxSheet>,
    entries: Vec<ConversionEntry>,
    cell_total: usize,
    /// Depth inside an element whose content is skipped.
    skip_depth: usize,
    reported: BTreeSet<(Option<usize>, String)>,
    sheet: Option<SheetState>,
    row: Option<RowState>,
    cell: Option<CellState>,
}

/// One `table:table` being read.
#[derive(Default)]
struct SheetState {
    name: String,
    cells: BTreeMap<CellAddress, XlsxCell>,
    merges: Vec<String>,
    next_row: usize,
    next_column: usize,
    /// Column ranges with their default cell style.
    column_styles: Vec<(usize, usize, String)>,
    hidden_columns: Vec<(usize, usize)>,
    hidden_rows: Vec<(usize, usize)>,
}

/// One `table:table-row`, which may stand for several identical rows.
#[derive(Default)]
struct RowState {
    repeat: usize,
    hidden: bool,
    default_style: Option<String>,
    next_column: usize,
    cells: Vec<(usize, XlsxCell)>,
    /// Column, row span, and column span of cells that start a merged area.
    spans: Vec<(usize, usize, usize)>,
}

impl Walker<'_> {
    /// Handles an opening tag; `has_body` is false for self-closing tags.
    fn start(&mut self, element: &BytesStart<'_>, has_body: bool) -> Result<(), Message> {
        let name = String::from_utf8_lossy(element.name().as_ref()).into_owned();
        if self.skip_depth > 0 {
            if has_body {
                self.skip_depth += 1;
            }
            if name == "table:filter" {
                self.report("table:filter");
            }
            return Ok(());
        }
        let skipped_reported = REPORTED_ELEMENTS.contains(&name.as_str())
            || (name.starts_with("draw:") && self.cell.is_some());
        if skipped_reported || SILENT_ELEMENTS.contains(&name.as_str()) {
            // Empty containers such as `<table:named-expressions/>` hold nothing to lose.
            if has_body {
                if skipped_reported {
                    self.report(&name);
                }
                self.skip_depth = 1;
            }
            return Ok(());
        }
        let attributes = attributes(element, "content.xml")?;
        match name.as_str() {
            "table:table" => self.start_sheet(&attributes),
            "table:table-column" => self.start_column(&attributes)?,
            "table:table-row" => self.start_row(&attributes)?,
            "table:table-cell" | "table:covered-table-cell" => {
                self.cell = Some(CellState {
                    attributes,
                    text: CellText::default(),
                });
            }
            _ => {
                if let Some(cell) = self.cell.as_mut() {
                    start_text_element(&name, &attributes, self.styles, &mut cell.text);
                }
            }
        }
        Ok(())
    }

    /// Handles a closing tag; returns the index of a sheet that just finished.
    fn end(&mut self, name: &[u8]) -> Result<Option<usize>, Message> {
        if self.skip_depth > 0 {
            self.skip_depth -= 1;
            return Ok(None);
        }
        match name {
            b"table:table" => return Ok(self.finish_sheet()),
            b"table:table-row" => self.finish_row()?,
            b"table:table-cell" | b"table:covered-table-cell" => self.finish_cell()?,
            _ => {
                if let Some(cell) = self.cell.as_mut() {
                    match name {
                        b"text:p" | b"text:h" => cell.text.end_paragraph(),
                        b"text:span" => cell.text.pop_span(),
                        b"text:a" => cell.text.set_link(None),
                        _ => {}
                    }
                }
            }
        }
        Ok(None)
    }

    /// Adds character data to the current cell; text outside cells is ignored.
    fn text(&mut self, text: &str) {
        if self.skip_depth == 0
            && let Some(cell) = self.cell.as_mut()
        {
            cell.text.text(text);
        }
    }

    /// Records one lost element per sheet in the report.
    fn report(&mut self, element: &str) {
        let index = self.sheet.as_ref().map(|_| self.sheets.len());
        if self.reported.insert((index, element.to_owned())) {
            let name = self.sheet.as_ref().map(|sheet| sheet.name.as_str());
            self.entries.push(omitted_entry(index.zip(name), element));
        }
    }

    /// Opens a sheet and reports when its style hides it.
    fn start_sheet(&mut self, attributes: &BTreeMap<String, String>) {
        self.sheet = Some(SheetState {
            name: attributes.get("table:name").cloned().unwrap_or_default(),
            ..SheetState::default()
        });
        if attributes
            .get("table:style-name")
            .is_some_and(|style| self.styles.hides_table(style))
        {
            self.report("table:display");
        }
    }

    /// Records a column's default cell style and hidden state.
    fn start_column(&mut self, attributes: &BTreeMap<String, String>) -> Result<(), Message> {
        let Some(sheet) = self.sheet.as_mut() else {
            return Ok(());
        };
        let repeat = repeat_count(attributes, "table:number-columns-repeated");
        let first = sheet.next_column;
        let last = first.saturating_add(repeat - 1);
        if let Some(style) = attributes.get("table:default-cell-style-name") {
            sheet.column_styles.push((first, last, style.clone()));
        }
        if is_hidden(attributes) {
            sheet.hidden_columns.push((first, last));
        }
        sheet.next_column = last.saturating_add(1);
        Ok(())
    }

    /// Opens a row, which may repeat.
    fn start_row(&mut self, attributes: &BTreeMap<String, String>) -> Result<(), Message> {
        self.row = Some(RowState {
            repeat: repeat_count(attributes, "table:number-rows-repeated"),
            hidden: is_hidden(attributes),
            default_style: attributes.get("table:default-cell-style-name").cloned(),
            ..RowState::default()
        });
        Ok(())
    }

    /// Converts the finished cell and places its copies in the row.
    fn finish_cell(&mut self) -> Result<(), Message> {
        let (Some(cell), Some(row), Some(sheet)) =
            (self.cell.take(), self.row.as_mut(), self.sheet.as_ref())
        else {
            return Ok(());
        };
        let repeat = repeat_count(&cell.attributes, "table:number-columns-repeated");
        let first_column = row.next_column;
        row.next_column = first_column.saturating_add(repeat);
        let style_name = cell
            .attributes
            .get("table:style-name")
            .or(row.default_style.as_ref())
            .or_else(|| {
                sheet
                    .column_styles
                    .iter()
                    .find(|(first, last, _)| (*first..=*last).contains(&first_column))
                    .map(|(_, _, style)| style)
            });
        let style = self.styles.cell_style(style_name.map(String::as_str));
        let row_index = sheet.next_row;
        let address = CellAddress {
            row: row_index,
            column: first_column,
        };
        let sheet_ref = Some((self.sheets.len(), sheet.name.as_str()));
        let (converted, fallback) = convert_cell(cell, style, sheet_ref, address);
        if let Some(entry) = fallback {
            self.entries.push(entry);
        }
        let Some((converted, spans)) = converted else {
            return Ok(());
        };
        if first_column.saturating_add(repeat) > MAX_COLUMNS {
            return Err(Message::new("status-ods-sheet-too-large"));
        }
        for offset in 0..repeat {
            if let Some(spans) = spans {
                row.spans.push((first_column + offset, spans.0, spans.1));
            }
            if let Some(cell) = converted.clone() {
                row.cells.push((first_column + offset, cell));
            }
        }
        Ok(())
    }

    /// Copies the finished row into the sheet as many times as it repeats.
    fn finish_row(&mut self) -> Result<(), Message> {
        let (Some(row), Some(sheet)) = (self.row.take(), self.sheet.as_mut()) else {
            return Ok(());
        };
        let first = sheet.next_row;
        let last = first.saturating_add(row.repeat - 1);
        if row.hidden {
            sheet.hidden_rows.push((first, last));
        }
        if !row.cells.is_empty() || !row.spans.is_empty() {
            let added = row.cells.len().saturating_mul(row.repeat);
            self.cell_total = self.cell_total.saturating_add(added);
            if self.cell_total > MAX_CELLS {
                return Err(Message::new("status-ods-too-many-cells"));
            }
            if last >= MAX_ROWS {
                return Err(Message::new("status-ods-sheet-too-large"));
            }
            for row_index in first..=last {
                for (column, cell) in &row.cells {
                    sheet.cells.insert(
                        CellAddress {
                            row: row_index,
                            column: *column,
                        },
                        cell.clone(),
                    );
                }
                for (column, rows, columns) in &row.spans {
                    let start = CellAddress {
                        row: row_index,
                        column: *column,
                    };
                    let end = CellAddress {
                        row: row_index.saturating_add(rows - 1).min(MAX_ROWS - 1),
                        column: column.saturating_add(columns - 1).min(MAX_COLUMNS - 1),
                    };
                    sheet.merges.push(format!(
                        "{}:{}",
                        crate::xlsx_adapter::cell_reference(start),
                        crate::xlsx_adapter::cell_reference(end)
                    ));
                }
            }
        }
        sheet.next_row = last.saturating_add(1);
        Ok(())
    }

    /// Stores the finished sheet with hidden rows and columns clipped to its content.
    fn finish_sheet(&mut self) -> Option<usize> {
        let sheet = self.sheet.take()?;
        let index = self.sheets.len();
        let mut end = sheet
            .cells
            .keys()
            .fold(None::<CellAddress>, |end, address| {
                Some(end.map_or(*address, |end| CellAddress {
                    row: end.row.max(address.row),
                    column: end.column.max(address.column),
                }))
            });
        for merge in &sheet.merges {
            if let Some((_, last)) = merge.split_once(':')
                && let Some(last) = crate::xlsx_adapter::parse_cell_reference(last)
            {
                let current = end.unwrap_or(last);
                end = Some(CellAddress {
                    row: current.row.max(last.row),
                    column: current.column.max(last.column),
                });
            }
        }
        let end = end.unwrap_or_default();
        // Only hidden rows and columns inside the content matter; the rest repeat to the sheet edge.
        let rows = clipped(&sheet.hidden_rows, end.row)
            .map(|row| XlsxRow {
                index: row as u32 + 1,
                height: 0.0,
                height_present: false,
                custom_height: false,
                custom_height_present: false,
                hidden: true,
            })
            .collect();
        let columns = clipped(&sheet.hidden_columns, end.column)
            .map(|column| XlsxColumn {
                index: column as u32 + 1,
                width: 8.43,
                hidden: true,
            })
            .collect();
        self.sheets.push(XlsxSheet {
            table_id: index as u64 + 1,
            name: sheet.name,
            cells: sheet.cells,
            columns,
            rows,
            merges: sheet.merges,
            frozen_rows: 0,
            frozen_columns: 0,
            freeze_top_left: String::new(),
            rule_losses: Vec::new(),
        });
        Some(index)
    }
}

/// Reads a repeat attribute, treating missing or invalid values as one.
fn repeat_count(attributes: &BTreeMap<String, String>, key: &str) -> usize {
    attributes
        .get(key)
        .and_then(|count| count.parse::<usize>().ok())
        .unwrap_or(1)
        .clamp(1, MAX_ROWS)
}

/// Says whether a row or column is collapsed or filtered out.
fn is_hidden(attributes: &BTreeMap<String, String>) -> bool {
    matches!(
        attributes.get("table:visibility").map(String::as_str),
        Some("collapse" | "filter")
    )
}

/// Lists the hidden indexes up to `last`, without expanding ranges past it.
fn clipped(ranges: &[(usize, usize)], last: usize) -> impl Iterator<Item = usize> + '_ {
    ranges
        .iter()
        .filter(move |(first, _)| *first <= last)
        .flat_map(move |(first, end)| *first..=(*end).min(last))
}
