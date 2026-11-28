use super::*;

// Saves a base snapshot and its revision manifest with bounded files.
pub(crate) fn save_snapshot(
    root: &Path,
    snapshot: RecoverySnapshot,
) -> Result<(), crate::i18n::Message> {
    fs::create_dir_all(root).map_err(recovery_io_message)?;
    let manifest_path = manifest_path(root, &snapshot.key)?;
    let base_generation = (snapshot.structure_revision, snapshot.saved_revision);
    let old = read_manifest(&manifest_path).ok();
    if old.as_ref().is_some_and(|record| {
        record.revision > snapshot.revision || record.generation > snapshot.generation
    }) {
        return Ok(());
    }
    let base_file = old
        .as_ref()
        .filter(|record| record.base_generation == base_generation)
        .map(|record| record.base_file.clone())
        .unwrap_or_else(|| base_filename(&snapshot.key, base_generation));
    let base_path = root.join(&base_file);
    if !base_path.exists() {
        let base = snapshot.base.into_file();
        write_json_atomic(&base_path, &base)?;
    }
    let record = RecoveryRecord {
        key: snapshot.key.clone(),
        document_id: snapshot.document_id,
        name: snapshot.name,
        original_path: snapshot.original_path,
        original_identity: snapshot.original_identity,
        original_changed: false,
        requires_initial_save: snapshot.requires_initial_save,
        revision: snapshot.revision,
        saved_revision: snapshot.saved_revision,
        generation: snapshot.generation,
        structure_revision: snapshot.structure_revision,
        text_overlay_revision: snapshot.text_overlay_revision,
        text_overlay_cells: (*snapshot.text_overlay_cells).clone(),
        base_generation,
        base_file: base_file.clone(),
        cells: snapshot.cells,
        tables: snapshot.tables,
        cell_edit: snapshot.cell_edit,
    };
    write_json_atomic(&manifest_path, &record)?;
    if let Some(old) = old
        && old.base_file != base_file
    {
        let _ = fs::remove_file(root.join(old.base_file));
    }
    Ok(())
}

/// Rebuilds a recovered document from its stored source base and sparse edits.
pub(crate) fn restore_item(
    item: RecoveryItem,
    document_id: DocumentId,
    language: crate::i18n::Language,
) -> Result<RecoveredDocument, crate::i18n::Message> {
    let RecoveryItem { record, base } = item;
    let original_changed = record.original_path.as_ref().is_some_and(|path| {
        record.original_identity.as_ref().is_none_or(|expected| {
            FileIdentity::read(path).map_or(true, |current| &current != expected)
        })
    });
    let requires_initial_save = record.requires_initial_save || original_changed;
    let stable_table_ids = record
        .tables
        .iter()
        .map(|table| table.id)
        .collect::<Vec<_>>();
    let (mut document, source) = match base {
        RecoveryBasePayload::Csv {
            bytes,
            encoding,
            delimiter,
        } => {
            let choice = if encoding == CsvEncoding::ShiftJis {
                EncodingChoice::ShiftJis
            } else {
                EncodingChoice::Auto
            };
            let source = Arc::new(
                CsvDocument::parse(bytes, choice, delimiter)
                    .map_err(crate::i18n::csv_error_message)?,
            );
            let source_document = SourceDocument::Csv(Arc::clone(&source));
            let document = crate::structure::source_to_document(
                &source_document,
                document_id,
                &stable_table_ids,
            )?;
            (document, source_document)
        }
        RecoveryBasePayload::Markdown { bytes } => {
            let source = Arc::new(
                MarkdownDocument::parse(bytes).map_err(crate::i18n::markdown_error_message)?,
            );
            let source_document = SourceDocument::Markdown(Arc::clone(&source));
            let document = crate::structure::source_to_document_with_language(
                &source_document,
                document_id,
                &stable_table_ids,
                language,
            )?;
            (document, source_document)
        }
    };
    document.set_source(Some(Arc::new(source)));
    for info in &record.tables {
        let table = document
            .tables
            .iter_mut()
            .find(|table| table.id == info.id)
            .ok_or_else(|| {
                crate::i18n::Message::new("status-recovery-table-missing")
                    .with_number("table_id", info.id as i64)
            })?;
        table.name.clone_from(&info.name);
        table.set_dimensions(info.rows.max(1), info.columns.max(1));
        table.set_loaded_string_cells(Arc::new(
            info.string_cells.iter().copied().collect::<BTreeSet<_>>(),
        ));
        if let Some(view) = document.views.get_mut(&info.id) {
            view.header_row = info.header_row;
            view.selection = info.selection;
            view.scroll = info.scroll;
        }
    }
    for (table_id, address, cell) in &record.text_overlay_cells {
        let table = document
            .tables
            .iter_mut()
            .find(|table| table.id == *table_id)
            .ok_or_else(|| {
                crate::i18n::Message::new("status-recovery-table-missing")
                    .with_number("table_id", *table_id as i64)
            })?;
        table.apply_snapshot_cell(*address, Some(cell.clone()));
    }
    document.install_text_overlay_snapshot(crate::document::TextOverlaySnapshot {
        revision: record.text_overlay_revision,
        cells: Arc::new(record.text_overlay_cells.clone()),
    });
    document.restore_recovery_deltas(
        &record.cells,
        crate::document::RecoveryRevisions {
            revision: record.revision,
            saved_revision: record.saved_revision,
            structure_revision: record.structure_revision,
        },
    )?;
    let import_report = None;
    let calculation = crate::calc::CalculationSnapshot::from_document(&document, false);
    Ok(RecoveredDocument {
        key: record.key,
        generation: record.generation,
        document,
        cell_edit: record.cell_edit,
        original_changed,
        requires_initial_save,
        original_path: record.original_path,
        original_identity: record.original_identity,
        calculation,
        import_report,
    })
}

