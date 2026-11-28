use super::super::*;
use super::ImeFrameState;
use super::events::*;

impl TexcelApp {
    /// Routes IME events in order while keeping a restore point for each preedit.
    #[allow(deprecated)]
    pub(in crate::ui) fn sync_ime_events(&mut self, ctx: &egui::Context) -> ImeFrameState {
        let events = ctx.input(|input| input.events.clone());
        let current_owner = self
            .current_ime_editor(ctx)
            .or_else(|| self.requested_ime_editor(&events));
        let owner_lost_focus = self.ime_composition.as_ref().is_some_and(|composition| {
            !current_owner.is_some_and(|(_, _, session_id, id)| {
                composition.session_id == session_id && composition.editor_id == id
            })
        });
        let mut events = events;
        if owner_lost_focus {
            self.cancel_ime_composition_with_terminal(ctx, true);
            events.retain(|event| !is_ime_event(event));
            ctx.input_mut(|input| input.events.retain(|event| !is_ime_event(event)));
        }

        let Some((document_id, table_id, session_id, editor_id)) = current_owner else {
            return ImeFrameState {
                navigation_blocked: self.modal_ime_composition.is_some(),
                text_event: false,
                commit_before_pointer: false,
            };
        };

        let mut committed =
            snapshot_for_editor(document_id, table_id, session_id, editor_id, &self.edit);
        let mut composition = self
            .ime_composition
            .take()
            .filter(|state| state.session_id == session_id && state.editor_id == editor_id);
        let mut composition_start = composition.as_ref().map(|_| 0);
        let mut commit_before_pointer = None;
        let mut text_event = false;

        // The snapshot changes only for committed text before the next preedit.
        for (index, event) in events.iter().enumerate() {
            match event {
                egui::Event::Text(text) | egui::Event::Paste(text) => {
                    if let Some(state) = composition.as_mut() {
                        apply_text_to_snapshot(state, text);
                    } else {
                        apply_text_to_snapshot(&mut committed, text);
                        if let Some((_, commit, pointer, state)) = commit_before_pointer.as_mut()
                            && index > *commit
                            && index < *pointer
                        {
                            apply_text_to_snapshot(state, text);
                        }
                    }
                }
                egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) => {
                    text_event |= !text.is_empty();
                    if composition.is_none() {
                        if commit_before_pointer.is_some() {
                            commit_before_pointer = None;
                        }
                        composition_start = Some(index);
                        composition = Some(committed.clone());
                    }
                    if let Some(state) = composition.as_mut() {
                        state.preedit_active = !text.is_empty();
                    }
                    self.edit.set_composing(!text.is_empty());
                }
                egui::Event::Ime(egui::ImeEvent::Commit(text)) => {
                    text_event = true;
                    let start = composition_start.take().unwrap_or(index);
                    let mut state = composition.take().unwrap_or_else(|| committed.clone());
                    apply_text_to_snapshot(&mut state, text);
                    state.preedit_active = false;
                    committed = state;
                    self.edit.set_composing(false);
                    if let Some(pointer) = events
                        .iter()
                        .enumerate()
                        .skip(index + 1)
                        .find_map(|(index, event)| is_pointer_press(event).then_some(index))
                        && !text.is_empty()
                        && !events[..index].iter().any(is_untracked_cursor_input)
                        && !events[..index].iter().any(is_ime_commit_event)
                    {
                        commit_before_pointer = Some((start, index, pointer, committed.clone()));
                    }
                }
                event if is_ime_disabled_event(event) && composition.is_some() => {
                    self.ime_composition = composition.take();
                    self.cancel_ime_composition_with_terminal(ctx, false);
                }
                _ => (),
            }
        }

