use super::*;

impl TexcelApp {
    // Builds the screen state with the OS theme and an empty Markdown document.
    pub fn new(creation: &eframe::CreationContext<'_>) -> Self {
        #[cfg(target_os = "macos")]
        crate::macos_app::register(creation.egui_ctx.clone());
        let mut app = Self::new_with_settings_path(creation, app_settings_path());
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        match crate::native_menu::NativeMenu::new(creation, app.language) {
            Ok(menu) => app.native_menu = Some(menu),
            Err(detail) => {
                app.status = Some(
                    Message::new("status-native-menu-start-failed").with_text("detail", detail),
                );
            }
        }
        #[cfg(target_os = "macos")]
        app.queue_open_paths(crate::macos_app::take_open_paths());
        app
    }

    // Create the app, load settings, and prepare its worker state.
    pub(super) fn new_with_settings_path(
        creation: &eframe::CreationContext<'_>,
        settings_path: Option<std::path::PathBuf>,
    ) -> Self {
        let recovery_root = settings_path
            .as_ref()
            .and_then(|path| path.parent().map(|parent| parent.join("recovery")));
        let recovery_worker = recovery_root.map(crate::recovery::RecoveryWorker::new);
        let recovery_session_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        creation.egui_ctx.options_mut(|options| {
            options.fallback_theme = egui::Theme::Light;
            options.zoom_with_keyboard = false;
        });
        creation.egui_ctx.set_theme(egui::ThemePreference::System);
        ime::pass_focus::track_pass_focus(&creation.egui_ctx);
        let language = crate::i18n::resolve_language(LanguageChoice::System);
        let installed_font_preset = resolved_font_preset(FontPreset::Auto, language);
        install_texcel_grid_fonts(&creation.egui_ctx, installed_font_preset);
        let (settings_worker, settings_error) = match settings_path {
            Some(path) => match SettingsWorker::new(path) {
                Ok(worker) => (Some(worker), None),
                Err(error) => (
                    None,
                    Some(
                        Message::new("status-settings-start-failed")
                            .with_message("detail", crate::file_worker::file_error_message(error)),
                    ),
                ),
            },
            None => (
                None,
                Some(Message::new("status-settings-save-location-unavailable")),
            ),
        };
        let mut app = Self {
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            native_menu: None,
            #[cfg(all(test, any(target_os = "macos", target_os = "windows")))]
            native_menu_events_for_test: Vec::new(),
            workspace: Workspace::default(),
            window_states: BTreeMap::new(),
            current_viewport: None,
            multi_window_mode: false,
            host_result_dispatch: false,
            app_exit_start_pending: false,
            app_exit_transaction: false,
            app_exit_ready: false,
            edit: EditSession::default(),
            edit_document_id: None,
            ime_composition: None,
            ime_interrupt_pending: false,
            ime_interrupt_this_frame: false,
            ime_escape_cancelled_this_frame: false,
            modal_ime_composition: None,
            late_ime_editor: None,
            raw_input_viewport: None,
            startup_placeholder: None,
            formula_references: FormulaReferenceState::default(),
            pending_edit_input: None,
            long_text_panel_open: false,
            long_text_panel_dismissed_session: None,
            theme: ThemeChoice::System,
            language_choice: LanguageChoice::System,
            language,
            font_preset: FontPreset::Auto,
            installed_font_preset,
            ui_font_points: crate::settings::default_ui_font_points(),
            sheet_zoom: SheetZoom::default(),
            grid_character_width: crate::document::REFERENCE_CHARACTER_WIDTH,
            completion_notices: BTreeMap::new(),
            show_invisibles: false,
            minimap_width: crate::settings::MinimapWidth::default(),
            settings_window_open: false,
            about_dialog_open: false,
            help_viewport_open: false,
            help_blocks: None,
            help_outline: help_window::HelpOutline::default(),
            minimap_open: false,
            minimap_shown_table: None,
            minimap_caches: BTreeMap::new(),
            settings_worker,
            settings_file: SettingsFile::default(),
            settings_loaded: false,
            settings_write_requested: false,
            settings_save_generation: 0,
            settings_pending_save: None,
            settings_in_flight_generation: None,
            settings_saved_generation: 0,
            settings_failed_generation: None,
            settings_failure_notice_visible: false,
            settings_close_attempt_generation: None,
            settings_exit_authorized_generation: None,
            close_after_settings_save: false,
            settings_applied_documents: BTreeSet::new(),
            document_preference_changes: BTreeMap::new(),
            app_preference_changes: AppPreferenceChanges::default(),
            pending_save_preferences: BTreeMap::new(),
            coordinate_preference_generations: BTreeMap::new(),
            frozen_coordinate_preference_generations: BTreeMap::new(),
            pending_saved_document_preferences: Vec::new(),
            accesskit_table_tokens: BTreeMap::new(),
            accesskit_tables_by_token: BTreeMap::new(),
            pending_accesskit_focus: None,
            request_scroll_to_address: None,
            accesskit_table_namespace: accesskit_session_namespace(),
            next_accesskit_table_slot: 1,
            font_layout_generation: 0,
            last_pixels_per_point: 1.0,
            last_viewport_title: None,
            grid_focused: false,
            text_dialog_open_previous_frame: false,
            dialog_return_editor: None,
            grid_keyboard_owner: None,
            input_sequence_origin: None,
            last_text_input_focus: None,
            last_modal_text_input_focus: None,
            active_modal_text_input_focus: None,
            modal_menu_text_focus: None,
            #[cfg(any(test, not(any(target_os = "macos", target_os = "windows"))))]
            shared_edit_menu_target: None,
            restore_modal_menu_focus_this_frame: false,
            preserve_text_input_focus: false,
            suppress_grid_keyboard: false,
            axis_space_text_pending: None,
            request_editor_focus: false,
            request_grid_focus: false,
            request_scroll_to_selection: false,
            status: settings_error,
            status_lifetime: None,
            import_notice: None,
            deferred_commands: Vec::new(),
            deferred_text_commands: Vec::new(),
            pending_close_document: None,
            close_after_save: BTreeSet::new(),
            close_after_save_edit: BTreeMap::new(),
            save_conflict_dialog: None,
            app_close_dialog_open: false,
            app_close_save_tokens: BTreeMap::new(),
            app_close_save_generation: 0,
            app_close_save_failed: false,
            app_close_save_batch_active: false,
            app_close_save_collecting_targets: false,
            app_close_after_documents: false,
            app_close_discard_pending: false,
            app_close_waiting_recovery_removal: false,
            app_close_recovery_removal_failed: false,
            app_close_waiting_recovery_maintenance: false,
            app_close_recovery_maintenance_failed: false,
            app_close_after_recovery_maintenance: None,
            app_close_discard_state: None,
            app_close_discard_exit_ready: false,
            #[cfg(test)]
            test_save_path_selection: None,
            #[cfg(test)]
            test_close_button_rects: BTreeMap::new(),
            #[cfg(test)]
            test_app_close_button_rects: BTreeMap::new(),
            recovery_worker,
            recovery_session_id,
            recovery_keys: BTreeMap::new(),
            recovery_schedules: BTreeMap::new(),
            pending_recovered_cell_edits: BTreeMap::new(),
            recovery_items: Vec::new(),
            recovery_restore_pending: BTreeMap::new(),
            file_worker: FileWorker::new_with_load_progress(),
            queued_open_paths: VecDeque::new(),
            active_queued_open: None,
            load_request_viewports: BTreeMap::new(),
            queued_open_failure: None,
            recent_files: VecDeque::new(),
            recent_files_changed_before_load: false,
            recent_save_paths: BTreeMap::new(),
            calculation_worker: CalculationWorker::new(),
            file_backings: BTreeMap::new(),
            user_file_paths: BTreeMap::new(),
            import_origins: BTreeMap::new(),
            recovered_documents_needing_save: BTreeSet::new(),
            pending_file_load_paths: BTreeMap::new(),
            pending_document_focus: None,
            preferred_settings_identity_by_path: BTreeMap::new(),
            pending_csv_opens: BTreeMap::new(),
            pending_markdown_loads: BTreeMap::new(),
            pending_html_loads: BTreeMap::new(),
            pending_workbook_loads: BTreeMap::new(),
            pending_load_previews: BTreeMap::new(),
            pending_saves: BTreeMap::new(),
            pending_save_history_states: BTreeMap::new(),
            pending_save_recovery_generations: BTreeMap::new(),
            cancelling_saves: BTreeMap::new(),
            pending_recovery_flushes: BTreeMap::new(),
            pending_recovery_cleanups: BTreeMap::new(),
            pending_recovery_removals: BTreeSet::new(),
            in_flight_recovery_cleanups: BTreeMap::new(),
            in_flight_recovery_maintenance: BTreeSet::new(),
            in_flight_recovery_removals: BTreeSet::new(),
            failed_recovery_maintenance: BTreeSet::new(),
            pending_report_saves: BTreeMap::new(),
            import_reports: BTreeMap::new(),
            active_import_report: None,
            queued_import_reports: VecDeque::new(),
            selected_import_group: 0,
            export_dialog: None,
            export_configs: BTreeMap::new(),
            save_as_export_documents: BTreeSet::new(),
            pending_exports: BTreeMap::new(),
            pending_export_writes: BTreeMap::new(),
            cancelling_exports: BTreeMap::new(),
            pending_export_report_saves: BTreeMap::new(),
            active_export_confirmation: None,
            pending_app_close_save_queue: VecDeque::new(),
            pending_app_close_save_targets: BTreeMap::new(),
            cell_link_dialog: None,
            layout_dialog: None,
            sheet_zoom_dialog: None,
            search_dialog: None,
            paste_special_dialog: None,
            clipboard_mark: None,
            copy_options_dialog: None,
            data_edit_dialog: None,
            pending_searches: BTreeMap::new(),
            pending_data_edits: BTreeMap::new(),
            pending_structures: BTreeMap::new(),
            pending_text_growth_sessions: BTreeMap::new(),
            pending_growth_selection: None,
            pending_bulk_pastes: BTreeSet::new(),
            pending_table_management: BTreeSet::new(),
            pending_save_intents: BTreeMap::new(),
            pending_csv_promotion_cell_edits: BTreeMap::new(),
            pending_csv_promotion_cell_batches: BTreeMap::new(),
            pending_markdown_link_edits: BTreeMap::new(),
            fill_handle_drag: None,
            grid_selection_drag: None,
            header_resize_drag: None,
            grid_page_extent: Vec2::ZERO,
            pending_history_restores: BTreeMap::new(),
            retired_structure_roots: Vec::new(),
            text_selection: None,
            name_box: "A1".to_owned(),
            grid_row_layouts: BTreeMap::new(),
            markdown_cell_displays: BTreeMap::new(),
            pending_markdown_displays: BTreeMap::new(),
            pending_auto_fits: BTreeMap::new(),
            deferred_auto_fits: BTreeMap::new(),
            opening_fits: BTreeMap::new(),
            opening_since: BTreeMap::new(),
            pending_calculations: BTreeMap::new(),
            pending_initial_calculations: BTreeMap::new(),
            calculation_initial_snapshots: BTreeMap::new(),
            submitted_calculation_changes: BTreeMap::new(),
            calculation_snapshots: BTreeMap::new(),
            calculation_retries: BTreeSet::new(),
            calculation_retry_available: BTreeSet::new(),
            deferred_file_results: Vec::new(),
            last_load_progress_frame: None,
            next_job_id: 0,
        };
        let startup_status = app.status.take();
        if let Err(message) = app.create_new_document(text(app.language, "document-default-name")) {
            app.status = Some(message);
        } else {
            app.status = startup_status;
            app.startup_placeholder = app
                .workspace
                .active_document()
                .map(|document| (document.id, document.revision));
        }
        // File results and load previews are taken in the root window's pass, so the worker
        // wakes the root whichever window started the job.
        let wake_context = creation.egui_ctx.clone();
        app.file_worker.set_waker(move || {
            wake_context.request_repaint_of(egui::ViewportId::ROOT);
        });
        app
    }

