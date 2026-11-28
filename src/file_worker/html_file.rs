use super::*;

/// Parses an HTML source file and builds its supported table model.
pub(super) fn run_import_html(
    token: JobToken,
    path: PathBuf,
    language: crate::i18n::Language,
    cancelled: Arc<AtomicBool>,
    progress_queue: Option<&ProgressQueue>,
    shutdown: &AtomicBool,
) -> FileResult {
    let path = super::canonical_load_path(path);
    let mut progress = LoadProgressSender::new(
        token,
        FileOperation::LoadHtml,
        &path,
        progress_queue,
        &cancelled,
        shutdown,
    );
    if !progress.stage(LoadStage::ReadingSource) {
        return cancelled_load_result(token, path, FileOperation::LoadHtml);
    }
    let result = (|| {
        let (bytes, identity) = read_source(&path, &cancelled)?;
        if !progress.stage(LoadStage::ParsingSource) {
            return Err(cancelled_error());
        }
        let source = HtmlDocument::parse(bytes).map_err(|error| {
            user_io_error(
                io::ErrorKind::InvalidData,
                crate::i18n::html_error_message(error),
            )
        })?;
        for (table_index, table) in source.tables.iter().enumerate() {
            let table_name = table.name_with_language(table_index, language);
            for (row_index, row) in table.rows.iter().enumerate() {
                if cancelled.load(Ordering::Acquire) {
                    return Err(cancelled_error());
                }
                if !progress.push_row(
                    table_index,
                    &table_name,
                    row_index,
                    row.iter()
                        .enumerate()
                        .map(|(column, cell)| LoadPreviewCell {
                            column,
                            text: cell.display_value.clone(),
                        })
                        .collect(),
                ) {
                    return Err(cancelled_error());
                }
            }
        }
        if cancelled.load(Ordering::Acquire) {
            return Err(cancelled_error());
        }
        let name = path.file_stem().map_or_else(
            || "HTML".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        let source = Arc::new(source);
        let (source_markdown, document, report) = super::import_markdown::html_to_markdown(
            Arc::clone(&source),
            token.document_id,
            name,
            language,
            &cancelled,
        )?;
        let calculation = CalculationSnapshot::from_document(&document, false);
        if !progress.finish() {
            return Err(cancelled_error());
        }
        Ok((source_markdown, document, calculation, identity, report))
    })();
    let kind = match result {
        Ok((source, document, calculation, identity, report)) => FileResultKind::ImportedMarkdown {
            source,
            document,
            calculation,
            identity,
            report,
            input_format: ImportedFormat::Html,
        },
        Err(_error) if cancelled.load(Ordering::Acquire) => FileResultKind::Cancelled,
        Err(error) => FileResultKind::failed(error),
    };
    FileResult {
        token,
        operation: FileOperation::LoadHtml,
        path,
        kind,
    }
}
