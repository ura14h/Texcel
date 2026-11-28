use super::*;

impl TexcelApp {
    /// Freezes document work while settings and recovery files are checked for exit.
    pub(in crate::ui) fn discard_all_before_exit(&mut self) {
        if self.app_close_has_active_composition() || self.app_close_has_pending_operations() {
            return;
        }
        self.settings_file = self.snapshot_settings();
        let latest = self.settings_save_generation;
        let settings_was_explicitly_approved = self.settings_failed_generation == Some(latest)
            && self.settings_exit_authorized_generation == Some(latest);
        if !settings_was_explicitly_approved {
            self.mark_settings_write();
        }
        self.close_after_settings_save = false;
        self.app_close_discard_state = Some(self.capture_app_close_discard_state());
        self.app_close_discard_exit_ready = false;
        self.pending_recovery_flushes.clear();
        self.settings_window_open = false;
        self.app_close_dialog_open = false;
        self.app_close_discard_pending = true;
        self.app_close_save_batch_active = false;
        self.app_close_waiting_recovery_removal = false;
        self.app_close_recovery_removal_failed = false;
        self.app_close_after_documents = true;
    }

    /// Leaves every document intact when the close request is cancelled or fails.
    pub(in crate::ui) fn cancel_app_close(&mut self) {
        self.app_exit_transaction = false;
        self.app_exit_ready = false;
        if self.app_close_discard_state.is_some() {
            self.cancel_unsent_discard_removals();
            self.resume_recovery_after_discard();
        }
        self.app_close_dialog_open = false;
        self.app_close_save_generation = self.app_close_save_generation.wrapping_add(1);
        self.app_close_save_tokens.clear();
        self.app_close_save_batch_active = false;
        self.app_close_save_collecting_targets = false;
        self.pending_app_close_save_queue.clear();
        self.pending_app_close_save_targets.clear();
        // A deferred Save may continue, but its acknowledgement cannot resume app exit.
        for growth in self.pending_text_growth_sessions.values_mut() {
            growth.app_close_save = false;
        }
        for intent in self.pending_save_intents.values_mut() {
            intent.app_close_generation = None;
        }
        self.app_close_save_failed = false;
        self.app_close_after_documents = false;
        self.app_close_discard_pending = false;
        self.app_close_waiting_recovery_removal = false;
        self.app_close_recovery_removal_failed = false;
        self.app_close_discard_state = None;
        self.app_close_discard_exit_ready = false;
        self.settings_close_attempt_generation = None;
        self.settings_exit_authorized_generation = None;
        self.close_after_settings_save = false;
    }

    /// Captures versions and only the buffers that can still change.
    pub(super) fn capture_app_close_discard_state(&self) -> AppCloseDiscardState {
        let document_versions = self
            .workspace
            .documents
            .iter()
            .map(|(id, document)| {
                (
                    *id,
                    (
                        document.revision,
                        document.saved_revision,
                        document.structure_revision,
                    ),
                )
            })
            .collect();
        let drafts = BTreeMap::new();
        let backings = self
            .file_backings
            .iter()
            .map(|(id, backing)| (*id, (backing.path.clone(), backing.identity.clone())))
            .collect();
        AppCloseDiscardState {
            settings_generation: self.settings_save_generation,
            document_order: self.workspace.order.clone(),
            document_versions,
            drafts,
            recovered_edits: self.pending_recovered_cell_edits.clone(),
            recovered_needing_save: self.recovered_documents_needing_save.clone(),
            recovery_keys: self.recovery_keys.clone(),
            pending_cleanups: self.pending_recovery_cleanups.clone(),
            queued_removals: BTreeSet::new(),
            backings,
            window_edits: self.app_close_window_edit_states(),
        }
    }

    /// Checks that no document or live input changed after discard was approved.
    pub(in crate::ui) fn app_close_discard_state_is_current(&self) -> bool {
        let Some(state) = self.app_close_discard_state.as_ref() else {
            return false;
        };
        if state.settings_generation != self.settings_save_generation
            || state.document_order != self.workspace.order
            || state.window_edits != self.app_close_window_edit_states()
            || state.document_versions.len() != self.workspace.documents.len()
            || state.recovered_edits != self.pending_recovered_cell_edits
            || state.recovered_needing_save != self.recovered_documents_needing_save
            || state.recovery_keys != self.recovery_keys
            || state.backings.len() != self.file_backings.len()
        {
            return false;
        }
        for (document_id, document) in &self.workspace.documents {
            let version = (
                document.revision,
                document.saved_revision,
                document.structure_revision,
            );
            if state.document_versions.get(document_id) != Some(&version)
                || state.drafts.contains_key(document_id)
            {
                return false;
            }
        }
        for (document_id, backing) in &self.file_backings {
            let Some(captured) = state.backings.get(document_id) else {
                return false;
            };
            if captured.0 != backing.path || captured.1 != backing.identity {
                return false;
            }
        }
        true
    }

