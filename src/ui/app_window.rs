use super::*;

impl TexcelApp {
    /// Converts the UI text size into egui's scale for every widget outside the sheet.
    pub(super) fn ui_zoom(&self) -> f32 {
        crate::settings::ui_font_points_to_logical(self.ui_font_points) / visual::BODY_SIZE
    }

    /// Returns the factor the grid applies on top of the UI scale to show this window's zoom.
    pub(super) fn grid_scale(&self) -> f32 {
        self.sheet_zoom.0 / self.ui_zoom()
    }

    /// Updates the app-wide UI text size and applies it to every window.
    pub(super) fn set_ui_font_points(&mut self, ctx: &egui::Context, size: f32) {
        let next = size.round().clamp(
            crate::settings::MIN_UI_FONT_POINTS,
            crate::settings::MAX_UI_FONT_POINTS,
        );
        if (next - self.ui_font_points).abs() < f32::EPSILON {
            return;
        }
        self.ui_font_points = next;
        self.font_layout_generation = self.font_layout_generation.saturating_add(1);
        self.mark_app_preference_change(AppPreferenceField::UiFontSize);
        self.apply_appearance(ctx);
        self.invalidate_shared_window_appearance(ctx);
    }

    /// Changes this window's sheet zoom and keeps the same cells at the top left.
    pub(super) fn set_sheet_zoom(&mut self, zoom: f32) {
        let next = clamp_sheet_zoom(zoom);
        let previous = self.sheet_zoom.0;
        if (next - previous).abs() < f32::EPSILON {
            return;
        }
        let ratio = next / previous;
        let shown_here = self.workspace.active_document;
        let multi_window = self.multi_window_mode;
        for (document_id, document) in &mut self.workspace.documents {
            if multi_window && Some(*document_id) != shown_here {
                continue;
            }
            for view in document.views.values_mut() {
                view.scroll.x *= ratio;
                view.scroll.y *= ratio;
            }
        }
        self.sheet_zoom = SheetZoom(next);
        self.request_scroll_to_selection = false;
    }

    // Handle keyboard shortcuts that change this window's sheet zoom.
    pub(super) fn handle_zoom_shortcuts(&mut self, ctx: &egui::Context) {
        let focused_id = ctx.memory(|memory| memory.focused());
        let grid_owns_keyboard = self.grid_focused
            && !self.suppress_grid_keyboard
            && self.grid_keyboard_owner.is_some()
            && focused_id == self.grid_keyboard_owner;
        if self.edit.is_editing()
            || self.suppress_grid_keyboard
            || (ctx.egui_wants_keyboard_input() && !grid_owns_keyboard)
        {
            return;
        }
        let zoom = self.sheet_zoom.0;
        let action = ctx.input_mut(|input| {
            use egui::gui_zoom::kb_shortcuts;
            if input.consume_shortcut(&kb_shortcuts::ZOOM_RESET) {
                Some(1.0)
            } else if input.consume_shortcut(&kb_shortcuts::ZOOM_IN)
                || input.consume_shortcut(&kb_shortcuts::ZOOM_IN_SECONDARY)
            {
                Some(step_sheet_zoom(zoom, true))
            } else if input.consume_shortcut(&kb_shortcuts::ZOOM_OUT) {
                Some(step_sheet_zoom(zoom, false))
            } else {
                None
            }
        });
        if let Some(zoom) = action {
            self.set_sheet_zoom(zoom);
        }
    }

    // Save settings and resolve pending edits before closing the app.
    pub(super) fn handle_app_close_request(&mut self, ctx: &egui::Context) {
        let viewport_close_requested = ctx.input(|input| input.viewport().close_requested());
        let menu_quit_requested = std::mem::take(&mut self.app_exit_start_pending);
        #[cfg(target_os = "macos")]
        let native_terminate_requested = crate::macos_app::take_terminate_request();
        #[cfg(not(target_os = "macos"))]
        let native_terminate_requested = false;
        if menu_quit_requested || native_terminate_requested {
            self.app_exit_transaction = true;
        }
        if !viewport_close_requested && !native_terminate_requested && !menu_quit_requested {
            return;
        }
        let was_hidden = ctx.input(|input| input.viewport().visible() == Some(false));
        if !self.app_close_discard_pending
            && !self.app_close_waiting_recovery_removal
            && (self.app_close_waiting_recovery_maintenance || self.recovery_maintenance_pending())
        {
            if !self.app_close_waiting_recovery_maintenance {
                self.failed_recovery_maintenance.clear();
                self.app_close_recovery_maintenance_failed = false;
                self.app_close_waiting_recovery_maintenance = true;
            }
            self.app_close_after_recovery_maintenance = Some(
                self.app_close_after_recovery_maintenance.unwrap_or(false)
                    || native_terminate_requested,
            );
            if viewport_close_requested {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
            if native_terminate_requested || was_hidden {
                Self::show_window_for_close_prompt(ctx);
            }
            ctx.request_repaint_after(Duration::from_millis(50));
            return;
        }
        if self.app_close_discard_exit_ready {
            let approved_state = self.app_close_discard_state_is_current()
                && !self.app_close_has_pending_operations();
            if approved_state {
                if native_terminate_requested {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
                self.app_close_discard_exit_ready = false;
                self.app_close_discard_pending = false;
                self.app_close_discard_state = None;
                return;
            }
            self.abort_discard_exit(Message::new("status-app-close-save-failed"));
            if viewport_close_requested {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
            if native_terminate_requested || was_hidden {
                Self::show_window_for_close_prompt(ctx);
            }
            return;
        }
        if self.app_close_discard_pending || self.app_close_waiting_recovery_removal {
            if viewport_close_requested {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
            if native_terminate_requested || was_hidden {
                Self::show_window_for_close_prompt(ctx);
            }
            return;
        }
        let latest = self.settings_save_generation;
        let settings_failed = self.settings_failed_generation == Some(latest);
        if settings_failed && self.settings_exit_authorized_generation != Some(latest) {
            if self.settings_close_attempt_generation == Some(latest) {
                self.settings_exit_authorized_generation = Some(latest);
            } else {
                self.settings_close_attempt_generation = Some(latest);
                self.settings_failure_notice_visible = true;
                self.close_after_settings_save = false;
                self.status = Some(Message::new("status-settings-save-failed-before-exit"));
                if viewport_close_requested {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                }
                if native_terminate_requested || was_hidden {
                    Self::show_window_for_close_prompt(ctx);
                }
                return;
            }
        }
        if self.app_close_needs_save() {
            self.app_close_dialog_open = true;
            if viewport_close_requested {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
            if native_terminate_requested || was_hidden {
                Self::show_window_for_close_prompt(ctx);
            }
            return;
        }
        if settings_failed && self.settings_exit_authorized_generation == Some(latest) {
            if self.app_exit_transaction && self.multi_window_mode {
                self.app_exit_ready = true;
            } else if native_terminate_requested {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            return;
        }
        let pending = self.settings_write_requested
            || self.settings_pending_save.is_some()
            || self.settings_in_flight_generation.is_some()
            || self.settings_saved_generation < latest;
        if pending {
            self.close_after_settings_save = true;
            if viewport_close_requested {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
            if native_terminate_requested || was_hidden {
                Self::show_window_for_close_prompt(ctx);
            }
            ctx.request_repaint_after(Duration::from_millis(50));
        } else if self.app_exit_transaction && self.multi_window_mode {
            self.app_exit_ready = true;
        } else if native_terminate_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Restores a hidden window so the user can answer the close prompt.
    fn show_window_for_close_prompt(ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }
}
