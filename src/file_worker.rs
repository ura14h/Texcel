//!  Keeps file loading and saving off the UI thread and ties async results to document versions.
//!
//!  The worker only parses CSV and safely replaces files through a temporary file. Requests use a bounded
//!  queue. Results include the document ID, requested version, and job ID so callers can discard stale results
//!  instead of applying them to a document.

#[path = "file_worker/message.rs"]
mod message;
use self::message::WorkerMessage;

use std::collections::{BTreeSet, HashMap};
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::calc::{CalculationSnapshot, FormulaResultSnapshot};
use crate::conversion_report::{
    ConversionClass, ConversionEntry, ImportReport, XlsxPackageInspection, XlsxPackageLimits,
};
use crate::csv_adapter::{
    CsvCellChange, CsvDocument, DelimiterAnalysis, EncodingChoice, analyze_delimiters,
};
use crate::document::{TextOverlayChange, TextOverlaySnapshot};
use crate::export::{ExportArtifact, ExportRequest, PreparedExport};
use crate::html_adapter::HtmlDocument;
use crate::i18n::Message;
use crate::markdown_adapter::{MarkdownCellChange, MarkdownCellDisplay, MarkdownDocument};
use crate::xlsx_adapter::XlsxDocument;

const JOB_QUEUE_CAPACITY: usize = 4;
const RESULT_QUEUE_CAPACITY: usize = 8;
const IO_CHUNK_SIZE: usize = 64 * 1024;
const MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024;

/// Resolves symlink and `..` aliases on the worker before a file becomes a backing.
pub(super) fn canonical_load_path(path: PathBuf) -> PathBuf {
    fs::canonicalize(&path).unwrap_or(path)
}

// An immutable token that identifies an async document operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct JobToken {
    pub document_id: u64,
    pub revision: u64,
    pub job_id: u64,
}

// The destination state recorded at load time. Compare its content hash as well as its modified time.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FileIdentity {
    pub size: u64,
    pub modified_unix_nanos: Option<u128>,
    pub sha256: [u8; 32],
}

impl FileIdentity {
    // Reads a file in chunks and returns its identity only when metadata stays stable before and after.
    pub fn read(path: &Path) -> io::Result<Self> {
        Self::read_cancellable(path, None)
    }

    // Read file identity data while rejecting symlinks and checking cancellation.
    fn read_cancellable(path: &Path, cancelled: Option<&AtomicBool>) -> io::Result<Self> {
        if fs::symlink_metadata(path)?.file_type().is_symlink() {
            return Err(user_io_error(
                io::ErrorKind::InvalidInput,
                Message::new("status-file-symlink-unsupported"),
            ));
        }
        let before = fs::metadata(path)?;
        if before.len() > MAX_SOURCE_BYTES {
            return Err(source_size_error());
        }
        let mut file = File::open(path)?;
        let mut hasher = Sha256::new();
        let mut size = 0_u64;
        let mut buffer = [0_u8; IO_CHUNK_SIZE];
        loop {
            if cancelled.is_some_and(|flag| flag.load(Ordering::Acquire)) {
                return Err(cancelled_error());
            }
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
            size = size.checked_add(count as u64).ok_or_else(|| {
                user_io_error(
                    io::ErrorKind::InvalidData,
                    Message::new("status-file-size-limit"),
                )
            })?;
            if size > MAX_SOURCE_BYTES {
                return Err(source_size_error());
            }
        }
        let after = fs::metadata(path)?;
        let before_modified = modified_unix_nanos(&before);
        let after_modified = modified_unix_nanos(&after);
        if before.len() != size
            || after.len() != size
            || before_modified != after_modified
            || before.len() != after.len()
        {
            return Err(user_io_error(
                io::ErrorKind::Interrupted,
                Message::new("status-file-changed-during-read"),
            ));
        }
        Ok(Self {
            size,
            modified_unix_nanos: after_modified,
            sha256: hasher.finalize().into(),
        })
    }
}

