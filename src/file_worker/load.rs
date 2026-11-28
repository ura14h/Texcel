use super::*;

mod report;
pub(super) use report::*;

/// Loads a text file and reports its detected format.
pub(super) fn run_load(
    token: JobToken,
    path: PathBuf,
    request: CsvLoadRequest,
    cancelled: Arc<AtomicBool>,
    progress_queue: Option<&ProgressQueue>,
    shutdown: &AtomicBool,
) -> FileResult {
    let CsvLoadRequest {
        encoding,
        delimiter,
        settings_only,
    } = request;
    let path = super::canonical_load_path(path);
    enum LoadOutcome {
        Loaded(
            Arc<CsvDocument>,
            Box<crate::document::Document>,
            CalculationSnapshot,
            FileIdentity,
        ),
        NeedEncoding {
            suggested: Option<EncodingChoice>,
            analysis: Option<DelimiterAnalysis>,
            error: Option<&'static str>,
        },
        NeedDelimiter(DelimiterAnalysis),
    }
    let mut progress = LoadProgressSender::new(
        token,
        FileOperation::LoadCsv,
        &path,
        progress_queue,
        &cancelled,
        shutdown,
    );
    if !settings_only && !progress.stage(LoadStage::ReadingSource) {
        return cancelled_load_result(token, path, FileOperation::LoadCsv);
    }
    let result: io::Result<LoadOutcome> = (|| {
        let (bytes, identity) = read_source(&path, &cancelled)?;
        if starts_with_zip_signature(&bytes) {
            return Err(user_io_error(
                io::ErrorKind::InvalidData,
                crate::i18n::Message::new("status-file-format-mismatch")
                    .with_text("expected", "CSV")
                    .with_text("actual", "XLSX"),
            ));
        }
        let delimiter = match delimiter {
            Some(delimiter) => delimiter,
            None => match analyze_delimiters(&bytes, encoding) {
                Ok(analysis) if settings_only => return Ok(LoadOutcome::NeedDelimiter(analysis)),
                Ok(analysis) => match analysis.suggested {
                    Some(delimiter) => delimiter,
                    None => return Ok(LoadOutcome::NeedDelimiter(analysis)),
                },
                Err(crate::csv_adapter::CsvError::EncodingChoiceRequired) => {
                    let probe = analyze_delimiters(&bytes, EncodingChoice::ShiftJis);
                    return match probe {
                        Ok(analysis) => Ok(LoadOutcome::NeedEncoding {
                            suggested: Some(EncodingChoice::ShiftJis),
                            analysis: Some(analysis),
                            error: None,
                        }),
                        Err(crate::csv_adapter::CsvError::InvalidEncoding) => {
                            Ok(LoadOutcome::NeedEncoding {
                                suggested: None,
                                analysis: None,
                                error: Some("status-csv-encoding-undetermined"),
                            })
                        }
                        Err(error) => Err(user_io_error(
                            io::ErrorKind::InvalidData,
                            crate::i18n::csv_error_message(error),
                        )),
                    };
                }
                Err(crate::csv_adapter::CsvError::InvalidEncoding) if settings_only => {
                    return Ok(LoadOutcome::NeedEncoding {
                        suggested: None,
                        analysis: None,
                        error: Some("status-csv-encoding-invalid"),
                    });
                }
                Err(error) => {
                    return Err(user_io_error(
                        io::ErrorKind::InvalidData,
                        crate::i18n::csv_error_message(error),
                    ));
                }
            },
        };
        if !progress.stage(LoadStage::ParsingSource) {
            return Err(cancelled_error());
        }
        // The preview tab uses the same name as the loaded table: the file name without extension.
        let table_name = path.file_stem().map_or_else(
            || "CSV".to_owned(),
            |stem| stem.to_string_lossy().into_owned(),
        );
        let source = CsvDocument::parse_with_progress(bytes, encoding, delimiter, |row, fields| {
            if !progress.enabled() {
                return true;
            }
            progress.push_row(
                0,
                &table_name,
                row,
                fields
                    .iter()
                    .enumerate()
                    .map(|(column, field)| LoadPreviewCell {
                        column,
                        text: field.value.clone(),
                    })
                    .collect(),
            )
        })
        .map_err(|error| {
            user_io_error(
                io::ErrorKind::InvalidData,
                crate::i18n::csv_error_message(error),
            )
        })?
        .ok_or_else(cancelled_error)?;
        if looks_like_markdown_table(&source) {
            return Err(user_io_error(
                io::ErrorKind::InvalidData,
                crate::i18n::Message::new("status-file-format-mismatch")
                    .with_text("expected", "CSV")
                    .with_text("actual", "Markdown"),
            ));
        }
        let name = path.file_name().map_or_else(
            || "CSV".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        let document = source.to_document(token.document_id, name);
        let calculation = CalculationSnapshot::from_document(&document, false);
        if !progress.finish() {
            return Err(cancelled_error());
        }
        Ok(LoadOutcome::Loaded(
            Arc::new(source),
            Box::new(document),
            calculation,
            identity,
        ))
    })();
    let mut kind = match result {
        Ok(LoadOutcome::Loaded(source, document, calculation, identity)) => {
            FileResultKind::LoadedCsv {
                source,
                document: *document,
                calculation,
                identity,
            }
        }
        Ok(LoadOutcome::NeedEncoding {
            suggested,
            analysis,
            error,
        }) => FileResultKind::LoadNeedsEncodingChoice {
            suggested,
            analysis,
            error,
        },
        Ok(LoadOutcome::NeedDelimiter(analysis)) => {
            FileResultKind::LoadNeedsDelimiterChoice { analysis }
        }
        Err(_error) if cancelled.load(Ordering::Acquire) => FileResultKind::Cancelled,
        Err(error) => FileResultKind::failed(error),
    };
    if cancelled.load(Ordering::Acquire) {
        kind = FileResultKind::Cancelled;
    }
    FileResult {
        token,
        operation: FileOperation::LoadCsv,
        path,
        kind,
    }
}

/// Creates a terminal cancellation response before a load starts.
pub(super) fn cancelled_load_result(
    token: JobToken,
    path: PathBuf,
    operation: FileOperation,
) -> FileResult {
    FileResult {
        token,
        operation,
        path,
        kind: FileResultKind::Cancelled,
    }
}

/// Reads source bytes while checking cancellation and size limits.
pub(super) fn read_source(
    path: &Path,
    cancelled: &AtomicBool,
) -> io::Result<(Vec<u8>, FileIdentity)> {
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(user_io_error(
            io::ErrorKind::InvalidInput,
            crate::i18n::Message::new("status-file-symlink-unsupported"),
        ));
    }
    let before = fs::metadata(path)?;
    if before.len() > MAX_SOURCE_BYTES {
        return Err(source_size_error());
    }
    let mut file = File::open(path)?;
    let initial_capacity = usize::try_from(before.len()).unwrap_or(0);
    let mut bytes = Vec::with_capacity(initial_capacity.min(MAX_SOURCE_BYTES as usize));
    let mut hasher = Sha256::new();
    let mut chunk = [0_u8; IO_CHUNK_SIZE];
    let mut size = 0_u64;
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Err(cancelled_error());
        }
        let count = file.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        size = size.checked_add(count as u64).ok_or_else(|| {
            user_io_error(
                io::ErrorKind::InvalidData,
                crate::i18n::Message::new("status-file-size-limit"),
            )
        })?;
        if size > MAX_SOURCE_BYTES {
            return Err(source_size_error());
        }
        hasher.update(&chunk[..count]);
        bytes.extend_from_slice(&chunk[..count]);
    }
    let after = fs::metadata(path)?;
    let modified_before = modified_unix_nanos(&before);
    let modified_after = modified_unix_nanos(&after);
    if before.len() != size
        || after.len() != size
        || modified_before != modified_after
        || before.len() != after.len()
    {
        return Err(user_io_error(
            io::ErrorKind::Interrupted,
            crate::i18n::Message::new("status-file-changed-during-read"),
        ));
    }
    Ok((
        bytes,
        FileIdentity {
            size,
            modified_unix_nanos: modified_after,
            sha256: hasher.finalize().into(),
        },
    ))
}

