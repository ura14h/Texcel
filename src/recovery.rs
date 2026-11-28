//! Stores crash-recovery snapshots without blocking the UI thread.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TryRecvError, TrySendError};
use std::thread::{self, JoinHandle};

use serde::{Deserialize, Serialize};

use crate::csv_adapter::{CsvDocument, CsvEncoding, EncodingChoice};
use crate::document::{Cell, CellAddress, Document, DocumentId, TextOverlayCell};
use crate::file_worker::FileIdentity;
use crate::markdown_adapter::MarkdownDocument;
use crate::source::SourceDocument;

const MAX_RECOVERY_BYTES: u64 = 128 * 1024 * 1024;
const REQUEST_CAPACITY: usize = 8;

/// An immutable source root shared with the recovery worker.
#[derive(Clone)]
pub(crate) enum RecoveryBase {
    Csv(Arc<CsvDocument>),
    Markdown(Arc<MarkdownDocument>),
}

/// The active cell editor buffer saved without an egui or IME session identifier.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct RecoveryCellEdit {
    pub table_id: u64,
    pub address: CellAddress,
    pub original: crate::document::CellInput,
    pub buffer: String,
    pub cursor_byte: usize,
    pub anchor_byte: usize,
}

/// One bounded request containing the document's sparse changes and current source root.
pub(crate) struct RecoverySnapshot {
    pub key: String,
    pub document_id: DocumentId,
    pub name: String,
    pub original_path: Option<PathBuf>,
    pub original_identity: Option<FileIdentity>,
    /// Keeps a detached recovery marked unsaved across another restart.
    pub requires_initial_save: bool,
    pub revision: u64,
    pub saved_revision: u64,
    pub generation: u64,
    pub structure_revision: u64,
    pub base: RecoveryBase,
    pub cells: Vec<(u64, CellAddress, Option<Cell>)>,
    pub tables: Vec<RecoveryTableInfo>,
    pub text_overlay_revision: u64,
    pub text_overlay_cells: Arc<Vec<TextOverlayCell>>,
    pub cell_edit: Option<RecoveryCellEdit>,
}

/// Small table metadata needed to rebuild the document model on recovery.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct RecoveryTableInfo {
    pub id: u64,
    pub name: String,
    pub rows: usize,
    pub columns: usize,
    pub string_cells: Vec<CellAddress>,
    pub header_row: Option<usize>,
    pub selection: crate::document::CellRange,
    pub scroll: crate::document::ScrollPosition,
}

/// The durable recovery record points to a separately written immutable base.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct RecoveryRecord {
    pub key: String,
    pub document_id: DocumentId,
    pub name: String,
    pub original_path: Option<PathBuf>,
    pub original_identity: Option<FileIdentity>,
    pub original_changed: bool,
    /// Keeps a detached recovery marked unsaved across another restart.
    #[serde(default)]
    pub requires_initial_save: bool,
    pub revision: u64,
    pub saved_revision: u64,
    #[serde(default)]
    pub generation: u64,
    pub structure_revision: u64,
    #[serde(default)]
    pub text_overlay_revision: u64,
    #[serde(default)]
    pub text_overlay_cells: Vec<TextOverlayCell>,
    pub base_generation: (u64, u64),
    pub base_file: String,
    pub cells: Vec<(u64, CellAddress, Option<Cell>)>,
    pub tables: Vec<RecoveryTableInfo>,
    pub cell_edit: Option<RecoveryCellEdit>,
}

/// One saved recovery record and its already-read base payload.
#[derive(Clone, Debug)]
pub(crate) struct RecoveryItem {
    pub record: RecoveryRecord,
    pub base: RecoveryBasePayload,
}

/// Serializable source data used to reconstruct recovery without reading the original file.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) enum RecoveryBasePayload {
    Csv {
        bytes: Vec<u8>,
        encoding: CsvEncoding,
        delimiter: char,
    },
    Markdown {
        bytes: Vec<u8>,
    },
}

/// Worker messages returned to the app.
#[derive(Debug)]
pub(crate) enum RecoveryEvent {
    Loaded(Result<(Vec<RecoveryItem>, usize), crate::i18n::Message>),
    Saved {
        key: String,
        revision: u64,
        generation: u64,
        result: Result<(), crate::i18n::Message>,
    },
    Removed {
        key: String,
        result: Result<(), crate::i18n::Message>,
    },
    Restored {
        key: String,
        /// Boxed because a restored document is much larger than the other events.
        result: Result<Box<RecoveredDocument>, crate::i18n::Message>,
    },
}

/// A restored document keeps the saved source as its base and changes as dirty deltas.
#[derive(Debug)]
pub(crate) struct RecoveredDocument {
    pub key: String,
    pub generation: u64,
    pub document: Document,
    pub cell_edit: Option<RecoveryCellEdit>,
    pub original_changed: bool,
    pub requires_initial_save: bool,
    pub original_path: Option<PathBuf>,
    pub original_identity: Option<FileIdentity>,
    pub calculation: crate::calc::CalculationSnapshot,
    pub import_report: Option<Arc<crate::conversion_report::ImportReport>>,
}

mod storage;
pub(crate) use storage::RecoveryWorker;

#[cfg(test)]
#[path = "recovery_test.rs"]
mod tests;
