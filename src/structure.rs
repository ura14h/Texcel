//!  Worker-side requests and public results for document structure operations.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::display::Base64Display;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};

use crate::calc::{CalculationSnapshot, rewrite_structure_formulas};
use crate::clipboard::{PasteOptions, parse_clipboard, transpose};
use crate::csv_adapter::{CsvCellChange, CsvEncoding, EncodingChoice};
use crate::document::{
    CalcStatus, Cell, CellAddress, CellInput, CellRange, Document, ScrollPosition, StructureAxis,
    StructureOperation, StructurePlan, TableId, TableLayout, TextOverlayCell,
};
use crate::i18n::Message;
use crate::markdown_adapter::{MarkdownCellChange, MarkdownDocument};
use crate::source::SourceDocument;

#[derive(Clone, Debug, Default)]
pub struct StructureTableMeta {
    pub id: TableId,
    pub name: String,
    pub rows: usize,
    pub columns: usize,
    pub string_cells: Arc<BTreeSet<CellAddress>>,
    /// Table-specific view state used when a context action targets an inactive table.
    pub view_layout: Arc<TableLayout>,
    pub selection: CellRange,
    pub scroll: ScrollPosition,
}

#[derive(Clone, Debug)]
pub struct StructureRequest {
    pub document_id: u64,
    pub revision: u64,
    pub calculation_revision: u64,
    pub source: Option<Arc<SourceDocument>>,
    pub operation: Option<StructureOperation>,
    pub tables: Vec<StructureTableMeta>,
    pub changes: Vec<(TableId, CellAddress, Option<crate::document::Cell>)>,
    pub text_overlay_cells: Arc<Vec<TextOverlayCell>>,
    pub text_overlay_revision: u64,
    pub view_table_id: TableId,
    pub view_layout: Arc<TableLayout>,
    pub selection: CellRange,
    /// Raw clipboard inputs; parsing and all cell expansion run on the file worker.
    pub clipboard_paste: Option<ClipboardPasteRequest>,
    pub prepared_selection_after: Option<CellRange>,
    pub scroll: ScrollPosition,
    pub cross_sheet_references: bool,
}

/// One worker-prepared document table or worksheet management action.
#[derive(Clone, Debug)]
pub enum TableAction {
    Add {
        new_table_id: TableId,
        name: String,
    },
    Duplicate {
        table_id: TableId,
        new_table_id: TableId,
        name: String,
    },
    Delete {
        table_id: TableId,
    },
    Rename {
        table_id: TableId,
        name: String,
    },
    MarkdownFormat {
        table_id: TableId,
        operation: MarkdownTableOperation,
    },
    PromoteCsv {
        table_id: TableId,
    },
    PromoteCsvCell {
        table_id: TableId,
        address: CellAddress,
        value: String,
    },
    PromoteCsvCells {
        table_id: TableId,
        changes: Vec<(CellAddress, String)>,
    },
    MarkdownLinks {
        table_id: TableId,
        range: CellRange,
        hyperlink: Option<crate::document::CellHyperlink>,
    },
}

#[derive(Clone, Debug)]
pub enum MarkdownTableOperation {
    SetColumnAlignment {
        columns: std::ops::RangeInclusive<usize>,
        alignment: crate::markdown_adapter::MarkdownTableAlignment,
    },
}

#[derive(Clone, Debug)]
pub struct ClipboardPasteRequest {
    pub text: String,
    pub html: Option<String>,
    pub options: PasteOptions,
    pub selection: CellRange,
    pub rows: usize,
    pub columns: usize,
    /// The cut range in the target table when this paste moves it there.
    pub cut_source: Option<CellRange>,
}

