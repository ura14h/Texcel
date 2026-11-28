use super::*;

impl eframe::App for TexcelApp {
    // Route close requests even when eframe skips painting a hidden window.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_load_progress(ctx);
        self.handle_app_close_request(ctx);
    }

    // Update input handling for an active cell text composition.
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.prepare_raw_input(ctx, raw);
    }

    // Draw the root viewport with the same screen path used by native child windows.
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw_contents(root);
    }
}

impl TexcelApp {
    /// Queues input before egui routes it to widgets in the matching viewport.
    pub(super) fn prepare_raw_input(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if raw
            .events
            .iter()
            .any(|event| matches!(event, egui::Event::WindowFocused(false)))
        {
            self.grid_selection_drag = None;
            self.formula_references.drag_anchor = None;
            self.fill_handle_drag = None;
            self.axis_space_text_pending = None;
        }
        let dropped_paths = raw
            .dropped_files
            .iter()
            .map(|file| file.path().to_path_buf())
            .collect::<Vec<_>>();
        self.queue_open_paths(dropped_paths);
        // egui's memory may still answer for another window here; focus is read from the
        // record of this window's last pass until its own pass starts.
        self.raw_input_viewport = Some(raw.viewport_id);
        self.stage_ordered_edit_input(ctx, raw);
        self.prepare_ime_input(ctx, raw);
        self.raw_input_viewport = None;
    }
    /// Runs the shared screen path for one viewport's local window state.
    pub(super) fn draw_contents(&mut self, root: &mut egui::Ui) {
        let ctx = root.ctx().clone();
        self.clear_status_for_user_action(&ctx);
        let can_retain_grid_focus =
            self.grid_focused && !self.edit.is_editing() && !self.modal_input_blocked();
        grid_focus::retain_grid_keyboard_focus(
            &ctx,
            self.grid_keyboard_owner,
            can_retain_grid_focus,
        );
        let editor_before_dialog = self
            .edit_document_id
            .zip(self.edit.table_id)
            .filter(|(document_id, _)| {
                self.workspace.active_document == Some(*document_id)
                    && matches!(
                        self.edit.target,
                        Some(
                            crate::document::EditTarget::Cell
                                | crate::document::EditTarget::FormulaBar
                        )
                    )
                    && self.edit.is_editing()
                    && ctx.text_edit_focused()
                    && ctx.memory(|memory| {
                        memory.focused() == Some(editor_id(*document_id, self.edit.session_id))
                    })
            })
            .and_then(|(document_id, table_id)| {
                self.edit
                    .target
                    .map(|target| (document_id, table_id, self.edit.session_id, target))
            });
        self.suppress_grid_keyboard = false;
        self.active_modal_text_input_focus = None;
        self.restore_modal_menu_focus_this_frame = false;
        visual::configure_text_roles(&ctx);
        let pixels_per_point = ctx.pixels_per_point();
        if (pixels_per_point - self.last_pixels_per_point).abs() > f32::EPSILON {
            self.last_pixels_per_point = pixels_per_point;
            self.font_layout_generation = self.font_layout_generation.saturating_add(1);
        }
        let window_focus_lost = ctx.input(|input| {
            input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::WindowFocused(false)))
        });
        if window_focus_lost {
            self.grid_selection_drag = None;
            self.formula_references.drag_anchor = None;
            self.fill_handle_drag = None;
            self.axis_space_text_pending = None;
        }
        #[cfg(target_os = "macos")]
        self.queue_open_paths(crate::macos_app::take_open_paths());
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        let native_events = self
            .native_menu
            .as_ref()
            .map(|menu| {
                if self.multi_window_mode {
                    menu.take_events_for_document(
                        self.workspace.active_document().map(|document| document.id),
                    )
                } else {
                    menu.take_events()
                }
            })
            .unwrap_or_default();
        #[cfg(all(test, any(target_os = "macos", target_os = "windows")))]
        let native_events = native_events
            .into_iter()
            .chain(std::mem::take(&mut self.native_menu_events_for_test))
            .collect::<Vec<_>>();
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        let native_menu_interaction = !native_events.is_empty();
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let native_menu_interaction = false;
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        self.suppress_native_menu_accelerators(&ctx, &native_events);
        self.apply_deferred_text_commands(&ctx);
        self.process_settings(&ctx);
        crate::i18n::set_context_language(&ctx, self.language);
        if !self.host_result_dispatch {
            self.process_file_results_with_native_interaction(&ctx, native_menu_interaction);
        }
        self.process_recovery(&ctx);
        self.advance_app_close(&ctx);
        self.process_calculation_results(&ctx);
        // The window host moves openings forward itself; a single window does it here.
        if !self.multi_window_mode {
            self.advance_opening_fits(&ctx);
            self.refresh_opening_progress();
            if self.has_opening_documents() {
                ctx.request_repaint_after(Duration::from_millis(50));
            }
        }
        self.draw_close_document_dialog(&ctx);
        self.draw_save_conflict_dialog(&ctx);
        self.normalize_active_selection();
        let top_blocked_without_ime = self.modal_input_blocked();
        let ime_frame = self.sync_ime_events(&ctx);
        // Carry a raw-input cancel past begin_pass and ask the platform to stop composition.
        // A later preedit in the same frame owns the next platform composition.
        let has_new_composition = self.edit.composing || self.modal_ime_composition.is_some();
        self.ime_interrupt_this_frame =
            std::mem::take(&mut self.ime_interrupt_pending) && !has_new_composition;
        if self.ime_interrupt_this_frame {
            ctx.memory_mut(|memory| memory.interrupt_ime());
        }
        let navigation_blocked = ime_frame.navigation_blocked;
        let top_blocked = navigation_blocked || top_blocked_without_ime;
        let active_document_edit_locked = self
            .workspace
            .active_document()
            .is_some_and(|document| self.document_bulk_edit_pending(document.id));
        // Let only the active formula editor finish IME text input through navigation blocks.
        let formula_ime_active = self.edit.target == Some(crate::document::EditTarget::FormulaBar)
            && (self.edit.composing || ime_frame.text_event)
            && !top_blocked_without_ime
            && self.workspace.active_document().is_some_and(|document| {
                !self.cell_edit_locked(document.id) && !self.close_after_save.contains(&document.id)
            });
        let (app_shortcut, mut top_actions) = self.draw_top(
            root,
            &ctx,
            top_blocked,
            formula_ime_active,
            active_document_edit_locked,
        );
        // Keep assistive focus requests pending while IME blocks grid navigation.
        self.process_accesskit_action_requests(&ctx, top_blocked_without_ime, navigation_blocked);
        let modal_blocked = top_blocked;
        self.draw_file_prompts(&ctx);
        self.restore_csv_menu_text_focus(&ctx);
        self.draw_search_dialog(&ctx);
        self.draw_paste_special_dialog(&ctx);
        self.draw_copy_options_dialog(&ctx);
        self.draw_data_edit_dialog(&ctx);
        self.draw_bottom(root, modal_blocked);
        let active_close_pending = self
            .workspace
            .active_document()
            .is_some_and(|document| self.close_after_save.contains(&document.id));
        // Apply the committed text before allowing only its following grid click.
        let grid_interaction_blocked = top_blocked_without_ime
            || navigation_blocked && !ime_frame.commit_before_pointer
            || active_close_pending
            || window_focus_lost;
        self.draw_grid(root, grid_interaction_blocked);
        self.draw_long_text_panel(&ctx);
        self.process_keyboard(&ctx, grid_interaction_blocked || navigation_blocked);
        self.finish_ordered_edit_input_frame();
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        let late_native_text_events =
            self.apply_native_menu_events(native_events, &mut top_actions, top_blocked, &ctx);
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        let native_menu_state = top_actions.clone();
        self.apply_top_actions(top_actions, top_blocked, &ctx);
        self.apply_app_shortcut(app_shortcut, top_blocked, &ctx);
        let commands = std::mem::take(&mut self.deferred_commands);
        for deferred in commands {
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            if let Some(target) = &deferred.native_target
                && !self.native_command_target_is_current(target, deferred.blocked, &ctx)
            {
                continue;
            }
            self.dispatch_command(
                deferred.command,
                deferred.owner,
                deferred.blocked,
                deferred.focus,
                &ctx,
            );
        }
        self.finish_ordered_input_owner_change();
        self.request_initial_dialog_focus(&ctx);
        self.draw_settings_dialog(&ctx);
        self.draw_about_dialog(&ctx);
        if !self.multi_window_mode {
            self.draw_help_viewport_immediate(&ctx);
        }
        self.draw_import_report(&ctx);
        self.draw_cell_link_dialog(&ctx);
        self.draw_layout_dialog(&ctx);
        self.draw_sheet_zoom_dialog(&ctx);
        if self.active_import_report.is_none() {
            self.draw_export_settings(&ctx);
            self.draw_export_confirmation(&ctx);
        }
        if self.active_import_report.is_none()
            && self.active_export_confirmation.is_none()
            && self.export_dialog.is_none()
        {
            self.draw_export_progress(&ctx);
        }
        self.draw_load_preview(&ctx);
        self.draw_app_close_dialog(&ctx);
        // Recheck Save All after worker-owned recovery cleanup or other work settles.
        self.finish_app_close_save_batch();
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            self.apply_late_native_text_events(late_native_text_events, top_blocked, &ctx);
            self.sync_native_menu_after_dialogs(native_menu_state, top_blocked, &ctx);
        }
        if !self.preserve_text_input_focus {
            self.last_text_input_focus = ctx
                .text_edit_focused()
                .then(|| ctx.memory(|memory| memory.focused()))
                .flatten();
        }
        let text_dialog_open = self.settings_window_open
            || self.about_dialog_open
            || self.pending_close_document.is_some()
            || self.save_conflict_dialog.is_some()
            || self.cell_link_dialog.is_some()
            || self.layout_dialog.is_some()
            || self.sheet_zoom_dialog.is_some()
            || self.search_dialog.is_some()
            || self.paste_special_dialog.is_some()
            || self.copy_options_dialog.is_some()
            || self.data_edit_dialog.is_some()
            || self.export_dialog.is_some()
            || self.active_export_confirmation.is_some()
            || self.active_import_report.is_some()
            || self.long_text_panel_open
                && self.edit.target == Some(crate::document::EditTarget::DetailPanel)
                && self.edit.is_editing()
            || self.app_close_dialog_open
            || self.pending_csv_opens.iter().any(|(document_id, pending)| {
                pending.prompt.is_some()
                    && self.load_request_belongs_to_current_viewport(*document_id)
            })
            || self
                .pending_load_previews
                .keys()
                .any(|document_id| self.load_request_belongs_to_current_viewport(*document_id));
        let focused_id = ctx.memory(|memory| memory.focused());
        let closed_dialog_still_owns_focus = self
            .last_modal_text_input_focus
            .is_some_and(|focus| focused_id == Some(focus));
        if !self.text_dialog_open_previous_frame && text_dialog_open {
            self.dialog_return_editor = editor_before_dialog;
        }
        if self.text_dialog_open_previous_frame
            && !text_dialog_open
            && (!ctx.text_edit_focused() || closed_dialog_still_owns_focus)
            && !self.edit.composing
        {
            self.grid_focused = false;
            let return_to_editor = self.dialog_return_editor.take().is_some_and(
                |(document_id, table_id, session_id, target)| {
                    self.workspace.active_document == Some(document_id)
                        && self.edit_document_id == Some(document_id)
                        && self.edit.table_id == Some(table_id)
                        && self.edit.session_id == session_id
                        && self.edit.target == Some(target)
                        && self.edit.is_editing()
                },
            );
            self.request_editor_focus = return_to_editor;
            self.request_grid_focus = !return_to_editor;
        }
        self.text_dialog_open_previous_frame = text_dialog_open;
        self.last_modal_text_input_focus = self.active_modal_text_input_focus;
        self.update_viewport_title(&ctx);
        self.advance_queued_open(&ctx);
        self.schedule_recovery();
    }
}

