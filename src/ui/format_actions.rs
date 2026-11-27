use super::*;

impl TexcelApp {
    // Show the full content of the selected cell in a separate panel.
    pub(super) fn draw_long_text_panel(&mut self, ctx: &egui::Context) {
        if !self.long_text_panel_open
            || self.edit.target != Some(crate::document::EditTarget::DetailPanel)
            || !self.edit.is_editing()
        {
            return;
        }
        let Some(document_id) = self.edit_document_id else {
            self.long_text_panel_open = false;
            return;
        };
        let session_id = self.edit.session_id;
        let editor = editor_id(document_id, session_id);
        let mut cancel = false;
        let mut selection = None;
        let ctx = ctx.clone();
        let modal_id = Id::unique(("texcel_long_text_panel", document_id, session_id));
        let cancel_key = !self.app_close_dialog_open
            && self.dialog_key_pressed_in_modal(&ctx, modal_id, Key::Escape, self.edit.composing);
        let mut commit = !self.app_close_dialog_open
            && self.dialog_command_key_pressed_in_modal(
                &ctx,
                modal_id,
                Key::Enter,
                self.edit.composing,
            );
        egui::Modal::new(modal_id).show(&ctx, |ui| {
            let width = (ctx.content_rect().width() - 40.0).clamp(360.0, 820.0);
            ui.set_min_width(width.min(620.0));
            ui.set_max_width(width);
            ui.set_max_height((ctx.content_rect().height() - 40.0).max(300.0));
            visual::dialog_surface(ui, |ui| {
                visual::prepare_dialog_body(ui);
                visual::dialog_title(ui, &context_text(ui.ctx(), "formula-detail-title"));
                visual::caption(ui, &context_text(ui.ctx(), "formula-detail-help"));
                let mut output = None;
                egui::ScrollArea::both()
                    .id_salt(("cell_detail_editor", document_id, session_id))
                    .auto_shrink([false, true])
                    .max_height((ui.available_height() - 2.0 * visual::BUTTON_HEIGHT).max(160.0))
                    .show(ui, |ui| {
                        let width = ui.available_width();
                        output = Some(show_bounded_edit_text(
                            ui,
                            &ctx,
                            editor,
                            &mut self.edit,
                            CellEditWidget::Multiline {
                                width,
                                rows: 14,
                                lock_focus: true,
                            },
                        ));
                    });
                let Some(output) = output else {
                    return;
                };
                if self.request_editor_focus {
                    // Requesting focus again would interrupt an IME composition.
                    if !output.response.has_focus() {
                        output.response.request_focus();
                    }
                    self.request_editor_focus = false;
                }
                if output.response.has_focus() {
                    self.active_modal_text_input_focus = Some(editor);
                }
                selection = output.selection_bytes;
                ui.separator();
                visual::dialog_footer(ui, |ui| {
                    if cfg!(target_os = "macos") {
                        let commit_button = egui::Button::new(commit_edit_label(ui.ctx()))
                            .fill(ui.visuals().selection.bg_fill);
                        if ui
                            .add_sized([132.0, visual::BUTTON_HEIGHT], commit_button)
                            .clicked()
                        {
                            commit = true;
                        }
                        if ui
                            .button(context_text(ui.ctx(), "action-cancel-edit"))
                            .clicked()
                        {
                            cancel = true;
                        }
                    } else {
                        if ui
                            .button(context_text(ui.ctx(), "action-cancel-edit"))
                            .clicked()
                        {
                            cancel = true;
                        }
                        let commit_button = egui::Button::new(commit_edit_label(ui.ctx()))
                            .fill(ui.visuals().selection.bg_fill);
                        if ui
                            .add_sized([132.0, visual::BUTTON_HEIGHT], commit_button)
                            .clicked()
                        {
                            commit = true;
                        }
                    }
                });
            });
        });
        if let (Some(table_id), Some(address), Some(selection)) =
            (self.edit.table_id, self.edit.address, selection)
        {
            self.capture_text_selection(document_id, table_id, address, Some(selection));
        }
        if cancel || cancel_key {
            self.cancel_edit();
        } else if commit {
            self.commit_edit(None);
        }
    }