/// Search and optional replacement over a revision-pinned document snapshot.
#[derive(Clone, Debug)]
pub struct SearchRequest {
    pub structure: StructureRequest,
    pub language: crate::i18n::Language,
    pub query: String,
    pub replacement: Option<String>,
    pub ranges: Vec<(TableId, CellRange)>,
    pub case_sensitive: bool,
    pub regular_expression: bool,
    pub whole_cell: bool,
    pub representation: SearchRepresentation,
    pub formula_results_revision: Option<u64>,
    pub formula_results: Option<crate::calc::FormulaResultSnapshot>,
}

/// Which text representation is searched. Display searches require current formula results.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchRepresentation {
    Source,
    Display,
}

/// Identifies calculation context without storing translated display text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchResultState {
    Calculation(CalcStatus),
    Display,
    Missing,
}

/// One worker-found search result. It contains the original text and the proposed source edit.
#[derive(Clone, Debug)]
pub struct SearchHit {
    pub table_id: TableId,
    pub address: CellAddress,
    pub source_text: String,
    pub searched_text: String,
    pub replacement_text: Option<String>,
    pub result_state: Option<SearchResultState>,
}

/// Search results and a fully prepared, but not yet applied, replacement root.
#[derive(Debug)]
pub struct PreparedSearch {
    pub document_revision: u64,
    pub calculation_revision: u64,
    pub hits: Vec<SearchHit>,
    pub matched_cells: usize,
    pub total_matches: usize,
    pub hits_truncated: bool,
    pub skipped_non_fresh: usize,
    pub prepared: Option<crate::document::PreparedStructure>,
}

