//!  Document operations for snapshots.

use crate::document::*;

/// The edit and save counters a recovered document resumes from.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RecoveryRevisions {
    pub(crate) revision: u64,
    pub(crate) saved_revision: u64,
    pub(crate) structure_revision: u64,
}

impl Document {
    /// Applies persisted recovery deltas and restores their dirty-version markers.
    pub(crate) fn restore_recovery_deltas(
        &mut self,
        cells: &[(TableId, CellAddress, Option<Cell>)],
        revisions: RecoveryRevisions,
    ) -> Result<(), crate::i18n::Message> {
        for (table_id, address, cell) in cells {
            let table = self
                .tables
                .iter_mut()
                .find(|table| table.id == *table_id)
                .ok_or_else(|| {
                    crate::i18n::Message::new("status-recovery-table-missing")
                        .with_number("table_id", *table_id as i64)
                })?;
            table.apply_snapshot_cell(*address, cell.clone());
            self.record_changed_cells([(*table_id, *address)]);
        }
        self.revision = revisions.revision;
        self.saved_revision = revisions.saved_revision;
        self.structure_revision = revisions.structure_revision;
        self.record_recovered_history_state();
        Ok(())
    }

    // Returns whether the current undo state differs from the saved state.
    pub fn is_dirty(&self) -> bool {
        self.history_state_id() != self.saved_history_state_id
    }

    /// Reports whether a cell input differs from the current immutable text source snapshot.
    pub(crate) fn cell_input_is_dirty(&self, table_id: TableId, address: CellAddress) -> bool {
        self.changed_cells.contains(&(table_id, address))
    }

    /// Gives display caches a cell-local content version without rescanning a long cell string.
    pub(crate) fn cell_input_revision(
        &self,
        table_id: TableId,
        address: CellAddress,
    ) -> (u64, u64) {
        (
            self.cell_input_generation,
            self.cell_input_revisions
                .get(&(table_id, address))
                .copied()
                .unwrap_or_default(),
        )
    }

    // Record the input revision for each cell changed by a document command.
    pub(super) fn mark_cell_inputs_changed(&mut self, addresses: &[(TableId, CellAddress)]) {
        for address in addresses {
            self.cell_input_revisions.insert(*address, self.revision);
        }
    }

    // Advance the generation used to reject stale calculation results.
    pub(super) fn advance_cell_input_generation(&mut self) {
        self.cell_input_generation = self.cell_input_generation.saturating_add(1);
        self.cell_input_revisions.clear();
    }

    // Returns sparse cells changed since the save request version and their current values.
    pub fn changed_cell_snapshot(&self) -> Vec<(TableId, CellAddress, Option<Cell>)> {
        self.changed_cells
            .iter()
            .map(|(table_id, address)| {
                let cell = self
                    .tables
                    .iter()
                    .find(|table| table.id == *table_id)
                    .and_then(|table| table.cell(*address))
                    .cloned();
                (*table_id, *address, cell)
            })
            .collect()
    }

    /// Copies only edited cells on one header axis for background auto-fit.
    pub(crate) fn changed_cell_snapshot_for_column(
        &self,
        table_id: TableId,
        column: usize,
    ) -> Vec<(TableId, CellAddress, Option<Cell>)> {
        let addresses = self
            .changed_cells_by_column
            .get(&(table_id, column))
            .into_iter()
            .flat_map(|addresses| addresses.iter().copied())
            .collect::<Vec<_>>();
        let Some(table) = self.tables.iter().find(|table| table.id == table_id) else {
            return Vec::new();
        };
        addresses
            .into_iter()
            .map(|address| (table_id, address, table.cell(address).cloned()))
            .collect()
    }

    /// Keeps the column index aligned with the shared dirty-cell set.
    pub(crate) fn record_changed_cells(
        &mut self,
        addresses: impl IntoIterator<Item = (TableId, CellAddress)>,
    ) {
        for (table_id, address) in addresses {
            self.changed_cells.insert((table_id, address));
            self.changed_cells_by_column
                .entry((table_id, address.column))
                .or_default()
                .insert(address);
            self.refresh_source_byte_delta(table_id, address);
        }
    }

    /// Clears the dirty-cell set and its lookup index together.
    pub(crate) fn clear_changed_cells(&mut self) {
        self.changed_cells.clear();
        self.changed_cells_by_column.clear();
        self.source_byte_deltas.clear();
        self.source_byte_delta_unknown = 0;
        self.source_byte_delta_total = 0;
    }

