use super::*;

#[path = "grid/display_requests.rs"]
mod display_requests;

impl TexcelApp {
    // Handle grid input and draw the visible spreadsheet area.
    pub(super) fn draw_grid(&mut self, root: &mut egui::Ui, navigation_blocked: bool) {
        self.refresh_formula_references();
        let mut outcome = GridOutcome::default();
        let mut markdown_display_requests = Vec::new();
        let ctx = root.ctx().clone();
        let show_invisibles = self.show_invisibles;
        let font_layout_generation = self.font_layout_generation;
        let grid_scale = self.grid_scale();
        let active_grid = self
            .workspace
            .active_document()
            .and_then(|document| document.active_table().map(|table| (document.id, table.id)));
        let pending_accesskit_focus = if navigation_blocked {
            None
        } else {
            self.pending_accesskit_focus
                .take()
                .filter(|(document_id, table_id, _)| active_grid == Some((*document_id, *table_id)))
        };
        let request_scroll_to_address = self
            .request_scroll_to_address
            .take()
            .filter(|(document_id, table_id, _)| active_grid == Some((*document_id, *table_id)))
            .map(|(_, _, address)| address);
        let accesskit_table_token = active_grid.and_then(|(document_id, table_id)| {
            self.ensure_accesskit_table_token(document_id, table_id)
        });
        if self
            .fill_handle_drag
            .is_some_and(|drag| active_grid != Some((drag.document_id, drag.table_id)))
        {
            self.fill_handle_drag = None;
        }
        if self
            .grid_selection_drag
            .is_some_and(|drag| active_grid != Some((drag.document_id, drag.table_id)))
        {
            self.grid_selection_drag = None;
            self.formula_references.drag_anchor = None;
        }
        if self
            .header_resize_drag
            .is_some_and(|drag| active_grid != Some((drag.document_id, drag.table_id)))
        {
            self.header_resize_drag = None;
        }
        if self.grid_selection_drag.is_some_and(|drag| {
            drag.formula_reference_session.is_some()
                && !self.formula_reference_drag_is_current(drag)
        }) {
            self.grid_selection_drag = None;
            self.formula_references.drag_anchor = None;
        }
        let fill_handle_drag = self.fill_handle_drag;
        let grid_selection_drag = self.grid_selection_drag;
        let structure_block_reason_key = if navigation_blocked {
            Some("status-file-wait")
        } else if self.active_structure_pending() {
            Some("grid-structure-preparing")
        } else if self.active_history_restore_pending() {
            Some("grid-history-restoring")
        } else if self
            .workspace
            .active_document()
            .is_some_and(|document| self.pending_saves.contains_key(&document.id))
        {
            Some("status-row-column-change-after-save")
        } else if self
            .workspace
            .active_document()
            .is_some_and(|document| self.document_bulk_edit_pending(document.id))
        {
            Some("status-cell-edit-paused-during-bulk")
        } else {
            self.workspace.active_document().and_then(|document| {
                let table = document.active_table()?;
                let view = document.view(table.id)?;
                (!view.additional_selections.is_empty())
                    .then_some("status-row-column-operation-single-range-required")
            })
        };
        let structure_block_reason = structure_block_reason_key.map(|key| text(self.language, key));
        let calculation_pending = self.workspace.active_document().is_some_and(|document| {
            self.pending_calculations.contains_key(&document.id)
                || self.pending_initial_calculations.contains_key(&document.id)
                || self.calculation_retries.contains(&document.id)
        });
        let formula_reference_mode = self.formula_reference_edit_is_current()
            && self.edit.mode == EditMode::FormulaReference;
        let formula_reference_overlays = self.formula_references.overlays();
        let cell_edit_locked = self
            .workspace
            .active_document()
            .is_some_and(|document| self.cell_edit_locked(document.id));
        let context_commands_enabled = self.grid_context_command_availability(navigation_blocked);
        self.expire_clipboard_mark();
        let copy_marquee = self
            .active_copy_marquee()
            .map(|marquee| (marquee.ranges.to_vec(), marquee.cut));
        {
            let workspace = &self.workspace;
            let grid_row_layouts = &mut self.grid_row_layouts;
            let markdown_cell_displays = &mut self.markdown_cell_displays;
            let markdown_display_requests = &mut markdown_display_requests;
            let edit = &mut self.edit;
            let request_editor_focus = &mut self.request_editor_focus;
            let request_grid_focus = &mut self.request_grid_focus;
            let request_scroll_to_selection = &mut self.request_scroll_to_selection;
            let header_resize_drag = &mut self.header_resize_drag;
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::central_panel(root.style()).inner_margin(egui::Margin {
                        left: CHROME_EDGE_INSET,
                        right: CHROME_EDGE_INSET,
                        top: CHROME_EDGE_INSET,
                        bottom: 0,
                    }),
                )
                .show(root, |ui| {
                    // A window without a document is closing, hiding, or waiting for a file
                    // to open, so it stays blank instead of showing a passing notice.
                    let Some(document) = workspace.active_document() else {
                        return;
                    };
                    let Some(table) = document.active_table() else {
                        ui.centered_and_justified(|ui| {
                            ui.label(context_text(ui.ctx(), "grid-no-table"))
                        });
                        return;
                    };
                    let display = GridDisplay {
                        mode: match document.display_mode {
                            crate::document::DocumentDisplayMode::Live => CellDisplayMode::View,
                            crate::document::DocumentDisplayMode::Source => CellDisplayMode::Source,
                        },
                        show_invisibles,
                        wrap_text: document.view(table.id).is_some_and(|view| view.wrap_text),
                        font_layout_generation,
                        scale: grid_scale,
                    };
                    let row_layout = grid_row_layouts.entry((document.id, table.id)).or_default();
                    outcome = draw_virtual_grid(
                        ui,
                        GridTable::new(document, table, grid_scale),
                        display,
                        row_layout,
                        GridInput {
                            allow_edit: !cell_edit_locked,
                            accesskit_table_token,
                            navigation_blocked,
                            calculation_pending,
                            structure_block_reason: structure_block_reason.as_deref(),
                            context_commands_enabled,
                            formula_reference_mode,
                            formula_reference_overlays: &formula_reference_overlays,
                            fill_handle_drag,
                            selection_drag: grid_selection_drag,
                            scroll_to_address: request_scroll_to_address,
                            copy_marquee: copy_marquee.as_ref().map(|(ranges, cut)| {
                                super::clipboard::CopyMarquee { ranges, cut: *cut }
                            }),
                        },
                        GridState {
                            edit,
                            request_editor_focus,
                            request_grid_focus,
                            request_scroll_to_selection,
                            header_resize_drag,
                            markdown_cell_displays,
                            markdown_display_requests,
                        },
                    );
                });
        }

        if let Some((_, table_id, address)) = pending_accesskit_focus
            && outcome.table_id == table_id
        {
            outcome.clicked = Some(address);
        }

        self.restore_unmarked_ime_after_grid(&ctx);
        self.request_cell_display_previews(markdown_display_requests);

        self.refresh_formula_references();

        self.apply_grid_interaction_results(
            outcome,
            pending_accesskit_focus,
            fill_handle_drag,
            navigation_blocked,
            &ctx,
        );
    }
}
