use super::*;

// Describes one bounded job sent to the background file thread.
#[derive(Debug)]
pub(super) enum WorkerMessage {
    LoadCsv {
        token: JobToken,
        path: PathBuf,
        request: CsvLoadRequest,
        cancelled: Arc<AtomicBool>,
    },
    LoadMarkdown {
        token: JobToken,
        path: PathBuf,
        language: crate::i18n::Language,
        cancelled: Arc<AtomicBool>,
    },
    ImportHtml {
        token: JobToken,
        path: PathBuf,
        language: crate::i18n::Language,
        cancelled: Arc<AtomicBool>,
    },
    ImportWorkbook {
        token: JobToken,
        path: PathBuf,
        format: WorkbookFormat,
        language: crate::i18n::Language,
        cancelled: Arc<AtomicBool>,
    },
    SaveConvertedCsv {
        token: JobToken,
        path: PathBuf,
        bytes: Vec<u8>,
        delimiter: char,
        expected: SaveExpectation,
        cancelled: Arc<AtomicBool>,
    },
    SaveReport {
        token: JobToken,
        path: PathBuf,
        source_path: PathBuf,
        report: Arc<ImportReport>,
        language: crate::i18n::Language,
        cancelled: Arc<AtomicBool>,
    },
    PrepareExport {
        token: JobToken,
        request: ExportRequest,
        cancelled: Arc<AtomicBool>,
    },
    Search {
        token: JobToken,
        request: crate::structure::SearchRequest,
        cancelled: Arc<AtomicBool>,
    },
    DataEdit {
        token: JobToken,
        /// Boxed, like the auto-fit request, so small jobs do not carry its size.
        request: Box<crate::structure::DataEditRequest>,
        cancelled: Arc<AtomicBool>,
    },
    PrepareStructure {
        token: JobToken,
        request: crate::structure::StructureRequest,
        cancelled: Arc<AtomicBool>,
    },
    AutoFit {
        token: JobToken,
        request: Box<AutoFitRequest>,
        cancelled: Arc<AtomicBool>,
    },
    PrepareMarkdownCellDisplay {
        token: JobToken,
        table_id: u64,
        address: crate::document::CellAddress,
        cell_input_revision: (u64, u64),
        source: Arc<MarkdownDocument>,
        change: MarkdownCellChange,
        cancelled: Arc<AtomicBool>,
    },
    PrepareTableStructure {
        token: JobToken,
        request: crate::structure::StructureRequest,
        action: crate::structure::TableAction,
        cancelled: Arc<AtomicBool>,
    },
    RestoreStructureHistory {
        token: JobToken,
        request: crate::structure::StructureHistoryRestoreRequest,
        cancelled: Arc<AtomicBool>,
    },
    DiscardStructureTables {
        tables: Vec<crate::document::Table>,
    },
    Export {
        token: JobToken,
        artifacts: Vec<ExportArtifact>,
        cancelled: Arc<AtomicBool>,
    },
    ExportReport {
        token: JobToken,
        path: PathBuf,
        source_path: Option<PathBuf>,
        report: Arc<crate::conversion_report::ExportReport>,
        language: crate::i18n::Language,
        cancelled: Arc<AtomicBool>,
    },
    SaveCsv {
        token: JobToken,
        path: PathBuf,
        request: CsvSaveRequest,
        expected: SaveExpectation,
        cancelled: Arc<AtomicBool>,
    },
    SaveMarkdown {
        token: JobToken,
        path: PathBuf,
        source: Arc<MarkdownDocument>,
        changes: Vec<MarkdownCellChange>,
        text_overlay: TextOverlaySaveRequest,
        expected: SaveExpectation,
        cancelled: Arc<AtomicBool>,
    },
}
