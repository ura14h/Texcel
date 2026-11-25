use super::*;

pub(super) enum PendingGrowthSelection {
    Header {
        document_id: DocumentId,
        table_id: TableId,
        range: CellRange,
        additive: bool,
    },
    Table {
        document_id: DocumentId,
        table_id: TableId,
    },
}

impl TexcelApp {
    /// Apply grid focus, selection, editing, and structure actions after drawing.
    pub(super) fn apply_grid_interaction_results(
        &mut self,
        mut outcome: GridOutcome,
        pending_accesskit_focus: Option<(DocumentId, u64, CellAddress)>,
        fill_handle_drag: Option<FillHandleDrag>,
        navigation_blocked: bool,
        ctx: &egui::Context,
    ) {
        let formula_reference_pick = self.formula_reference_edit_is_current()
            && self.edit.mode == EditMode::FormulaReference;
        if outcome.clicked.is_some()
            || outcome.selected_range.is_some()
            || outcome.add_selection_range.is_some()
            || outcome.selection_drag_started.is_some()
            || outcome.selection_drag_update.is_some()
        {
            self.input_sequence_origin = None;
        }
        if navigation_blocked {
            self.grid_selection_drag = None;
            self.formula_references.drag_anchor = None;
            self.header_resize_drag = None;
        }
        if !navigation_blocked {
            self.grid_focused = outcome.focused
                || outcome.clicked.is_some()
                || outcome.selected_range.is_some()
                || outcome.add_selection_range.is_some()
                || outcome.selection_drag_started.is_some()
                || outcome.selection_drag_update.is_some()
                || self.request_grid_focus;
        }
        if !navigation_blocked {
            if self.grid_focused {
                self.grid_keyboard_owner = ctx.memory(|memory| memory.focused());
            } else {
                self.grid_keyboard_owner = None;
            }
        }
        // Keep the OS input method enabled while a cell is only selected, so the
        // first key of a Japanese word starts a composition instead of plain text.
        if !navigation_blocked
            && self.grid_focused
            && !self.edit.is_editing()
            && !ctx.text_edit_focused()
            && let Some(rect) = outcome.active_cell_rect
        {
            request_grid_ime(ctx, rect, self.ime_interrupt_this_frame);
        }
        if let Some((address, range)) = outcome.text_selection.take() {
            let keep_formula_selection = formula_reference_pick
                && range.is_empty()
                && self.text_selection.is_some_and(|selection| {
                    Some(selection.document_id) == self.edit_document_id
                        && Some(selection.table_id) == self.edit.table_id
                        && Some(selection.address) == self.edit.address
                        && selection.session_id == self.edit.session_id
                        && selection.buffer_revision == self.edit.buffer_revision
                });
            if !keep_formula_selection && let Some(document) = self.workspace.active_document() {
                self.capture_text_selection(document.id, outcome.table_id, address, Some(range));
            }
        } else if !formula_reference_pick
            && (self.edit.target == Some(crate::document::EditTarget::Cell)
                || !self.edit.is_editing())
        {
            self.text_selection = None;
        }
        if let Some(offset) = outcome.scroll
            && let Some(document) = self.workspace.active_document_mut()
            && let Some(table) = document.active_table()
        {
            let table_id = table.id;
            if let Some(view) = document.view_mut(table_id) {
                view.scroll = offset;
            }
        }
        if let Some(extent) = outcome.page_extent {
            self.grid_page_extent = extent;
        }
        if !navigation_blocked
            && let Some((document_id, table_id, target, value)) = outcome.resized_layout.take()
        {
            let applied = self
                .workspace
                .documents
                .get_mut(&document_id)
                .and_then(|document| document.view_mut(table_id))
                .is_some_and(|view| {
                    view.set_layout_value(target, value);
                    true
                });
            if applied && outcome.resize_finished {
                self.mark_document_preference_change(
                    document_id,
                    table_id,
                    TablePreferenceField::ViewLayout,
                );
            }
        }
        if !navigation_blocked
            && let Some((document_id, table_id, column)) = outcome.auto_fit_requested.take()
        {
            self.request_column_auto_fit(ctx, document_id, table_id, column);
        }
        if let Some(drag) = outcome.selection_drag_started {
            if formula_reference_pick {
                self.start_formula_reference_drag(drag, ctx);
            } else {
                self.grid_selection_drag = Some(drag);
            }
        }
        if let Some(address) = outcome.selection_drag_update
            && let Some(drag) = self.grid_selection_drag
        {
            if drag.formula_reference_session.is_some() {
                if self.formula_reference_drag_is_current(drag)
                    && self.formula_references.drag_anchor == Some(drag.anchor)
                {
                    self.update_formula_reference_drag(drag, address, ctx);
                    self.request_scroll_to_selection = false;
                } else {
                    self.grid_selection_drag = None;
                    self.formula_references.drag_anchor = None;
                }
            } else if let Some(document) = self.workspace.documents.get_mut(&drag.document_id)
                && let Some(view) = document.view_mut(drag.table_id)
            {
                if view.selection.head != address {
                    // The status line was drawn before this change; show the new cell count too.
                    ctx.request_repaint();
                }
                view.selection.head = address;
                view.active_cell = Some(drag.anchor);
                self.request_grid_focus = true;
                self.request_scroll_to_selection = false;
            }
        }
        if outcome.selection_drag_finished {
            let finished_reference_drag = self.grid_selection_drag.is_some_and(|drag| {
                drag.formula_reference_session.is_some()
                    && self.formula_reference_drag_is_current(drag)
            });
            self.grid_selection_drag = None;
            if finished_reference_drag && self.formula_references.drag_anchor.take().is_some() {
                self.request_editor_focus = true;
            } else if self.formula_references.drag_anchor.is_some() {
                self.formula_references.drag_anchor = None;
            }
        }
        if outcome.fill_handle_cancelled {
            self.fill_handle_drag = None;
        }
        if let Some(source) = outcome.fill_handle_started {
            self.fill_handle_drag = Some(FillHandleDrag {
                document_id: self
                    .workspace
                    .active_document()
                    .map_or(0, |document| document.id),
                table_id: outcome.table_id,
                source,
                preview: None,
            });
        }
        if let Some(drag) = self.fill_handle_drag.as_mut()
            && drag.table_id == outcome.table_id
            && drag.preview != outcome.fill_handle_preview
        {
            // The status line is drawn before the grid, so draw once more to show the new count.
            drag.preview = outcome.fill_handle_preview;
            ctx.request_repaint();
        }
        let fill_handle_finished = outcome.fill_handle_finished;
        if fill_handle_finished.is_some() {
            self.fill_handle_drag = None;
        }
        if navigation_blocked {
            return;
        }
        if !formula_reference_pick
            && (outcome.selected_range.is_some() || outcome.add_selection_range.is_some())
            && self.edit.is_editing()
        {
            let Some(document_id) = self.edit_document_id else {
                return;
            };
            self.commit_edit(None);
            if self.edit.is_editing() {
                return;
            }
            if self.pending_text_growth_sessions.contains_key(&document_id) {
                let additive = outcome.selected_range.is_none();
                let range = outcome
                    .selected_range
                    .take()
                    .or_else(|| outcome.add_selection_range.take());
                if let Some(range) = range {
                    self.pending_growth_selection = Some(PendingGrowthSelection::Header {
                        document_id,
                        table_id: outcome.table_id,
                        range,
                        additive,
                    });
                }
                return;
            }
        }
        if formula_reference_pick && let Some(range) = outcome.formula_reference_shift_range.take()
        {
            let anchor = self
                .formula_references
                .current_range
                .map_or(range.anchor, |current| current.anchor);
            self.insert_formula_reference(
                CellRange {
                    anchor,
                    head: range.head,
                },
                ctx,
            );
            self.request_editor_focus = true;
        }
        if let Some(endpoint) = fill_handle_finished
            && let Some(source) = fill_handle_drag.map(|drag| drag.source)
        {
            self.request_fill_handle(source, endpoint);
            return;
        }
        if let Some(range) = outcome.selected_range
            && !formula_reference_pick
            && let Some(document) = self.workspace.active_document_mut()
            && let Some(view) = document.view_mut(outcome.table_id)
        {
            if let Some(active) = outcome.selection_active_cell {
                if outcome.preserve_additional_selections {
                    view.selection = range;
                    view.active_cell = Some(active);
                } else {
                    view.set_selection_with_active(range, active);
                }
            } else {
                view.set_selection(range);
            }
        }
        if let Some(range) = outcome.add_selection_range
            && !formula_reference_pick
            && let Some(document) = self.workspace.active_document_mut()
            && let Some(view) = document.view_mut(outcome.table_id)
        {
            match view.toggle_disjoint_selection(range) {
                Ok(()) => self.request_scroll_to_selection = true,
                Err(error) => self.status = Some(error),
            }
        }
        let open_link = outcome.open_link;
        let structure_action = outcome.structure_action;
        if let Some(address) = outcome.clicked.filter(|_| {
            outcome.fill_handle_started.is_none() && outcome.fill_handle_finished.is_none()
        }) {
            if formula_reference_pick {
                self.insert_formula_reference(CellRange::single(address), ctx);
                self.request_editor_focus = true;
            } else if let Some(edit_table) = self.edit.table_id
                && self.edit.is_editing()
                && (edit_table != outcome.table_id || self.edit.address != Some(address))
            {
                if outcome.double_clicked {
                    self.commit_edit(None);
                    self.begin_cell_edit(outcome.table_id, address);
                } else {
                    self.commit_edit(None);
                    self.select_cell(outcome.table_id, address, false);
                }
            } else if outcome.double_clicked {
                self.begin_cell_edit(outcome.table_id, address);
            } else {
                self.select_cell(outcome.table_id, address, false);
                self.request_scroll_to_selection = false;
                if self.edit.is_editing() {
                    self.edit.switch_target(crate::document::EditTarget::Cell);
                    self.request_editor_focus = true;
                }
            }
        }
        if pending_accesskit_focus.is_some() {
            self.request_grid_focus = true;
            self.request_scroll_to_selection = true;
        }
        if open_link {
            self.open_selected_link(ctx);
        }
        if let Some(action) = structure_action {
            self.request_structure_action(action);
        }
        for command in outcome.commands {
            self.dispatch_command(command, commands::InputOwner::Grid, false, None, ctx);
        }
    }