        self.ime_composition = composition;
        let preedit_active = self
            .ime_composition
            .as_ref()
            .is_some_and(|state| state.preedit_active);
        self.edit.set_composing(preedit_active);
        let mut commit_before_pointer_applied = false;
        if let Some((start, commit, pointer, snapshot)) = commit_before_pointer
            && !preedit_active
        {
            self.restore_ime_snapshot(ctx, &snapshot);
            commit_before_pointer_applied = true;
            ctx.input_mut(|input| {
                input.events = input
                    .events
                    .drain(..)
                    .enumerate()
                    .filter_map(|(index, event)| {
                        let already_applied = (start..=commit).contains(&index)
                            && is_ime_event(&event)
                            || index < pointer && is_tracked_text_event(&event);
                        (!already_applied).then_some(event)
                    })
                    .collect();
            });
        }
        let escape_cancelled = std::mem::take(&mut self.ime_escape_cancelled_this_frame);
        ImeFrameState {
            navigation_blocked: preedit_active
                || self.modal_ime_composition.is_some()
                || text_event
                || escape_cancelled,
            text_event,
            commit_before_pointer: commit_before_pointer_applied,
        }
    }

    /// Finds the shared editor only while it owns the current keyboard focus.
    #[allow(deprecated)]
    pub(super) fn current_ime_editor(
        &self,
        ctx: &egui::Context,
    ) -> Option<(DocumentId, TableId, u64, Id)> {
        let owner = self.ime_editor_identity()?;
        (self.text_edit_has_focus(ctx) && self.focused_widget(ctx) == Some(owner.3))
            .then_some(owner)
    }

    /// Returns the focused widget of the window being handled: before its pass starts, the
    /// focus it had at the end of its last pass; during the pass, egui's own answer.
    pub(in crate::ui) fn focused_widget(&self, ctx: &egui::Context) -> Option<Id> {
        match self.raw_input_viewport {
            Some(viewport) => super::pass_focus::focus_before_pass(ctx, viewport),
            None => ctx.memory(|memory| memory.focused()),
        }
    }

    /// Tells whether the focused widget of the window being handled is a text field.
    pub(in crate::ui) fn text_edit_has_focus(&self, ctx: &egui::Context) -> bool {
        self.focused_widget(ctx)
            .is_some_and(|id| egui::text_edit::TextEditState::load(ctx, id).is_some())
    }

    /// Moves keyboard focus to a widget of the window being handled. Before its pass starts,
    /// egui's memory may belong to another window, so the move waits for the draw instead.
    pub(in crate::ui) fn focus_editor_before_pass(&mut self, ctx: &egui::Context, id: Id) {
        let another_window = self
            .raw_input_viewport
            .is_some_and(|viewport| super::pass_focus::memory_is_for_another_window(ctx, viewport));
        if another_window {
            self.request_editor_focus = true;
        } else {
            ctx.memory_mut(|memory| memory.request_focus(id));
        }
    }

    /// Finds an editor that will receive an IME preedit after the current draw.
    pub(super) fn requested_ime_editor(
        &self,
        events: &[egui::Event],
    ) -> Option<(DocumentId, TableId, u64, Id)> {
        (self.request_editor_focus
            && events
                .iter()
                .any(|event| matches!(event, egui::Event::Ime(egui::ImeEvent::Preedit { .. }))))
        .then(|| self.ime_editor_identity())
        .flatten()
    }

    /// Returns the active document and editor session without assuming focus.
    pub(super) fn ime_editor_identity(&self) -> Option<(DocumentId, TableId, u64, Id)> {
        let document_id = self.edit_document_id?;
        let table_id = self.edit.table_id?;
        if !self.edit.is_editing()
            || self.workspace.active_document != Some(document_id)
            || self.workspace.documents.get(&document_id)?.active_table != table_id
        {
            return None;
        }
        Some((
            document_id,
            table_id,
            self.edit.session_id,
            editor_id(document_id, self.edit.session_id),
        ))
    }

    /// Restores the confirmed buffer and selection without committing preedit text.
    pub(super) fn cancel_ime_composition_with_terminal(
        &mut self,
        ctx: &egui::Context,
        await_terminal: bool,
    ) {
        if await_terminal {
            self.ime_interrupt_pending = true;
        }
        let Some(composition) = self.ime_composition.take() else {
            self.edit.set_composing(false);
            return;
        };
        if self.edit_document_id != Some(composition.document_id)
            || self.edit.table_id != Some(composition.table_id)
            || self.edit.session_id != composition.session_id
        {
            return;
        }

        if await_terminal {
            self.late_ime_editor = Some(composition.editor_id);
        }
        self.restore_ime_snapshot(ctx, &composition);
    }

    /// Restores one editor snapshot and its cursor without replacing undo history.
    pub(super) fn restore_ime_snapshot(
        &mut self,
        ctx: &egui::Context,
        composition: &ImeCompositionState,
    ) {
        if self.edit_document_id != Some(composition.document_id)
            || self.edit.table_id != Some(composition.table_id)
            || self.edit.session_id != composition.session_id
        {
            return;
        }
        let buffer_changed = self.edit.buffer != composition.buffer;
        self.edit.buffer.clone_from(&composition.buffer);
        let cursor_byte = clamp_byte_boundary(
            &self.edit.buffer,
            composition.cursor_byte.min(self.edit.buffer.len()),
        );
        let anchor_byte = clamp_byte_boundary(
            &self.edit.buffer,
            composition.anchor_byte.min(self.edit.buffer.len()),
        );
        self.edit.cursor_byte = cursor_byte;
        self.edit.anchor_byte = anchor_byte;
        self.edit.edit_window_start = composition.edit_window_start;
        self.edit.set_composing(false);
        if buffer_changed {
            self.edit.mark_buffer_changed();
        }

        // Keeps the editor undo history while clearing its private IME cursor marker.
        let mut state = egui::widgets::text_edit::TextEditState::load(ctx, composition.editor_id)
            .unwrap_or_default();
        let cursor = cursor_character_index(&self.edit.buffer, cursor_byte);
        let anchor = cursor_character_index(&self.edit.buffer, anchor_byte);
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(anchor),
                egui::text::CCursor::new(cursor),
            )));
        state.store(ctx, composition.editor_id);
    }

    /// Restores selection text after egui applies an empty preedit to the widget.
    pub(in crate::ui) fn restore_unmarked_ime_after_grid(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|input| input.events.clone());
        let saw_unmark = events.iter().any(is_empty_preedit);
        let has_later_work = events.iter().any(|event| {
            interrupts_unmarked_ime(event)
                || is_ime_terminal_event(event)
                || matches!(
                    event,
                    egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) if !text.is_empty()
                )
        });
        if !saw_unmark || has_later_work {
            return;
        }
        let Some(snapshot) = self
            .ime_composition
            .as_ref()
            .filter(|state| !state.preedit_active)
            .cloned()
        else {
            return;
        };
        self.restore_ime_snapshot(ctx, &snapshot);
    }
}

