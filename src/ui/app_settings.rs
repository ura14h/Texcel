use super::*;

impl TexcelApp {
    /// Queues the latest settings snapshot for saving.
    pub(super) fn mark_settings_write(&mut self) {
        self.settings_save_generation = self.settings_save_generation.saturating_add(1).max(1);
        self.settings_write_requested = true;
        self.settings_failed_generation = None;
        self.settings_failure_notice_visible = false;
        self.settings_close_attempt_generation = None;
        self.settings_exit_authorized_generation = None;
    }

    /// Marks an application preference as changed.
    pub(super) fn mark_app_preference_change(&mut self, field: AppPreferenceField) {
        match field {
            AppPreferenceField::Language => self.app_preference_changes.language = true,
            AppPreferenceField::Theme => self.app_preference_changes.theme = true,
            AppPreferenceField::FontPreset => self.app_preference_changes.font_preset = true,
            AppPreferenceField::UiFontSize => self.app_preference_changes.ui_font_points = true,
            AppPreferenceField::ShowInvisibles => {
                self.app_preference_changes.show_invisibles = true;
            }
            AppPreferenceField::MinimapWidth => self.app_preference_changes.minimap_width = true,
        }
        self.mark_settings_write();
    }

    /// Applies one application preference update to the UI state.
    pub(super) fn apply_app_preferences(
        &mut self,
        ctx: &egui::Context,
        preferences: &AppPreferences,
    ) {
        let before = (
            self.language_choice,
            self.language,
            self.font_preset,
            self.theme,
            self.ui_font_points,
            self.show_invisibles,
        );
        self.language_choice = preferences.language;
        self.language = crate::i18n::resolve_language(self.language_choice);
        if self.font_preset != preferences.font_preset {
            self.font_preset = preferences.font_preset;
        }
        if (self.ui_font_points - preferences.ui_font_points).abs() > f32::EPSILON {
            self.ui_font_points = preferences.ui_font_points;
            self.font_layout_generation = self.font_layout_generation.saturating_add(1);
        }
        self.theme = preferences.theme;
        self.show_invisibles = preferences.show_invisibles;
        self.minimap_width = preferences.minimap_width;
        self.apply_appearance(ctx);
        let changed = before.0 != self.language_choice
            || before.1 != self.language
            || before.2 != self.font_preset
            || before.3 != self.theme
            || (before.4 - self.ui_font_points).abs() > f32::EPSILON
            || before.5 != self.show_invisibles;
        if changed {
            self.invalidate_shared_window_appearance(ctx);
        }
    }

    /// Repaints windows only after app-wide appearance preferences change.
    pub(super) fn invalidate_shared_window_appearance(&mut self, ctx: &egui::Context) {
        if !self.multi_window_mode {
            ctx.request_repaint();
            return;
        }
        let current = self.current_viewport;
        for (viewport, state) in &mut self.window_states {
            if Some(*viewport) == current {
                continue;
            }
            state.font_layout_generation = state.font_layout_generation.saturating_add(1);
            ctx.request_repaint_of(*viewport);
        }
        ctx.request_repaint();
    }

    /// Updates the egui theme and the UI text scale from current preferences.
    pub(super) fn apply_appearance(&mut self, ctx: &egui::Context) {
        let font_preset = resolved_font_preset(self.font_preset, self.language);
        if font_preset != self.installed_font_preset {
            install_texcel_grid_fonts(ctx, font_preset);
            self.installed_font_preset = font_preset;
            self.font_layout_generation = self.font_layout_generation.saturating_add(1);
        }
        ctx.options_mut(|options| {
            options.fallback_theme = egui::Theme::Light;
            // The zoom shortcuts change the sheet zoom, never the UI scale.
            options.zoom_with_keyboard = false;
        });
        ctx.set_zoom_factor(self.ui_zoom());
        ctx.set_theme(match self.theme {
            ThemeChoice::System => egui::ThemePreference::System,
            ThemeChoice::Light => egui::ThemePreference::Light,
            ThemeChoice::Dark => egui::ThemePreference::Dark,
        });
    }