// The destination state required before saving.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SaveExpectation {
    // The destination does not exist. Used for a new Save As file.
    Absent,
    // The destination matches the identity recorded when it was opened.
    Matches(FileIdentity),
    // If the destination is the same file as the source, use the identity recorded at load time.
    KnownPath {
        source_path: PathBuf,
        identity: FileIdentity,
    },
    // Capture the current destination selected in the OS save dialog, then check it again just before replacement.
    CurrentAtStart,
}

/// How the worker should read a CSV file.
#[derive(Clone, Copy, Debug)]
pub(super) struct CsvLoadRequest {
    pub(super) encoding: EncodingChoice,
    /// The delimiter the user chose, or `None` to detect it.
    pub(super) delimiter: Option<char>,
    /// True to only analyze the encoding and delimiter, without creating a document.
    pub(super) settings_only: bool,
}

/// The parsed CSV source and the edits to patch into its original bytes.
#[derive(Clone, Debug)]
pub struct CsvSaveRequest {
    pub source: Arc<CsvDocument>,
    pub table_id: crate::document::TableId,
    pub changes: Vec<CsvCellChange>,
    pub text_overlay: TextOverlaySaveRequest,
}

/// Saved text-source overlay is prepared on the worker from a base and dirty-cell snapshot.
#[derive(Clone, Debug)]
pub struct TextOverlaySaveRequest {
    pub base: TextOverlaySnapshot,
    pub changes: Arc<Vec<TextOverlayChange>>,
    /// Stable table order maps text overlays back to parsed source tables.
    pub table_ids: Arc<Vec<u64>>,
    /// Cells that must stay literal when their sparse values are written to a text source.
    pub string_cells: Arc<BTreeSet<(u64, crate::document::CellAddress)>>,
}

/// Inputs captured for a header auto-fit on the background worker.
#[derive(Clone, Debug)]
pub struct AutoFitRequest {
    pub structure: crate::structure::StructureRequest,
    /// Columns measured in one pass over the rebuilt table.
    pub columns: Vec<usize>,
    pub layout: crate::document::TableLayout,
    pub display_mode: crate::document::CellDisplayMode,
    pub language: crate::i18n::Language,
    pub wrap_text: bool,
    pub show_invisibles: bool,
    pub calculation: Option<FormulaResultSnapshot>,
    pub font_definitions: eframe::egui::FontDefinitions,
    pub text_options: eframe::egui::epaint::TextOptions,
    pub pixels_per_point: f32,
    pub font_layout_generation: u64,
    pub minimum_column_width_pixels: f32,
    pub maximum_column_width_pixels: f32,
}

// The kind of async operation, so the UI can show clear progress and errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileOperation {
    LoadCsv,
    LoadMarkdown,
    LoadHtml,
    LoadXlsx,
    Save,
    SaveReport,
    PrepareExport,
    Search,
    DataEdit,
    PrepareStructure,
    AutoFit,
    PrepareMarkdownDisplay,
    RestoreStructureHistory,
    Export,
    ExportReport,
}

