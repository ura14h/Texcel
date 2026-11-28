use super::*;

impl XlsxDocument {
    // Converts the intermediate model to a document and marks saved formula values as unverified imports.
    pub fn to_document(&self, document_id: u64, name: impl Into<String>) -> Document {
        self.to_document_checked(document_id, name, || true)
            .expect("unconditional XLSX conversion succeeds")
    }

    // Converts workbook sheets while allowing the caller to stop long imports.
    fn to_document_checked(
        &self,
        document_id: u64,
        name: impl Into<String>,
        mut keep_going: impl FnMut() -> bool,
    ) -> Result<Document, crate::i18n::Message> {
        if !keep_going() {
            return Err(crate::i18n::Message::new("status-operation-cancelled"));
        }
        let table_names = self
            .sheets
            .iter()
            .map(|sheet| sheet.name.as_str())
            .collect::<Vec<_>>();
        let mut document = Document::new(document_id, name, &table_names);
        for (index, sheet) in self.sheets.iter().enumerate() {
            if !keep_going() {
                return Err(crate::i18n::Message::new("status-operation-cancelled"));
            }
            let Some(table) = document.tables.get_mut(index) else {
                continue;
            };
            let generated_id = table.id;
            table.id = sheet.table_id;
            if generated_id != sheet.table_id
                && let Some(view) = document.views.remove(&generated_id)
            {
                document.views.insert(sheet.table_id, view);
            }
        }
        if let Some(first) = self.sheets.first() {
            document.active_table = first.table_id;
        }
        for (index, sheet) in self.sheets.iter().enumerate() {
            if !keep_going() {
                return Err(crate::i18n::Message::new("status-operation-cancelled"));
            }
            let Some(table) = document.tables.get_mut(index) else {
                continue;
            };
            let mut layout = TableLayout::default();
            let mut row_count = sheet
                .cells
                .keys()
                .map(|address| address.row + 1)
                .max()
                .unwrap_or(1);
            let mut column_count = sheet
                .cells
                .keys()
                .map(|address| address.column + 1)
                .max()
                .unwrap_or(1);
            for row in &sheet.rows {
                let index = usize::try_from(row.index.saturating_sub(1)).unwrap_or(0);
                row_count = row_count.max(index.saturating_add(1));
            }
            // Markdown cannot keep row heights, so the import report only needs to know they existed.
            if !sheet.rows.is_empty() {
                layout.mark_imported_row_layout();
            }
            for column in &sheet.columns {
                if !keep_going() {
                    return Err(crate::i18n::Message::new("status-operation-cancelled"));
                }
                let index = usize::try_from(column.index.saturating_sub(1)).unwrap_or(0);
                column_count = column_count.max(index.saturating_add(1));
                layout.apply_value(
                    LayoutTarget::Column(index),
                    LayoutValue::Column(Some(ColumnLayout {
                        width: Some(column.width),
                        fitted_on_opening: false,
                    })),
                );
            }
            for merge in &sheet.merges {
                if !keep_going() {
                    return Err(crate::i18n::Message::new("status-operation-cancelled"));
                }
                if let Some(range) = parse_merged_range(merge) {
                    row_count = row_count.max(range.end.row.saturating_add(1));
                    column_count = column_count.max(range.end.column.saturating_add(1));
                    layout.apply_value(LayoutTarget::Merge(range), LayoutValue::Merge(true));
                }
            }
            row_count = row_count.max(sheet.frozen_rows as usize);
            column_count = column_count.max(sheet.frozen_columns as usize);
            if (sheet.frozen_rows > 0 || sheet.frozen_columns > 0)
                && let Some(top_left) = parse_cell_reference(&sheet.freeze_top_left)
            {
                row_count = row_count.max(top_left.row.saturating_add(1));
                column_count = column_count.max(top_left.column.saturating_add(1));
            }
            layout.apply_value(
                LayoutTarget::Freeze,
                LayoutValue::Freeze(FrozenPanes {
                    rows: sheet.frozen_rows as usize,
                    columns: sheet.frozen_columns as usize,
                }),
            );
            table.set_dimensions(row_count, column_count);
            table.set_loaded_layout(layout);
            for (address, source) in &sheet.cells {
                if !keep_going() {
                    return Err(crate::i18n::Message::new("status-operation-cancelled"));
                }
                let input = if let Some(formula) = &source.formula {
                    CellInput::Formula(format!("={formula}"))
                } else {
                    match &source.value {
                        XlsxValue::Empty => CellInput::Empty,
                        XlsxValue::String(value) => CellInput::Text(value.clone()),
                        XlsxValue::RichText(runs) => CellInput::Text(
                            runs.iter().map(|run| run.text.as_str()).collect::<String>(),
                        ),
                        XlsxValue::Number(value) => CellInput::Number(*value),
                        XlsxValue::Boolean(value) => CellInput::Boolean(*value),
                        XlsxValue::Error(value) => CellInput::Error(value.clone()),
                    }
                };
                let imported_result = source
                    .formula
                    .as_ref()
                    .and_then(|_| value_as_calculation(&source.value));
                let calculation = imported_result.as_ref().cloned().map(|value| CalcResult {
                    revision: 0,
                    status: CalcStatus::Imported,
                    value: Some(value),
                });
                let rich_text = match (&source.formula, &source.value) {
                    (None, XlsxValue::RichText(runs)) => Some(
                        runs.iter()
                            .map(|run| CellTextRun {
                                text: run.text.clone(),
                                bold: run.bold,
                                italic: run.italic,
                                strike: run.strike,
                                code: run.code,
                                hyperlink: run.hyperlink.clone(),
                                font_name: run.font_name.clone(),
                                font_size: run.font_size,
                                font_color: run.font_color.clone(),
                            })
                            .collect(),
                    ),
                    _ => None,
                };
                table.set_loaded_cell(
                    *address,
                    Cell {
                        input,
                        imported_result,
                        calculation,
                    },
                );
                table.set_loaded_format(
                    *address,
                    crate::document::CellFormat {
                        style: document_style(&source.style),
                        hyperlink: source.hyperlink.clone(),
                        rich_text,
                    },
                );
            }
        }
        Ok(document)
    }
}