    /// Applies settings worker results and starts pending writes.
    pub(super) fn process_settings(&mut self, ctx: &egui::Context) {
        if self.settings_worker.is_none() {
            self.settings_loaded = true;
        }
        let mut loaded_preferences = None;
        let mut loaded_recent_files = None;
        if let Some(worker) = self.settings_worker.as_ref() {
            loop {
                match worker.try_recv() {
                    Ok(event) => {
                        match event {
                            SettingsEvent::Loaded(Ok(settings)) => {
                                loaded_recent_files = Some(settings.recent_files.clone());
                                self.settings_file = settings;
                                loaded_preferences = Some(true);
                            }
                            SettingsEvent::Loaded(Err(error)) => {
                                self.settings_file = SettingsFile::default();
                                loaded_recent_files = Some(Vec::new());
                                self.status = Some(error);
                                loaded_preferences = Some(false);
                            }
                            SettingsEvent::Saved { generation, result } => {
                                if self.settings_in_flight_generation == Some(generation) {
                                    self.settings_in_flight_generation = None;
                                }
                                match result {
                                    Ok(outcome) => {
                                        self.settings_saved_generation =
                                            self.settings_saved_generation.max(generation);
                                        if generation == self.settings_save_generation
                                            && !self.settings_write_requested
                                            && self.settings_pending_save.is_none()
                                        {
                                            self.settings_file = outcome.persisted;
                                            self.settings_failed_generation = None;
                                            self.settings_close_attempt_generation = None;
                                            self.settings_exit_authorized_generation = None;
                                        }
                                    }
                                    Err(error) => {
                                        self.settings_failed_generation = Some(generation);
                                        self.settings_failure_notice_visible = true;
                                        self.status = Some(error);
                                        self.close_after_settings_save = false;
                                    }
                                }
                            }
                        }
                        self.settings_loaded = true;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        if !self.settings_loaded {
                            self.settings_file = SettingsFile::default();
                            self.settings_loaded = true;
                        }
                        if self.settings_in_flight_generation.is_some() {
                            self.settings_failed_generation = Some(self.settings_save_generation);
                            self.close_after_settings_save = false;
                        }
                        self.status = Some(Message::new("status-settings-worker-stopped"));
                        break;
                    }
                }
            }
        }
        if let Some(recent_files) = loaded_recent_files {
            self.restore_recent_files(recent_files);
        }
        if loaded_preferences == Some(true) {
            let preferences = self.settings_file.app.clone();
            self.apply_app_preferences_merged(ctx, &preferences);
        }
        if self.settings_loaded && self.recent_files_changed_before_load {
            self.recent_files_changed_before_load = false;
            self.settings_file.recent_files = self.recent_files.iter().cloned().collect();
            self.mark_settings_write();
        }

        if self.settings_worker.is_none() && self.settings_write_requested {
            self.settings_write_requested = false;
            self.settings_failed_generation = Some(self.settings_save_generation);
            self.status = Some(Message::new("status-settings-location-unavailable"));
        } else if self.settings_write_requested && self.settings_loaded {
            let snapshot = self.snapshot_settings();
            self.settings_file = snapshot.clone();
            self.settings_pending_save = Some((self.settings_save_generation, snapshot));
            self.settings_write_requested = false;
        }
        if let (Some(worker), Some((generation, file))) = (
            self.settings_worker.as_ref(),
            self.settings_pending_save.take(),
        ) {
            match worker.try_save(generation, file) {
                Ok(()) => self.settings_in_flight_generation = Some(generation),
                Err(SettingsSubmitError::QueueFull(file)) => {
                    self.settings_pending_save = Some((generation, file));
                }
                Err(SettingsSubmitError::Disconnected(_file)) => {
                    self.settings_in_flight_generation = None;
                    self.settings_write_requested = false;
                    self.settings_pending_save = None;
                    self.settings_failed_generation = Some(self.settings_save_generation);
                    self.settings_failure_notice_visible = false;
                    self.close_after_settings_save = false;
                    self.status = Some(Message::new("status-settings-worker-save-stopped"));
                }
            }
        }
        self.handle_zoom_shortcuts(ctx);
        self.apply_appearance(ctx);
        if !self.settings_loaded {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if self.settings_pending_save.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        self.finish_settings_close_if_ready(ctx);
    }

    /// Merges saved app preferences with newer in-memory changes.
    pub(super) fn apply_app_preferences_merged(
        &mut self,
        ctx: &egui::Context,
        loaded: &AppPreferences,
    ) {
        let merged = AppPreferences {
            language: if self.app_preference_changes.language {
                self.language_choice
            } else {
                loaded.language
            },
            theme: if self.app_preference_changes.theme {
                self.theme
            } else {
                loaded.theme
            },
            font_preset: if self.app_preference_changes.font_preset {
                self.font_preset
            } else {
                loaded.font_preset
            },
            ui_font_points: if self.app_preference_changes.ui_font_points {
                self.ui_font_points
            } else {
                loaded.ui_font_points
            },
            show_invisibles: if self.app_preference_changes.show_invisibles {
                self.show_invisibles
            } else {
                loaded.show_invisibles
            },
            minimap_width: if self.app_preference_changes.minimap_width {
                self.minimap_width
            } else {
                loaded.minimap_width
            },
        };
        self.apply_app_preferences(ctx, &merged);
    }

    /// Closes settings after its pending save finishes.
    pub(super) fn finish_settings_close_if_ready(&mut self, ctx: &egui::Context) {
        if !self.close_after_settings_save {
            return;
        }
        let latest = self.settings_save_generation;
        if self.settings_saved_generation >= latest
            && !self.settings_write_requested
            && self.settings_pending_save.is_none()
            && self.settings_in_flight_generation.is_none()
        {
            self.close_after_settings_save = false;
            if self.app_exit_transaction && self.multi_window_mode {
                self.app_exit_ready = true;
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        } else if self.settings_failed_generation == Some(latest) {
            self.close_after_settings_save = false;
            if self.settings_exit_authorized_generation == Some(latest) {
                if self.app_exit_transaction && self.multi_window_mode {
                    self.app_exit_ready = true;
                } else {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            } else {
                self.settings_close_attempt_generation = Some(latest);
                self.status = Some(Message::new("status-settings-save-failed-before-exit"));
            }
        } else {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }
}
