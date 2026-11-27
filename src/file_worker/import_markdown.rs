use super::*;

use crate::export::ExportSource;

#[cfg(test)]
#[path = "import_markdown_test.rs"]
mod tests;

mod formulas;
use self::formulas::*;

/// Converts one import-only source into the Markdown model used by the editor.
pub(crate) fn html_to_markdown(
    source: Arc<HtmlDocument>,
    document_id: u64,
    name: String,
    language: crate::i18n::Language,
    cancelled: &AtomicBool,
) -> io::Result<(
    Arc<MarkdownDocument>,
    crate::document::Document,
    Arc<ImportReport>,
)> {
    let document = source.to_document_with_language(document_id, name.clone(), language);
    let mut changes = Vec::new();
    let mut format_changes = Vec::new();
    for (table_index, table) in document.tables.iter().enumerate() {
        for (address, original) in table.stored_cells() {
            let Some(source_cell) = source
                .tables
                .get(table_index)
                .and_then(|source_table| source_table.rows.get(address.row))
                .and_then(|row| row.get(address.column))
            else {
                continue;
            };
            let mut cell = original.clone();
            cell.input = crate::document::CellInput::Text(crate::document::encode_literal_text(
                &source_cell.display_value,
            ));
            changes.push((table.id, address, Some(cell)));
            // Keep the cell's alignment, but replace its link and runs with the plain display text.
            let mut format = table.format(address).cloned().unwrap_or_default();
            format.hyperlink = None;
            format.rich_text = Some(source_cell.rich_text.clone().unwrap_or_else(|| {
                vec![crate::document::CellTextRun {
                    text: source_cell.display_value.clone(),
                    ..crate::document::CellTextRun::default()
                }]
            }));
            format_changes.push((table.id, address, format));
        }
    }
    let entries = html_diagnostic_entries(&source);
    let imported = ImportedTables {
        source: ExportSource::Html(source),
        document: &document,
        changes,
        format_changes,
        entries,
        input_format: "HTML",
    };
    markdown_from_import(document_id, name, imported, language, cancelled)
}

/// Converts an imported workbook and reports only losses from its Markdown result.
pub(crate) fn workbook_to_markdown(
    source: Arc<XlsxDocument>,
    document_id: u64,
    name: String,
    input_format: &'static str,
    language: crate::i18n::Language,
    mut entries: Vec<ConversionEntry>,
    cancelled: &AtomicBool,
) -> io::Result<(
    Arc<MarkdownDocument>,
    crate::document::Document,
    Arc<ImportReport>,
)> {
    entries.retain(|entry| entry.class != ConversionClass::Retained);
    if source.date_1904 {
        entries.push(
            ConversionEntry::new(
                ConversionClass::Converted,
                None,
                None,
                None,
                "The 1904 workbook date system is not carried over; date interpretation and date formulas may change.",
            )
            .with_message(crate::i18n::Message::new(
                "report-reason-xlsx-date-system-1904",
            )),
        );
    }
    entries.extend(super::load::xlsx_rule_report_entries(&source));
    let mut document = source.to_document(document_id, name.clone());
    let sheet_names = source
        .sheets
        .iter()
        .map(|sheet| sheet.name.clone())
        .collect::<Vec<_>>();
    let mut changes = Vec::new();
    let mut format_changes = Vec::new();
    for (sheet_index, sheet) in source.sheets.iter().enumerate() {
        let Some(table) = document
            .tables
            .iter_mut()
            .find(|table| table.id == sheet.table_id)
        else {
            continue;
        };
        for (address, source_cell) in &sheet.cells {
            if cancelled.load(Ordering::Acquire) {
                return Err(cancelled_error());
            }
            let Some(mut cell) = table.cell(*address).cloned() else {
                continue;
            };
            if let Some(formula) = &source_cell.formula {
                match normalize_import_formula(formula, &sheet_names, sheet_index, *address) {
                    FormulaImport::Local(value) => {
                        cell.input = crate::document::CellInput::Formula(format!("={value}"));
                    }
                    fallback @ (FormulaImport::External | FormulaImport::Invalid) => {
                        let cached = xlsx_value_text(&source_cell.value);
                        cell.input =
                            cached
                                .as_ref()
                                .map_or(crate::document::CellInput::Empty, |value| {
                                    crate::document::CellInput::Text(
                                        crate::document::encode_literal_text(value),
                                    )
                                });
                        cell.imported_result = None;
                        cell.calculation = None;
                        entries.push(formula_fallback_entry(
                            fallback,
                            cached.is_some(),
                            sheet_index,
                            &sheet.name,
                            *address,
                        ));
                    }
                }
            } else if let crate::document::CellInput::Text(value) = &cell.input {
                cell.input =
                    crate::document::CellInput::Text(crate::document::encode_literal_text(value));
            }
            // A rewritten formula keeps the formatting the sheet gave it.
            if let Some(format) = table.format(*address) {
                format_changes.push((table.id, *address, format.clone()));
            }
            changes.push((table.id, *address, Some(cell)));
        }
    }
    let imported = ImportedTables {
        source: ExportSource::Xlsx(source),
        document: &document,
        changes,
        format_changes,
        entries,
        input_format,
    };
    markdown_from_import(document_id, name, imported, language, cancelled)
}

