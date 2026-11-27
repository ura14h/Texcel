use super::*;
use crate::file_worker::load_progress::LoadProgressSender;

type ImportedWorkbook = (
    Arc<MarkdownDocument>,
    crate::document::Document,
    CalculationSnapshot,
    FileIdentity,
    Arc<ImportReport>,
);

/// Loads an XLSX or ODS workbook and previews converted sheets after the full book parse.
pub(super) fn run_import_workbook(
    token: JobToken,
    path: PathBuf,
    format: WorkbookFormat,
    language: crate::i18n::Language,
    cancelled: Arc<AtomicBool>,
    progress_queue: Option<&SyncSender<LoadProgress>>,
    shutdown: &AtomicBool,
) -> FileResult {
    let path = super::canonical_load_path(path);
    let mut progress = LoadProgressSender::new(
        token,
        FileOperation::LoadWorkbook,
        &path,
        progress_queue,
        &cancelled,
        shutdown,
    );
    if !progress.stage(LoadStage::ReadingSource) {
        return super::load::cancelled_load_result(token, path, FileOperation::LoadWorkbook);
    }
    let result: io::Result<ImportedWorkbook> = (|| {
        let (bytes, identity) = super::load::read_source(&path, &cancelled)?;
        if !progress.stage(LoadStage::ParsingWorkbook) {
            return Err(cancelled_error());
        }
        let label = match format {
            WorkbookFormat::Xlsx => "XLSX",
            WorkbookFormat::Ods => "ODS",
        };
        let file_name = path.file_stem().map_or_else(
            || label.to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        // Both readers hand each converted sheet row to the load preview.
        let mut preview = |table: usize, name: &str, row: usize, cells: &[(usize, String)]| {
            if !progress.enabled() {
                return true;
            }
            progress.push_row(
                table,
                name,
                row,
                cells
                    .iter()
                    .map(|(column, text)| LoadPreviewCell {
                        column: *column,
                        text: text.clone(),
                    })
                    .collect(),
            )
        };
        let (source, entries) = match format {
            WorkbookFormat::Xlsx => {
                let inspection = XlsxPackageInspection::scan(&bytes, XlsxPackageLimits::default())
                    .map_err(|error| {
                        user_io_error(
                            io::ErrorKind::InvalidData,
                            crate::i18n::xlsx_scan_error_message(error),
                        )
                    })?;
                if cancelled.load(Ordering::Acquire) || shutdown.load(Ordering::Acquire) {
                    return Err(cancelled_error());
                }
                let source =
                    XlsxDocument::parse_with_progress(&bytes, token.document_id, &mut preview)
                        .map_err(|error| {
                            user_io_error(
                                io::ErrorKind::InvalidData,
                                crate::i18n::xlsx_error_message(error),
                            )
                        })?
                        .ok_or_else(cancelled_error)?;
                (source, inspection.entries)
            }
            WorkbookFormat::Ods => {
                let import = crate::ods_adapter::parse_ods_with_progress(&bytes, &mut preview)
                    .map_err(|message| user_io_error(io::ErrorKind::InvalidData, message))?
                    .ok_or_else(cancelled_error)?;
                (import.workbook, import.entries)
            }
        };
        let (markdown, document, report) = super::import_markdown::workbook_to_markdown(
            Arc::new(source),
            token.document_id,
            file_name,
            label,
            language,
            entries,
            &cancelled,
        )?;
        let calculation = CalculationSnapshot::from_document(&document, false);
        if !progress.finish() {
            return Err(cancelled_error());
        }
        Ok((markdown, document, calculation, identity, report))
    })();
    let mut kind = match result {
        Ok((source, document, calculation, identity, report)) => FileResultKind::ImportedMarkdown {
            source,
            document,
            calculation,
            identity,
            report,
            input_format: match format {
                WorkbookFormat::Xlsx => ImportedFormat::Xlsx,
                WorkbookFormat::Ods => ImportedFormat::Ods,
            },
        },
        Err(_error) if cancelled.load(Ordering::Acquire) => FileResultKind::Cancelled,
        Err(error) => FileResultKind::failed(error),
    };
    if cancelled.load(Ordering::Acquire) {
        kind = FileResultKind::Cancelled;
    }
    FileResult {
        token,
        operation: FileOperation::LoadWorkbook,
        path,
        kind,
    }
}

#[cfg(test)]
#[path = "load_workbook_test.rs"]
mod tests;
