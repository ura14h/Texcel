use super::*;

mod permissions;
#[path = "commands/shared_menu_target.rs"]
mod shared_menu_target;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DeferredTextTarget {
    focus: Id,
    document_id: Option<DocumentId>,
    table_id: Option<u64>,
    edit_session: Option<(DocumentId, u64, Option<CellAddress>)>,
    modal: bool,
    modal_session: Option<(u8, u64, u64)>,
}

#[derive(Clone, Debug)]
pub(super) struct DeferredTextCommand {
    command: AppCommand,
    target: DeferredTextTarget,
}

#[derive(Clone, Debug)]
pub(super) struct DeferredAppCommand {
    pub(super) command: AppCommand,
    pub(super) owner: InputOwner,
    pub(super) blocked: bool,
    pub(super) focus: Option<Id>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub(super) native_target: Option<crate::native_menu::NativeActionTarget>,
}

/// Names the user actions shared by menus, shortcuts, and the grid.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum AppCommand {
    Undo,
    Redo,
    CopyAs(CopyFormat),
    CutAs(CopyFormat),
    PasteText { text: String, options: PasteOptions },
    PasteSystem(PasteOptions),
    OpenPasteSpecial,
    OpenCopyOptions,
    ClearSelection,
    SelectAll,
}

/// Records which part of the window owns the current input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum InputOwner {
    Grid,
    CellEditor,
    OtherText,
    Modal,
    Ime,
    Chrome,
}

impl TexcelApp {
    /// Runs one command after the input widget has consumed its frame updates.
    pub(super) fn dispatch_command(
        &mut self,
        command: AppCommand,
        owner: InputOwner,
        blocked: bool,
        text_focus: Option<Id>,
        ctx: &egui::Context,
    ) {
        if !self.can_execute_command(&command, owner, blocked) {
            return;
        }
        if matches!(owner, InputOwner::CellEditor | InputOwner::OtherText) {
            self.defer_text_command(command, owner, text_focus, ctx);
            return;
        }
        match command {
            AppCommand::Undo => self.apply_undo(false),
            AppCommand::Redo => self.apply_undo(true),
            AppCommand::CopyAs(format) => {
                self.copy_selected_grid(ctx, format);
            }
            AppCommand::CutAs(format) => {
                // Like Excel, the cut cells stay until a paste in the same table moves them.
                if self.copy_selected_grid(ctx, format) {
                    self.mark_clipboard_as_cut();
                }
            }
            AppCommand::PasteText { text, options } => {
                self.paste_selected_grid(text, options);
            }
            AppCommand::PasteSystem(options) => match crate::clipboard::read_system_text() {
                Ok(text) => self.paste_selected_grid(text, options),
                Err(error) => self.status = Some(error.0),
            },
            AppCommand::OpenPasteSpecial => {
                self.paste_special_dialog = Some(PasteOptions::default());
            }
            AppCommand::OpenCopyOptions => {
                self.copy_options_dialog = Some(CopyOptionsDialog::default());
            }
            AppCommand::ClearSelection => self.clear_selected_range(),
            AppCommand::SelectAll => self.select_table_all(),
        }
    }

    /// Queues standard text editing events for the widget that owned the menu input.
    fn defer_text_command(
        &mut self,
        command: AppCommand,
        owner: InputOwner,
        text_focus: Option<Id>,
        ctx: &egui::Context,
    ) {
        let Some(focus) = text_focus else {
            return;
        };
        let document = self.workspace.active_document();
        let table_id = document.and_then(|document| document.active_table().map(|table| table.id));
        let target = DeferredTextTarget {
            focus,
            document_id: document.map(|document| document.id),
            table_id,
            edit_session: (owner == InputOwner::CellEditor)
                .then(|| {
                    self.edit_document_id
                        .map(|document_id| (document_id, self.edit.session_id, self.edit.address))
                })
                .flatten(),
            modal: owner == InputOwner::OtherText
                && self.active_modal_text_input_focus == Some(focus),
            modal_session: self.modal_text_session(),
        };
        let command = match command {
            AppCommand::PasteSystem(_) => match crate::clipboard::read_system_text() {
                Ok(text) => AppCommand::PasteText {
                    text,
                    options: PasteOptions::default(),
                },
                Err(error) => {
                    self.status = Some(error.0);
                    return;
                }
            },
            command => command,
        };
        self.deferred_text_commands
            .push(DeferredTextCommand { command, target });
        self.grid_focused = false;
        self.suppress_grid_keyboard = true;
        ctx.request_repaint();
    }