/// Parses Markdown source and builds its initial document.
pub(super) fn run_load_markdown(
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
        FileOperation::LoadMarkdown,
        &path,
        progress_queue,
        &cancelled,
        shutdown,
    );
    if !progress.stage(LoadStage::ReadingSource) {
        return cancelled_load_result(token, path, FileOperation::LoadMarkdown);
    }
    let result: io::Result<(
        Arc<MarkdownDocument>,
        crate::document::Document,
        CalculationSnapshot,
        FileIdentity,
    )> = (|| {
        let (bytes, identity) = read_source(&path, &cancelled)?;
        if starts_with_zip_signature(&bytes) {
            return Err(user_io_error(
                io::ErrorKind::InvalidData,
                crate::i18n::Message::new("status-file-format-mismatch")
                    .with_text("expected", "Markdown")
                    .with_text("actual", "XLSX"),
            ));
        }
        if !progress.stage(LoadStage::ParsingSource) {
            return Err(cancelled_error());
        }
        let source = MarkdownDocument::parse_with_progress(bytes, |table, row, cells| {
            if !progress.enabled() {
                return true;
            }
            progress.push_row(
                table,
                &format!("Table {}", table + 1),
                row,
                cells
                    .iter()
                    .enumerate()
                    .map(|(column, cell)| LoadPreviewCell {
                        column,
                        text: cell.value.clone(),
                    })
                    .collect(),
            )
        })
        .map_err(|error| {
            user_io_error(
                io::ErrorKind::InvalidData,
                crate::i18n::markdown_error_message(error),
            )
        })?
        .ok_or_else(cancelled_error)?;
        if source.tables.is_empty() {
            return Err(user_io_error(
                io::ErrorKind::InvalidData,
                crate::i18n::Message::new("status-file-markdown-table-required"),
            ));
        }
        let name = path.file_name().map_or_else(
            || "Markdown".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        let source = Arc::new(source);
        let mut document = source.to_document_with_language(token.document_id, name, language);
        document.set_source(Some(Arc::new(crate::source::SourceDocument::Markdown(
            Arc::clone(&source),
        ))));
        let calculation = CalculationSnapshot::from_document(&document, false);
        if !progress.finish() {
            return Err(cancelled_error());
        }
        Ok((source, document, calculation, identity))
    })();
    let mut kind = match result {
        Ok((source, document, calculation, identity)) => FileResultKind::LoadedMarkdown {
            source,
            document,
            calculation,
            identity,
        },
        Err(_error) if cancelled.load(Ordering::Acquire) => FileResultKind::Cancelled,
        Err(error) => FileResultKind::failed(error),
    };
    if cancelled.load(Ordering::Acquire) {
        kind = FileResultKind::Cancelled;
    }
    FileResult {
        token,
        operation: FileOperation::LoadMarkdown,
        path,
        kind,
    }
}

