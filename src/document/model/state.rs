//!  Document operations for state.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::document::*;
use crate::source::SourceDocument;

impl Document {
    // Creates a new unchanged document with a table of the given name.
    pub fn new(id: DocumentId, name: impl Into<String>, table_names: &[&str]) -> Self {
        let mut tables = Vec::with_capacity(table_names.len());
        let mut views = BTreeMap::new();
        for (index, name) in table_names.iter().enumerate() {
            let table_id = index as u64 + 1;
            tables.push(Table::new(table_id, *name));
            views.insert(table_id, TableViewState::default());
        }
        let active_table = tables.first().map_or(1, |table| table.id);
        Self {
            id,
            name: name.into(),
            tables,
            active_table,
            display_mode: DocumentDisplayMode::Live,
            views,
            revision: 0,
            saved_revision: 0,
            structure_revision: 0,
            calculation_revision: 0,
            calculation_results: None,
            source: None,
            text_overlay_cells: Arc::new(Vec::new()),
            text_overlay_revision: 0,
            pending_calculation_reset: None,
            history: Vec::new(),
            history_cursor: 0,
            history_state_ids: vec![0],
            next_history_state_id: 1,
            saved_history_state_id: 0,
            changed_cells: BTreeSet::new(),
            changed_cells_by_column: BTreeMap::new(),
            source_byte_deltas: BTreeMap::new(),
            source_byte_delta_unknown: 0,
            source_byte_delta_total: 0,
            resource_limit_notice: None,
            pending_text_source_changes: None,
            cell_input_generation: 0,
            cell_input_revisions: BTreeMap::new(),
            calculation_cells: BTreeSet::new(),
        }
    }

    /// Returns a table's source extent plus any sparse edits outside that source.
    pub fn extent_end(&self, table_id: TableId) -> Option<CellAddress> {
        let table = self.tables.iter().find(|table| table.id == table_id)?;
        if self.source.is_some() {
            Some(table.extent_end())
        } else {
            Some(table.last_used_cell().unwrap_or_default())
        }
    }

    // Replace the source document associated with this model.
    pub(crate) fn set_source(&mut self, source: Option<Arc<SourceDocument>>) {
        self.source = source;
        self.rebuild_source_byte_deltas();
    }

    // Share the current text overlay cells with a reader.
    pub(crate) fn text_overlay_cells(&self) -> Arc<Vec<TextOverlayCell>> {
        Arc::clone(&self.text_overlay_cells)
    }

    // Capture the overlay revision and cell list as one snapshot.
    pub(crate) fn text_overlay_snapshot(&self) -> TextOverlaySnapshot {
        TextOverlaySnapshot {
            revision: self.text_overlay_revision,
            cells: Arc::clone(&self.text_overlay_cells),
        }
    }

    /// Installs a worker-built text overlay without copying its potentially large cell vector.
    pub(crate) fn install_text_overlay_snapshot(&mut self, snapshot: TextOverlaySnapshot) -> bool {
        if snapshot.revision < self.text_overlay_revision {
            return false;
        }
        self.text_overlay_revision = snapshot.revision;
        self.text_overlay_cells = snapshot.cells;
        true
    }

    // Take the pending calculation reset snapshot for the worker.
    pub(crate) fn take_calculation_reset_snapshot(
        &mut self,
    ) -> Option<Arc<crate::calc::CalculationSnapshot>> {
        self.pending_calculation_reset.take()
    }

    /// Advances the calculation-input version without tying it to file dirtiness.
    pub(super) fn advance_calculation_revision(&mut self) {
        self.calculation_revision = self
            .calculation_revision
            .saturating_add(1)
            .max(self.revision);
    }

    // Returns the currently selected table.
    pub fn active_table(&self) -> Option<&Table> {
        self.tables
            .iter()
            .find(|table| table.id == self.active_table)
    }

    // Returns a table selection and scroll state.
    pub fn view(&self, table_id: TableId) -> Option<&TableViewState> {
        self.views.get(&table_id)
    }

    // Returns a mutable reference to a table selection and scroll state.
    pub fn view_mut(&mut self, table_id: TableId) -> Option<&mut TableViewState> {
        self.views.get_mut(&table_id)
    }

    // Selects a table in the document. Returns `false` if the ID does not exist.
    pub fn select_table(&mut self, table_id: TableId) -> bool {
        if self.tables.iter().any(|table| table.id == table_id) {
            self.active_table = table_id;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
impl Document {
    // Returns a mutable reference to the selected table.
    pub fn active_table_mut(&mut self) -> Option<&mut Table> {
        self.tables
            .iter_mut()
            .find(|table| table.id == self.active_table)
    }
    /// Marks or unmarks a cell as kept text, as replacing a formula with its text result does.
    pub fn set_string_cell_local(
        &mut self,
        table_id: TableId,
        address: CellAddress,
        is_string: bool,
    ) -> Result<bool, DocumentError> {
        let table = self
            .tables
            .iter_mut()
            .find(|table| table.id == table_id)
            .ok_or(DocumentError::MissingTable(table_id))?;
        if table.is_string_cell(address) == is_string {
            return Ok(false);
        }
        table.set_string_cell(address, is_string);
        self.advance_calculation_revision();
        Ok(true)
    }
}
