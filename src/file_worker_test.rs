use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

#[test]
// Keeps common operating system errors local and easy to understand.
fn file_error_messages_translate_common_causes_without_os_details() {
    for (kind, expected_key) in [
        (io::ErrorKind::NotFound, "status-file-not-found"),
        (io::ErrorKind::PermissionDenied, "status-file-access-denied"),
        (io::ErrorKind::StorageFull, "status-file-storage-full"),
        (io::ErrorKind::AlreadyExists, "status-file-already-exists"),
    ] {
        let error = io::Error::new(kind, "/private/secret/input.csv: os error 2");
        let message = crate::file_worker::file_error_message(error);
        assert_eq!(message.key(), expected_key);
        let english = message.resolve(crate::i18n::Language::English);
        let japanese = message.resolve(crate::i18n::Language::Japanese);
        assert!(!english.contains("/private/secret"));
        assert!(!japanese.contains("/private/secret"));
        assert!(!english.contains("os error"));
        assert!(!japanese.contains("os error"));
    }

    let typed = user_io_error(
        io::ErrorKind::InvalidData,
        crate::i18n::Message::new("status-file-symlink-unsupported"),
    );
    assert_eq!(
        crate::file_worker::file_error_message(typed).key(),
        "status-file-symlink-unsupported"
    );
}

#[test]
// Verify that every XLSX filter, input rule, and conditional format is reported as lost.
fn xlsx_rules_are_reported_as_lost_in_markdown() {
    let source = XlsxDocument::parse(include_bytes!("../samples/rules.xlsx"), 1)
        .expect("parse dedicated rules sample through the adopted adapter");
    let entries = xlsx_rule_report_entries(&source);
    let key_count = |key: &str| {
        entries
            .iter()
            .filter(|entry| {
                entry
                    .message
                    .as_ref()
                    .is_some_and(|message| message.key() == key)
            })
            .count()
    };
    assert!(
        entries
            .iter()
            .all(|entry| entry.class == ConversionClass::Omitted)
    );
    assert!(entries.iter().any(|entry| {
        entry.range.as_deref() == Some("A1:C5")
            && entry.message.as_ref().map(|message| message.key())
                == Some("report-reason-xlsx-filter-not-text")
    }));
    assert_eq!(key_count("report-reason-xlsx-validation-not-text"), 2);
    assert!(key_count("report-reason-xlsx-conditional-not-text") >= 1);
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    // Create a file worker with the standard worker threads.
    fn new() -> Self {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("texcel-file-worker-{id}"));
        fs::create_dir(&path).expect("create isolated test directory");
        Self(path)
    }
}

