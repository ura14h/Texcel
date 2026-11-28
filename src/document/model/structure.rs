//!  Document operations for structure.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::document::*;

impl Document {
    /// Applies a fully prepared structural replacement through the shared history.
    pub fn execute_prepared_structure(
        &mut self,
        mut prepared: PreparedStructure,
    ) -> Result<StructureSwapResult, DocumentError> {
        if self.calculation_revision != prepared.calculation_revision {
            return Err(DocumentError::StalePreparedStructure);
        }
        let prepared_ids = prepared
            .tables
            .iter()
            .map(|table| table.id)
            .collect::<BTreeSet<_>>();
        let invalid_table_set = prepared.replace_table_set
            && (prepared.tables.is_empty()
                || prepared_ids.len() != prepared.tables.len()
                || !prepared_ids.contains(&prepared.view_table_id));
        let invalid_incremental_set = !prepared.replace_table_set
            && (!self
                .tables
                .iter()
                .any(|table| table.id == prepared.view_table_id)
                || !self.views.contains_key(&prepared.view_table_id)
                || prepared
                    .tables
                    .iter()
                    .any(|table| !self.tables.iter().any(|current| current.id == table.id)));
        if invalid_table_set || invalid_incremental_set {
            return Err(DocumentError::MissingTable(prepared.view_table_id));
        }
        let new_views = self.new_table_view(&prepared);
        if let Some(history_file) = prepared.history_file.take() {
            let target = prepared.target;
            let swap_source = prepared.source_before.is_some() || prepared.source_after.is_some();
            let text_overlay_after = prepared.text_overlay_after.clone();
            let mut string_cells_archive = BTreeMap::new();
            if prepared.replace_table_set {
                let current_settings = prepare_table_set_string_cells(
                    &mut prepared.tables,
                    &self.tables,
                    &mut string_cells_archive,
                );
                archive_removed_table_settings(
                    current_settings,
                    &prepared.tables,
                    &mut string_cells_archive,
                );
            }
            let retired_tables = std::mem::replace(&mut self.tables, prepared.tables);
            let mut view_archive = new_views;
            if prepared.replace_table_set {
                reconcile_table_views(&mut self.views, &self.tables, &mut view_archive);
            }
            if swap_source {
                self.source = prepared.source_after.clone();
            }
            self.text_overlay_cells = Arc::clone(&text_overlay_after);
            if !prepared.replace_table_set
                && let Some(view) = self.views.get_mut(&prepared.view_table_id)
            {
                view.layout = prepared.view_layout_after;
                view.set_selection(prepared.selection_after);
                view.scroll = prepared.scroll_after;
            }
            self.active_table = prepared.view_table_id;
            self.clear_changed_cells();
            self.calculation_cells.clear();
            self.history.truncate(self.history_cursor);
            self.history.push(EditCommand::ReplaceStructure {
                tables: Vec::new(),
                text_overlay_before: None,
                text_overlay_after: None,
                swap_source,
                source: prepared.source_after,
                view_layout: None,
                selection: None,
                scroll: None,
                target,
                target_before: prepared.target_before,
                replace_table_set: prepared.replace_table_set,
                view_archive: prepared.replace_table_set.then_some(view_archive),
                string_cells_archive: prepared.replace_table_set.then_some(string_cells_archive),
                before_calculation: None,
                after_calculation: None,
                history_file: Some(history_file),
                use_after: true,
            });
            self.history_cursor += 1;
            self.record_history_state();
            self.revision = self.revision.saturating_add(1);
            self.note_structure_change();
            self.advance_cell_input_generation();
            self.text_overlay_revision = self.revision;
            self.advance_calculation_revision();
            self.pending_calculation_reset = Some(prepared.after_calculation);
            if let Some(snapshot) = &mut self.pending_calculation_reset {
                Arc::make_mut(snapshot).preserve_string_cells(&self.tables);
            }
            return Ok((target, Some(retired_tables)));
        }
        let Some(before_calculation) = prepared.before_calculation else {
            return Err(DocumentError::NeedsWorkerHistoryRestore);
        };
        let previous_source = self.source.clone();
        let swap_source = prepared.source_before.is_some() || prepared.source_after.is_some();
        if swap_source {
            self.source = prepared.source_before;
        }
        let mut command = EditCommand::ReplaceStructure {
            tables: prepared.tables,
            text_overlay_before: Some(prepared.text_overlay_before),
            text_overlay_after: Some(prepared.text_overlay_after),
            swap_source,
            source: prepared.source_after,
            view_layout: (!prepared.replace_table_set)
                .then_some((prepared.view_table_id, prepared.view_layout_after.clone())),
            selection: (!prepared.replace_table_set)
                .then_some((prepared.view_table_id, prepared.selection_after)),
            scroll: (!prepared.replace_table_set)
                .then_some((prepared.view_table_id, prepared.scroll_after)),
            target: prepared.target,
            target_before: prepared.target_before,
            replace_table_set: prepared.replace_table_set,
            view_archive: prepared.replace_table_set.then_some(new_views),
            string_cells_archive: prepared.replace_table_set.then(BTreeMap::new),
            before_calculation: Some(before_calculation),
            after_calculation: Some(prepared.after_calculation),
            history_file: None,
            use_after: false,
        };
        if let Err(error) = command.apply(self, true, false) {
            if swap_source {
                self.source = previous_source;
            }
            return Err(error);
        }
        if prepared.replace_table_set {
            self.active_table = prepared.view_table_id;
        }
        self.history.truncate(self.history_cursor);
        self.history.push(command);
        self.history_cursor += 1;
        self.record_history_state();
        self.revision = self.revision.saturating_add(1);
        self.note_structure_change();
        self.advance_cell_input_generation();
        self.text_overlay_revision = self.revision;
        self.advance_calculation_revision();
        self.pending_calculation_reset = self
            .history
            .last()
            .and_then(EditCommand::current_calculation_snapshot);
        if let Some(snapshot) = &mut self.pending_calculation_reset {
            Arc::make_mut(snapshot).preserve_string_cells(&self.tables);
        }
        Ok((prepared.target, None))
    }