/// The tables read from an import-only file, ready to be written as Markdown.
struct ImportedTables<'a> {
    source: ExportSource,
    document: &'a crate::document::Document,
    /// Cells rewritten for Markdown output, such as HTML text with its formatting runs.
    changes: Vec<(
        u64,
        crate::document::CellAddress,
        Option<crate::document::Cell>,
    )>,
    /// Import-only formatting replaced for cells rewritten for Markdown output.
    format_changes: Vec<(
        u64,
        crate::document::CellAddress,
        crate::document::CellFormat,
    )>,
    /// Losses already found while reading the source file.
    entries: Vec<ConversionEntry>,
    input_format: &'static str,
}

/// Uses the existing Markdown exporter so import and export share the same markup rules.
fn markdown_from_import(
    document_id: u64,
    name: String,
    imported: ImportedTables<'_>,
    language: crate::i18n::Language,
    cancelled: &AtomicBool,
) -> io::Result<(
    Arc<MarkdownDocument>,
    crate::document::Document,
    Arc<ImportReport>,
)> {
    let ImportedTables {
        source,
        document,
        changes,
        format_changes,
        mut entries,
        input_format,
    } = imported;
    let tables = document
        .tables
        .iter()
        .map(|table| crate::export::ExportTableMeta {
            id: table.id,
            name: table.name.clone(),
            rows: table.row_count(),
            columns: table.column_count(),
        })
        .collect::<Vec<_>>();
    let request = ExportRequest {
        document_id,
        revision: 0,
        options: crate::export::ExportOptions {
            format: crate::export::ExportFormat::Markdown,
            scope: crate::export::ExportScope::AllTables,
            formulas: crate::export::FormulaOutput::Formula,
            modifiers: crate::export::ModifierOutput::Convert,
            markdown_header: crate::export::MarkdownHeader::UseFirstRow,
        },
        destination: PathBuf::from("import.md"),
        source_path: None,
        source: Some(source),
        source_report: None,
        tables,
        changes,
        format_changes,
        formula_results: None,
    };
    let prepared = crate::export::prepare_export_cancellable(&request, None, cancelled)
        .map_err(|message| user_io_error(io::ErrorKind::InvalidData, message))?;
    let mut bytes = prepared
        .artifacts
        .first()
        .map(|artifact| artifact.bytes.clone())
        .ok_or_else(|| {
            user_io_error(
                io::ErrorKind::InvalidData,
                Message::new("status-import-markdown-empty"),
            )
        })?;
    entries.extend(prepared.report.entries.iter().cloned());
    let unheaded = MarkdownDocument::parse(bytes.clone()).map_err(|error| {
        user_io_error(
            io::ErrorKind::InvalidData,
            crate::i18n::markdown_error_message(error),
        )
    })?;
    if unheaded.tables.len() == document.tables.len() {
        for (table, imported) in unheaded.tables.iter().zip(&document.tables).rev() {
            let title = imported
                .name
                .chars()
                .map(|character| {
                    if matches!(character, '\r' | '\n') {
                        ' '
                    } else {
                        character
                    }
                })
                .collect::<String>();
            let heading = format!("## {title}\n\n");
            bytes.splice(
                table.source_range.start..table.source_range.start,
                heading.bytes(),
            );
        }
    }
    let markdown = MarkdownDocument::parse(bytes).map_err(|error| {
        user_io_error(
            io::ErrorKind::InvalidData,
            crate::i18n::markdown_error_message(error),
        )
    })?;
    let source = Arc::new(markdown);
    let mut imported = source.to_document_with_language(document_id, name, language);
    imported.set_source(Some(Arc::new(crate::source::SourceDocument::Markdown(
        Arc::clone(&source),
    ))));
    for view in imported.views.values_mut() {
        view.header_row = Some(0);
    }
    let report = Arc::new(ImportReport::now(
        document_name(document),
        input_format,
        entries.to_vec(),
    ));
    Ok((source, imported, report))
}

/// Keeps HTML parser diagnostics in the report for the final imported document.
fn html_diagnostic_entries(source: &HtmlDocument) -> Vec<ConversionEntry> {
    source
        .diagnostics
        .iter()
        .map(|diagnostic| {
            let (table, class, reason, message_key) = match diagnostic {
                crate::html_adapter::HtmlDiagnostic::NestedTable { table } => (
                    *table,
                    ConversionClass::Omitted,
                    "The outer HTML table was kept; its nested table and content were omitted.",
                    "report-reason-html-nested-table-omitted",
                ),
                crate::html_adapter::HtmlDiagnostic::MalformedTable { table } => (
                    *table,
                    ConversionClass::Converted,
                    "Ambiguous HTML table structure was read as one Markdown table.",
                    "report-reason-html-ambiguous-table-converted",
                ),
            };
            ConversionEntry::new(class, Some(table + 1), None, None, reason)
                .with_message(crate::i18n::Message::new(message_key))
        })
        .collect()
}

// Reads the name that will be kept when the document is imported.
fn document_name(document: &crate::document::Document) -> String {
    document.name.clone()
}

// Converts supported XLSX values into the text used by Markdown cells.
fn xlsx_value_text(value: &crate::xlsx_adapter::XlsxValue) -> Option<String> {
    match value {
        crate::xlsx_adapter::XlsxValue::Empty => None,
        crate::xlsx_adapter::XlsxValue::String(value) => Some(value.clone()),
        crate::xlsx_adapter::XlsxValue::RichText(runs) => {
            Some(runs.iter().map(|run| run.text.as_str()).collect())
        }
        crate::xlsx_adapter::XlsxValue::Number(value) => Some(value.to_string()),
        crate::xlsx_adapter::XlsxValue::Boolean(value) => {
            Some(if *value { "TRUE" } else { "FALSE" }.to_owned())
        }
        crate::xlsx_adapter::XlsxValue::Error(value) => Some(value.clone()),
    }
}
