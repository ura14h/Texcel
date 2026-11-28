use super::*;
use std::thread;
use std::time::Duration;

const PREVIEW_CHUNK_ROWS: usize = 128;
const PREVIEW_CHUNK_CELLS: usize = 2_048;
const PREVIEW_CHUNK_BYTES: usize = 64 * 1024;

/// Describes one worker phase or one confirmed read-only row chunk.
#[derive(Clone, Debug)]
pub struct LoadProgress {
    pub token: JobToken,
    pub operation: FileOperation,
    pub path: PathBuf,
    pub kind: LoadProgressKind,
}

/// Gives the UI a clear reading phase before confirmed rows arrive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadStage {
    ReadingSource,
    ParsingSource,
    ParsingWorkbook,
}

/// Separates indeterminate work from confirmed preview data.
#[derive(Clone, Debug)]
pub enum LoadProgressKind {
    Stage(LoadStage),
    Chunk(Arc<LoadPreviewChunk>),
}

/// Holds one owned part of parsed table data for read-only preview.
#[derive(Clone, Debug)]
pub struct LoadPreviewChunk {
    pub table_index: usize,
    pub table_name: String,
    pub first_row: usize,
    pub last_row: usize,
    pub column_count: usize,
    pub rows: Arc<[LoadPreviewRow]>,
}

/// Stores source row and column positions with their display text.
#[derive(Clone, Debug)]
pub struct LoadPreviewRow {
    pub index: usize,
    pub cells: Arc<[LoadPreviewCell]>,
}

/// Stores one visible cell without changing its source type in the final document.
#[derive(Clone, Debug)]
pub struct LoadPreviewCell {
    pub column: usize,
    pub text: String,
}

struct PendingChunk {
    table_index: usize,
    table_name: String,
    rows: Vec<LoadPreviewRow>,
    cells: usize,
    bytes: usize,
    column_count: usize,
}

/// The preview channel to the UI; each accepted preview wakes the UI to take it.
pub(crate) struct ProgressQueue {
    sender: SyncSender<LoadProgress>,
    waker: WorkerWaker,
}

impl ProgressQueue {
    /// Wraps the bounded preview channel with the worker's wake function.
    pub(crate) fn new(sender: SyncSender<LoadProgress>, waker: WorkerWaker) -> Self {
        Self { sender, waker }
    }

    /// Hands one preview to the UI and wakes it when the channel accepted it.
    fn try_send(&self, message: LoadProgress) -> Result<(), TrySendError<LoadProgress>> {
        self.sender.try_send(message)?;
        self.waker.wake();
        Ok(())
    }
}

/// Collects complete rows until one preview chunk should be sent.
pub(super) struct LoadProgressSender<'a> {
    token: JobToken,
    operation: FileOperation,
    path: &'a Path,
    sender: Option<&'a ProgressQueue>,
    cancelled: &'a AtomicBool,
    shutdown: &'a AtomicBool,
    pending: Option<PendingChunk>,
}

impl<'a> LoadProgressSender<'a> {
    /// Creates a progress sender for one active load token.
    pub(super) fn new(
        token: JobToken,
        operation: FileOperation,
        path: &'a Path,
        sender: Option<&'a ProgressQueue>,
        cancelled: &'a AtomicBool,
        shutdown: &'a AtomicBool,
    ) -> Self {
        Self {
            token,
            operation,
            path,
            sender,
            cancelled,
            shutdown,
            pending: None,
        }
    }

    /// Sends a phase update before parsed rows are available.
    pub(super) fn stage(&self, stage: LoadStage) -> bool {
        self.send(LoadProgressKind::Stage(stage))
    }

    /// Reports whether this worker publishes preview rows.
    pub(super) fn enabled(&self) -> bool {
        self.sender.is_some()
    }

    /// Adds one confirmed row and sends a chunk when its target size is reached.
    pub(super) fn push_row(
        &mut self,
        table_index: usize,
        table_name: &str,
        index: usize,
        cells: Vec<LoadPreviewCell>,
    ) -> bool {
        if self.sender.is_none() {
            return true;
        }
        let row_bytes = cells.iter().map(|cell| cell.text.len()).sum::<usize>();
        let pending_changes_table = self.pending.as_ref().is_some_and(|pending| {
            pending.table_index != table_index || pending.table_name != table_name
        });
        let pending_would_overflow = self.pending.as_ref().is_some_and(|pending| {
            !pending.rows.is_empty()
                && (pending.rows.len() >= PREVIEW_CHUNK_ROWS
                    || pending.cells.saturating_add(cells.len()) > PREVIEW_CHUNK_CELLS
                    || pending.bytes.saturating_add(row_bytes) > PREVIEW_CHUNK_BYTES)
        });
        if (pending_changes_table || pending_would_overflow) && !self.flush() {
            return false;
        }

        let pending = self.pending.get_or_insert_with(|| PendingChunk {
            table_index,
            table_name: table_name.to_owned(),
            rows: Vec::new(),
            cells: 0,
            bytes: 0,
            column_count: 0,
        });
        let column_count = cells.last().map_or(0, |cell| cell.column.saturating_add(1));
        pending.cells = pending.cells.saturating_add(cells.len());
        pending.bytes = pending.bytes.saturating_add(row_bytes);
        pending.column_count = pending.column_count.max(column_count);
        pending.rows.push(LoadPreviewRow {
            index,
            cells: cells.into(),
        });
        let target_reached = pending.rows.len() >= PREVIEW_CHUNK_ROWS
            || pending.cells >= PREVIEW_CHUNK_CELLS
            || pending.bytes >= PREVIEW_CHUNK_BYTES;
        !target_reached || self.flush()
    }

    /// Flushes the final short row group when parsing finishes.
    pub(super) fn finish(&mut self) -> bool {
        self.flush()
    }

    /// Moves the pending rows into an Arc and waits for bounded queue space.
    fn flush(&mut self) -> bool {
        let Some(pending) = self.pending.take() else {
            return true;
        };
        let chunk = Arc::new(LoadPreviewChunk {
            table_index: pending.table_index,
            table_name: pending.table_name,
            first_row: pending.rows.first().map_or(0, |row| row.index),
            last_row: pending.rows.last().map_or(0, |row| row.index),
            column_count: pending.column_count,
            rows: pending.rows.into(),
        });
        self.send(LoadProgressKind::Chunk(chunk))
    }

    /// Retries a full progress queue without ignoring cancellation or shutdown.
    fn send(&self, kind: LoadProgressKind) -> bool {
        let Some(sender) = self.sender else {
            return true;
        };
        let mut message = LoadProgress {
            token: self.token,
            operation: self.operation,
            path: self.path.to_owned(),
            kind,
        };
        loop {
            if self.cancelled.load(Ordering::Acquire) || self.shutdown.load(Ordering::Acquire) {
                return false;
            }
            match sender.try_send(message) {
                Ok(()) => return true,
                Err(TrySendError::Disconnected(_)) => return false,
                Err(TrySendError::Full(returned)) => {
                    message = returned;
                    thread::sleep(Duration::from_millis(2));
                }
            }
        }
    }
}
