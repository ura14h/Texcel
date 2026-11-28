//!  Calculates column positions and applies sparse layout edits.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::ColumnWidthSum;
use crate::document::frozen_prefix;
use crate::document::{
    COLUMN_TEXT_PADDING_PIXELS, CellAddress, ColumnLayout, DEFAULT_COLUMN_WIDTH_PIXELS,
    FrozenPanes, LayoutTarget, LayoutValue, MAX_COLUMN_WIDTH_UNITS, MAX_COLUMNS, MAX_ROWS,
    MERGE_ROW_BLOCK_SIZE, StructureAxis, StructurePlan, TableLayout,
};
use crate::i18n::Message;

impl TableLayout {
    /// Returns the column layout stored at an index.
    pub fn column(&self, index: usize) -> Option<ColumnLayout> {
        self.columns.get(&index).copied()
    }

    /// Iterates over the stored column layouts.
    pub fn columns(&self) -> impl Iterator<Item = (usize, ColumnLayout)> + '_ {
        self.columns.iter().map(|(index, layout)| (*index, *layout))
    }

    /// Returns the layout to save as the user's choice: without the widths fitted on opening,
    /// which the next opening measures again with the fonts and content of that time.
    pub fn without_opening_fits(&self) -> Self {
        let mut layout = self.clone();
        for (index, column) in self.columns() {
            if column.fitted_on_opening {
                layout.apply_value(LayoutTarget::Column(index), LayoutValue::Column(None));
            }
        }
        layout
    }

    /// Reports whether an imported sheet had row heights or hidden rows.
    pub fn has_imported_row_layout(&self) -> bool {
        self.imported_row_layout
    }

    /// Records that an imported sheet had row heights or hidden rows.
    pub(crate) fn mark_imported_row_layout(&mut self) {
        self.imported_row_layout = true;
    }

    /// Returns the frozen pane settings.
    pub fn frozen(&self) -> &FrozenPanes {
        &self.frozen
    }

    /// Returns the layout revision used by caches.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the visible width of a column at 100%, with the reference character width.
    #[cfg(test)]
    pub fn column_width_pixels(&self, index: usize) -> f64 {
        self.column_width_pixels_with(index, crate::document::REFERENCE_CHARACTER_WIDTH)
    }

    /// Returns the total width before a column index, with the reference character width.
    #[cfg(test)]
    pub fn prefix_column_width_pixels(&self, columns: usize) -> f64 {
        self.prefix_column_width_pixels_with(columns, crate::document::REFERENCE_CHARACTER_WIDTH)
    }

    /// Returns a column's width at 100% when one width unit is `character_width` wide.
    /// Unsized columns keep the default width; sized ones hold that many characters.
    pub fn column_width_pixels_with(&self, index: usize, character_width: f64) -> f64 {
        match sized_width(self.column(index).unwrap_or_default()) {
            None => DEFAULT_COLUMN_WIDTH_PIXELS,
            Some(0.0) => 0.0,
            Some(units) => units * character_width + COLUMN_TEXT_PADDING_PIXELS,
        }
    }

    /// Returns the total width before a column index when one unit is `character_width` wide.
    pub fn prefix_column_width_pixels_with(&self, columns: usize, character_width: f64) -> f64 {
        let columns = columns.min(MAX_COLUMNS);
        let sums = fenwick_prefix(&self.column_width_sums, columns);
        let unsized_columns = columns as i64 - sums.sized - sums.zero;
        unsized_columns as f64 * DEFAULT_COLUMN_WIDTH_PIXELS
            + sums.units * character_width
            + sums.sized as f64 * COLUMN_TEXT_PADDING_PIXELS
    }

    /// Reads one layout value for an undo target.
    pub fn value(&self, target: LayoutTarget) -> LayoutValue {
        match target {
            LayoutTarget::Column(index) => LayoutValue::Column(self.column(index)),
            LayoutTarget::Merge(range) => {
                LayoutValue::Merge(self.merge_geometry.merges.contains(&range))
            }
            LayoutTarget::Freeze => LayoutValue::Freeze(self.frozen.clone()),
        }
    }

    /// Applies a column, merge, or freeze layout value.
    pub fn apply_value(&mut self, target: LayoutTarget, value: LayoutValue) {
        match (target, value) {
            (LayoutTarget::Column(index), LayoutValue::Column(layout)) => {
                let previous = self.column(index).unwrap_or_default();
                let next = layout.unwrap_or_default();
                if previous == next {
                    return;
                }
                update_fenwick(
                    &mut self.column_width_sums,
                    index,
                    width_sum(next).minus(width_sum(previous)),
                    MAX_COLUMNS,
                );
                if next == ColumnLayout::default() {
                    self.columns.remove(&index);
                } else {
                    self.columns.insert(index, next);
                }
                self.revision = self.revision.saturating_add(1);
            }
            (LayoutTarget::Merge(range), LayoutValue::Merge(merged)) => {
                let geometry = Arc::make_mut(&mut self.merge_geometry);
                let changed = if merged {
                    let changed = geometry.merges.insert(range);
                    if changed {
                        geometry
                            .merges_by_start_row
                            .entry(range.start.row)
                            .or_default()
                            .insert(range);
                        Self::refresh_merge_block(geometry, range.start.row / MERGE_ROW_BLOCK_SIZE);
                    }
                    changed
                } else {
                    let changed = geometry.merges.remove(&range);
                    if changed
                        && let Some(ranges) = geometry.merges_by_start_row.get_mut(&range.start.row)
                    {
                        ranges.remove(&range);
                        if ranges.is_empty() {
                            geometry.merges_by_start_row.remove(&range.start.row);
                        }
                        Self::refresh_merge_block(geometry, range.start.row / MERGE_ROW_BLOCK_SIZE);
                    }
                    changed
                };
                if changed {
                    self.revision = self.revision.saturating_add(1);
                }
            }
            (LayoutTarget::Freeze, LayoutValue::Freeze(frozen)) => {
                if self.frozen != frozen {
                    self.frozen = frozen;
                    self.revision = self.revision.saturating_add(1);
                }
            }
            _ => unreachable!("layout target and value kinds must match"),
        }
    }

    /// Builds the layout after a structural row or column operation.
    pub(crate) fn transformed(&self, plan: &StructurePlan) -> Result<Self, Message> {
        let mut next = Self::default();
        {
            for (index, value) in self.columns() {
                for output in plan.output_positions(CellAddress {
                    row: 0,
                    column: index,
                }) {
                    next.apply_value(
                        LayoutTarget::Column(output.column),
                        LayoutValue::Column(Some(value)),
                    );
                }
            }
            // Edited documents never hold merged cells, so there are no merges to move.
            let old_frozen = &self.frozen;
            let rows = frozen_prefix(
                &plan.row_sources,
                old_frozen.rows,
                &plan.operation,
                StructureAxis::Rows,
            );
            let columns = frozen_prefix(
                &plan.column_sources,
                old_frozen.columns,
                &plan.operation,
                StructureAxis::Columns,
            );
            next.apply_value(
                LayoutTarget::Freeze,
                LayoutValue::Freeze(FrozenPanes {
                    rows: rows.min(MAX_ROWS.saturating_sub(1)),
                    columns: columns.min(MAX_COLUMNS.saturating_sub(1)),
                }),
            );
        }
        Ok(next)
    }
}

