//!  Sparse column, merge, and frozen-pane metadata. Row heights follow the content.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::coordinate::MergedRange;

/// Explicit column width in Excel character units.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ColumnLayout {
    pub width: Option<f64>,
}

/// Frozen row and column counts.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct FrozenPanes {
    pub rows: usize,
    pub columns: usize,
}

/// A sparse column, merge, or freeze metadata key.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum LayoutTarget {
    Column(usize),
    Merge(MergedRange),
    Freeze,
}

/// Value of one sparse table-layout metadata key.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum LayoutValue {
    Column(Option<ColumnLayout>),
    Merge(bool),
    Freeze(FrozenPanes),
}

/// Sparse table layout. The Fenwick map keeps cumulative column positions bounded by the
/// number of tree levels, even when many columns have explicit widths.
#[derive(Clone, Debug, Default)]
pub struct TableLayout {
    columns: BTreeMap<usize, ColumnLayout>,
    merge_geometry: Arc<MergeGeometry>,
    frozen: FrozenPanes,
    column_width_sums: BTreeMap<usize, ColumnWidthSum>,
    /// Set when an imported sheet had row heights or hidden rows; only the import report reads it.
    imported_row_layout: bool,
    revision: u64,
}

/// Fenwick-tree node for sized columns: their width units, how many there are, and how
/// many are zero wide. Positions follow from these for any character width.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct ColumnWidthSum {
    units: f64,
    sized: i64,
    zero: i64,
}

/// Immutable merge ranges and indexes shared by source and view layouts.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct MergeGeometry {
    merges: BTreeSet<MergedRange>,
    merges_by_start_row: BTreeMap<usize, BTreeSet<MergedRange>>,
    merge_row_blocks: BTreeMap<usize, MergeRowBlock>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct MergeRowBlock {
    max_end_row: usize,
}

#[derive(Serialize)]
struct TableLayoutWireRef<'a> {
    columns: &'a BTreeMap<usize, ColumnLayout>,
    merges: &'a BTreeSet<MergedRange>,
    merges_by_start_row: &'a BTreeMap<usize, BTreeSet<MergedRange>>,
    merge_row_blocks: &'a BTreeMap<usize, MergeRowBlock>,
    frozen: &'a FrozenPanes,
    revision: u64,
}

#[derive(Deserialize)]
struct TableLayoutWire {
    #[serde(default)]
    columns: BTreeMap<usize, ColumnLayout>,
    #[serde(default)]
    merges: BTreeSet<MergedRange>,
    #[serde(default)]
    merges_by_start_row: BTreeMap<usize, BTreeSet<MergedRange>>,
    #[serde(default)]
    merge_row_blocks: BTreeMap<usize, MergeRowBlock>,
    #[serde(default)]
    frozen: FrozenPanes,
    #[serde(default)]
    revision: u64,
}

impl Serialize for TableLayout {
    /// Writes the layout with the merge index fields at the top level.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        TableLayoutWireRef {
            columns: &self.columns,
            merges: &self.merge_geometry.merges,
            merges_by_start_row: &self.merge_geometry.merges_by_start_row,
            merge_row_blocks: &self.merge_geometry.merge_row_blocks,
            frozen: &self.frozen,
            revision: self.revision,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TableLayout {
    /// Reads a saved layout and rebuilds the shared merge geometry.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = TableLayoutWire::deserialize(deserializer)?;
        // The width sums are derived data, so they are rebuilt from the columns.
        let column_width_sums = geometry::column_width_sums(&wire.columns);
        Ok(Self {
            columns: wire.columns,
            merge_geometry: Arc::new(MergeGeometry {
                merges: wire.merges,
                merges_by_start_row: wire.merges_by_start_row,
                merge_row_blocks: wire.merge_row_blocks,
            }),
            frozen: wire.frozen,
            imported_row_layout: false,
            column_width_sums,
            revision: wire.revision,
        })
    }
}

mod geometry;
mod merges;