    // Apply a row or column visibility, size, or freeze action.
    pub(super) fn apply_layout_action(&mut self, action: LayoutAction, ctx: &egui::Context) {
        if self.edit.is_editing() {
            self.commit_edit(None);
        }
        let Some((document_id, table_id, active_cell, has_disjoint_ranges)) =
            self.workspace.active_document().and_then(|document| {
                let table = document.active_table()?;
                let view = document.view(table.id)?;
                Some((
                    document.id,
                    table.id,
                    view.active_cell(),
                    !view.additional_selections.is_empty(),
                ))
            })
        else {
            return;
        };
        if has_disjoint_ranges
            && !matches!(
                action,
                LayoutAction::Unfreeze | LayoutAction::AutoFitAllColumns
            )
        {
            self.status = Some(Message::new("status-single-range-required"));
            return;
        }
        if self.pending_structures.contains_key(&document_id)
            || self.pending_history_restores.contains_key(&document_id)
        {
            self.status = Some(Message::new(
                "status-finish-row-column-preparation-before-layout-change",
            ));
            return;
        }
        match action {
            LayoutAction::EditColumnWidth => {
                self.begin_layout_dialog(document_id, table_id, active_cell);
                return;
            }
            LayoutAction::AutoFitAllColumns => {
                self.request_all_columns_auto_fit(ctx, document_id, table_id);
                return;
            }
            LayoutAction::FreezeRows | LayoutAction::FreezeColumns | LayoutAction::Unfreeze => {}
        }
        let address = active_cell;
        let values = self
            .workspace
            .documents
            .get(&document_id)
            .and_then(|document| {
                let layout = &document.view(table_id)?.layout;
                let values = match action {
                    LayoutAction::FreezeRows => {
                        let frozen = layout.frozen();
                        vec![(
                            LayoutTarget::Freeze,
                            LayoutValue::Freeze(FrozenPanes {
                                rows: address.row.saturating_add(1),
                                columns: frozen.columns,
                            }),
                        )]
                    }
                    LayoutAction::FreezeColumns => {
                        let frozen = layout.frozen();
                        vec![(
                            LayoutTarget::Freeze,
                            LayoutValue::Freeze(FrozenPanes {
                                rows: frozen.rows,
                                columns: address.column.saturating_add(1),
                            }),
                        )]
                    }
                    LayoutAction::Unfreeze => vec![(
                        LayoutTarget::Freeze,
                        LayoutValue::Freeze(FrozenPanes::default()),
                    )],
                    LayoutAction::EditColumnWidth | LayoutAction::AutoFitAllColumns => Vec::new(),
                };
                Some(values)
            })
            .unwrap_or_default();
        let changed = self.apply_layout_values(document_id, table_id, values);
        match changed {
            Ok(false) => self.status = Some(Message::new("status-layout-unchanged")),
            Ok(true) => {
                self.normalize_selection_for_layout(document_id, table_id);
                self.schedule_calculation(document_id);
            }
            Err(error) => self.status = Some(error),
        }
    }

    // Open a dialog to edit the active column width.
    pub(super) fn begin_layout_dialog(
        &mut self,
        document_id: DocumentId,
        table_id: u64,
        address: CellAddress,
    ) {
        let Some(layout) = self
            .workspace
            .documents
            .get(&document_id)
            .and_then(|document| document.view(table_id))
            .map(|view| &view.layout)
        else {
            return;
        };
        let value = super::layout_dialog::format_column_width(
            layout
                .column(address.column)
                .and_then(|column| column.width)
                .unwrap_or_else(|| {
                    crate::document::column_width_units_for_pixels(
                        crate::document::DEFAULT_COLUMN_WIDTH_PIXELS,
                        self.grid_character_width,
                    )
                }),
        );
        self.layout_dialog = Some(LayoutDialogState {
            document_id,
            table_id,
            address,
            value,
            focus_input: true,
        });
    }
}

/// Shows the platform's real shortcut beside the edit action.
fn commit_edit_label(ctx: &egui::Context) -> String {
    let shortcut = ctx.format_shortcut(&egui::KeyboardShortcut::new(
        egui::Modifiers::COMMAND,
        egui::Key::Enter,
    ));
    format!("{} ({shortcut})", context_text(ctx, "action-commit-edit"))
}
