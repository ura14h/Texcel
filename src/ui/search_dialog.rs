use super::*;

impl TexcelApp {
    // Draw and handle the find and replace dialog.
    pub(super) fn draw_search_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.search_dialog.take() else {
            return;
        };
        let mut submit = false;
        let mut apply = false;
        let mut cancel = false;
        let mut modal_text_focus = None;
        let language = self.language;
        let pending = self.pending_searches.contains_key(&dialog.document_id);
        let initial_options = search_dialog_options(&dialog);
        let modal_id = Id::unique(("search_dialog", dialog.document_id));
        let enter_key =
            self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Enter, self.edit.composing);
        let cancel_key =
            self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Escape, self.edit.composing);
        egui::Modal::new(modal_id).show(ctx, |ui| {
            let width = (ctx.content_rect().width() - 40.0).clamp(360.0, 760.0);
            ui.set_min_width(width.min(520.0));
            ui.set_max_width(width);
            ui.set_max_height((ctx.content_rect().height() - 40.0).max(260.0));
            visual::dialog_surface(ui, |ui| {
                visual::prepare_dialog_body(ui);
                visual::dialog_title(ui, &text(language, "dialog-search-title"));
                visual::group_gap(ui);
                let body_height =
                    (ui.available_height() - visual::BUTTON_HEIGHT - 3.0 * visual::SPACE_8)
                        .max(120.0);
                egui::ScrollArea::vertical()
                    .max_height(body_height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            add_search_form_label(ui, text(language, "dialog-search"));
                            let query_id =
                                Id::unique(egui::IdSalt::new(("search_query", dialog.document_id)));
                            if dialog.focus_query {
                                ui.memory_mut(|memory| memory.request_focus(query_id));
                            }
                            let query = ui.add_sized(
                                [ui.available_width(), visual::BUTTON_HEIGHT],
                                egui::TextEdit::singleline(&mut dialog.query).id(query_id),
                            );
                            if query.has_focus() || dialog.focus_query {
                                modal_text_focus = Some(query.id);
                            }
                            if dialog.focus_query {
                                query.request_focus();
                                dialog.focus_query = false;
                            }
                        });
                        ui.checkbox(
                            &mut dialog.replace_mode,
                            text(language, "dialog-search-replace-preview"),
                        );
                        if dialog.replace_mode {
                            ui.horizontal(|ui| {
                                add_search_form_label(
                                    ui,
                                    text(language, "dialog-search-replacement"),
                                );
                                let replacement = ui.add_sized(
                                    [ui.available_width(), visual::BUTTON_HEIGHT],
                                    egui::TextEdit::singleline(&mut dialog.replacement),
                                );
                                if replacement.has_focus() {
                                    modal_text_focus = Some(replacement.id);
                                }
                            });
                        }
                        ui.horizontal(|ui| {
                            egui::ComboBox::from_id_salt("search_scope")
                                .selected_text(match dialog.scope {
                                    SearchScope::Selection => {
                                        text(language, "dialog-search-scope-selection")
                                    }
                                    SearchScope::Table => {
                                        text(language, "dialog-search-scope-table")
                                    }
                                    SearchScope::Document => {
                                        text(language, "dialog-search-scope-document")
                                    }
                                })
                                .show_ui(ui, |ui| {
                                    visual::prepare_context_menu(ui);
                                    ui.selectable_value(
                                        &mut dialog.scope,
                                        SearchScope::Selection,
                                        text(language, "dialog-search-scope-selection"),
                                    );
                                    ui.selectable_value(
                                        &mut dialog.scope,
                                        SearchScope::Table,
                                        text(language, "dialog-search-scope-table"),
                                    );
                                    ui.selectable_value(
                                        &mut dialog.scope,
                                        SearchScope::Document,
                                        text(language, "dialog-search-scope-document"),
                                    );
                                });
                            egui::ComboBox::from_id_salt("search_representation")
                                .selected_text(match dialog.representation {
                                    crate::structure::SearchRepresentation::Source => {
                                        text(language, "dialog-search-source-result")
                                    }
                                    crate::structure::SearchRepresentation::Display => {
                                        text(language, "dialog-search-display-result")
                                    }
                                })
                                .show_ui(ui, |ui| {
                                    visual::prepare_context_menu(ui);
                                    ui.selectable_value(
                                        &mut dialog.representation,
                                        crate::structure::SearchRepresentation::Source,
                                        text(language, "dialog-search-source-result"),
                                    );
                                    ui.selectable_value(
                                        &mut dialog.representation,
                                        crate::structure::SearchRepresentation::Display,
                                        text(language, "dialog-search-display-result"),
                                    );
                                });
                        });
                        ui.horizontal(|ui| {
                            ui.checkbox(
                                &mut dialog.whole_cell,
                                text(language, "dialog-exact-match"),
                            );
                            ui.checkbox(
                                &mut dialog.case_sensitive,
                                text(language, "dialog-case-sensitive"),
                            );
                            ui.checkbox(
                                &mut dialog.regular_expression,
                                text(language, "dialog-regex"),
                            );
                        });
                        if dialog.representation == crate::structure::SearchRepresentation::Display
                        {
                            ui.label(text(language, "dialog-search-formula-note"));
                        }
                        if dialog.preview.is_some() || dialog.message.is_some() {
                            ui.separator();
                        }
                        if let Some(preview) = dialog.preview.as_ref() {
                            ui.label(
                                crate::i18n::Message::new("dialog-search-match-count")
                                    .with_number("matches", preview.total_matches as i64)
                                    .with_number("cells", preview.matched_cells as i64)
                                    .resolve(language),
                            );
                            if preview.hits_truncated {
                                ui.label(text(language, "dialog-search-preview-limit-note"));
                            }
                            if preview.skipped_non_fresh > 0 {
                                ui.label(
                                    crate::i18n::Message::new("dialog-search-skipped-fresh")
                                        .with_number("count", preview.skipped_non_fresh as i64)
                                        .resolve(language),
                                );
                            }
                            egui::ScrollArea::vertical().max_height(220.0).show_rows(
                                ui,
                                20.0,
                                preview.hits.len(),
                                |ui, visible_rows| {
                                    for index in visible_rows {
                                        let hit = &preview.hits[index];
                                        let table_name = self
                                            .workspace
                                            .documents
                                            .get(&dialog.document_id)
                                            .and_then(|document| {
                                                document
                                                    .tables
                                                    .iter()
                                                    .find(|table| table.id == hit.table_id)
                                            })
                                            .map_or("?", |table| table.name.as_str());
                                        let state = search_hit_state_message(hit.result_state);
                                        if hit.searched_text.is_empty()
                                            && hit.replacement_text.is_none()
                                            && hit.result_state.is_some()
                                        {
                                            ui.add(
                                                egui::Label::new(
                                                    crate::i18n::Message::new(
                                                        "dialog-search-hit-missing",
                                                    )
                                                    .with_text("table", table_name)
                                                    .with_text("cell", cell_coordinate(hit.address))
                                                    .with_message("state", state)
                                                    .with_text(
                                                        "source",
                                                        text_preview(&hit.source_text, 100),
                                                    )
                                                    .resolve(language),
                                                )
                                                .truncate(),
                                            );
                                        } else {
                                            let source_context =
                                                if hit.searched_text != hit.source_text {
                                                    Some(
                                                        crate::i18n::Message::new(
                                                            "dialog-search-source-context",
                                                        )
                                                        .with_text(
                                                            "source",
                                                            text_preview(&hit.source_text, 80),
                                                        ),
                                                    )
                                                } else {
                                                    None
                                                };
                                            let replacement_context =
                                                hit.replacement_text.as_ref().map(|replacement| {
                                                    crate::i18n::Message::new(
                                                        "dialog-search-replacement-context",
                                                    )
                                                    .with_text(
                                                        "replacement",
                                                        text_preview(replacement, 100),
                                                    )
                                                });
                                            let mut message =
                                                crate::i18n::Message::new("dialog-search-hit")
                                                    .with_text("table", table_name)
                                                    .with_text("cell", cell_coordinate(hit.address))
                                                    .with_message("state", state)
                                                    .with_text(
                                                        "searched",
                                                        text_preview(&hit.searched_text, 100),
                                                    );
                                            message = if let Some(source_context) = source_context {
                                                message
                                                    .with_message("source_context", source_context)
                                            } else {
                                                message.with_text("source_context", "")
                                            };
                                            message = if let Some(replacement_context) =
                                                replacement_context
                                            {
                                                message.with_message(
                                                    "replacement_context",
                                                    replacement_context,
                                                )
                                            } else {
                                                message.with_text("replacement_context", "")
                                            };
                                            ui.add(
                                                egui::Label::new(message.resolve(language))
                                                    .truncate(),
                                            );
                                        }
                                    }
                                },
                            );
                            if dialog.replace_mode
                                && dialog.representation
                                    == crate::structure::SearchRepresentation::Display
                            {
                                ui.colored_label(
                                    ui.visuals().error_fg_color,
                                    text(language, "dialog-search-result-warning"),
                                );
                            }
                            if dialog.replace_mode {
                                ui.label(text(language, "dialog-search-notice-all"));
                            }
                            if dialog.replace_mode && preview.prepared.is_none() {
                                ui.label(text(language, "dialog-search-none"));
                            }
                        }
                        if let Some(message) = dialog.message.as_ref() {
                            ui.label(message.resolve(language));
                        }
                    });
                ui.separator();
                visual::dialog_footer(ui, |ui| {
                    let can_apply = !pending
                        && search_dialog_options(&dialog) == initial_options
                        && dialog
                            .preview
                            .as_ref()
                            .is_some_and(|preview| preview.prepared.is_some());
                    let preview_primary = !can_apply;
                    if cfg!(target_os = "macos") {
                        if search_dialog_button(
                            ui,
                            text(language, "action-apply-diff"),
                            can_apply,
                            can_apply,
                        ) {
                            apply = true;
                        }
                        if search_dialog_button(
                            ui,
                            text(language, "dialog-search-preview"),
                            !pending && !dialog.query.is_empty(),
                            preview_primary,
                        ) {
                            submit = true;
                        }
                        if ui.button(text(language, "dialog-search-close")).clicked() {
                            cancel = true;
                        }
                    } else {
                        if ui.button(text(language, "dialog-search-close")).clicked() {
                            cancel = true;
                        }
                        if search_dialog_button(
                            ui,
                            text(language, "dialog-search-preview"),
                            !pending && !dialog.query.is_empty(),
                            preview_primary,
                        ) {
                            submit = true;
                        }
                        if search_dialog_button(
                            ui,
                            text(language, "action-apply-diff"),
                            can_apply,
                            can_apply,
                        ) {
                            apply = true;
                        }
                    }
                });
            });
        });
        if modal_text_focus.is_some() {
            self.active_modal_text_input_focus = modal_text_focus;
        }
        let current_options = search_dialog_options(&dialog);
        if enter_key && !pending && !dialog.query.is_empty() {
            if current_options == initial_options
                && dialog
                    .preview
                    .as_ref()
                    .is_some_and(|preview| preview.prepared.is_some())
            {
                apply = true;
            } else {
                submit = true;
            }
        }
        if current_options != initial_options && !submit {
            apply = false;
            dialog.preview = None;
            dialog.preview_key = None;
            dialog.message = Some(Message::new("dialog-search-recheck"));
            if let Some(pending) = self.pending_searches.remove(&dialog.document_id) {
                self.file_worker.cancel(pending.token);
            }
        }
        if submit {
            dialog.message = Some(Message::new("dialog-search-preparing"));
            let state = SearchDialogState {
                document_id: dialog.document_id,
                table_id: dialog.table_id,
                query: dialog.query.clone(),
                replacement: dialog.replacement.clone(),
                scope: dialog.scope,
                representation: dialog.representation,
                case_sensitive: dialog.case_sensitive,
                regular_expression: dialog.regular_expression,
                whole_cell: dialog.whole_cell,
                replace_mode: dialog.replace_mode,
                message: dialog.message.clone(),
                preview: None,
                preview_key: None,
                focus_query: dialog.focus_query,
            };
            self.search_dialog = Some(dialog);
            self.submit_search(&state);
            return;
        }
        if apply {
            let document_id = dialog.document_id;
            let preview = dialog.preview.take();
            self.search_dialog = Some(dialog);
            if let Some(preview) = preview
                && let Some(prepared) = preview.prepared
            {
                self.apply_search_prepared(
                    document_id,
                    prepared,
                    preview.document_revision,
                    preview.calculation_revision,
                );
            }
            return;
        }
        if cancel || cancel_key {
            if let Some(pending) = self.pending_searches.remove(&dialog.document_id) {
                self.file_worker.cancel(pending.token);
            }
            return;
        }
        self.search_dialog = Some(dialog);
    }
}

