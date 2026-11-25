use super::*;

impl TexcelApp {
    /// Applies actions collected from the header after its controls finish drawing.
    pub(in crate::ui) fn apply_top_actions(
        &mut self,
        mut state: TopActionState,
        navigation_blocked: bool,
        ctx: &egui::Context,
    ) {
        self.preserve_text_input_focus = state.preserve_text_focus;
        self.deferred_commands
            .extend(
                std::mem::take(&mut state.commands)
                    .into_iter()
                    .map(|command| commands::DeferredAppCommand {
                        command,
                        owner: state.command_owner.unwrap_or(InputOwner::Chrome),
                        blocked: navigation_blocked,
                        focus: state.command_focus,
                        #[cfg(any(target_os = "macos", target_os = "windows"))]
                        native_target: state.native_command_target.clone(),
                    }),
            );
        if !navigation_blocked {
            if let Some(zoom_factor) = state.zoom_action {
                self.set_zoom_factor(ctx, zoom_factor);
            }
            if state.minimize_window {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
            if state.toggle_window_zoom {
                let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }
            if let Some((document_id, table_id, previous_mode)) = state.display_state
                && let Some(document) = self.workspace.documents.get_mut(&document_id)
                && let Some(mode) = state.table_display_mode
                && mode != previous_mode
            {
                document.display_mode = mode;
                state.local_preference_changes.push((
                    document_id,
                    table_id,
                    TablePreferenceField::DisplayMode,
                ));
                ctx.request_repaint();
            }
            if let Some((document_id, table_id, previous_mode)) = state.wrap_state
                && let Some(mode) = state.table_wrap_mode
                && mode != previous_mode
                && let Some(view) = self
                    .workspace
                    .documents
                    .get_mut(&document_id)
                    .and_then(|document| document.view_mut(table_id))
            {
                view.wrap_text = mode == WrapMode::On;
                state.local_preference_changes.push((
                    document_id,
                    table_id,
                    TablePreferenceField::WrapMode,
                ));
                self.font_layout_generation = self.font_layout_generation.saturating_add(1);
                ctx.request_repaint();
            }
            if let Some(document_id) = state.close_document {
                self.request_close_document(document_id);
            }
            if state.replace_formula_results && !state.document_edit_locked {
                if self.edit.is_editing() {
                    self.commit_edit(None);
                }
                let target = self.workspace.active_document().and_then(|document| {
                    let table = document.active_table()?;
                    let view = document.view(table.id)?;
                    Some((
                        document.id,
                        table.id,
                        view.selected_ranges().collect::<Vec<_>>(),
                    ))
                });
                if let Some((document_id, table_id, ranges)) = target {
                    let mut pending_text_changes = None;
                    if let Some(document) = self.workspace.documents.get_mut(&document_id) {
                        match document.replace_formula_results_in_ranges(table_id, &ranges) {
                            Ok(_) => {
                                if let Some(limit) = document.take_resource_limit_notice() {
                                    self.status = Some(
                                        Message::new("status-resource-limit-reached").with_message(
                                            "limit",
                                            crate::i18n::resource_limit_message(limit),
                                        ),
                                    );
                                }
                                self.schedule_calculation(document_id);
                                self.request_grid_focus = true;
                            }
                            Err(error) => {
                                pending_text_changes = document.take_pending_text_source_changes();
                                if pending_text_changes.is_none() {
                                    self.status = Some(error);
                                }
                            }
                        }
                    }
                    if let Some(pending) = pending_text_changes {
                        self.submit_text_source_changes(pending, None);
                    }
                }
            }
            if state.open_search_dialog && !state.document_edit_locked {
                if self.edit.is_editing() {
                    self.commit_edit(None);
                }
                self.start_search_dialog();
            }
            if state.open_replace_dialog && !state.document_edit_locked {
                if self.edit.is_editing() {
                    self.commit_edit(None);
                }
                self.start_search_dialog();
                if let Some(dialog) = self.search_dialog.as_mut() {
                    dialog.replace_mode = true;
                }
            }
            if let Some(kind) = state.open_data_edit_dialog {
                if self.edit.is_editing() {
                    self.commit_edit(None);
                }
                self.start_data_edit_dialog(kind);
            }
            if let Some(action) = state.format_action
                && state.can_markdown_modifiers
            {
                self.apply_markdown_modifier(action, ctx);
            }
            if let Some(action) = state.layout_action {
                self.apply_layout_action(action, ctx);
            }
            if state.edit_link {
                self.start_cell_link_dialog();
            }
            if state.open_link {
                self.open_selected_link(ctx);
            }
            if state.open_file {
                self.choose_file_to_open();
            }
            if !state.open_recent_paths.is_empty() {
                self.queue_open_paths(state.open_recent_paths);
            }
            if state.save_as {
                self.save_active(true);
            } else if state.save_file {
                self.save_active(false);
            }
        }
        if let Some(document_id) = state.focus_document {
            if self.multi_window_mode {
                self.pending_document_focus = Some(document_id);
            } else {
                self.workspace.select_document(document_id);
            }
        }
        if let Some(action) = state.structure_action {
            self.request_structure_action(action);
        }
        if state.open_settings {
            self.settings_window_open = true;
        }
        if state.open_help {
            self.help_window_open = true;
        }
        if state.quit_app {
            if self.multi_window_mode {
                self.app_exit_start_pending = true;
                ctx.request_repaint();
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if state.create_document && !navigation_blocked {
            let name = text(self.language, "document-default-name");
            if let Err(message) = self.create_new_document(name) {
                self.status = Some(message);
            }
        }
        for (document_id, table_id, field) in state.local_preference_changes {
            self.mark_document_preference_change(document_id, table_id, field);
        }
        self.apply_appearance(ctx);
    }
}