// Loads valid recovery entries and counts items that could not be read.
pub(crate) fn load_items(root: &Path) -> Result<(Vec<RecoveryItem>, usize), crate::i18n::Message> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok((Vec::new(), 0)),
        Err(error) => return Err(recovery_io_message(error)),
    };
    let mut items = Vec::new();
    let mut skipped = 0;
    for entry in entries {
        let Ok(entry) = entry else {
            skipped += 1;
            continue;
        };
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        // Base files are referenced by manifests, so they are not recovery items themselves.
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".base.json"))
        {
            continue;
        }
        let record = match read_manifest(&path) {
            Ok(record) => record,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        if !valid_key(&record.key) || !valid_base_file(&record.base_file) {
            skipped += 1;
            continue;
        }
        let base_path = root.join(&record.base_file);
        let Ok(base) = read_json::<RecoveryBaseFile>(&base_path) else {
            skipped += 1;
            continue;
        };
        let original_changed = record.original_path.as_ref().is_some_and(|path| {
            record.original_identity.as_ref().is_none_or(|expected| {
                FileIdentity::read(path).map_or(true, |current| &current != expected)
            })
        });
        let mut record = record;
        record.original_changed = original_changed;
        items.push(RecoveryItem {
            record,
            base: match base {
                RecoveryBaseFile::Csv {
                    bytes,
                    encoding,
                    delimiter,
                } => RecoveryBasePayload::Csv {
                    bytes,
                    encoding,
                    delimiter,
                },
                RecoveryBaseFile::Markdown { bytes } => RecoveryBasePayload::Markdown { bytes },
            },
        });
    }
    Ok((items, skipped))
}

// Removes recovery data only when its saved revision is no longer needed.
pub(crate) fn cleanup_saved(
    root: &Path,
    key: &str,
    saved_revision: u64,
    saved_generation: u64,
) -> Result<(), crate::i18n::Message> {
    let manifest = manifest_path(root, key)?;
    let Ok(record) = read_manifest(&manifest) else {
        return Ok(());
    };
    if !valid_base_file(&record.base_file) {
        return Ok(());
    }
    if record.revision > saved_revision || record.generation > saved_generation {
        return Ok(());
    }
    let base = root.join(record.base_file);
    fs::remove_file(manifest).map_err(recovery_io_message)?;
    let _ = fs::remove_file(base);
    Ok(())
}

// Removes one validated recovery entry and its base file.
pub(crate) fn remove_recovery(root: &Path, key: &str) -> Result<(), crate::i18n::Message> {
    let manifest = manifest_path(root, key)?;
    let Ok(record) = read_manifest(&manifest) else {
        return Ok(());
    };
    if !valid_base_file(&record.base_file) {
        return Ok(());
    }
    fs::remove_file(manifest).map_err(recovery_io_message)?;
    let _ = fs::remove_file(root.join(record.base_file));
    Ok(())
}

// Writes bounded JSON through a temporary file before replacing the target.
fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), crate::i18n::Message> {
    let bytes = serde_json::to_vec(value)
        .map_err(|_| crate::i18n::Message::new("status-recovery-record-invalid"))?;
    if bytes.len() as u64 > MAX_RECOVERY_BYTES {
        return Err(crate::i18n::Message::new("status-recovery-size-limit"));
    }
    let mut temp = path.to_path_buf();
    temp.set_extension("tmp");
    let write_result = (|| -> io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp)?;
        file.write_all(&bytes)?;
        file.flush()?;
        file.sync_all()?;
        crate::file_worker::replace_file_atomic(&temp, path)
    })();
    if let Err(error) = write_result {
        let _ = fs::remove_file(temp);
        return Err(recovery_io_message(error));
    }
    Ok(())
}

/// Converts recovery file errors into language-neutral app messages.
fn recovery_io_message(error: io::Error) -> crate::i18n::Message {
    crate::file_worker::file_error_message(error)
}

// Reads JSON only when its file size stays within the recovery limit.
fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> io::Result<T> {
    let file = File::open(path)?;
    let size = file.metadata()?.len();
    if size > MAX_RECOVERY_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "recovery file is too large",
        ));
    }
    let mut bytes = Vec::with_capacity(size as usize);
    file.take(MAX_RECOVERY_BYTES + 1).read_to_end(&mut bytes)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

// Reads one bounded recovery manifest from disk.
fn read_manifest(path: &Path) -> io::Result<RecoveryRecord> {
    read_json(path)
}

// Builds a manifest path only for a safe recovery key.
fn manifest_path(root: &Path, key: &str) -> Result<PathBuf, crate::i18n::Message> {
    if !valid_key(key) {
        return Err(crate::i18n::Message::new("status-recovery-record-invalid"));
    }
    Ok(root.join(format!("{key}.json")))
}

// Names the base file for one recovery generation.
fn base_filename(key: &str, generation: (u64, u64)) -> String {
    format!("{key}-{}-{}.base.json", generation.0, generation.1)
}

// Accepts only short hexadecimal keys with optional dashes.
fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 128
        && key
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
}

/// Accepts only one file name inside the recovery directory.
fn valid_base_file(name: &str) -> bool {
    let path = Path::new(name);
    path.components().count() == 1 && path.file_name().and_then(|part| part.to_str()) == Some(name)
}