    // Build the next undo or redo request from the saved structure history.
    pub(crate) fn structure_history_restore_request(
        &self,
        redo: bool,
    ) -> Option<crate::structure::StructureHistoryRestoreRequest> {
        let index = if redo {
            self.history_cursor
        } else {
            self.history_cursor.checked_sub(1)?
        };
        let EditCommand::ReplaceStructure {
            history_file,
            use_after,
            replace_table_set,
            ..
        } = self.history.get(index)?
        else {
            return None;
        };
        if *use_after == redo {
            return None;
        }
        // Only spilled history is restored on the worker; in-memory history swaps tables directly.
        let history_file = history_file.as_ref()?;
        Some(crate::structure::StructureHistoryRestoreRequest {
            document_id: self.id,
            history_file: Arc::clone(history_file),
            redo,
            replace_table_set: *replace_table_set,
            calculation_revision: self.calculation_revision,
        })
    }

    // Apply a prepared restore after checking its document and calculation versions.
    pub(crate) fn apply_prepared_structure_restore(
        &mut self,
        mut prepared: crate::structure::PreparedStructureRestore,
    ) -> Result<((TableId, CellAddress), Vec<Table>), DocumentError> {
        if prepared.document_id != self.id
            || prepared.calculation_revision != self.calculation_revision
        {
            return Err(DocumentError::NeedsWorkerHistoryRestore);
        }
        let index = if prepared.redo {
            self.history_cursor
        } else {
            self.history_cursor
                .checked_sub(1)
                .ok_or(DocumentError::NeedsWorkerHistoryRestore)?
        };
        let matches_history = matches!(
            self.history.get(index),
            Some(EditCommand::ReplaceStructure {
                history_file: Some(history_file),
                use_after,
                replace_table_set,
                ..
            }) if Arc::ptr_eq(history_file, &prepared.history_file)
                && *use_after != prepared.redo
                && *replace_table_set == prepared.replace_table_set
        );
        let invalid_table_set = prepared.replace_table_set
            && (prepared.tables.is_empty()
                || prepared
                    .tables
                    .iter()
                    .map(|table| table.id)
                    .collect::<BTreeSet<_>>()
                    .len()
                    != prepared.tables.len()
                || !prepared
                    .tables
                    .iter()
                    .any(|table| table.id == prepared.view_table_id)
                || !prepared
                    .tables
                    .iter()
                    .any(|table| table.id == prepared.target.0));
        let invalid_incremental_set = !prepared.replace_table_set
            && (prepared.tables.len() != self.tables.len()
                || prepared
                    .tables
                    .iter()
                    .any(|table| !self.tables.iter().any(|current| current.id == table.id))
                || !self.views.contains_key(&prepared.view_table_id));
        if !matches_history || invalid_table_set || invalid_incremental_set {
            return Err(DocumentError::NeedsWorkerHistoryRestore);
        }

        let swap_source = match self.history.get(index) {
            Some(EditCommand::ReplaceStructure { swap_source, .. }) => *swap_source,
            _ => return Err(DocumentError::NeedsWorkerHistoryRestore),
        };
        let current_settings = if prepared.replace_table_set {
            let Some(EditCommand::ReplaceStructure {
                string_cells_archive: Some(archive),
                ..
            }) = self.history.get_mut(index)
            else {
                return Err(DocumentError::NeedsWorkerHistoryRestore);
            };
            Some(prepare_table_set_string_cells(
                &mut prepared.tables,
                &self.tables,
                archive,
            ))
        } else {
            None
        };
        Arc::make_mut(&mut prepared.calculation).preserve_string_cells(&prepared.tables);
        let retired_tables = std::mem::replace(&mut self.tables, prepared.tables);
        if prepared.replace_table_set {
            if let Some(EditCommand::ReplaceStructure { view_archive, .. }) =
                self.history.get_mut(index)
                && let Some(archive) = view_archive.as_mut()
            {
                reconcile_table_views(&mut self.views, &self.tables, archive);
            }
            if let Some(current_settings) = current_settings
                && let Some(EditCommand::ReplaceStructure {
                    string_cells_archive: Some(archive),
                    ..
                }) = self.history.get_mut(index)
            {
                archive_removed_table_settings(current_settings, &self.tables, archive);
            }
        }
        if swap_source || prepared.source.is_some() {
            self.source = prepared.source;
        }
        self.text_overlay_cells = prepared.text_overlay_cells;
        self.active_table = prepared.view_table_id;
        if !prepared.replace_table_set
            && let Some(view) = self.views.get_mut(&prepared.view_table_id)
        {
            view.layout = prepared.view_layout;
            view.selection = prepared.selection;
            view.scroll = prepared.scroll;
        }
        if let Some(EditCommand::ReplaceStructure { use_after, .. }) = self.history.get_mut(index) {
            *use_after = prepared.redo;
        }
        if prepared.redo {
            self.history_cursor += 1;
        } else {
            self.history_cursor -= 1;
        }
        self.clear_changed_cells();
        self.calculation_cells.clear();
        self.revision = self.revision.saturating_add(1);
        self.note_structure_change();
        self.advance_cell_input_generation();
        self.text_overlay_revision = self.revision;
        self.advance_calculation_revision();
        self.pending_calculation_reset = Some(prepared.calculation);
        Ok((prepared.target, retired_tables))
    }
}

impl Document {
    /// Gives a table that the structure change creates its prepared view (a duplicate's copied
    /// widths, say). Views of tables that come into being are taken from the archive, so the
    /// view is put there; an existing table keeps its own.
    fn new_table_view(&self, prepared: &PreparedStructure) -> BTreeMap<TableId, TableViewState> {
        let mut archive = BTreeMap::new();
        if prepared.replace_table_set && !self.views.contains_key(&prepared.view_table_id) {
            let mut view = TableViewState {
                layout: Arc::clone(&prepared.view_layout_after),
                scroll: prepared.scroll_after,
                ..TableViewState::default()
            };
            view.set_selection(prepared.selection_after);
            archive.insert(prepared.view_table_id, view);
        }
        archive
    }
}