    // Create a token that identifies work for one document revision.
    pub(super) fn next_job_token(&mut self, document_id: DocumentId, revision: u64) -> JobToken {
        self.next_job_id = self.next_job_id.saturating_add(1).max(1);
        JobToken {
            document_id,
            revision,
            job_id: self.next_job_id,
        }
    }

    // Assign a stable accessibility token to a table in this document.
    pub(super) fn ensure_accesskit_table_token(
        &mut self,
        document_id: DocumentId,
        table_id: u64,
    ) -> Option<u32> {
        if let Some(token) = self.accesskit_table_tokens.get(&(document_id, table_id)) {
            return Some(*token);
        }
        let slot = self.next_accesskit_table_slot;
        if slot == 0 || slot > ACCESSKIT_TABLE_TOKEN_MASK {
            return None;
        }
        self.next_accesskit_table_slot = slot.saturating_add(1);
        let token = self.accesskit_table_namespace.wrapping_add(slot) & ACCESSKIT_TABLE_TOKEN_MASK;
        self.accesskit_table_tokens
            .insert((document_id, table_id), token);
        self.accesskit_tables_by_token
            .insert(token, (document_id, table_id));
        Some(token)
    }

    // Remove accessibility IDs owned by a closed document.
    pub(super) fn forget_accesskit_document(&mut self, document_id: DocumentId) {
        let removed = self
            .accesskit_table_tokens
            .iter()
            .filter_map(|((stored_document, _), token)| {
                (*stored_document == document_id).then_some(*token)
            })
            .collect::<Vec<_>>();
        self.accesskit_table_tokens
            .retain(|(stored_document, _), _| *stored_document != document_id);
        for token in removed {
            self.accesskit_tables_by_token.remove(&token);
        }
    }