// Places search field labels at the left edge and center of their fixed row.
fn add_search_form_label(ui: &mut egui::Ui, label: String) {
    ui.allocate_ui_with_layout(
        egui::vec2(132.0, visual::BUTTON_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_width(132.0);
            ui.set_max_width(132.0);
            ui.add(egui::Label::new(label).halign(egui::Align::Min));
        },
    );
}

/// Captures the options that decide whether the visible search preview is current.
fn search_dialog_options(
    dialog: &SearchDialogState,
) -> (
    String,
    String,
    SearchScope,
    crate::structure::SearchRepresentation,
    bool,
    bool,
    bool,
    bool,
    Option<SearchJobKey>,
) {
    (
        dialog.query.clone(),
        dialog.replacement.clone(),
        dialog.scope,
        dialog.representation,
        dialog.case_sensitive,
        dialog.regular_expression,
        dialog.whole_cell,
        dialog.replace_mode,
        dialog.preview_key.clone(),
    )
}

/// Highlights the action that is ready in the find and replace dialog.
fn search_dialog_button(ui: &mut egui::Ui, label: String, enabled: bool, primary: bool) -> bool {
    let button = search_dialog_styled_button(ui, label, primary);
    ui.add_enabled(enabled, button).clicked()
}

/// Gives the ready search action a visible primary style.
fn search_dialog_styled_button(
    ui: &egui::Ui,
    label: String,
    primary: bool,
) -> egui::Button<'static> {
    let button = egui::Button::new(label);
    if primary {
        button.fill(ui.visuals().selection.bg_fill)
    } else {
        button
    }
}

/// Builds a calculation-state message without storing translated text in the search result.
fn search_hit_state_message(
    state: Option<crate::structure::SearchResultState>,
) -> crate::i18n::Message {
    match state {
        Some(crate::structure::SearchResultState::Calculation(status)) => {
            crate::i18n::Message::new(match status {
                crate::document::CalcStatus::Fresh => "calc-state-fresh",
                crate::document::CalcStatus::Pending => "calc-state-pending",
                crate::document::CalcStatus::Imported => "calc-state-imported",
                crate::document::CalcStatus::Unsupported => "calc-state-unsupported",
                crate::document::CalcStatus::Stale => "calc-state-stale",
                crate::document::CalcStatus::Error => "calc-state-error",
            })
        }
        Some(crate::structure::SearchResultState::Display) => {
            crate::i18n::Message::new("dialog-search-display-result")
        }
        Some(crate::structure::SearchResultState::Missing) => {
            crate::i18n::Message::new("calc-result-missing")
        }
        None => crate::i18n::Message::new("dialog-search-source-result"),
    }
}