    /// Returns only cells queued for recalculation since the prior worker submit.
    pub fn calculation_cell_snapshot(&self) -> Vec<(TableId, CellAddress, Option<Cell>)> {
        self.calculation_cells
            .iter()
            .map(|(table_id, address)| {
                let cell = self
                    .tables
                    .iter()
                    .find(|table| table.id == *table_id)
                    .and_then(|table| table.cell(*address))
                    .cloned();
                (*table_id, *address, cell)
            })
            .collect()
    }

    /// Reports queued calculation work without copying its cell payloads.
    pub fn has_pending_calculation_changes(&self) -> bool {
        !self.calculation_cells.is_empty()
    }

    /// Clears only the submitted calculation keys; save tracking remains independent.
    pub fn mark_calculation_submitted(&mut self, keys: &[(TableId, CellAddress)]) {
        for key in keys {
            self.calculation_cells.remove(key);
        }
    }

    /// Requeues changes after the calculation worker rejected an update batch.
    pub fn requeue_calculation_changes(&mut self, keys: &[(TableId, CellAddress)]) {
        self.calculation_cells.extend(keys.iter().copied());
    }

    // Advances the saved version and keeps later changes made while saving.
    pub fn mark_saved(&mut self, revision: u64) {
        self.saved_revision = revision;
        if revision == self.revision {
            self.saved_history_state_id = self.history_state_id();
        }
        if self.revision == revision {
            self.clear_changed_cells();
        }
    }

    /// Records the saved version and the undo state that a specific save request wrote.
    pub(crate) fn mark_saved_at_state(&mut self, revision: u64, state_id: u64) {
        self.saved_revision = revision;
        self.saved_history_state_id = state_id;
        if self.revision == revision {
            self.clear_changed_cells();
        }
    }

    /// Counts a change to the table structure; recovery writes a new base snapshot after one.
    pub(super) fn note_structure_change(&mut self) {
        self.structure_revision = self.structure_revision.saturating_add(1);
    }

    /// Replaces the visible result root only when the calculation input revision still matches.
    pub fn install_calculation_results(
        &mut self,
        results: crate::calc::FormulaResultSnapshot,
    ) -> bool {
        if self.calculation_revision != results.revision {
            return false;
        }
        self.calculation_results = Some(results);
        true
    }

    /// Builds a complete result root from test or compatibility result batches.
    #[cfg(test)]
    pub fn apply_calculation_results(
        &mut self,
        revision: u64,
        results: impl IntoIterator<Item = (TableId, CellAddress, CalcStatus, Option<CalcValue>)>,
    ) -> bool {
        if self.calculation_revision != revision {
            return false;
        }
        self.install_calculation_results(Arc::new(crate::calc::FormulaResultRoot::new(
            revision,
            results.into_iter().collect(),
        )))
    }

    /// Returns an indexed runtime result or a separate imported cache for one formula cell.
    pub fn calculation_result_for(
        &self,
        table_id: TableId,
        address: CellAddress,
    ) -> Option<CalcResult> {
        let table = self.tables.iter().find(|table| table.id == table_id)?;
        let cell = table.cell(address)?;
        let is_formula =
            crate::calc::formula_source(&cell.input, table.is_string_cell(address)).is_some();
        if !is_formula {
            return None;
        }
        if let Some(root) = &self.calculation_results {
            if let Some((status, value)) = root.get(table_id, address) {
                return Some(CalcResult {
                    revision: self.calculation_revision,
                    status: if root.revision == self.calculation_revision {
                        status
                    } else {
                        CalcStatus::Stale
                    },
                    value: value.cloned(),
                });
            }
            if root.revision != self.calculation_revision {
                return Some(CalcResult {
                    revision: self.calculation_revision,
                    status: CalcStatus::Stale,
                    value: cell.imported_result.clone(),
                });
            }
        }
        cell.imported_result.clone().map(|value| CalcResult {
            revision: self.calculation_revision,
            status: CalcStatus::Imported,
            value: Some(value),
        })
    }

    // Returns whether an undo command is available.
    pub fn can_undo(&self) -> bool {
        self.history_cursor > 0
    }

    // Returns whether a redo command is available.
    pub fn can_redo(&self) -> bool {
        self.history_cursor < self.history.len()
    }
}

#[cfg(test)]
#[path = "snapshots_test.rs"]
mod tests;