    /// Applies a tab or header choice after its adjacent-cell edit has succeeded.
    pub(super) fn apply_pending_growth_selection(&mut self, document_id: DocumentId) {
        let Some(selection) = self.pending_growth_selection.take() else {
            return;
        };
        match selection {
            PendingGrowthSelection::Header {
                document_id: target_document,
                table_id,
                range,
                additive,
            } if target_document == document_id => {
                if let Some(view) = self
                    .workspace
                    .documents
                    .get_mut(&document_id)
                    .and_then(|document| document.view_mut(table_id))
                {
                    if additive {
                        let _ = view.toggle_disjoint_selection(range);
                    } else {
                        view.set_selection(range);
                    }
                }
            }
            PendingGrowthSelection::Table {
                document_id: target_document,
                table_id,
            } if target_document == document_id => {
                if let Some(document) = self.workspace.documents.get_mut(&document_id) {
                    document.select_table(table_id);
                }
            }
            _ => (),
        }
        if let Some(active_document) = self.workspace.active_document {
            self.request_grid_focus = true;
            self.request_scroll_to_selection = false;
            if active_document == document_id {
                self.request_editor_focus = false;
            }
        }
    }
}

/// Asks the platform to keep IME enabled with candidates placed at the active cell.
fn request_grid_ime(ctx: &egui::Context, rect: Rect, interrupt: bool) {
    let cursor_rect = Rect::from_min_size(rect.left_top(), Vec2::new(1.0, rect.height()));
    ctx.output_mut(|output| {
        output.ime = Some(egui::output::IMEOutput {
            purpose: egui::IMEPurpose::Normal,
            rect,
            cursor_rect,
            should_interrupt_composition: interrupt,
        });
    });
}