impl TexcelApp {
    /// Routes shortcuts only to the highest app-level modal that is open.
    pub(super) fn dialog_key_pressed_in_modal(
        &self,
        ctx: &egui::Context,
        modal_id: egui::Id,
        key: egui::Key,
        composing: bool,
    ) -> bool {
        if !self.dialog_owns_shortcuts(modal_id) {
            return false;
        }
        visual::dialog_key_pressed_in_modal(
            ctx,
            modal_id,
            key,
            composing || self.modal_ime_owner_is_focused(ctx),
        )
    }

    /// Keeps a command shortcut inside the detail editor's active modal.
    pub(super) fn dialog_command_key_pressed_in_modal(
        &self,
        ctx: &egui::Context,
        modal_id: egui::Id,
        key: egui::Key,
        composing: bool,
    ) -> bool {
        self.dialog_owns_shortcuts(modal_id)
            && visual::dialog_command_key_pressed_in_modal(
                ctx,
                modal_id,
                key,
                composing || self.modal_ime_owner_is_focused(ctx),
            )
    }

    /// Keeps modal shortcuts with the TextEdit that owns the unfinished preedit.
    fn modal_ime_owner_is_focused(&self, ctx: &egui::Context) -> bool {
        self.modal_ime_composition.is_some_and(|editor_id| {
            !ctx.text_edit_focused() || ctx.memory(|memory| memory.focused()) == Some(editor_id)
        })
    }

