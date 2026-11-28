use super::*;

mod search_data;
pub(super) use search_data::*;

/// Captures the document versions and live input authorized for one discard.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct AppCloseDiscardState {
    pub(super) settings_generation: u64,
    pub(super) document_order: Vec<DocumentId>,
    pub(super) document_versions: BTreeMap<DocumentId, (u64, u64, u64, u64)>,
    pub(super) drafts: BTreeMap<DocumentId, DiscardDraftState>,
    pub(super) recovered_edits: BTreeMap<DocumentId, crate::recovery::RecoveryCellEdit>,
    pub(super) recovered_needing_save: BTreeSet<DocumentId>,
    pub(super) recovery_keys: BTreeMap<DocumentId, String>,
    pub(super) pending_cleanups: BTreeMap<String, (u64, u64)>,
    pub(super) queued_removals: BTreeSet<String>,
    pub(super) backings: BTreeMap<DocumentId, (PathBuf, FileIdentity)>,
    pub(super) window_edits:
        BTreeMap<egui::ViewportId, (Option<DocumentId>, Option<DiscardEditState>)>,
}

/// Stores the small editable state of one Markdown source draft.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct DiscardDraftState {
    pub(super) id: u64,
    pub(super) revision: u64,
    pub(super) cursor: usize,
    pub(super) anchor: usize,
    pub(super) composing: bool,
    pub(super) changed: bool,
}

/// Stores only the active cell buffer and its selection for discard checks.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct DiscardEditState {
    pub(super) session_id: u64,
    pub(super) table_id: Option<u64>,
    pub(super) address: Option<CellAddress>,
    pub(super) target: Option<crate::document::EditTarget>,
    pub(super) original: Option<crate::document::CellInput>,
    pub(super) buffer: String,
    pub(super) cursor_byte: usize,
    pub(super) anchor_byte: usize,
    pub(super) window_start: Option<usize>,
    pub(super) composing: bool,
}

#[derive(Clone, Debug)]
pub(super) struct FileBacking {
    pub(super) path: PathBuf,
    pub(super) identity: FileIdentity,
    pub(super) source: Option<SourceDocument>,
    // Set after the user chose to overwrite a file that changed on disk.
    pub(super) overwrite_confirmed: bool,
}

#[derive(Clone, Debug)]
pub(super) struct RecoverySchedule {
    pub(super) signature: Option<u64>,
    pub(super) generation: u64,
    pub(super) changed_at: Instant,
    pub(super) interval_started: Instant,
    pub(super) last_success: Option<Instant>,
    pub(super) in_flight: Option<(u64, u64)>,
}

#[derive(Clone, Debug)]
pub(super) struct ExportDialogState {
    pub(super) document_id: DocumentId,
    pub(super) options: ExportOptions,
    pub(super) save_intent: Option<SaveDialogIntent>,
}

/// Stores the output choices for a one-time clipboard copy.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct CopyOptionsDialog {
    pub(super) content: CopyContentChoice,
    pub(super) format: CopyFormatChoice,
}

/// Selects visible values or original cell source for a special copy.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum CopyContentChoice {
    #[default]
    Display,
    Source,
    Standard,
}

/// Selects the plain-text structure used by a special copy.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum CopyFormatChoice {
    #[default]
    Tsv,
    Csv,
    Markdown,
    Html,
}

