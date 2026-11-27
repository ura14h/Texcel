use super::*;

impl XlsxDocument {
    /// Parses a workbook once and reports each converted sheet as row chunks.
    pub(crate) fn parse_with_progress(
        bytes: &[u8],
        document_id: u64,
        mut progress: impl FnMut(usize, &str, usize, &[(usize, String)]) -> bool,
    ) -> Result<Option<Self>, XlsxError> {
        let workbook = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            umya_spreadsheet::reader::xlsx::read_reader(Cursor::new(bytes), true)
        }))
        .map_err(|_| XlsxError(crate::i18n::Message::new("status-xlsx-parser-panicked")))??;
        let theme = workbook.theme();
        let mut cancelled = false;
        let converted = workbook
            .sheet_collection()
            .iter()
            .enumerate()
            .map(|(index, sheet)| {
                if cancelled {
                    return Ok(None);
                }
                let mut cells = BTreeMap::new();
                for cell in sheet.cells() {
                    let coordinate = cell.coordinate();
                    let row =
                        usize::try_from(coordinate.row_num().saturating_sub(1)).map_err(|_| {
                            XlsxError(crate::i18n::Message::new("status-xlsx-row-index-invalid"))
                        })?;
                    let column =
                        usize::try_from(coordinate.col_num().saturating_sub(1)).map_err(|_| {
                            XlsxError(crate::i18n::Message::new(
                                "status-xlsx-column-index-invalid",
                            ))
                        })?;
                    let address = CellAddress { row, column };
                    let raw = cell.cell_value().raw_value();
                    let value = match raw {
                        umya_spreadsheet::CellRawValue::Empty => XlsxValue::Empty,
                        umya_spreadsheet::CellRawValue::String(value)
                        | umya_spreadsheet::CellRawValue::Lazy(value) => {
                            XlsxValue::String(value.to_string())
                        }
                        umya_spreadsheet::CellRawValue::RichText(rich_text) => XlsxValue::RichText(
                            rich_text
                                .rich_text_elements()
                                .iter()
                                .map(|run| {
                                    let font = run.run_properties();
                                    XlsxTextRun {
                                        text: run.text().to_owned(),
                                        bold: font.is_some_and(Font::bold),
                                        italic: font.is_some_and(Font::italic),
                                        strike: font.is_some_and(Font::strikethrough),
                                        code: false,
                                        hyperlink: None,
                                        font_name: font
                                            .map(Font::name)
                                            .filter(|name| !name.is_empty())
                                            .map(str::to_owned),
                                        font_size: font
                                            .map(Font::size)
                                            .filter(|size| size.is_finite() && *size > 0.0),
                                        font_color: font.and_then(|font| font_color(font, theme)),
                                    }
                                })
                                .collect(),
                        ),
                        umya_spreadsheet::CellRawValue::Numeric(value) => XlsxValue::Number(*value),
                        umya_spreadsheet::CellRawValue::Bool(value) => XlsxValue::Boolean(*value),
                        umya_spreadsheet::CellRawValue::Error(value) => {
                            XlsxValue::Error(value.to_string())
                        }
                    };
                    let style = cell.style();
                    let font = style.font();
                    let formula = (!cell.cell_value().formula().is_empty())
                        .then(|| cell.cell_value().formula().to_owned());
                    let style = XlsxStyle {
                        bold: font.is_some_and(Font::bold),
                        italic: font.is_some_and(Font::italic),
                        strike: font.is_some_and(Font::strikethrough),
                        font_name: font
                            .map(Font::name)
                            .filter(|name| !name.is_empty())
                            .map(str::to_owned),
                        font_size: font
                            .map(Font::size)
                            .filter(|size| size.is_finite() && *size > 0.0),
                        font_color: font.and_then(|font| font_color(font, theme)),
                        fill_color: style
                            .background_color()
                            .map(|color| color.argb_with_theme(theme).into_owned()),
                        number_format: style
                            .numbering_format()
                            .map(|format| format.format_code().to_owned()),
                        wrap_text: style
                            .alignment()
                            .is_some_and(|alignment| alignment.wrap_text()),
                        horizontal_alignment: style
                            .alignment()
                            .map(|alignment| alignment.horizontal().value_string().to_owned())
                            .filter(|alignment| alignment != "general"),
                        vertical_alignment: style
                            .alignment()
                            .map(|alignment| alignment.vertical().value_string().to_owned())
                            .filter(|alignment| alignment != "bottom"),
                        top_border: style
                            .borders()
                            .and_then(|borders| border_from(borders.top_border(), theme)),
                        bottom_border: style
                            .borders()
                            .and_then(|borders| border_from(borders.bottom_border(), theme)),
                        left_border: style
                            .borders()
                            .and_then(|borders| border_from(borders.left_border(), theme)),
                        right_border: style
                            .borders()
                            .and_then(|borders| border_from(borders.right_border(), theme)),
                    };
                    let hyperlink = cell.hyperlink().map(|link| CellHyperlink {
                        target: link.url().to_owned(),
                        tooltip: (!link.tooltip().is_empty()).then(|| link.tooltip().to_owned()),
                        location: link.location(),
                    });
                    if value != XlsxValue::Empty
                        || formula.is_some()
                        || style != XlsxStyle::default()
                        || hyperlink.is_some()
                    {
                        cells.insert(
                            address,
                            XlsxCell {
                                value,
                                formula,
                                style,
                                hyperlink,
                            },
                        );
                    }
                }
                let mut columns: Vec<XlsxColumn> = sheet
                    .column_dimensions()
                    .iter()
                    .map(|column| XlsxColumn {
                        index: column.col_num(),
                        width: column.width(),
                        hidden: column.hidden(),
                    })
                    .collect();
                if columns.iter().any(|column| {
                    column.index == 0
                        || column.index as usize > crate::document::MAX_COLUMNS
                        || !column.width.is_finite()
                        || column.width < 0.0
                }) {
                    return Err(XlsxError(crate::i18n::Message::new(
                        "status-xlsx-column-dimension-invalid",
                    )));
                }
                columns.sort_by_key(|column: &XlsxColumn| column.index);
                if !emit_preview_rows(index, sheet.name(), &cells, &mut progress) {
                    cancelled = true;
                    return Ok(None);
                }
                let mut rows: Vec<XlsxRow> = sheet
                    .row_dimensions()
                    .into_iter()
                    .filter(|row| row.has_height() || row.hidden() || row.has_custom_height())
                    .map(|row| XlsxRow {
                        index: row.row_num(),
                        height: row.height(),
                        height_present: row.has_height(),
                        custom_height: row.custom_height(),
                        custom_height_present: row.has_custom_height(),
                        hidden: row.hidden(),
                    })
                    .collect();
                if rows.iter().any(|row| {
                    row.index == 0
                        || row.index as usize > crate::document::MAX_ROWS
                        || !row.height.is_finite()
                        || row.height < 0.0
                }) {
                    return Err(XlsxError(crate::i18n::Message::new(
                        "status-xlsx-row-dimension-invalid",
                    )));
                }
                rows.sort_by_key(|row: &XlsxRow| row.index);
                let pane = sheet
                    .sheets_views()
                    .sheet_view_list()
                    .first()
                    .and_then(|view| view.pane());
                let frozen = pane.filter(|pane| {
                    matches!(
                        pane.state(),
                        PaneStateValues::Frozen | PaneStateValues::FrozenSplit
                    )
                });
                let vertical_split = frozen.map_or(0.0, |pane| pane.vertical_split());
                let horizontal_split = frozen.map_or(0.0, |pane| pane.horizontal_split());
                if !vertical_split.is_finite()
                    || !horizontal_split.is_finite()
                    || vertical_split < 0.0
                    || horizontal_split < 0.0
                    || vertical_split > crate::document::MAX_ROWS as f64
                    || horizontal_split > crate::document::MAX_COLUMNS as f64
                    || vertical_split.fract() != 0.0
                    || horizontal_split.fract() != 0.0
                {
                    return Err(XlsxError(crate::i18n::Message::new(
                        "status-xlsx-frozen-range-invalid",
                    )));
                }
                let frozen_rows = vertical_split as u32;
                let frozen_columns = horizontal_split as u32;
                let mut freeze_top_left = pane
                    .filter(|pane| pane.has_top_left_cell())
                    .map_or_else(String::new, |pane| pane.top_left_cell().to_string());
                if frozen.is_some() {
                    let top_left = if freeze_top_left.is_empty() {
                        cell_reference(CellAddress {
                            row: frozen_rows as usize,
                            column: frozen_columns as usize,
                        })
                    } else {
                        freeze_top_left.clone()
                    };
                    let address = parse_cell_reference(&top_left).ok_or_else(|| {
                        XlsxError(crate::i18n::Message::new("status-xlsx-frozen-cell-invalid"))
                    })?;
                    if address.row < frozen_rows as usize
                        || address.column < frozen_columns as usize
                    {
                        return Err(XlsxError(crate::i18n::Message::new(
                            "status-xlsx-frozen-cell-inside-range",
                        )));
                    }
                    freeze_top_left = cell_reference(address);
                }
                let mut merges: Vec<String> = sheet
                    .merge_cells()
                    .iter()
                    .map(|range| range.range())
                    .collect();
                merges.sort();
                for merge in &merges {
                    parse_merged_range(merge).ok_or_else(|| {
                        XlsxError(
                            crate::i18n::Message::new("status-xlsx-merge-range-invalid")
                                .with_text("range", merge),
                        )
                    })?;
                }
                Ok(Some(XlsxSheet {
                    table_id: index as u64 + 1,
                    name: sheet.name().to_owned(),
                    cells,
                    columns,
                    rows,
                    merges,
                    frozen_rows,
                    frozen_columns,
                    freeze_top_left,
                    rule_losses: scan_sheet_rules(sheet),
                }))
            })
            .collect::<Result<Vec<_>, XlsxError>>()?;
        if cancelled {
            return Ok(None);
        }
        let sheets = converted.into_iter().flatten().collect::<Vec<_>>();
        if sheets.is_empty() {
            return Err(XlsxError(crate::i18n::Message::new(
                "status-xlsx-no-worksheets",
            )));
        }
        let _ = document_id;
        Ok(Some(Self {
            date_1904: workbook.date1904(),
            sheets,
        }))
    }
}