    /// Gives the last-drawn close modal priority over dialogs behind it.
    fn dialog_owns_shortcuts(&self, modal_id: egui::Id) -> bool {
        if self.app_close_dialog_open {
            return modal_id == egui::Id::unique("app_close_confirmation");
        }
        true
    }

    /// Shares one modal gate across grid, native input, and formula picking.
    pub(super) fn modal_input_blocked(&self) -> bool {
        self.settings_window_open
            || self.about_dialog_open
            || self.active_import_report.is_some()
            || self.active_export_confirmation.is_some()
            || self.export_dialog.is_some()
            || self.cell_link_dialog.is_some()
            || self.layout_dialog.is_some()
            || self.sheet_zoom_dialog.is_some()
            || self.search_dialog.is_some()
            || self.paste_special_dialog.is_some()
            || self.copy_options_dialog.is_some()
            || self.data_edit_dialog.is_some()
            || (self.long_text_panel_open
                && self.edit.target == Some(crate::document::EditTarget::DetailPanel)
                && self.edit.is_editing())
            || self.pending_close_document.is_some()
            || self.app_close_dialog_open
            || self.app_exit_transaction
            || self.app_close_discard_pending
            || !self.recovery_restore_pending.is_empty()
            || self.save_conflict_dialog.is_some()
            || self.pending_csv_opens.iter().any(|(document_id, pending)| {
                pending.prompt.is_some()
                    && self.load_request_belongs_to_current_viewport(*document_id)
            })
    }

    /// Restores a CSV field after its text widget has handled this frame's pointer input.
    fn restore_csv_menu_text_focus(&mut self, ctx: &egui::Context) {
        if !self.restore_modal_menu_focus_this_frame {
            return;
        }
        let Some(focus) = self.modal_menu_text_focus else {
            return;
        };
        let active_prompt = self.pending_csv_opens.iter().any(|(document_id, pending)| {
            pending.prompt.is_some()
                && self.load_request_belongs_to_current_viewport(*document_id)
                && focus == Id::unique(egui::IdSalt::new(("csv_custom_delimiter", document_id)))
        });
        if active_prompt {
            ctx.memory_mut(|memory| memory.request_focus(focus));
            self.active_modal_text_input_focus = Some(focus);
        }
    }

    /// Keeps a new modal's first text field focused after editor widgets have drawn.
    fn request_initial_dialog_focus(&self, ctx: &egui::Context) {
        if let Some(dialog) = self
            .search_dialog
            .as_ref()
            .filter(|dialog| dialog.focus_query)
        {
            let id = egui::Id::unique(egui::IdSalt::new(("search_query", dialog.document_id)));
            ctx.memory_mut(|memory| memory.request_focus(id));
        }
    }
}