    // Apply accessibility actions requested for visible or virtual cells.
    pub(super) fn process_accesskit_action_requests(
        &mut self,
        ctx: &egui::Context,
        modal_blocked: bool,
        navigation_blocked: bool,
    ) {
        if modal_blocked {
            self.pending_accesskit_focus = None;
            self.request_scroll_to_address = None;
            return;
        }
        let requests = ctx.input(|input| {
            input
                .events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::AccessKitActionRequest(request) => Some(request.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        });
        for request in requests {
            if request.target_tree != egui::accesskit::TreeId::ROOT
                || !matches!(
                    request.action,
                    egui::accesskit::Action::Focus
                        | egui::accesskit::Action::Click
                        | egui::accesskit::Action::ScrollIntoView
                )
            {
                continue;
            }
            // Keep the active editor in view while it owns IME composition.
            if navigation_blocked && request.action == egui::accesskit::Action::ScrollIntoView {
                continue;
            }
            let Some((table_token, address, _segment)) =
                decode_grid_cell_node_id(request.target_node)
            else {
                continue;
            };
            let Some(&(document_id, table_id)) = self.accesskit_tables_by_token.get(&table_token)
            else {
                continue;
            };
            let Some(document) = self.workspace.documents.get(&document_id) else {
                continue;
            };
            if !document.tables.iter().any(|table| table.id == table_id) {
                continue;
            }
            if address.row >= MAX_ROWS || address.column >= MAX_COLUMNS {
                continue;
            }
            if self.workspace.active_document != Some(document_id)
                || self
                    .workspace
                    .active_document()
                    .and_then(|document| document.active_table())
                    .is_none_or(|table| table.id != table_id)
            {
                continue;
            }
            match request.action {
                egui::accesskit::Action::ScrollIntoView => {
                    self.request_scroll_to_address = Some((document_id, table_id, address));
                }
                egui::accesskit::Action::Focus | egui::accesskit::Action::Click => {
                    self.pending_accesskit_focus = Some((document_id, table_id, address));
                    if navigation_blocked {
                        continue;
                    }
                }
                _ => (),
            }
        }
    }
}