// The result of a background operation.
#[derive(Debug)]
pub enum FileResultKind {
    LoadedCsv {
        source: Arc<CsvDocument>,
        document: crate::document::Document,
        calculation: CalculationSnapshot,
        identity: FileIdentity,
    },
    LoadedMarkdown {
        source: Arc<MarkdownDocument>,
        document: crate::document::Document,
        calculation: CalculationSnapshot,
        identity: FileIdentity,
    },
    ImportedMarkdown {
        source: Arc<MarkdownDocument>,
        document: crate::document::Document,
        calculation: CalculationSnapshot,
        identity: FileIdentity,
        report: Arc<ImportReport>,
        input_format: ImportedFormat,
    },
    LoadNeedsEncodingChoice {
        suggested: Option<EncodingChoice>,
        analysis: Option<DelimiterAnalysis>,
        error: Option<&'static str>,
    },
    LoadNeedsDelimiterChoice {
        analysis: DelimiterAnalysis,
    },
    Saved {
        identity: FileIdentity,
        csv_source: Option<Arc<CsvDocument>>,
        markdown_source: Option<Arc<MarkdownDocument>>,
        text_overlay: Option<TextOverlaySnapshot>,
    },
    SaveConflict,
    ReportSaved,
    ExportPrepared(PreparedExport),
    SearchPrepared(crate::structure::PreparedSearch),
    DataEditPrepared(crate::structure::PreparedDataEdit),
    StructurePrepared(crate::document::PreparedStructure),
    /// Measured widths, one per requested column, in Excel-style character units.
    AutoFitPrepared {
        widths: Vec<(usize, f64)>,
    },
    MarkdownCellDisplayPrepared {
        table_id: u64,
        address: crate::document::CellAddress,
        cell_input_revision: (u64, u64),
        display: MarkdownCellDisplay,
    },
    StructureHistoryRestored(crate::structure::PreparedStructureRestore),
    Exported {
        written: Vec<PathBuf>,
        failed: Option<(PathBuf, Message)>,
        not_written: Vec<PathBuf>,
    },
    Failed(Message),
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImportedFormat {
    Html,
    Xlsx,
}

impl FileResultKind {
    /// Preserves a typed app message and translates operating system details at the UI.
    pub(super) fn failed(error: io::Error) -> Self {
        Self::Failed(file_error_message(error))
    }
}

// Resolves typed application causes before falling back to operating system details.
pub(crate) fn file_error_message(error: io::Error) -> Message {
    if let Some(message) = error
        .get_ref()
        .and_then(|cause| cause.downcast_ref::<Message>())
    {
        return message.clone();
    }
    let key = match error.kind() {
        io::ErrorKind::NotFound => "status-file-not-found",
        io::ErrorKind::PermissionDenied => "status-file-access-denied",
        io::ErrorKind::StorageFull => "status-file-storage-full",
        io::ErrorKind::AlreadyExists => "status-file-already-exists",
        _ => "status-file-operation-failed",
    };
    Message::new(key)
}

// Carries a translated application cause through APIs that report I/O errors.
pub(super) fn user_io_error(kind: io::ErrorKind, message: Message) -> io::Error {
    io::Error::new(kind, message)
}

// A worker response with its document ID, version, and target path.
#[derive(Debug)]
pub struct FileResult {
    pub token: JobToken,
    pub operation: FileOperation,
    pub path: PathBuf,
    pub kind: FileResultKind,
}

// Why a request could not enter the queue. Callers can retry when full without freezing the UI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubmitError {
    QueueFull,
    Stopped,
}

// A bounded file worker that runs only loading and saving jobs one at a time.
pub struct FileWorker {
    sender: SyncSender<WorkerMessage>,
    results: Receiver<FileResult>,
    load_progress: Option<Receiver<LoadProgress>>,
    cancellation: Arc<Mutex<HashMap<JobToken, Arc<AtomicBool>>>>,
    shutdown: Arc<AtomicBool>,
    stopped: AtomicBool,
    thread: Option<JoinHandle<()>>,
}

mod api_core;
mod autofit;
mod dispatch;
mod html_file;
pub(crate) mod import_markdown;
mod load;
mod load_api;
mod load_progress;
mod load_xlsx;
mod prepare_api;
mod safe_write;
mod save;
mod save_api;
mod save_conversion;
mod save_text_overlay;
mod tasks;
#[cfg(test)]
#[path = "file_worker/test_support.rs"]
pub(crate) mod test_support;

use self::dispatch::*;
use self::html_file::*;
use self::load::*;
use self::load_progress::LoadProgressSender;
use self::load_xlsx::*;
pub(crate) use self::safe_write::replace_file_atomic;
use self::safe_write::*;
use self::save::*;
use self::save_conversion::*;
use self::tasks::*;

#[cfg(test)]
#[path = "file_worker_load_progress_test.rs"]
mod load_progress_tests;
#[cfg(test)]
#[path = "file_worker_test.rs"]
mod tests;

pub use self::load_progress::{
    LoadPreviewCell, LoadPreviewChunk, LoadPreviewRow, LoadProgress, LoadProgressKind, LoadStage,
};
