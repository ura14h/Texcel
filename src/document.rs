//!  Defines the document model and its stable public types.

use std::collections::BTreeSet;
use std::sync::Arc;

/// Serializes sparse maps as entry arrays so structured keys remain lossless in JSON recovery files.
pub(crate) mod map_entries {
    use std::collections::BTreeMap;

    use serde::de::Error as _;
    use serde::ser::SerializeSeq;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    // Write map entries as a sequence for stable document serialization.
    pub fn serialize<K, V, S>(values: &BTreeMap<K, V>, serializer: S) -> Result<S::Ok, S::Error>
    where
        K: Serialize + Ord,
        V: Serialize,
        S: Serializer,
    {
        let mut sequence = serializer.serialize_seq(Some(values.len()))?;
        for (key, value) in values {
            sequence.serialize_element(&(key, value))?;
        }
        sequence.end()
    }

    // Read map entries from a sequence and rebuild the ordered map.
    pub fn deserialize<'de, K, V, D>(deserializer: D) -> Result<BTreeMap<K, V>, D::Error>
    where
        K: Deserialize<'de> + Ord,
        V: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        let entries = Vec::<(K, V)>::deserialize(deserializer)?;
        let mut values = BTreeMap::new();
        for (key, value) in entries {
            if values.insert(key, value).is_some() {
                return Err(D::Error::custom("recovery map contains a duplicate key"));
            }
        }
        Ok(values)
    }
}

/// Identifies one document in the workspace.
pub type DocumentId = u64;
/// Identifies one table inside a document.
pub type TableId = u64;

pub type TextOverlayCell = (TableId, CellAddress, Cell);
pub type TextOverlayChange = (TableId, CellAddress, Option<Cell>);

/// Cells whose text starting with `=` is kept as text, kept aside while a table is removed.
#[derive(Clone, Debug)]
pub struct TableStringCells {
    pub(super) string_cells: Arc<BTreeSet<CellAddress>>,
}

#[derive(Clone, Debug)]
pub struct TextOverlaySnapshot {
    pub revision: u64,
    pub cells: Arc<Vec<TextOverlayCell>>,
}

pub const MAX_ROWS: usize = 1_048_576;
pub const MAX_COLUMNS: usize = 16_384;
pub(super) const MAX_FORMAT_RANGE_CELLS: usize = 50_000;
pub const DEFAULT_COLUMN_WIDTH_PIXELS: f64 = 112.0;
/// Room around a sized column's text: the cell's 6-point inset on each side.
pub const COLUMN_TEXT_PADDING_PIXELS: f64 = 12.0;
/// One half-width character of the bundled grid monospace font at 100%, the width unit
/// assumed where the drawn font is unknown.
pub const REFERENCE_CHARACTER_WIDTH: f64 = 7.0;
pub const MAX_COLUMN_WIDTH_UNITS: f64 = 255.0;

/// Converts a column's drawn width at 100% to width units: half-width monospace characters.
pub fn column_width_units_for_pixels(pixels: f64, character_width: f64) -> f64 {
    ((pixels - COLUMN_TEXT_PADDING_PIXELS) / character_width.max(1.0))
        .clamp(0.1, MAX_COLUMN_WIDTH_UNITS)
}
pub(super) const MERGE_ROW_BLOCK_SIZE: usize = 256;

mod cell;
mod command;
mod coordinate;
mod edit_session;
mod layout;
mod model;
mod structure;
mod table;
mod view;
mod workspace;

pub use cell::{
    CalcResult, CalcStatus, CalcValue, Cell, CellBorder, CellFormat, CellHyperlink, CellInput,
    CellStyle, CellTextRun,
};
pub(crate) use cell::{encode_literal_text, literal_text_value};
pub use command::EditCommand;
pub(crate) use command::{PendingTextSourceChanges, StringOverride};
pub use coordinate::{CellAddress, CellRange, MergedRange};
pub use edit_session::{EditMode, EditSession, EditTarget, InputAction, InputKey};
pub use layout::{ColumnLayout, FrozenPanes, LayoutTarget, LayoutValue, TableLayout};
pub use model::{Document, DocumentError};
pub(crate) use model::{RecoveryRevisions, TextCellResourceCheck};
pub(crate) use structure::StructureSwapResult;
pub(super) use structure::frozen_prefix;
pub use structure::{
    CellChange, FillMode, PreparedStructure, StructureAxis, StructureOperation, StructurePlan,
};
pub(super) use structure::{SeriesKind, validate_series_source};
pub(super) use structure::{SeriesValue, replace_formula_input, structure_table_id};
pub use table::Table;
pub(crate) use table::cell_needs_text_overlay;
pub use view::{CellDisplayMode, DocumentDisplayMode, ScrollPosition, TableViewState};
pub(crate) use view::{
    archive_removed_table_settings, prepare_table_set_string_cells, reconcile_table_views,
};
pub use workspace::Workspace;

#[cfg(test)]
#[path = "document_test.rs"]
mod tests;
