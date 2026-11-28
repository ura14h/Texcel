use super::*;

#[path = "app_close/discard.rs"]
mod discard;
#[path = "app_close/save_batch.rs"]
mod save_batch;
#[path = "app_close/window_edits.rs"]
mod window_edits;

#[cfg(test)]
#[path = "app_close_test.rs"]
mod tests;

enum AppCloseAction {
    Save,
    Discard,
    Cancel,
    ResolveDraft(DocumentId),
}

impl TexcelApp {
    /// Opens a blocking app-level confirmation only when a document needs attention.
    pub(super) fn draw_app_close_dialog(&mut self, ctx: &egui::Context) {
        if !self.app_close_dialog_open
            || self.export_dialog.is_some()
            || self.active_export_confirmation.is_some()
        {
            return;
        }
        let mut action = None;
        let modal_id = Id::unique("app_close_confirmation");
        let save_key =
            visual::dialog_key_pressed_in_modal(ctx, modal_id, Key::Enter, self.edit.composing);
        let can_save = self.app_close_save_tokens.is_empty()
            && self.pending_saves.is_empty()
            && !self.app_close_has_unresolved_drafts()
            && !self.app_close_has_active_composition()
            && !self.app_close_has_pending_operations();
        let can_discard = self.app_close_save_tokens.is_empty()
            && self.pending_saves.is_empty()
            && !self.app_close_has_active_composition()
            && !self.app_close_has_pending_operations();
        let modal_response = egui::Modal::new(modal_id).show(ctx, |ui| {
            let width = (ctx.content_rect().width() - 40.0).clamp(300.0, 720.0);
            ui.set_min_width(width.min(480.0));
            ui.set_max_width(width);
            ui.set_max_height((ctx.content_rect().height() - 40.0).max(240.0));
            visual::dialog_surface(ui, |ui| {
                visual::prepare_dialog_body(ui);
                visual::dialog_title(ui, &context_text(ui.ctx(), "dialog-app-close-title"));
                visual::group_gap(ui);
                // Let the list use its natural height until the modal needs to scroll.
                let body_height =
                    (ui.available_height() - 2.0 * visual::BUTTON_HEIGHT - 3.0 * visual::SPACE_8)
                        .max(96.0);
                egui::ScrollArea::vertical()
                    .max_height(body_height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        self.draw_app_close_document_rows(ui, &mut action);
                        if self.app_close_has_pending_operations() {
                            visual::caption(
                                ui,
                                &context_text(ui.ctx(), "dialog-app-close-wait-operations"),
                            );
                        }
                        if self.app_close_has_active_composition() {
                            visual::caption(
                                ui,
                                &context_text(ui.ctx(), "dialog-app-close-wait-composition"),
                            );
                        }
                        if !self.app_close_save_tokens.is_empty() {
                            visual::caption(ui, &context_text(ui.ctx(), "dialog-app-close-saving"));
                        }
                    });
                ui.separator();
                visual::dialog_footer(ui, |ui| {
                    #[cfg(target_os = "macos")]
                    {
                        if self.draw_app_close_button(
                            ui,
                            "save",
                            context_text(ui.ctx(), "action-save-all-and-exit"),
                            can_save,
                            true,
                        ) {
                            action = Some(AppCloseAction::Save);
                        }
                        if self.draw_app_close_button(
                            ui,
                            "discard",
                            context_text(ui.ctx(), "action-discard-all-and-exit"),
                            can_discard,
                            false,
                        ) {
                            action = Some(AppCloseAction::Discard);
                        }
                        if self.draw_app_close_button(
                            ui,
                            "cancel",
                            context_text(ui.ctx(), "action-cancel"),
                            true,
                            false,
                        ) {
                            action = Some(AppCloseAction::Cancel);
                        }
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        if self.draw_app_close_button(
                            ui,
                            "cancel",
                            context_text(ui.ctx(), "action-cancel"),
                            true,
                            false,
                        ) {
                            action = Some(AppCloseAction::Cancel);
                        }
                        if self.draw_app_close_button(
                            ui,
                            "discard",
                            context_text(ui.ctx(), "action-discard-all-and-exit"),
                            can_discard,
                            false,
                        ) {
                            action = Some(AppCloseAction::Discard);
                        }
                        if self.draw_app_close_button(
                            ui,
                            "save",
                            context_text(ui.ctx(), "action-save-all-and-exit"),
                            can_save,
                            true,
                        ) {
                            action = Some(AppCloseAction::Save);
                        }
                    }
                });
            });
        });
        let cancel_key = !self.edit.composing
            && !modal_response.any_popup_open
            && !ctx.input(|input| {
                input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Ime(_)))
            })
            && ctx.input(|input| input.key_pressed(Key::Escape));
        if cancel_key {
            ctx.input_mut(|input| {
                input.consume_key(egui::Modifiers::NONE, Key::Escape);
            });
        }
        if save_key && can_save {
            action = Some(AppCloseAction::Save);
        } else if cancel_key {
            action = Some(AppCloseAction::Cancel);
        }
        match action {
            Some(AppCloseAction::Save) => self.save_all_before_exit(),
            Some(AppCloseAction::Discard) => self.discard_all_before_exit(),
            Some(AppCloseAction::Cancel) => self.cancel_app_close(),
            Some(AppCloseAction::ResolveDraft(document_id)) => {
                self.workspace.select_document(document_id);
                self.restore_pending_text_growth_draft(document_id);
                self.restore_recovered_cell_edit(document_id);
                self.app_close_dialog_open = false;
                self.status = Some(Message::new("status-app-close-resolve-draft"));
            }
            None => (),
        }
    }

    /// Lists only the documents that need a choice before exit, with the same headed
    /// columns and plain rows as the import report.
    fn draw_app_close_document_rows(&self, ui: &mut egui::Ui, action: &mut Option<AppCloseAction>) {
        ui.label(context_text(ui.ctx(), "dialog-app-close-help"));
        if self.app_close_save_failed {
            ui.colored_label(
                ui.visuals().error_fg_color,
                context_text(ui.ctx(), "dialog-app-close-save-failed"),
            );
        }
        let rows = self
            .workspace
            .order
            .iter()
            .filter_map(|document_id| {
                let document = self.workspace.documents.get(document_id)?;
                let draft = self.pending_recovered_cell_edits.contains_key(document_id);
                let changed = self.document_has_unsaved_changes(*document_id)
                    || self.pending_saves.contains_key(document_id)
                    || self.app_close_window_has_changed_edit(*document_id);
                (draft || changed).then(|| (*document_id, document.name.clone(), draft))
            })
            .collect::<Vec<_>>();
        if rows.is_empty() {
            return;
        }
        ui.add_space(visual::SPACE_8);
        let document_heading = context_text(ui.ctx(), "dialog-app-close-column-document");
        let status_heading = context_text(ui.ctx(), "dialog-app-close-column-status");
        let changed = context_text(ui.ctx(), "dialog-app-close-changed");
        let draft = context_text(ui.ctx(), "dialog-app-close-draft");
        // The name column is as wide as its widest name, so the statuses line up after it.
        let name_width = rows
            .iter()
            .map(|(_, name, _)| name.as_str())
            .chain(std::iter::once(document_heading.as_str()))
            .map(|name| report_rows::text_width(ui, name))
            .fold(0.0_f32, f32::max)
            .min(ui.available_width() * 0.5)
            + visual::SPACE_16;
        let inset = report_rows::REPORT_ROW_INSET;
        report_rows::column_headings(
            ui,
            &[
                (inset, document_heading.as_str()),
                (inset + name_width, status_heading.as_str()),
            ],
        );
        for (document_id, name, needs_draft_choice) in rows {
            let status = if needs_draft_choice { &draft } else { &changed };
            let (rect, response) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), visual::BUTTON_HEIGHT),
                Sense::hover(),
            );
            let summary = format!("{name} {status}");
            response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &summary));
            let font = FontId::proportional(visual::BODY_SIZE);
            let color = ui.visuals().text_color();
            let name_rect = Rect::from_min_max(
                rect.min,
                egui::pos2(
                    rect.left() + inset + name_width - visual::SPACE_8,
                    rect.max.y,
                ),
            );
            ui.painter()
                .with_clip_rect(name_rect.intersect(ui.clip_rect()))
                .text(
                    egui::pos2(rect.left() + inset, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    name,
                    font.clone(),
                    color,
                );
            ui.painter().text(
                egui::pos2(rect.left() + inset + name_width, rect.center().y),
                egui::Align2::LEFT_CENTER,
                status,
                font,
                color,
            );
            if needs_draft_choice {
                // The draft's own choice sits at the right end of its row.
                let label = context_text(ui.ctx(), "action-resolve-draft");
                let button_width = report_rows::text_width(ui, &label) + 24.0;
                let button_rect = Rect::from_min_max(
                    egui::pos2(rect.right() - button_width, rect.top()),
                    rect.max,
                );
                if ui.put(button_rect, egui::Button::new(label)).clicked() {
                    *action = Some(AppCloseAction::ResolveDraft(document_id));
                }
            }
        }
    }

    /// Reports whether any open document requires a save or explicit choice.
    pub(super) fn app_close_needs_save(&self) -> bool {
        self.app_close_has_pending_operations()
            || self.workspace.order.iter().any(|document_id| {
                self.workspace.documents.get(document_id).is_some_and(|_| {
                    self.document_has_unsaved_changes(*document_id)
                        || self.pending_saves.contains_key(document_id)
                        || self.app_close_window_has_changed_edit(*document_id)
                })
            })
    }

    /// Submits every pending document save and waits for matching worker results.
    pub(super) fn save_all_before_exit(&mut self) {
        self.start_app_close_save_batch();
    }

    /// Chooses every save destination before committing any document buffer.
    #[cfg(test)]
    pub(super) fn save_all_before_exit_with_path_picker(
        &mut self,
        mut pick_path: impl FnMut(&mut Self, DocumentId) -> Option<PathBuf>,
    ) {
        if !self.app_close_save_tokens.is_empty()
            || !self.pending_saves.is_empty()
            || !self.pending_save_intents.is_empty()
        {
            return;
        }
        self.app_close_save_failed = false;
        self.app_close_save_generation = self.app_close_save_generation.wrapping_add(1);
        let save_generation = self.app_close_save_generation;
        if self.app_close_has_unresolved_drafts() || self.app_close_has_active_composition() {
            return;
        }
        if self.app_close_has_pending_operations() {
            return;
        }
        let active_edit_document = self.edit_document_id.or(self.workspace.active_document);
        let mut document_ids = self
            .workspace
            .order
            .iter()
            .copied()
            .filter(|document_id| self.document_has_unsaved_changes(*document_id))
            .collect::<Vec<_>>();
        document_ids.sort_by_key(|document_id| Some(*document_id) != active_edit_document);
        let mut targets = Vec::with_capacity(document_ids.len());
        for document_id in document_ids {
            let Some(path) = pick_path(self, document_id) else {
                return;
            };
            if !self.save_as_path_supported(document_id, &path) {
                self.status = Some(Message::new("status-save-format-mismatch"));
                return;
            }
            targets.push((
                document_id,
                path,
                !self.backing_matches_document(document_id),
            ));
        }

        if !self.commit_app_close_window_edits() {
            self.status = Some(Message::new("dialog-app-close-wait-operations"));
            return;
        }
        self.app_close_save_batch_active = true;

        for (document_id, path, track_recent) in targets {
            self.save_as_to_path_with_intent(
                document_id,
                path,
                track_recent,
                false,
                None,
                Some(save_generation),
            );
            let waiting = self.pending_save_intents.contains_key(&document_id)
                || self.app_close_save_tokens.contains_key(&document_id)
                || self.pending_saves.contains_key(&document_id);
            if !waiting {
                self.app_close_save_failed = true;
                break;
            }
        }
        self.finish_app_close_save_batch();
    }

    /// Records a save result only for the version requested by app exit.
    pub(super) fn finish_app_close_save(
        &mut self,
        document_id: DocumentId,
        token: JobToken,
        succeeded: bool,
    ) {
        if self.app_close_save_tokens.get(&document_id) != Some(&token) {
            return;
        }
        self.app_close_save_tokens.remove(&document_id);
        let current_is_saved = self
            .workspace
            .documents
            .get(&document_id)
            .is_some_and(|_| !self.document_has_unsaved_changes(document_id));
        self.app_close_save_failed |= !succeeded || !current_is_saved;
        self.finish_app_close_save_batch();
    }

    /// Closes only after every required save succeeded at the current document version.
    pub(in crate::ui) fn finish_app_close_save_batch(&mut self) {
        if !self.app_close_save_batch_active
            || !self.app_close_save_tokens.is_empty()
            || !self.pending_saves.is_empty()
            || !self.pending_save_intents.is_empty()
            || !self.pending_app_close_save_queue.is_empty()
            || self.app_close_save_collecting_targets
            || self.export_dialog.is_some()
        {
            return;
        }
        if self.app_close_has_pending_operations() {
            return;
        }
        if self.app_close_save_failed || self.app_close_needs_save() {
            self.app_close_save_failed = true;
            self.app_close_save_batch_active = false;
            self.status = Some(Message::new("status-app-close-save-failed"));
            return;
        }
        self.app_close_save_batch_active = false;
        self.app_close_dialog_open = false;
        self.app_close_after_documents = true;
    }

    /// Detects source drafts that require a separate apply or discard choice.
    fn app_close_has_unresolved_drafts(&self) -> bool {
        self.workspace.documents.values().any(|_document| false)
    }

    /// Detects worker operations that must finish before documents can be saved or discarded.
    pub(super) fn app_close_has_pending_operations(&self) -> bool {
        !self.pending_csv_opens.is_empty()
            || !self.pending_markdown_loads.is_empty()
            || !self.pending_html_loads.is_empty()
            || !self.pending_workbook_loads.is_empty()
            || self.pending_close_document.is_some()
            || !self.pending_structures.is_empty()
            || !self.pending_history_restores.is_empty()
            || !self.pending_exports.is_empty()
            || !self.pending_export_writes.is_empty()
            || !self.pending_searches.is_empty()
            || !self.pending_data_edits.is_empty()
            || !self.pending_bulk_pastes.is_empty()
            || !self.pending_table_management.is_empty()
            || !self.pending_save_intents.is_empty()
            || self.recovery_maintenance_pending()
    }

    /// Keeps active IME composition out of Save All and Discard All.
    pub(super) fn app_close_has_active_composition(&self) -> bool {
        self.edit.composing
            || self.modal_ime_composition.is_some()
            || self
                .window_states
                .values()
                .any(|window| window.edit.composing || window.modal_ime_composition.is_some())
    }

    /// Draws one app-close action and records its actual rectangle for scenarios.
    fn draw_app_close_button(
        &mut self,
        ui: &mut egui::Ui,
        _test_rect_key: &'static str,
        label: String,
        enabled: bool,
        primary: bool,
    ) -> bool {
        let response = ui
            .add_enabled_ui(enabled, |ui| {
                let button = egui::Button::new(label);
                let button = if primary {
                    button.fill(ui.visuals().selection.bg_fill)
                } else {
                    button
                };
                ui.add_sized([132.0, visual::BUTTON_HEIGHT], button)
            })
            .inner;
        #[cfg(test)]
        self.test_app_close_button_rects
            .insert(_test_rect_key, response.rect);
        response.clicked()
    }
}
