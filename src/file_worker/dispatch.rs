use super::autofit::run_auto_fit;
use super::*;

// Run queued file jobs and send each result back to the application.
pub(super) fn worker_loop(
    receiver: Receiver<WorkerMessage>,
    results: SyncSender<FileResult>,
    progress: Option<SyncSender<LoadProgress>>,
    shutdown: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::Acquire) {
        let Ok(message) = receiver.recv() else {
            break;
        };
        let message = match message {
            WorkerMessage::DiscardStructureTables { tables } => {
                drop(tables);
                continue;
            }
            message => message,
        };
        let result = match message {
            WorkerMessage::LoadCsv {
                token,
                path,
                request,
                cancelled,
            } => run_load(
                token,
                path,
                request,
                cancelled,
                progress.as_ref(),
                &shutdown,
            ),
            WorkerMessage::LoadMarkdown {
                token,
                path,
                language,
                cancelled,
            } => run_load_markdown(
                token,
                path,
                language,
                cancelled,
                progress.as_ref(),
                &shutdown,
            ),
            WorkerMessage::ImportHtml {
                token,
                path,
                language,
                cancelled,
            } => run_import_html(
                token,
                path,
                language,
                cancelled,
                progress.as_ref(),
                &shutdown,
            ),
            WorkerMessage::ImportWorkbook {
                token,
                path,
                format,
                language,
                cancelled,
            } => run_import_workbook(
                token,
                path,
                format,
                language,
                cancelled,
                progress.as_ref(),
                &shutdown,
            ),
            WorkerMessage::SaveConvertedCsv {
                token,
                path,
                bytes,
                delimiter,
                expected,
                cancelled,
            } => run_save_converted_csv(token, path, bytes, delimiter, expected, cancelled),
            WorkerMessage::SaveReport {
                token,
                path,
                source_path,
                report,
                language,
                cancelled,
            } => run_save_report(token, path, source_path, report, language, cancelled),
            WorkerMessage::PrepareExport {
                token,
                request,
                cancelled,
            } => run_prepare_export(token, request, cancelled),
            WorkerMessage::Search {
                token,
                request,
                cancelled,
            } => run_search(token, request, cancelled),
            WorkerMessage::DataEdit {
                token,
                request,
                cancelled,
            } => run_data_edit(token, *request, cancelled),
            WorkerMessage::PrepareStructure {
                token,
                request,
                cancelled,
            } => run_prepare_structure(token, request, cancelled),
            WorkerMessage::AutoFit {
                token,
                request,
                cancelled,
            } => run_auto_fit(token, *request, cancelled),
            WorkerMessage::PrepareMarkdownCellDisplay {
                token,
                table_id,
                address,
                cell_input_revision,
                source,
                change,
                cancelled,
            } => FileResult {
                token,
                operation: FileOperation::PrepareMarkdownDisplay,
                path: PathBuf::new(),
                kind: if cancelled.load(Ordering::Acquire) {
                    FileResultKind::Cancelled
                } else {
                    FileResultKind::MarkdownCellDisplayPrepared {
                        table_id,
                        address,
                        cell_input_revision,
                        display: source.display_for_cell_change(&change),
                    }
                },
            },
            WorkerMessage::PrepareTableStructure {
                token,
                request,
                action,
                cancelled,
            } => run_prepare_table_structure(token, request, action, cancelled),
            WorkerMessage::RestoreStructureHistory {
                token,
                request,
                cancelled,
            } => run_restore_structure_history(token, request, cancelled),
            WorkerMessage::Export {
                token,
                artifacts,
                cancelled,
            } => run_export(token, artifacts, cancelled),
            WorkerMessage::ExportReport {
                token,
                path,
                source_path,
                report,
                language,
                cancelled,
            } => run_save_export_report(token, path, source_path, report, language, cancelled),
            WorkerMessage::SaveCsv {
                token,
                path,
                request,
                expected,
                cancelled,
            } => run_save_csv(token, path, request, expected, cancelled),
            WorkerMessage::SaveMarkdown {
                token,
                path,
                source,
                changes,
                text_overlay,
                expected,
                cancelled,
            } => run_save_markdown(
                token,
                path,
                source,
                &changes,
                text_overlay,
                expected,
                cancelled,
            ),
            WorkerMessage::DiscardStructureTables { .. } => unreachable!(),
        };
        if results.send(result).is_err() {
            break;
        }
    }
}
