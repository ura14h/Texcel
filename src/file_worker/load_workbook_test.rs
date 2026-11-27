use super::*;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Waits for the worker's single result.
fn wait_for_result(worker: &FileWorker) -> FileResult {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(result) = worker.poll().into_iter().next() {
            return result;
        }
        assert!(Instant::now() < deadline, "file import completes");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
/// Imports the sample ODS through the production worker into Markdown with a full report.
fn ods_import_builds_markdown_and_reports_losses() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock is after the epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("texcel-ods-import-{unique}.ods"));
    let original = include_bytes!("../../samples/workbook.ods");
    std::fs::write(&path, original).expect("write ODS source");
    let worker = FileWorker::new();
    let token = JobToken {
        document_id: 8830,
        revision: 0,
        job_id: 30,
    };
    worker
        .import_workbook_with_language(
            token,
            path.clone(),
            WorkbookFormat::Ods,
            crate::i18n::Language::English,
        )
        .expect("queue ODS import");
    let result = wait_for_result(&worker);
    assert_eq!(result.operation, FileOperation::LoadWorkbook);
    let FileResultKind::ImportedMarkdown {
        source,
        report,
        input_format,
        ..
    } = result.kind
    else {
        panic!(
            "ODS import returns a Markdown editing model: {:?}",
            result.kind
        );
    };
    assert_eq!(input_format, ImportedFormat::Ods);
    assert_eq!(report.input_format, "ODS");
    let names = source
        .tables
        .iter()
        .map(|table| table.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, ["売上", "型と書式"]);
    let sales = &source.tables[0];
    assert_eq!(sales.rows.len(), 4, "repeated empty rows are not written");
    assert_eq!(sales.rows[1][3].value, "=B2*C2");
    assert_eq!(sales.rows[3][3].value, "=SUM(D2:D3)");
    let types = &source.tables[1];
    assert!(
        types.rows[2][0].value.contains("*重要*"),
        "{}",
        types.rows[2][0].value
    );
    assert_eq!(types.rows[2][1].value, "[Example](https://example.com/)");
    // The other-sheet formula becomes its saved value; the local one stays a formula.
    assert_eq!(types.rows[5][0].value, "5200");
    assert_eq!(types.rows[5][1].value, "=IF(A1=\"a;b\",1,2)");
    let has = |key: &str| {
        report.entries.iter().any(|entry| {
            entry
                .message
                .as_ref()
                .is_some_and(|message| message.key() == key)
        })
    };
    for key in [
        "report-reason-xlsx-external-formula-value",
        "report-reason-merge-collapsed",
        "report-reason-cell-style-unsupported",
        "report-reason-table-layout-unsupported",
        "report-reason-ods-unsupported-element",
    ] {
        assert!(has(key), "the report includes {key}");
    }
    assert_eq!(std::fs::read(&path).expect("read source again"), original);
    std::fs::remove_file(path).expect("remove ODS fixture");
}