/// Creates an editor restore point from its current confirmed buffer and selection.
fn snapshot_for_editor(
    document_id: DocumentId,
    table_id: TableId,
    session_id: u64,
    editor_id: Id,
    edit: &EditSession,
) -> ImeCompositionState {
    ImeCompositionState {
        document_id,
        table_id,
        session_id,
        editor_id,
        buffer: edit.buffer.clone(),
        anchor_byte: edit.anchor_byte,
        cursor_byte: edit.cursor_byte,
        edit_window_start: edit.edit_window_start,
        preedit_active: false,
    }
}

/// Applies only confirmed characters to a snapshot that egui will later edit once.
fn apply_text_to_snapshot(state: &mut ImeCompositionState, text: &str) {
    if text.is_empty() {
        return;
    }
    let start = clamp_byte_boundary(
        &state.buffer,
        state
            .anchor_byte
            .min(state.cursor_byte)
            .min(state.buffer.len()),
    );
    let end = clamp_byte_boundary(
        &state.buffer,
        state
            .anchor_byte
            .max(state.cursor_byte)
            .min(state.buffer.len()),
    );
    state.buffer.replace_range(start..end, text);
    state.cursor_byte = start + text.len();
    state.anchor_byte = state.cursor_byte;
}

/// Converts one UTF-8 byte offset to egui's character cursor index.
fn cursor_character_index(text: &str, byte: usize) -> usize {
    text[..clamp_byte_boundary(text, byte)].chars().count()
}