/// One worker-prepared data transformation. The UI confirms the returned diff before applying it.
#[derive(Clone, Debug)]
pub struct DataEditRequest {
    pub structure: StructureRequest,
    pub action: DataEditAction,
    pub formula_results_revision: Option<u64>,
    pub formula_results: Option<crate::calc::FormulaResultSnapshot>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SortValueKind {
    Text,
    Number,
    Date,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SortKey {
    pub column: usize,
    pub kind: SortValueKind,
    pub descending: bool,
}

#[derive(Clone, Debug)]
pub enum DataEditAction {
    TrimWhitespace {
        ranges: Vec<(TableId, CellRange)>,
    },
    SplitColumn {
        table_id: TableId,
        rows: CellRange,
        source_column: usize,
        delimiter: String,
        destination_column: usize,
        overwrite: bool,
    },
    JoinColumns {
        table_id: TableId,
        range: CellRange,
        delimiter: String,
        destination_column: usize,
        overwrite: bool,
    },
    Transpose {
        table_id: TableId,
        range: CellRange,
        overwrite: bool,
    },
    SortRows {
        table_id: TableId,
        keys: Vec<SortKey>,
        header_row: Option<usize>,
    },
    Duplicates {
        table_id: TableId,
        range: CellRange,
        header_row: Option<usize>,
        delete: bool,
    },
}

#[derive(Clone, Debug)]
pub struct DataCellDiff {
    pub table_id: TableId,
    pub address: CellAddress,
    pub before: String,
    pub after: String,
}

#[derive(Clone, Debug)]
pub struct DuplicateRowPreview {
    pub row: usize,
    pub key_values: Vec<(usize, String)>,
}

#[derive(Debug)]
pub struct PreparedDataEdit {
    pub document_id: u64,
    pub document_revision: u64,
    pub calculation_revision: u64,
    pub changed_cells: usize,
    pub changed_rows: Option<usize>,
    pub duplicate_count: usize,
    pub duplicate_groups: usize,
    pub duplicate_rows: Vec<DuplicateRowPreview>,
    pub preview: Vec<DataCellDiff>,
    pub preview_truncated: bool,
    pub prepared: Option<crate::document::PreparedStructure>,
}

const STRUCTURE_HISTORY_SPILL_THRESHOLD_BYTES: u64 = 8 * 1024 * 1024;
static NEXT_RECOVERY_ID: AtomicU64 = AtomicU64::new(1);

/// Owns a private recovery directory containing the before and after JSON roots.
#[derive(Debug)]
pub struct StructureHistoryFile {
    directory: PathBuf,
}

impl Drop for StructureHistoryFile {
    // Remove temporary structure history files when their owner is dropped.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[derive(Debug)]
pub struct StructureHistoryRestoreRequest {
    pub document_id: u64,
    pub history_file: Arc<StructureHistoryFile>,
    pub redo: bool,
    pub replace_table_set: bool,
    pub calculation_revision: u64,
}

#[derive(Debug)]
pub struct PreparedStructureRestore {
    pub document_id: u64,
    pub calculation_revision: u64,
    pub history_file: Arc<StructureHistoryFile>,
    pub redo: bool,
    pub replace_table_set: bool,
    pub tables: Vec<crate::document::Table>,
    pub text_overlay_cells: Arc<Vec<TextOverlayCell>>,
    pub source: Option<Arc<SourceDocument>>,
    pub view_table_id: TableId,
    pub view_layout: Arc<TableLayout>,
    pub selection: CellRange,
    pub scroll: ScrollPosition,
    pub target: (TableId, CellAddress),
    pub calculation: Arc<CalculationSnapshot>,
}

mod bulk;
mod bulk_paste;
mod data_edit;
mod data_helpers;
mod data_sort;
mod duplicates;
mod history;
mod history_growth;
mod history_validation;
mod markdown;
mod paste;
mod paste_cut;
mod prepare;
mod search;
mod search_helpers;
mod selection;
mod source_document;
mod source_transform;
mod table_management;

pub(super) use bulk::prepare_cell_changes;
pub(super) use bulk_paste::prepare_bulk_cells;
pub(crate) use data_edit::prepare_data_edit;
pub(super) use data_helpers::{cell_has_content, find_table, is_formula_cell, validate_delimiter};
pub(crate) use data_sort::{SortAtom, prepare_sorted_rows};
pub(crate) use duplicates::{
    DuplicateScan, calc_value_sort_atom, compare_sort_atoms, prepare_duplicates, sort_number_atom,
    sort_text_atom,
};
pub(crate) use history::prepare_structure_restore;
pub(crate) use history::{
    StructureHistoryMetadata, create_history_file, history_root_ref,
    history_root_ref_with_text_overlay, serialized_size, write_history_root,
};
pub(crate) use history_growth::estimate_structure_growth;
pub(super) use history_validation::validate_recovery_root;
pub(crate) use markdown::materialize_current_source;
pub(super) use paste::apply_bulk_cell_changes_for_table_with_raw_markdown_sources;
pub(crate) use paste::expand_clipboard_paste;
pub(crate) use prepare::prepare_structure;
pub(crate) use search::prepare_search;
pub(super) use search_helpers::{continue_search_skip, search_pattern};
pub(super) use selection::check_cancelled;
pub(super) use selection::selection_for_operation;
pub(crate) use source_document::{
    source_to_document, source_to_document_with_cancel, source_to_document_with_language,
};
pub(crate) use source_transform::apply_requested_text_overlay;
pub(crate) use source_transform::apply_requested_text_overlay_with_cancel;
pub(super) use source_transform::{
    apply_changed_cells_with_cancel, apply_table_metadata, apply_table_metadata_with_cancel,
    collect_preserved_text_overlay, collect_structure_text_overlay, operation_extent,
    operation_table_id, source_extent, source_table_index, transform_source_after, used_dimensions,
};
pub(crate) use source_transform::{
    apply_outside_extent_structure, structure_operation_outside_extent,
};
pub(crate) use table_management::prepare_table_structure;

#[cfg(test)]
#[path = "structure_formula_paste_test.rs"]
mod formula_paste_tests;
#[cfg(test)]
#[path = "structure_move_paste_test.rs"]
mod move_paste_tests;
#[cfg(test)]
#[path = "structure_test.rs"]
mod tests;
#[cfg(test)]
#[path = "structure_grow_test.rs"]
mod text_growth_tests;
