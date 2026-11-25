use super::*;

#[cfg(test)]
#[path = "layout_dialog_test.rs"]
mod tests;

impl TexcelApp {
    /// Draws the column width dialog.
    pub(super) fn draw_layout_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog_info) = self.layout_dialog.as_ref() else {
            return;
        };
        let mut apply = false;
        let mut cancel = false;
        let mut modal_text_focus = None;
        let modal_id = Id::unique((
            "layout_dialog",
            dialog_info.document_id,
            dialog_info.table_id,
        ));
        let apply_key =
            self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Enter, self.edit.composing);
        let cancel_key =
            self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Escape, self.edit.composing);
        let Some(dialog) = self.layout_dialog.as_mut() else {
            return;
        };
        egui::Modal::new(modal_id).show(ctx, |ui| {
            let width = (ctx.content_rect().width() - 40.0).clamp(180.0, 520.0);
            ui.set_min_width(width.min(320.0));
            ui.set_max_width(width);
            visual::dialog_surface(ui, |ui| {
                visual::prepare_dialog_body(ui);
                visual::dialog_title(ui, &context_text(ui.ctx(), "dialog-layout-title"));
                visual::caption(ui, &context_text(ui.ctx(), "dialog-layout-width"));
                let value_id = Id::unique(egui::IdSalt::new((
                    "layout_dialog_value",
                    dialog.document_id,
                    dialog.table_id,
                )));
                if dialog.focus_input {
                    ui.memory_mut(|memory| memory.request_focus(value_id));
                }
                let value = ui.add_sized(
                    [ui.available_width(), visual::BUTTON_HEIGHT],
                    egui::TextEdit::singleline(&mut dialog.value).id(value_id),
                );
                if value.has_focus() {
                    modal_text_focus = Some(value.id);
                }
                if dialog.focus_input {
                    value.request_focus();
                    dialog.focus_input = false;
                }
                let value_is_invalid = match dialog.value.trim().parse::<f64>() {
                    Ok(value) => !value.is_finite() || value < 0.0,
                    Err(_) => {
                        !dialog.value.trim().is_empty()
                            || self.status.as_ref().is_some_and(|status| {
                                status.key() == "status-value-nonnegative-required"
                            })
                    }
                };
                if value_is_invalid {
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        context_text(ui.ctx(), "status-value-nonnegative-required"),
                    );
                }
                visual::group_gap(ui);
                ui.separator();
                visual::dialog_footer(ui, |ui| {
                    #[cfg(target_os = "macos")]
                    {
                        if layout_action_button(ui, context_text(ui.ctx(), "action-apply"), true) {
                            apply = true;
                        }
                        if layout_action_button(ui, context_text(ui.ctx(), "action-cancel"), false)
                        {
                            cancel = true;
                        }
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        if layout_action_button(ui, context_text(ui.ctx(), "action-cancel"), false)
                        {
                            cancel = true;
                        }
                        if layout_action_button(ui, context_text(ui.ctx(), "action-apply"), true) {
                            apply = true;
                        }
                    }
                });
            });
        });
        if modal_text_focus.is_some() {
            self.active_modal_text_input_focus = modal_text_focus;
        }
        if cancel || cancel_key {
            self.layout_dialog = None;
            return;
        }
        if apply || apply_key {
            let state = dialog.clone();
            if self.apply_dimension_dialog(&state) {
                self.layout_dialog = None;
            }
        }
    }

    /// Applies a submitted column width.
    pub(super) fn apply_dimension_dialog(&mut self, dialog: &LayoutDialogState) -> bool {
        let value = match dialog.value.trim().parse::<f64>() {
            Ok(value) if value.is_finite() && value >= 0.0 => round_column_width(value),
            _ => {
                self.status = Some(Message::new("status-value-nonnegative-required"));
                return false;
            }
        };
        let value = if value > 0.0 {
            value.clamp(0.1, 255.0)
        } else {
            0.0
        };
        let current = self
            .workspace
            .documents
            .get(&dialog.document_id)
            .and_then(|document| {
                let layout = &document.view(dialog.table_id)?.layout;
                let mut column = layout.column(dialog.address.column).unwrap_or_default();
                column.width = Some(value);
                Some((
                    LayoutTarget::Column(dialog.address.column),
                    LayoutValue::Column(Some(column)),
                ))
            });
        let Some(change) = current else {
            self.status = Some(Message::new("status-target-table-required"));
            return false;
        };
        match self.apply_layout_values(dialog.document_id, dialog.table_id, vec![change]) {
            Ok(true) => {
                self.schedule_calculation(dialog.document_id);
                true
            }
            Ok(false) => {
                self.status = Some(Message::new("status-size-unchanged"));
                true
            }
            Err(error) => {
                self.status = Some(error);
                false
            }
        }
    }

    /// Updates the selected layout values in the active document.
    pub(super) fn apply_layout_values(
        &mut self,
        document_id: DocumentId,
        table_id: u64,
        values: Vec<(LayoutTarget, LayoutValue)>,
    ) -> Result<bool, Message> {
        let changed = {
            let view = self
                .workspace
                .documents
                .get_mut(&document_id)
                .and_then(|document| document.view_mut(table_id))
                .ok_or_else(|| Message::new("status-no-table"))?;
            let mut changed = false;
            for (target, value) in values {
                let before = view.layout.value(target);
                if before != value {
                    view.set_layout_value(target, value);
                    changed = true;
                }
            }
            changed
        };
        if changed {
            self.mark_document_preference_change(
                document_id,
                table_id,
                TablePreferenceField::ViewLayout,
            );
        }
        Ok(changed)
    }

    /// Moves the selection to visible rows and columns.
    pub(super) fn normalize_selection_for_layout(
        &mut self,
        document_id: DocumentId,
        table_id: u64,
    ) -> Option<CellAddress> {
        let document = self.workspace.documents.get_mut(&document_id)?;
        // Stop when the table no longer exists.
        document.tables.iter().find(|table| table.id == table_id)?;
        let rows = MAX_ROWS;
        let columns = MAX_COLUMNS;
        let (active_cell, normalized) = {
            let view = document.views.get(&table_id)?;
            (
                view.active_cell(),
                crate::document::TableViewState::normalized_active_address(
                    view.selection,
                    view.active_cell(),
                    rows,
                    columns,
                ),
            )
        };
        let normalized = normalized?;
        if let Some(view) = document.views.get_mut(&table_id) {
            view.clamp_selection_bounds(rows, columns);
            let (selection_start, selection_end) = view.selection.normalized();
            let active_is_selected = normalized.row >= selection_start.row
                && normalized.row <= selection_end.row
                && normalized.column >= selection_start.column
                && normalized.column <= selection_end.column;
            if !active_is_selected {
                view.set_selection(CellRange::single(normalized));
            }
            if active_cell != normalized {
                view.active_cell = Some(normalized);
            }
        }
        Some(normalized)
    }

    /// Keeps the active selection on visible cells.
    pub(super) fn normalize_active_selection(&mut self) {
        let Some((document_id, table_id, address)) =
            self.workspace.active_document().and_then(|document| {
                let table = document.active_table()?;
                let address = document.view(table.id)?.active_cell();
                Some((document.id, table.id, address))
            })
        else {
            return;
        };
        match self.normalize_selection_for_layout(document_id, table_id) {
            Some(normalized) if normalized != address => {
                self.request_scroll_to_selection = true;
            }
            None => {
                self.request_editor_focus = false;
            }
            _ => (),
        }
    }

    /// Captures selected text from the active editor.
    pub(super) fn capture_text_selection(
        &mut self,
        document_id: DocumentId,
        table_id: u64,
        address: CellAddress,
        selection: Option<std::ops::Range<usize>>,
    ) {
        self.text_selection = selection
            .filter(|range| range.start < range.end)
            .map(|range| {
                let start = clamp_byte_boundary(&self.edit.buffer, range.start);
                let end = clamp_byte_boundary(&self.edit.buffer, range.end);
                TextSelection {
                    document_id,
                    table_id,
                    address,
                    session_id: self.edit.session_id,
                    buffer_revision: self.edit.buffer_revision,
                    start: self.edit.buffer[..start].chars().count(),
                    end: self.edit.buffer[..end].chars().count(),
                }
            });
    }
}

/// Rounds an entered column width to the two decimal places the dialog offers.
pub(super) fn round_column_width(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

/// Shows a column width with at most two decimal places and no trailing zeros.
pub(super) fn format_column_width(value: f64) -> String {
    let text = format!("{:.2}", round_column_width(value));
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// Keeps the size dialog's fixed footer aligned with other modal actions.
fn layout_action_button(ui: &mut egui::Ui, label: String, primary: bool) -> bool {
    let button = egui::Button::new(label);
    let button = if primary {
        button.fill(ui.visuals().selection.bg_fill)
    } else {
        button
    };
    ui.add_sized([112.0, visual::BUTTON_HEIGHT], button)
        .clicked()
}