/// Sends sparse cell rows from one converted sheet to the preview worker.
pub(crate) fn emit_preview_rows(
    table_index: usize,
    table_name: &str,
    cells: &BTreeMap<CellAddress, XlsxCell>,
    progress: &mut impl FnMut(usize, &str, usize, &[(usize, String)]) -> bool,
) -> bool {
    let mut row_index = None;
    let mut row_cells = Vec::new();
    for (address, cell) in cells {
        if row_index.is_some_and(|current| current != address.row) {
            if !progress(
                table_index,
                table_name,
                row_index.unwrap_or_default(),
                &row_cells,
            ) {
                return false;
            }
            row_cells.clear();
        }
        row_index = Some(address.row);
        row_cells.push((address.column, preview_cell_text(cell)));
    }
    match row_index {
        Some(index) => progress(table_index, table_name, index, &row_cells),
        None => progress(table_index, table_name, 0, &[]),
    }
}

/// Shows formula source or the parsed value without changing the final typed cell.
fn preview_cell_text(cell: &XlsxCell) -> String {
    if let Some(formula) = &cell.formula {
        return format!("={formula}");
    }
    match &cell.value {
        XlsxValue::Empty => String::new(),
        XlsxValue::String(value) | XlsxValue::Error(value) => value.clone(),
        XlsxValue::RichText(runs) => runs.iter().map(|run| run.text.as_str()).collect(),
        XlsxValue::Number(value) => value.to_string(),
        XlsxValue::Boolean(value) => value.to_string(),
    }
}

#[cfg(test)]
impl XlsxDocument {
    // Reads ZIP bytes with the library and moves supported data into a sparse intermediate model.
    pub fn parse(bytes: &[u8], document_id: u64) -> Result<Self, XlsxError> {
        // The always-accept callback cannot produce the cancelled outcome.
        match Self::parse_with_progress(bytes, document_id, |_, _, _, _| true)? {
            Some(source) => Ok(source),
            None => unreachable!("the parser callback always accepts rows"),
        }
    }
}