impl CopyFormatChoice {
    /// Maps the displayed format choice to the existing clipboard renderer.
    pub(super) fn clipboard_format(self) -> CopyFormat {
        match self {
            Self::Tsv => CopyFormat::DisplayTsv,
            Self::Csv => CopyFormat::Csv,
            Self::Markdown => CopyFormat::MarkdownTable,
            Self::Html => CopyFormat::Rich,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct SaveDialogIntent {
    pub(super) track_recent: bool,
    pub(super) allow_formula_results: bool,
    pub(super) close_after_save: bool,
    pub(super) close_edit: Option<EditSession>,
    pub(super) app_close_generation: Option<u64>,
}

#[derive(Clone, Debug)]
pub(super) struct CellLinkDialog {
    pub(super) document_id: DocumentId,
    pub(super) table_id: u64,
    pub(super) range: CellRange,
    pub(super) target: String,
    pub(super) tooltip: String,
    pub(super) location: bool,
    pub(super) focus_input: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct TextSelection {
    pub(super) document_id: DocumentId,
    pub(super) table_id: u64,
    pub(super) address: CellAddress,
    pub(super) session_id: u64,
    pub(super) buffer_revision: u64,
    pub(super) start: usize,
    pub(super) end: usize,
}

#[derive(Clone, Copy)]
pub(super) enum CellEditWidget {
    Singleline {
        width: f32,
        frame_none: bool,
    },
    FormulaBarSingleline {
        width: f32,
        height: f32,
    },
    Multiline {
        width: f32,
        rows: usize,
        lock_focus: bool,
    },
}

pub(super) struct BoundedEditOutput {
    pub(super) response: egui::WidgetAtomResponse,
    pub(super) selection_bytes: Option<std::ops::Range<usize>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CellFormatAction {
    Bold,
    Italic,
    Strike,
    Code,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum LayoutAction {
    EditColumnWidth,
    AutoFitAllColumns,
    FreezeCells,
    FreezeRows,
    FreezeColumns,
    Unfreeze,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StructureAction {
    InsertRows,
    InsertColumns,
    DeleteRows,
    DeleteColumns,
    DuplicateRows,
    DuplicateColumns,
    MoveRowsUp,
    MoveRowsDown,
    MoveColumnsLeft,
    MoveColumnsRight,
    FillCopy,
    FillSeries,
    FillDown,
    FillRight,
    AddTable,
    DuplicateTable,
    DeleteTable,
    RenameTable,
    MarkdownAlignLeft,
    MarkdownAlignCenter,
    MarkdownAlignRight,
    PromoteCsv,
}

#[derive(Clone, Debug)]
pub(super) struct LayoutDialogState {
    pub(super) document_id: DocumentId,
    pub(super) table_id: u64,
    pub(super) address: CellAddress,
    pub(super) value: String,
    pub(super) focus_input: bool,
}

#[derive(Debug)]
pub(super) struct ExportConfirmation {
    pub(super) document_id: DocumentId,
    pub(super) options: ExportOptions,
    pub(super) destination: PathBuf,
    pub(super) prepared: PreparedExport,
    pub(super) selected_group: usize,
    pub(super) queue_error: Option<Message>,
}

#[derive(Clone, Debug)]
pub(super) enum CsvLoadPrompt {
    Settings(CsvLoadSettingsPrompt),
}

#[derive(Clone, Debug)]
pub(super) struct CsvLoadSettingsPrompt {
    pub(super) candidates: Vec<char>,
    pub(super) delimiter_text: String,
    pub(super) previews: BTreeMap<char, Vec<Vec<String>>>,
    pub(super) selected_encoding: EncodingChoice,
    pub(super) needs_encoding: bool,
    pub(super) error: Option<&'static str>,
}

#[derive(Clone, Debug)]
pub(super) struct PendingCsvOpen {
    pub(super) path: PathBuf,
    pub(super) encoding: EncodingChoice,
    pub(super) delimiter: Option<char>,
    pub(super) prompt: Option<CsvLoadPrompt>,
    pub(super) job_id: u64,
    pub(super) retry_submission: Option<CsvSubmission>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CsvSubmission {
    Options,
    Load,
}

/// Keeps confirmed rows in a separate read-only view until load finishes.
#[derive(Clone, Debug)]
pub(super) struct LoadPreviewState {
    pub(super) token: JobToken,
    pub(super) path: PathBuf,
    pub(super) stage: LoadStage,
    pub(super) selected_table: usize,
    pub(super) tables: BTreeMap<usize, LoadPreviewTable>,
}

/// Indexes chunks by row start so only visible cells need to be inspected.
#[derive(Clone, Debug)]
pub(super) struct LoadPreviewTable {
    pub(super) name: String,
    pub(super) row_count: usize,
    pub(super) column_count: usize,
    pub(super) chunks: BTreeMap<usize, std::sync::Arc<LoadPreviewChunk>>,
}

impl LoadPreviewTable {
    /// Adds one owned chunk without walking through its cells.
    pub(super) fn add_chunk(&mut self, chunk: std::sync::Arc<LoadPreviewChunk>) {
        self.row_count = self.row_count.max(chunk.last_row.saturating_add(1));
        self.column_count = self.column_count.max(chunk.column_count);
        self.chunks.insert(chunk.first_row, chunk);
    }

    /// Finds one row in the chunk nearest its source index.
    pub(super) fn row(&self, index: usize) -> Option<&LoadPreviewRow> {
        let (_, chunk) = self.chunks.range(..=index).next_back()?;
        chunk
            .rows
            .binary_search_by_key(&index, |row| row.index)
            .ok()
            .map(|row| &chunk.rows[row])
    }
}

#[derive(Clone, Debug)]
pub(super) struct CachedMarkdownCellDisplay {
    pub(super) document_revision: u64,
    pub(super) calculation_revision: u64,
    pub(super) cell_input_revision: Option<(u64, u64)>,
    pub(super) raw_text: Option<String>,
    pub(super) edit_session_id: Option<u64>,
    pub(super) display: MarkdownCellDisplay,
}

pub(super) type MarkdownCellDisplayKey = (DocumentId, u64, CellAddress);
pub(super) type CellInputRevision = (u64, u64);
pub(super) type PendingMarkdownCellDisplay = (JobToken, CellInputRevision);