    /// Captures cell edit buffers by viewport so one window cannot hide another's draft.
    pub(super) fn app_close_window_edit_states(
        &self,
    ) -> BTreeMap<egui::ViewportId, (Option<DocumentId>, Option<DiscardEditState>)> {
        let mut states = self
            .window_states
            .iter()
            .map(|(viewport, state)| {
                (
                    *viewport,
                    (state.document_id, discard_edit_state(&state.edit)),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let viewport = self.current_viewport.unwrap_or(egui::ViewportId::ROOT);
        states.insert(
            viewport,
            (
                self.workspace.active_document,
                discard_edit_state(&self.edit),
            ),
        );
        states
    }

    /// Waits for settings and recovery removal before authorizing the final close.
    pub(in crate::ui) fn advance_app_close(&mut self, ctx: &egui::Context) {
        if !self.app_close_after_documents {
            return;
        }
        if !self.app_close_discard_pending {
            self.app_close_after_documents = false;
            self.close_after_settings_save = true;
            self.finish_settings_close_if_ready(ctx);
            return;
        }
        if !self.app_close_discard_state_is_current() {
            self.abort_discard_exit(Message::new("status-app-close-save-failed"));
            return;
        }
        let latest = self.settings_save_generation;
        let settings_failed = self.settings_failed_generation == Some(latest);
        if settings_failed && self.settings_exit_authorized_generation != Some(latest) {
            self.settings_close_attempt_generation = Some(latest);
            self.abort_discard_exit(Message::new("status-settings-save-failed-before-exit"));
            return;
        }
        if !settings_failed
            && (self.settings_write_requested
                || self.settings_pending_save.is_some()
                || self.settings_in_flight_generation.is_some()
                || self.settings_saved_generation < latest)
        {
            ctx.request_repaint_after(Duration::from_millis(50));
            return;
        }
        if !self.app_close_waiting_recovery_removal {
            let keys = self
                .app_close_discard_state
                .as_ref()
                .map(|state| state.recovery_keys.values().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            for key in keys {
                if self.pending_recovery_removals.contains(&key)
                    || self.in_flight_recovery_removals.contains(&key)
                {
                    continue;
                }
                self.queue_recovery_remove(key.clone());
                if let Some(state) = self.app_close_discard_state.as_mut() {
                    // The bounded worker may leave this discard request queued for later.
                    state.queued_removals.insert(key);
                }
            }
            self.app_close_waiting_recovery_removal = true;
            self.app_close_recovery_removal_failed = false;
        }
        self.submit_pending_recovery_maintenance();
        if self.app_close_recovery_removal_failed {
            self.abort_discard_exit(Message::new("status-app-close-recovery-remove-failed"));
            return;
        }
        if !self.pending_recovery_removals.is_empty()
            || !self.in_flight_recovery_removals.is_empty()
        {
            ctx.request_repaint_after(Duration::from_millis(50));
            return;
        }
        if !self.app_close_discard_state_is_current() {
            self.resume_recovery_after_discard();
            self.abort_discard_exit(Message::new("status-app-close-save-failed"));
            return;
        }
        self.app_close_after_documents = false;
        self.app_close_waiting_recovery_removal = false;
        self.app_close_discard_exit_ready = true;
        if self.app_exit_transaction && self.multi_window_mode {
            self.app_exit_ready = true;
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Reopens the choices without clearing any document or live input.
    pub(in crate::ui) fn abort_discard_exit(&mut self, status: Message) {
        self.cancel_unsent_discard_removals();
        self.app_close_after_documents = false;
        self.app_close_discard_pending = false;
        self.app_close_waiting_recovery_removal = false;
        self.app_close_recovery_removal_failed = false;
        self.app_close_discard_state = None;
        self.app_close_discard_exit_ready = false;
        self.close_after_settings_save = false;
        self.app_close_dialog_open = true;
        self.status = Some(status);
        self.resume_recovery_after_discard();
    }

    /// Cancels only unsent removals created by this discard request.
    fn cancel_unsent_discard_removals(&mut self) {
        let Some(state) = self.app_close_discard_state.as_ref() else {
            return;
        };
        let removals = state.queued_removals.clone();
        let cleanups = state.pending_cleanups.clone();
        for key in removals {
            if !self.in_flight_recovery_removals.contains(&key) {
                self.pending_recovery_removals.remove(&key);
            }
            if let Some((revision, generation)) = cleanups.get(&key) {
                self.queue_recovery_cleanup(key, *revision, *generation);
            }
        }
    }

    /// Makes the next recovery pass save every retained dirty document again.
    fn resume_recovery_after_discard(&mut self) {
        let now = Instant::now();
        for schedule in self.recovery_schedules.values_mut() {
            schedule.last_success = None;
            schedule.in_flight = None;
            schedule.changed_at = now - Duration::from_secs(3);
        }
    }
}

/// Copies only the live text and selection needed to validate a discard decision.
fn discard_edit_state(edit: &EditSession) -> Option<DiscardEditState> {
    edit.is_editing().then(|| DiscardEditState {
        session_id: edit.session_id,
        table_id: edit.table_id,
        address: edit.address,
        target: edit.target,
        original: edit.original.clone(),
        buffer: edit.buffer.clone(),
        cursor_byte: edit.cursor_byte,
        anchor_byte: edit.anchor_byte,
        window_start: edit.edit_window_start,
        composing: edit.composing,
    })
}
