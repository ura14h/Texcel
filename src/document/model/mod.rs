//!  Owns the document roots, edit history, and worker-facing change snapshots.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::document::*;
use crate::source::SourceDocument;

// Error returned when there is no table or a cell coordinate is outside its bounds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DocumentError {
    MissingTable(TableId),
    NeedsWorkerHistoryRestore,
    StalePreparedStructure,
    OutOfBounds(CellAddress),
    DuplicateCell(CellAddress),
    ResourceLimit(crate::i18n::ResourceLimit),
    TextSourceValidationRequired,
    FormatRangeTooLarge(usize),
}

// One document and the edit history shared by all its tables.
#[derive(Clone, Debug)]
pub struct Document {
    pub id: DocumentId,
    pub name: String,
    pub tables: Vec<Table>,
    pub active_table: TableId,
    /// Display choice shared by every table in this document.
    pub display_mode: DocumentDisplayMode,
    pub views: BTreeMap<TableId, TableViewState>,
    pub revision: u64,
    pub saved_revision: u64,
    pub(crate) structure_revision: u64,
    /// Monotone calculation-input version. It advances only when calculation inputs change,
    /// so formatting-only edits keep the current results valid.
    pub calculation_revision: u64,
    /// Immutable runtime results for the last fully accepted calculation revision.
    pub(crate) calculation_results: Option<crate::calc::FormulaResultSnapshot>,
    /// Date base read from XLSX. Formatting never changes the stored serial value.
    /// Current immutable file source, synchronized with FileBacking after save and structural edits.
    pub(crate) source: Option<Arc<SourceDocument>>,
    /// Sparse source cells and type/style cells re-applied after text-source materialization.
    pub(crate) text_overlay_cells: Arc<Vec<TextOverlayCell>>,
    /// Revision represented by `text_overlay_cells`; later cell edits stay in dirty snapshots.
    pub(crate) text_overlay_revision: u64,
    pub(super) pending_calculation_reset: Option<Arc<crate::calc::CalculationSnapshot>>,
    pub(super) history: Vec<EditCommand>,
    pub(super) history_cursor: usize,
    pub(super) history_state_ids: Vec<u64>,
    pub(super) next_history_state_id: u64,
    pub(super) saved_history_state_id: u64,
    pub(super) changed_cells: BTreeSet<(TableId, CellAddress)>,
    pub(super) changed_cells_by_column: BTreeMap<(TableId, usize), BTreeSet<CellAddress>>,
    /// Tracks exact source-byte changes for edited fields without rebuilding the file.
    pub(super) source_byte_deltas: BTreeMap<(TableId, CellAddress), Option<i128>>,
    /// Counts dirty cells that need worker validation because they lack a source field.
    pub(super) source_byte_delta_unknown: usize,
    /// Keeps the source size check constant-time for ordinary in-place edits.
    pub(super) source_byte_delta_total: i128,
    /// Carries an exact-cap warning from a committed command to its UI owner.
    pub(super) resource_limit_notice: Option<crate::i18n::ResourceLimit>,
    /// Keeps a deferred text command ready for the source worker when exact validation is needed.
    pub(super) pending_text_source_changes: Option<crate::document::PendingTextSourceChanges>,
    pub(super) cell_input_generation: u64,
    pub(super) cell_input_revisions: BTreeMap<(TableId, CellAddress), u64>,
    pub(super) calculation_cells: BTreeSet<(TableId, CellAddress)>,
}

// Partial document implementations grouped by edit and history responsibility.

mod cells;
mod history;
mod ranges;
mod snapshots;
mod source_budget;
pub(crate) use snapshots::RecoveryRevisions;
pub(crate) use source_budget::TextCellResourceCheck;
#[cfg(test)]
#[path = "source_budget_test.rs"]
mod source_budget_tests;
mod state;
mod structure;
mod undo;