    /// Delivers a pending command to the same focused text widget on the next frame.
    pub(super) fn apply_deferred_text_commands(&mut self, ctx: &egui::Context) {
        let commands = std::mem::take(&mut self.deferred_text_commands);
        if commands.is_empty() {
            return;
        }
        #[cfg(any(test, not(any(target_os = "macos", target_os = "windows"))))]
        let from_shared_edit_menu = commands
            .iter()
            .any(|deferred| self.shared_edit_menu_target_matches(deferred.target));
        let current_text_event = current_input_owns_text_entry(ctx);
        if self.edit.composing || current_text_event {
            return;
        }
        #[cfg(any(test, not(any(target_os = "macos", target_os = "windows"))))]
        if from_shared_edit_menu
            && let Some(target) = self.shared_edit_menu_target
            && let Some(cursor_range) = match target {
                SharedEditMenuTarget::CellEditor { cursor_range, .. }
                | SharedEditMenuTarget::NameBox { cursor_range, .. } => cursor_range,
            }
        {
            // Restore the real selection before the delayed text command reaches the field.
            let mut state = egui::TextEdit::load_state(ctx, target.focus()).unwrap_or_default();
            state.cursor.set_char_range(Some(cursor_range));
            egui::TextEdit::store_state(ctx, target.focus(), state);
        }
        let mut events = Vec::with_capacity(commands.len());
        let mut target_focus = None;
        for deferred in commands {
            if !self.deferred_text_target_is_live(deferred.target, ctx) {
                continue;
            }
            target_focus = Some(deferred.target.focus);
            let event = match deferred.command {
                AppCommand::Undo => Some(text_command_key(ctx, Key::Z, false)),
                AppCommand::Redo => Some(text_command_key(ctx, Key::Z, true)),
                AppCommand::CopyAs(_) => Some(egui::Event::Copy),
                AppCommand::CutAs(_) => Some(egui::Event::Cut),
                AppCommand::PasteText { text, .. } => Some(egui::Event::Paste(text)),
                AppCommand::PasteSystem(_) => None,
                AppCommand::OpenPasteSpecial => None,
                AppCommand::OpenCopyOptions => None,
                AppCommand::ClearSelection => Some(text_plain_key(Key::Backspace)),
                AppCommand::SelectAll => Some(text_command_key(ctx, Key::A, false)),
            };
            if let Some(event) = event {
                events.push(event);
            }
        }
        if let Some(target_focus) = target_focus {
            ctx.memory_mut(|memory| memory.request_focus(target_focus));
        }
        ctx.input_mut(|input| input.events.extend(events));
        #[cfg(any(test, not(any(target_os = "macos", target_os = "windows"))))]
        if from_shared_edit_menu {
            self.shared_edit_menu_target = None;
        }
    }

    /// Identifies which current dialog owns its active text widget.
    pub(super) fn modal_text_session(&self) -> Option<(u8, u64, u64)> {
        if let Some(dialog) = &self.search_dialog {
            return Some((1, dialog.document_id, dialog.table_id));
        }
        if let Some(dialog) = &self.layout_dialog {
            return Some((2, dialog.document_id, dialog.table_id));
        }
        if let Some(dialog) = &self.data_edit_dialog {
            return Some((4, dialog.document_id, dialog.table_id));
        }
        if let Some(dialog) = &self.cell_link_dialog {
            return Some((5, dialog.document_id, dialog.table_id));
        }
        if let Some((document_id, pending)) = self
            .pending_csv_opens
            .iter()
            .find(|(_, pending)| pending.prompt.is_some())
        {
            return Some((6, *document_id, pending.job_id));
        }
        if self.paste_special_dialog.is_some() {
            return self.workspace.active_document().and_then(|document| {
                document
                    .active_table()
                    .map(|table| (8, document.id, table.id))
            });
        }
        if self.long_text_panel_open
            && self.edit.target == Some(crate::document::EditTarget::DetailPanel)
            && self.edit.is_editing()
            && !self.settings_window_open
            && !self.about_dialog_open
            && self.active_import_report.is_none()
            && self.active_export_confirmation.is_none()
            && self.export_dialog.is_none()
            && !self.app_close_dialog_open
            && !self.app_close_discard_pending
            && self.pending_close_document.is_none()
            && self.recovery_restore_pending.is_empty()
            && self.save_conflict_dialog.is_none()
        {
            return self
                .edit_document_id
                .map(|document_id| (9, document_id, self.edit.session_id));
        }
        None
    }
}

/// Leaves current keyboard, pointer, and IME input with the widget that received it.
fn current_input_owns_text_entry(ctx: &egui::Context) -> bool {
    ctx.input(|input| {
        input.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Ime(_)
                    | egui::Event::Text(_)
                    | egui::Event::Paste(_)
                    | egui::Event::Key { pressed: true, .. }
                    | egui::Event::PointerButton { pressed: true, .. }
                    | egui::Event::Touch {
                        phase: egui::TouchPhase::Start,
                        ..
                    }
            )
        })
    })
}

/// Names commands that should edit the currently focused text widget.
fn is_standard_text_command(command: &AppCommand) -> bool {
    matches!(
        command,
        AppCommand::Undo
            | AppCommand::Redo
            | AppCommand::CopyAs(CopyFormat::DisplayTsv)
            | AppCommand::CutAs(CopyFormat::DisplayTsv)
            | AppCommand::PasteText { .. }
            | AppCommand::PasteSystem(_)
            | AppCommand::ClearSelection
            | AppCommand::SelectAll
    )
}

/// Builds a text-widget shortcut using the current platform's command key.
fn text_command_key(ctx: &egui::Context, key: Key, shift: bool) -> egui::Event {
    let is_mac = ctx.os().is_mac();
    let mut modifiers = egui::Modifiers::COMMAND;
    if is_mac {
        modifiers.mac_cmd = true;
    } else {
        modifiers.ctrl = true;
    }
    modifiers.shift = shift;
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// Builds a plain editing key for the focused text widget.
fn text_plain_key(key: Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

#[cfg(test)]
#[path = "commands_test.rs"]
mod tests;