/// Returns a column's set width in units, zero for a zero-wide column, or none when unsized.
fn sized_width(layout: ColumnLayout) -> Option<f64> {
    layout
        .width
        .filter(|width| width.is_finite() && *width >= 0.0)
        .map(|width| {
            if width == 0.0 {
                0.0
            } else {
                width.clamp(0.1, MAX_COLUMN_WIDTH_UNITS)
            }
        })
}

/// Describes one column's contribution to the width sums.
fn width_sum(layout: ColumnLayout) -> ColumnWidthSum {
    match sized_width(layout) {
        None => ColumnWidthSum::default(),
        Some(0.0) => ColumnWidthSum {
            zero: 1,
            ..ColumnWidthSum::default()
        },
        Some(units) => ColumnWidthSum {
            units,
            sized: 1,
            zero: 0,
        },
    }
}

/// Builds the width sums for a set of stored columns.
pub(super) fn column_width_sums(
    columns: &BTreeMap<usize, ColumnLayout>,
) -> BTreeMap<usize, ColumnWidthSum> {
    let mut tree = BTreeMap::new();
    for (index, layout) in columns {
        update_fenwick(&mut tree, *index, width_sum(*layout), MAX_COLUMNS);
    }
    tree
}

impl ColumnWidthSum {
    // Add two partial sums.
    fn plus(self, other: Self) -> Self {
        Self {
            units: self.units + other.units,
            sized: self.sized + other.sized,
            zero: self.zero + other.zero,
        }
    }

    // Subtract one partial sum from another.
    fn minus(self, other: Self) -> Self {
        Self {
            units: self.units - other.units,
            sized: self.sized - other.sized,
            zero: self.zero - other.zero,
        }
    }

    // Report a node that holds nothing, so the tree can drop it.
    fn is_empty(self) -> bool {
        self.units.abs() < 0.000_001 && self.sized == 0 && self.zero == 0
    }
}

/// Updates one value in the prefix-sum tree.
fn update_fenwick(
    tree: &mut BTreeMap<usize, ColumnWidthSum>,
    index: usize,
    delta: ColumnWidthSum,
    limit: usize,
) {
    if delta.is_empty() {
        return;
    }
    let mut node = index + 1;
    while node <= limit {
        let value = tree.get(&node).copied().unwrap_or_default().plus(delta);
        if value.is_empty() {
            tree.remove(&node);
        } else {
            tree.insert(node, value);
        }
        node += node.isolate_lowest_one();
    }
}

/// Sums the stored column widths before an index.
fn fenwick_prefix(tree: &BTreeMap<usize, ColumnWidthSum>, count: usize) -> ColumnWidthSum {
    let mut node = count;
    let mut total = ColumnWidthSum::default();
    while node > 0 {
        total = total.plus(tree.get(&node).copied().unwrap_or_default());
        node &= node - 1;
    }
    total
}