impl Drop for TestDirectory {
    // Stop the worker and cancel its pending jobs.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

// Wait until the worker returns the result for a submitted job.
fn wait_for_result(worker: &FileWorker) -> FileResult {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        if let Some(result) = worker.poll().into_iter().next() {
            return result;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "file worker timed out"
        );
        thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[test]
// Verify that large dirty Markdown is parsed on a worker and bounded for grid display.
fn large_dirty_markdown_display_is_parsed_on_worker_and_bounded_for_the_grid() {
    let worker = FileWorker::new();
    let token = JobToken {
        document_id: 8,
        revision: 3,
        job_id: 4,
    };
    let body = "日本語".repeat(6_000);
    let markdown = Arc::new(
        MarkdownDocument::parse(
            b"[ref]: https://example.test/path\n\n| value |\n| --- |\n| small |\n".to_vec(),
        )
        .expect("reference definition source parses"),
    );
    let value = format!("**{body}** [link][ref]");
    worker
        .prepare_markdown_cell_display(
            token,
            1,
            crate::document::CellAddress { row: 1, column: 0 },
            (2, 3),
            markdown,
            MarkdownCellChange {
                table: 0,
                row: 1,
                column: 0,
                value,
            },
        )
        .expect("enqueue large cell display");
    let result = wait_for_result(&worker);
    assert_eq!(result.token, token);
    let FileResultKind::MarkdownCellDisplayPrepared {
        table_id,
        address,
        cell_input_revision,
        display,
    } = result.kind
    else {
        panic!("worker prepares the parsed Markdown display");
    };
    assert_eq!(table_id, 1);
    assert_eq!(address, crate::document::CellAddress { row: 1, column: 0 });
    assert_eq!(cell_input_revision, (2, 3));
    assert_eq!(display.rendered_bytes, body.len() + " link".len());
    assert!(display.runs.iter().any(|run| run.bold));
    assert_eq!(
        display.first_link_target.as_deref(),
        Some("https://example.test/path")
    );
    assert!(
        display
            .runs
            .iter()
            .filter_map(|run| run.text.as_deref())
            .map(str::len)
            .sum::<usize>()
            <= 4 * 1024
    );
}

// Build an export request for worker tests.
fn new_export_request(document: &crate::document::Document, destination: PathBuf) -> ExportRequest {
    use crate::export::{
        ExportFormat, ExportOptions, ExportRequest, ExportScope, ExportTableMeta, FormulaOutput,
        MarkdownHeader, ModifierOutput,
    };

    ExportRequest {
        document_id: document.id,
        revision: document.revision,
        options: ExportOptions {
            format: ExportFormat::Csv,
            scope: ExportScope::Selected(
                document
                    .tables
                    .first()
                    .expect("worker export test has a table")
                    .id,
            ),
            formulas: FormulaOutput::Formula,
            modifiers: ModifierOutput::Convert,
            markdown_header: MarkdownHeader::UseFirstRow,
        },
        destination,
        source_path: None,
        source: None,
        source_report: None,
        tables: document
            .tables
            .iter()
            .map(|table| ExportTableMeta {
                id: table.id,
                name: table.name.clone(),
                rows: table.row_count(),
                columns: table.column_count(),
            })
            .collect(),
        changes: document.changed_cell_snapshot(),
        format_changes: Vec::new(),
        formula_results: None,
    }
}

#[test]
// Verify that a prepared export can save without marking later document edits as saved.
fn prepared_export_saves_without_marking_the_document_saved() {
    let directory = TestDirectory::new();
    let path = directory.0.join("export.csv");
    let mut document = crate::document::Document::new(21, "dirty", &["Sheet1"]);
    document
        .set_cell(
            1,
            crate::document::CellAddress { row: 0, column: 0 },
            Some(crate::document::Cell::text("value")),
        )
        .expect("edit");
    let revision = document.revision;
    let worker = FileWorker::new();
    let prepare_token = JobToken {
        document_id: 21,
        revision,
        job_id: 1,
    };
    worker
        .prepare_export(prepare_token, new_export_request(&document, path.clone()))
        .expect("enqueue preflight");
    let prepared_result = wait_for_result(&worker);
    let FileResultKind::ExportPrepared(prepared) = prepared_result.kind else {
        panic!("expected preflight result");
    };
    assert!(!prepared.report.requires_confirmation());
    let save_token = JobToken {
        document_id: 21,
        revision,
        job_id: 2,
    };
    worker
        .save_export(save_token, prepared.artifacts)
        .expect("enqueue export save");
    let saved_result = wait_for_result(&worker);
    let FileResultKind::Exported {
        written,
        failed,
        not_written,
    } = saved_result.kind
    else {
        panic!("expected export result");
    };
    assert_eq!(written, vec![path.clone()]);
    assert!(failed.is_none());
    assert!(not_written.is_empty());
    assert_eq!(fs::read(path).expect("read export"), b"value\n");
    assert!(document.is_dirty());
    assert_eq!(document.saved_revision, 0);
}

#[test]
// Verify that export refuses a target path created after preflight.
fn export_refuses_a_path_created_after_preflight() {
    let directory = TestDirectory::new();
    let path = directory.0.join("race.csv");
    let mut document = crate::document::Document::new(22, "dirty", &["Sheet1"]);
    document
        .set_cell(
            1,
            crate::document::CellAddress { row: 0, column: 0 },
            Some(crate::document::Cell::text("value")),
        )
        .expect("edit");
    let worker = FileWorker::new();
    let prepare_token = JobToken {
        document_id: 22,
        revision: document.revision,
        job_id: 1,
    };
    worker
        .prepare_export(prepare_token, new_export_request(&document, path.clone()))
        .expect("enqueue preflight");
    let prepared_result = wait_for_result(&worker);
    let FileResultKind::ExportPrepared(prepared) = prepared_result.kind else {
        panic!("expected preflight result");
    };
    fs::write(&path, b"external").expect("simulate competing writer");
    let save_token = JobToken {
        document_id: 22,
        revision: document.revision,
        job_id: 2,
    };
    worker
        .save_export(save_token, prepared.artifacts)
        .expect("enqueue export save");
    let saved_result = wait_for_result(&worker);
    let FileResultKind::Exported {
        written, failed, ..
    } = saved_result.kind
    else {
        panic!("expected partial export result");
    };
    assert!(written.is_empty());
    assert!(failed.is_some());
    assert_eq!(fs::read(path).expect("read competing file"), b"external");
}

#[test]
// Verify that cancelled export leaves its target path absent.
fn cancelled_export_does_not_create_the_output_path() {
    let directory = TestDirectory::new();
    let path = directory.0.join("cancelled.csv");
    let token = JobToken {
        document_id: 23,
        revision: 1,
        job_id: 1,
    };
    let result = run_export(
        token,
        vec![ExportArtifact {
            path: path.clone(),
            bytes: b"value\n".to_vec(),
            expected_identity: None,
            replace_existing: false,
        }],
        Arc::new(AtomicBool::new(true)),
    );
    assert!(matches!(result.kind, FileResultKind::Cancelled));
    assert!(!path.exists());
}

#[test]
// Verify that asynchronous CSV load, edit, and save preserve request revisions.
fn asynchronous_csv_load_edit_and_save_preserve_the_request_revision() {
    let directory = TestDirectory::new();
    let path = directory.0.join("worker.csv");
    fs::write(&path, b"a,b\nx,y\n").expect("create CSV input");
    let worker = FileWorker::new();
    let load_token = JobToken {
        document_id: 7,
        revision: 0,
        job_id: 1,
    };
    worker
        .load_csv(load_token, path.clone(), EncodingChoice::Auto, None)
        .expect("enqueue CSV load");
    let loaded = wait_for_result(&worker);
    assert_eq!(loaded.token, load_token);
    let (source, mut document, identity) = match loaded.kind {
        FileResultKind::LoadedCsv {
            source,
            document,
            identity,
            ..
        } => (source, document, identity),
        other => panic!("expected parsed CSV, got {other:?}"),
    };
    let address = crate::document::CellAddress { row: 1, column: 0 };
    document
        .set_cell(1, address, Some(crate::document::Cell::text("changed")))
        .expect("edit loaded cell");
    let overlay_changes = Arc::new(document.changed_cell_snapshot());
    let text_overlay = TextOverlaySaveRequest {
        base: document.text_overlay_snapshot(),
        changes: Arc::clone(&overlay_changes),
        table_ids: Arc::new(document.tables.iter().map(|table| table.id).collect()),
        string_cells: Arc::new(Default::default()),
    };
    let changes = overlay_changes
        .iter()
        .map(|(_, address, cell)| CsvCellChange {
            row: address.row,
            column: address.column,
            value: cell.as_ref().map(|cell| cell.input.source_text()),
        })
        .collect();
    let save_token = JobToken {
        document_id: 7,
        revision: document.revision,
        job_id: 2,
    };
    worker
        .save_csv(
            save_token,
            path.clone(),
            CsvSaveRequest {
                source,
                table_id: 1,
                changes,
                text_overlay,
            },
            SaveExpectation::Matches(identity),
        )
        .expect("enqueue CSV save");
    let saved = wait_for_result(&worker);
    assert_eq!(saved.token, save_token);
    assert!(matches!(saved.kind, FileResultKind::Saved { .. }));
    assert_eq!(fs::read(path).expect("read saved CSV"), b"a,b\nchanged,y\n");
    document.mark_saved(save_token.revision);
    assert!(!document.is_dirty());
}

#[test]
// Verify that text readers reject files with an XLSX archive signature.
fn text_readers_reject_a_zip_workbook_signature_as_a_format_mismatch() {
    let directory = TestDirectory::new();
    let csv_path = directory.0.join("workbook.csv");
    let markdown_path = directory.0.join("workbook.md");
    let workbook_stub = b"PK\x03\x04not-a-text-document";
    fs::write(&csv_path, workbook_stub).expect("create mislabeled CSV");
    fs::write(&markdown_path, workbook_stub).expect("create mislabeled Markdown");
    let worker = FileWorker::new();
    let csv_token = JobToken {
        document_id: 70,
        revision: 0,
        job_id: 1,
    };
    worker
        .load_csv(csv_token, csv_path, EncodingChoice::Auto, None)
        .expect("enqueue CSV read");
    let csv_result = wait_for_result(&worker);
    assert!(matches!(
        csv_result.kind,
        FileResultKind::Failed(message)
            if message.resolve(crate::i18n::Language::Japanese).contains("XLSX")
    ));

    let markdown_token = JobToken {
        document_id: 71,
        revision: 0,
        job_id: 2,
    };
    worker
        .load_markdown(markdown_token, markdown_path)
        .expect("enqueue Markdown read");
    let markdown_result = wait_for_result(&worker);
    assert!(matches!(
        markdown_result.kind,
        FileResultKind::Failed(message)
            if message.resolve(crate::i18n::Language::Japanese).contains("XLSX")
    ));
}

#[test]
// Verify that quoted CSV records are not mistaken for Markdown tables.
fn csv_quoted_records_do_not_trigger_markdown_table_detection() {
    let directory = TestDirectory::new();
    for (job_id, name, bytes, columns, expected_cell) in [
        (
            1,
            "quoted-multiline.csv",
            &b"id,note\n1,\"left | right\n--- | ---\nlast\"\n"[..],
            2,
            (1, 1, "left | right\n--- | ---\nlast"),
        ),
        (
            2,
            "quoted-single-column.csv",
            &b"\"A | B\"\n\"--- | ---\"\n"[..],
            1,
            (1, 0, "--- | ---"),
        ),
    ] {
        let path = directory.0.join(name);
        fs::write(&path, bytes).expect("create valid quoted CSV");
        let token = JobToken {
            document_id: 72,
            revision: 0,
            job_id,
        };
        let worker = FileWorker::new();
        worker
            .load_csv(token, path, EncodingChoice::Auto, Some(','))
            .expect("enqueue valid quoted CSV read");

        let result = wait_for_result(&worker);
        let FileResultKind::LoadedCsv {
            source, document, ..
        } = result.kind
        else {
            panic!("quoted CSV records remain a CSV document");
        };
        assert_eq!(source.records.len(), 2);
        assert_eq!(source.records[0].fields.len(), columns);
        assert_eq!(source.records[1].fields.len(), columns);
        let (row, column, value) = expected_cell;
        assert_eq!(source.records[row].fields[column].value, value);
        assert_eq!(
            document.tables[0]
                .cell(crate::document::CellAddress { row, column })
                .unwrap()
                .input
                .source_text(),
            value
        );
    }

    let markdown_path = directory.0.join("markdown-in-csv.csv");
    fs::write(&markdown_path, b"| A | B |\n| --- | --- |\n| x | y |\n")
        .expect("create Markdown mismatch fixture");
    let worker = FileWorker::new();
    worker
        .load_csv(
            JobToken {
                document_id: 72,
                revision: 0,
                job_id: 3,
            },
            markdown_path,
            EncodingChoice::Auto,
            Some(','),
        )
        .expect("enqueue Markdown mismatch check");
    assert!(matches!(
        wait_for_result(&worker).kind,
        FileResultKind::Failed(message)
            if message
                .resolve(crate::i18n::Language::Japanese)
                .contains("Markdown")
    ));
}

#[test]
// Verify that saving a Markdown cell edit keeps unrelated source text.
fn asynchronous_markdown_load_and_cell_save_keep_unrelated_source() {
    let directory = TestDirectory::new();
    let path = directory.0.join("tables.md");
    let original = include_bytes!("../samples/tables.md");
    fs::write(&path, original).expect("create Markdown input");
    let worker = FileWorker::new();
    let load_token = JobToken {
        document_id: 9,
        revision: 0,
        job_id: 1,
    };
    worker
        .load_markdown(load_token, path.clone())
        .expect("enqueue Markdown load");
    let loaded = wait_for_result(&worker);
    assert_eq!(loaded.token, load_token);
    let (source, mut document, identity) = match loaded.kind {
        FileResultKind::LoadedMarkdown {
            source,
            document,
            identity,
            ..
        } => (source, document, identity),
        other => panic!("expected parsed Markdown, got {other:?}"),
    };
    assert_eq!(document.tables[0].name, "商品");
    document
        .set_cell(
            1,
            crate::document::CellAddress { row: 1, column: 0 },
            Some(crate::document::Cell::text("**変更|済み**")),
        )
        .expect("edit table cell");
    let overlay_changes = Arc::new(document.changed_cell_snapshot());
    let text_overlay = TextOverlaySaveRequest {
        base: document.text_overlay_snapshot(),
        changes: Arc::clone(&overlay_changes),
        table_ids: Arc::new(document.tables.iter().map(|table| table.id).collect()),
        string_cells: Arc::new(Default::default()),
    };
    let changes = overlay_changes
        .iter()
        .filter_map(|(table_id, address, cell)| {
            document
                .tables
                .iter()
                .position(|table| table.id == *table_id)
                .map(|table| MarkdownCellChange {
                    table,
                    row: address.row,
                    column: address.column,
                    value: cell
                        .as_ref()
                        .map_or_else(String::new, |cell| cell.input.source_text()),
                })
        })
        .collect();
    let save_token = JobToken {
        document_id: 9,
        revision: document.revision,
        job_id: 2,
    };
    worker
        .save_markdown(
            save_token,
            path.clone(),
            source,
            changes,
            text_overlay,
            SaveExpectation::Matches(identity),
        )
        .expect("enqueue Markdown save");
    let saved = wait_for_result(&worker);
    assert_eq!(saved.token, save_token);
    assert!(matches!(saved.kind, FileResultKind::Saved { .. }));
    let output = fs::read_to_string(path).expect("read saved Markdown");
    assert!(output.contains("| **変更\\|済み** | 1200 | 3 | =B2*C2 |"));
    assert!(output.contains("この文章はセル編集で変化しない。"));
    assert!(output.contains("```markdown\n| 偽の表 | 編集対象外 |"));
}

#[test]
// Verify that save creates a new file and rejects a target created during the operation.
fn save_creates_new_file_and_rejects_a_racing_target() {
    let directory = TestDirectory::new();
    let path = directory.0.join("new.csv");
    let identity = atomic_save(
        &path,
        b"first,source\n",
        &SaveExpectation::Absent,
        1,
        &AtomicBool::new(false),
    )
    .expect("create the requested file");
    assert_eq!(
        fs::read(&path).expect("read completed file"),
        b"first,source\n"
    );

    fs::write(&path, b"external edit").expect("simulate a concurrent creator");
    let result = atomic_save(
        &path,
        b"must not replace",
        &SaveExpectation::Absent,
        2,
        &AtomicBool::new(false),
    );
    assert_eq!(
        result.expect_err("absent target now exists").kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(
        fs::read(&path).expect("original file remains"),
        b"external edit"
    );
    assert_ne!(
        FileIdentity::read(&path).expect("identify changed target"),
        identity
    );
}

#[test]
// Verify that save conflicts preserve external data and matching files replace atomically.
fn save_conflict_keeps_external_contents_and_matching_identity_replaces_atomically() {
    let directory = TestDirectory::new();
    let path = directory.0.join("existing.csv");
    fs::write(&path, b"original").expect("create source file");
    let expected = FileIdentity::read(&path).expect("capture source identity");

    fs::write(&path, b"changed elsewhere").expect("simulate external modification");
    let conflict = atomic_save(
        &path,
        b"local snapshot",
        &SaveExpectation::Matches(expected.clone()),
        3,
        &AtomicBool::new(false),
    );
    assert!(conflict.is_err());
    assert_eq!(
        fs::read(&path).expect("external content stays intact"),
        b"changed elsewhere"
    );

    let current = FileIdentity::read(&path).expect("capture current identity");
    let identity = atomic_save(
        &path,
        b"local snapshot",
        &SaveExpectation::Matches(current),
        4,
        &AtomicBool::new(false),
    )
    .expect("replace only the confirmed version");
    assert_eq!(
        fs::read(&path).expect("read replaced file"),
        b"local snapshot"
    );
    assert_eq!(identity.size, b"local snapshot".len() as u64);
    assert_eq!(
        FileIdentity::read(&path).expect("identify saved file"),
        identity
    );
}

#[test]
// Verify that a path alias still uses the loaded file identity.
fn path_alias_for_the_loaded_file_uses_its_original_identity() {
    let directory = TestDirectory::new();
    let subdirectory = directory.0.join("sub");
    fs::create_dir(&subdirectory).expect("create path component");
    let source_path = directory.0.join("private.csv");
    let alias_path = subdirectory.join("..").join("private.csv");
    fs::write(&source_path, b"original").expect("create source");
    let loaded_identity = FileIdentity::read(&source_path).expect("capture source identity");
    fs::write(&source_path, b"external edit").expect("simulate outside edit");

    let result = atomic_save(
        &alias_path,
        b"local edit",
        &SaveExpectation::KnownPath {
            source_path,
            identity: loaded_identity,
        },
        8,
        &AtomicBool::new(false),
    );
    assert!(result.is_err());
    assert_eq!(
        fs::read(alias_path).expect("read external contents"),
        b"external edit"
    );
}

#[cfg(unix)]
#[test]
// Verify that atomic save replacement keeps existing Unix permissions.
fn atomic_replacement_preserves_existing_unix_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let directory = TestDirectory::new();
    let path = directory.0.join("private.csv");
    fs::write(&path, b"old").expect("create source");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("set source permissions");
    let expected = FileIdentity::read(&path).expect("capture source identity");
    atomic_save(
        &path,
        b"new",
        &SaveExpectation::Matches(expected),
        9,
        &AtomicBool::new(false),
    )
    .expect("replace source atomically");

    assert_eq!(
        fs::metadata(path)
            .expect("stat saved file")
            .permissions()
            .mode()
            & 0o7777,
        0o600
    );
}

#[test]
// Verify that source size limits are checked before file contents are read.
fn source_size_limit_is_checked_before_allocating_the_file_contents() {
    let directory = TestDirectory::new();
    let path = directory.0.join("large.csv");
    File::create(&path)
        .and_then(|file| file.set_len(MAX_SOURCE_BYTES + 1))
        .expect("create sparse oversized fixture");
    let error = read_source(&path, &AtomicBool::new(false)).expect_err("reject oversized input");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("64 MiB"));
}

#[test]
// Verify that cancelling a save does not change the target file.
fn cancelled_save_leaves_the_target_untouched() {
    let directory = TestDirectory::new();
    let path = directory.0.join("cancelled.csv");
    fs::write(&path, b"old").expect("create source file");
    let expected = FileIdentity::read(&path).expect("capture source identity");
    let cancelled = AtomicBool::new(true);
    let result = atomic_save(
        &path,
        b"new",
        &SaveExpectation::Matches(expected),
        5,
        &cancelled,
    );
    assert_eq!(
        result.expect_err("cancel before replacement").kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(fs::read(&path).expect("source remains"), b"old");
}

#[test]
// Catch cancellation delivered while the final identity check is running.
fn cancellation_after_final_identity_check_stops_before_publish() {
    let directory = TestDirectory::new();
    let path = directory.0.join("cancel-after-identity.csv");
    fs::write(&path, b"old").expect("create source file");
    let expected = FileIdentity::read(&path).expect("capture source identity");
    let cancelled = AtomicBool::new(false);

    verify_expectation(&path, &SaveExpectation::Matches(expected), &cancelled)
        .expect("the destination still matches");
    cancelled.store(true, Ordering::Release);

    assert_eq!(
        check_not_cancelled_before_publish(&cancelled)
            .expect_err("cancellation is checked after identity verification")
            .kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(fs::read(&path).expect("source remains"), b"old");
}

#[test]
// Wakes the UI when a result is ready, so the window that takes results draws even when the
// job was started from another window.
fn worker_wakes_the_ui_when_a_result_is_ready() {
    let directory = std::env::temp_dir().join(format!("texcel-worker-wake-{}", std::process::id()));
    fs::create_dir_all(&directory).expect("create the test directory");
    let path = directory.join("wake.md");
    fs::write(&path, "| A |\n| --- |\n| 1 |\n").expect("write the input");
    let worker = FileWorker::new();
    let (woken, wakes) = std::sync::mpsc::channel();
    worker.set_waker(move || {
        let _ = woken.send(());
    });
    let token = JobToken {
        document_id: 1,
        revision: 0,
        job_id: 1,
    };
    worker.load_markdown(token, path).expect("enqueue the load");
    wakes
        .recv_timeout(std::time::Duration::from_secs(3))
        .expect("the worker wakes the UI");
    assert_eq!(wait_for_result(&worker).token, token);
    let _ = fs::remove_dir_all(&directory);
}
