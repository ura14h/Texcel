use super::super::*;
use super::events::*;

impl TexcelApp {
    /// Starts a cell editor when an IME begins directly from grid selection.
    #[allow(deprecated)]
    fn begin_grid_edit_for_ime(&mut self, ctx: &egui::Context, raw: &egui::RawInput) {
        let has_preedit = raw.events.iter().any(|event| {
            matches!(event, egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) if !text.is_empty())
        });
        if has_preedit
            && self.grid_focused
            && !self.edit.is_editing()
            && !self.text_edit_has_focus(ctx)
            && !self.modal_input_blocked()
        {
            self.begin_selected_edit();
            if self.edit.is_editing()
                && let (Some(document_id), Some(table_id), Some(address)) =
                    (self.edit_document_id, self.edit.table_id, self.edit.address)
            {
                self.edit.mode = EditMode::Input;
                self.edit.anchor_byte = 0;
                self.edit.cursor_byte = self.edit.buffer.len();
                self.start_input_sequence(document_id, table_id, address);
                // Move focus before the pass starts: egui drops this focus change's IME
                // interrupt at the start of the pass. An interrupt here would make winit
                // forget the marked text that macOS still holds, so Enter could not commit it.
                // The draw code sees the focus already in place and does not request it again.
                let editor = editor_id(document_id, self.edit.session_id);
                self.focus_editor_before_pass(ctx, editor);
            }
        }
    }

    /// Cancels only this editor's preedit when it is left or Escape is pressed.
    #[allow(deprecated)]
    pub(in crate::ui) fn prepare_ime_input(
        &mut self,
        ctx: &egui::Context,
        raw: &mut egui::RawInput,
    ) {
        self.expire_unmarked_ime_on_input(ctx, raw);
        self.begin_grid_edit_for_ime(ctx, raw);
        self.prepare_modal_ime_keys(ctx, raw);
        self.drop_late_ime_events(raw);
        self.drop_cursor_keys_during_preedit(raw);

        let requested_owner = self.requested_ime_editor(&raw.events);
        let owner = self.current_ime_editor(ctx).or(requested_owner);
        let owner_matches = self.ime_composition.as_ref().is_some_and(|composition| {
            owner.is_some_and(|(_, _, session_id, editor_id)| {
                composition.session_id == session_id && composition.editor_id == editor_id
            })
        });
        let mut cancel_from = None;
        let mut composition_start = owner_matches.then_some(0);
        let mut composing = owner_matches;
        let mut escape_cancels = false;
        let mut terminal_seen = false;
        let editor_exists = owner.is_some();

        // Tracks event order without taking ordinary text or clicks from egui.
        for (index, event) in raw.events.iter().enumerate() {
            match event {
                egui::Event::Ime(egui::ImeEvent::Preedit { .. }) if editor_exists => {
                    if !composing {
                        composition_start = Some(index);
                        terminal_seen = false;
                    }
                    composing = true;
                }
                egui::Event::Ime(egui::ImeEvent::Commit(text)) if composing => {
                    let start = composition_start.unwrap_or(index);
                    composing = false;
                    composition_start = None;
                    terminal_seen = !text.is_empty();
                    if text.is_empty() && !raw.events.iter().skip(index + 1).any(|later| {
                        matches!(later, egui::Event::Ime(egui::ImeEvent::Commit(text)) if !text.is_empty())
                    }) {
                        cancel_from = Some(start);
                    }
                }
                event if is_ime_disabled_event(event) && composing => {
                    cancel_from = composition_start.or(Some(index));
                    composing = false;
                    composition_start = None;
                    terminal_seen = true;
                }
                egui::Event::WindowFocused(false)
                    if composing
                        || editor_exists && raw.events.iter().skip(index + 1).any(is_ime_event) =>
                {
                    cancel_from = composition_start.or(Some(index));
                    composing = false;
                    composition_start = None;
                }
                egui::Event::PointerButton { pressed: true, .. }
                    if composing
                        || editor_exists && raw.events.iter().skip(index + 1).any(is_ime_event) =>
                {
                    cancel_from = composition_start.or(Some(index));
                    composing = false;
                    composition_start = None;
                }
                egui::Event::Key {
                    key: Key::Escape,
                    pressed: true,
                    ..
                } if composing => {
                    cancel_from = composition_start.or(Some(index));
                    composing = false;
                    composition_start = None;
                    escape_cancels = true;
                }
                _ => (),
            }
        }

        // A changed focus invalidates the old editor session before late events arrive.
        let owner_lost_focus = self.ime_composition.is_some() && !owner_matches;
        if owner_lost_focus {
            cancel_from = Some(0);
            terminal_seen = raw.events.iter().any(is_ime_terminal_event);
        }
        if let Some(start) = cancel_from {
            let escape_focus = escape_cancels
                .then(|| self.ime_composition.as_ref().map(|state| state.editor_id))
                .flatten();
            self.cancel_ime_composition_with_terminal(ctx, !terminal_seen);
            if let Some(editor_id) = escape_focus {
                self.focus_editor_before_pass(ctx, editor_id);
                self.request_editor_focus = true;
            }
            self.ime_escape_cancelled_this_frame |= escape_cancels;
            raw.events = raw
                .events
                .drain(..)
                .enumerate()
                .filter_map(|(index, event)| {
                    let old_ime = index >= start && is_ime_event(&event);
                    let handled_escape = escape_cancels
                        && matches!(
                            event,
                            egui::Event::Key {
                                key: Key::Escape,
                                pressed: true,
                                ..
                            }
                        );
                    (!old_ime && !handled_escape).then_some(event)
                })
                .collect();
        }
    }

    /// Keeps Enter and Escape with a focused modal field during its preedit.
    #[allow(deprecated)]
    fn prepare_modal_ime_keys(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        let focused_text = self
            .text_edit_has_focus(ctx)
            .then(|| self.focused_widget(ctx))
            .flatten();
        let shared_owner = self
            .current_ime_editor(ctx)
            .map(|(_, _, _, id)| id)
            .or_else(|| {
                self.requested_ime_editor(&raw.events)
                    .map(|(_, _, _, id)| id)
            });
        let modal_owner = focused_text.filter(|id| Some(*id) != shared_owner);
        let owner_changed = self
            .modal_ime_composition
            .is_some_and(|old| modal_owner != Some(old));
        let old_owner = self.modal_ime_composition;
        let mut composing = old_owner.is_some() && !owner_changed;
        let mut left_field = owner_changed;
        let mut discard_ime_suffix = owner_changed;
        if owner_changed {
            self.late_ime_editor = old_owner;
            self.modal_ime_composition = None;
        }

        let mut events = Vec::with_capacity(raw.events.len());
        for event in raw.events.drain(..) {
            if is_modal_leave_event(&event) && modal_owner.is_some() {
                left_field = true;
                discard_ime_suffix = true;
                if composing {
                    self.late_ime_editor = modal_owner;
                    self.modal_ime_composition = None;
                    composing = false;
                }
            }
            if discard_ime_suffix && is_ime_event(&event) {
                continue;
            }
            match &event {
                egui::Event::Ime(egui::ImeEvent::Preedit { .. })
                    if modal_owner.is_some() && !left_field =>
                {
                    self.modal_ime_composition = modal_owner;
                    composing = true;
                    events.push(event);
                }
                egui::Event::Ime(egui::ImeEvent::Commit(_)) if composing => {
                    self.modal_ime_composition = None;
                    composing = false;
                    events.push(event);
                }
                event if is_ime_disabled_event(event) && composing => {
                    self.modal_ime_composition = None;
                    composing = false;
                    events.push(event.clone());
                }
                egui::Event::Key {
                    key: Key::Enter | Key::Escape,
                    pressed: true,
                    ..
                } if composing => (),
                _ => events.push(event),
            }
        }
        raw.events = events;
    }

    /// Drops cursor keys that the platform forwards while a preedit is still open.
    ///
    /// macOS passes a key on when its IME does not use it; egui would then move the
    /// caret out of the preedit and the commit would be inserted beside stale text.
    fn drop_cursor_keys_during_preedit(&self, raw: &mut egui::RawInput) {
        let mut composing = self
            .ime_composition
            .as_ref()
            .is_some_and(|state| state.preedit_active);
        raw.events.retain(|event| {
            match event {
                egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) => {
                    composing = !text.is_empty();
                }
                egui::Event::Ime(egui::ImeEvent::Commit(_)) => composing = false,
                event if is_ime_disabled_event(event) => composing = false,
                egui::Event::Key {
                    key, pressed: true, ..
                } if composing && is_preedit_cursor_key(*key) => return false,
                _ => (),
            }
            true
        });
    }

    /// Drops one old terminal event so it cannot enter a newer editor session.
    fn drop_late_ime_events(&mut self, raw: &mut egui::RawInput) {
        let Some(_) = self.late_ime_editor else {
            return;
        };
        let mut stale = true;
        let mut kept = Vec::with_capacity(raw.events.len());
        for event in raw.events.drain(..) {
            if stale && is_empty_preedit(&event) {
                continue;
            }
            if stale && is_ime_terminal_event(&event) {
                stale = false;
                continue;
            }
            if stale
                && matches!(&event, egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) if !text.is_empty())
            {
                stale = false;
            }
            kept.push(event);
        }
        if !stale {
            self.late_ime_editor = None;
        }
        raw.events = kept;
    }

    /// Drops a pending unmark snapshot when normal input moves on first.
    fn expire_unmarked_ime_on_input(&mut self, ctx: &egui::Context, raw: &egui::RawInput) {
        let Some(snapshot) = self
            .ime_composition
            .as_ref()
            .filter(|state| !state.preedit_active)
            .cloned()
        else {
            return;
        };
        for event in &raw.events {
            if is_ime_terminal_event(event) {
                return;
            }
            match event {
                egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) if !text.is_empty() => {
                    return;
                }
                egui::Event::Ime(_) => (),
                _ if interrupts_unmarked_ime(event) && !is_escape_key(event) => {
                    self.ime_composition = None;
                    self.late_ime_editor = Some(snapshot.editor_id);
                    self.ime_interrupt_pending = true;
                    self.restore_ime_snapshot(ctx, &snapshot);
                    return;
                }
                _ => (),
            }
        }
    }
}