/// Recognizes the standard pipe-table delimiter line to catch obvious extension mismatches.
pub(super) fn looks_like_markdown_table(source: &CsvDocument) -> bool {
    let mut previous: Option<String> = None;
    for record in &source.records {
        if record.fields.iter().any(|field| {
            field.was_quoted || field.value.contains('\n') || field.value.contains('\r')
        }) {
            previous = None;
            continue;
        }
        let delimiter = source.delimiter.to_string();
        let line = record
            .fields
            .iter()
            .map(|field| field.value.as_str())
            .collect::<Vec<_>>()
            .join(&delimiter);
        if let Some(header) = previous.take()
            && is_markdown_separator(&line)
            && header.trim().trim_matches('|').split('|').count() >= 2
        {
            return true;
        }
        previous = Some(line);
    }
    false
}

/// Prevents a ZIP-based workbook from being accepted by a text-format reader.
pub(super) fn starts_with_zip_signature(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06")
        || bytes.starts_with(b"PK\x07\x08")
}

/// Checks whether a row is a Markdown table separator.
pub(super) fn is_markdown_separator(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.contains('|') {
        return false;
    }
    let cells = trimmed.trim_matches('|').split('|').collect::<Vec<_>>();
    cells.len() >= 2
        && cells.iter().all(|cell| {
            let cell = cell.trim();
            let core = cell.trim_matches(':');
            core.len() >= 3 && core.bytes().all(|byte| byte == b'-')
        })
}
