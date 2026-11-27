use super::*;

use super::commands::{AppCommand, InputOwner};
#[path = "keyboard_axis_space.rs"]
mod axis_space;

impl TexcelApp {
    // Handle grid keyboard input while respecting editor and navigation state.
    pub(super) fn process_keyboard(&mut self, ctx: &egui::Context, navigation_blocked: bool) {
        self.validate_input_sequence_context();
        if self
            .axis_space_text_pending
            .is_some_and(|target| self.current_axis_space_target(ctx) != Some(target))
        {
            self.axis_space_text_pending = None;
        }
        if self.edit.is_editing()
            && self.edit.target == Some(crate::document::EditTarget::DetailPanel)
        {
            return;
        }
        if navigation_blocked || self.edit.composing || self.suppress_grid_keyboard {
            if self.axis_space_text_pending.is_some() && ctx.input(|input| !input.events.is_empty())
            {
                self.axis_space_text_pending = None;
            }
            return;
        }
        if self
            .edit_document_id
            .is_some_and(|document_id| self.document_bulk_edit_pending(document_id))
        {
            if self.axis_space_text_pending.is_some() && ctx.input(|input| !input.events.is_empty())
            {
                self.axis_space_text_pending = None;
            }
            return;
        }
        let bulk_edit_locked = self
            .workspace
            .active_document()
            .is_some_and(|document| self.document_bulk_edit_pending(document.id));
        if self.edit.is_editing() {
            if self.axis_space_text_pending.is_some() && ctx.input(|input| !input.events.is_empty())
            {
                self.axis_space_text_pending = None;
            }
            let (key, shift) = ctx.input(|input| {
                if let Some((key, shift)) = input.events.iter().find_map(editing_key_from_event) {
                    (Some(key), shift)
                } else if input.key_pressed(Key::Enter) {
                    let key = if input.modifiers.alt {
                        InputKey::AltEnter
                    } else {
                        InputKey::Enter
                    };
                    (Some(key), input.modifiers.shift)
                } else if input.key_pressed(Key::Tab) {
                    (Some(InputKey::Tab), input.modifiers.shift)
                } else if input.key_pressed(Key::Escape) {
                    (Some(InputKey::Escape), false)
                } else if input.key_pressed(Key::F2) {
                    (Some(InputKey::F2), false)
                } else {
                    (None, false)
                }
            });
            if let Some(key) = key {
                let action = self.edit.key_action(key, shift);
                match action {
                    InputAction::CommitAndMoveDown => {
                        self.commit_edit_with_tab(Some((1, 0)), false, key == InputKey::Enter)
                    }
                    InputAction::CommitAndMoveUp => {
                        self.commit_edit_with_tab(Some((-1, 0)), false, key == InputKey::Enter)
                    }
                    InputAction::CommitAndMoveRight => {
                        self.commit_edit_with_tab(Some((0, 1)), key == InputKey::Tab, false)
                    }
                    InputAction::CommitAndMoveLeft => {
                        self.commit_edit_with_tab(Some((0, -1)), key == InputKey::Tab, false)
                    }
                    InputAction::Cancel => self.cancel_edit(),
                    InputAction::ToggleMode => {
                        if self.formula_reference_edit_is_current() {
                            self.toggle_formula_reference_mode();
                        } else {
                            self.edit.mode = if self.edit.mode == EditMode::Input {
                                EditMode::Edit
                            } else {
                                EditMode::Input
                            };
                        }
                    }
                    InputAction::InsertLineBreak => self.insert_line_break(ctx),
                    InputAction::PropagateToIme
                    | InputAction::MoveSelection
                    | InputAction::Ignore => (),
                }
            }
            return;
        }

        if !self.grid_focused {
            return;
        }
        // A focused text field (name box, formula bar, dialog) owns typed text even when
        // the grid still holds a focus request left over from an earlier command.
        if ctx.text_edit_focused() {
            return;
        }
        // Esc ends the copy outline before other grid keys; the clipboard keeps its data.
        if ctx.input(|input| input.key_pressed(Key::Escape)) && self.clear_clipboard_mark() {
            return;
        }
        let clipboard_event = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Copy => Some((0_u8, None)),
                egui::Event::Cut => Some((1_u8, None)),
                egui::Event::Paste(text) => Some((2_u8, Some(text.clone()))),
                _ => None,
            })
        });
        let paste_kind = ctx.input(|input| paste_shortcut_kind(input.modifiers));
        if let Some((kind, text)) = clipboard_event {
            self.axis_space_text_pending = None;
            if bulk_edit_locked && kind != 0 {
                return;
            }
            match kind {
                0 => {
                    self.dispatch_command(
                        AppCommand::CopyAs(CopyFormat::DisplayTsv),
                        InputOwner::Grid,
                        false,
                        None,
                        ctx,
                    );
                }
                1 => {
                    self.dispatch_command(
                        AppCommand::CutAs(CopyFormat::DisplayTsv),
                        InputOwner::Grid,
                        false,
                        None,
                        ctx,
                    );
                }
                2 if paste_kind == PasteShortcut::Special => self.dispatch_command(
                    AppCommand::OpenPasteSpecial,
                    InputOwner::Grid,
                    false,
                    None,
                    ctx,
                ),
                2 => {
                    if let Some(text) = text {
                        let options = if paste_kind == PasteShortcut::Text {
                            PasteOptions::TEXT
                        } else {
                            PasteOptions::default()
                        };
                        self.dispatch_command(
                            AppCommand::PasteText { text, options },
                            InputOwner::Grid,
                            false,
                            None,
                            ctx,
                        );
                    }
                }
                _ => (),
            }
            return;
        }
        let (pressed_key, modifiers) = ctx.input(|input| {
            let key = [
                Key::F2,
                Key::N,
                Key::ArrowUp,
                Key::ArrowDown,
                Key::ArrowLeft,
                Key::ArrowRight,
                Key::Enter,
                Key::Tab,
                Key::Delete,
                Key::Backspace,
                Key::Z,
                Key::D,
                Key::R,
                Key::Home,
                Key::End,
                Key::A,
                Key::PageUp,
                Key::PageDown,
                Key::Space,
                Key::Y,
            ]
            .into_iter()
            .find(|key| {
                input.key_pressed(*key)
                    && (!bulk_edit_locked
                        || matches!(
                            key,
                            Key::ArrowUp
                                | Key::ArrowDown
                                | Key::ArrowLeft
                                | Key::ArrowRight
                                | Key::Enter
                                | Key::Tab
                                | Key::Home
                                | Key::End
                                | Key::A
                                | Key::PageUp
                                | Key::PageDown
                                | Key::Space
                        ))
            });
            let modifiers = key
                .filter(|key| *key == Key::Space)
                .and_then(|_| {
                    input.events.iter().find_map(|event| match event {
                        egui::Event::Key {
                            key: Key::Space,
                            pressed: true,
                            modifiers,
                            ..
                        } => Some(*modifiers),
                        _ => None,
                    })
                })
                .unwrap_or(input.modifiers);
            (key, modifiers)
        });
        let shift = modifiers.shift;
        let command = modifiers.command;
        let control = modifiers.ctrl;
        let alt = modifiers.alt;
        let consume_axis_space_text =
            matches!(pressed_key, Some(Key::Space)) && ((control && !shift) || (shift && !control));
        if matches!(
            pressed_key,
            Some(
                Key::F2
                    | Key::ArrowUp
                    | Key::ArrowDown
                    | Key::ArrowLeft
                    | Key::ArrowRight
                    | Key::Enter
                    | Key::Tab
                    | Key::Delete
                    | Key::Backspace
                    | Key::Home
                    | Key::End
                    | Key::PageUp
                    | Key::PageDown
            )
        ) && !matches!(pressed_key, Some(Key::Tab | Key::Enter))
        {
            self.input_sequence_origin = None;
        }
        match pressed_key {
            Some(Key::F2) => self.begin_selected_edit(),
            Some(Key::ArrowUp) if command => self.move_to_data_region(-1, 0, shift),
            Some(Key::ArrowDown) if command => self.move_to_data_region(1, 0, shift),
            Some(Key::ArrowLeft) if command => self.move_to_data_region(0, -1, shift),
            Some(Key::ArrowRight) if command => self.move_to_data_region(0, 1, shift),
            Some(Key::Home) if command => self.select_boundary(CellAddress::default(), shift),
            Some(Key::End) if command => {
                let target = self
                    .workspace
                    .active_document()
                    .and_then(|document| document.active_table())
                    .and_then(crate::document::Table::last_used_cell)
                    .unwrap_or_default();
                self.select_boundary(target, shift);
            }
            Some(Key::Home) => self.move_row_boundary(false, shift),
            Some(Key::End) => self.move_row_boundary(true, shift),
            Some(Key::PageUp) if alt && cfg!(target_os = "windows") => {
                self.move_page(false, -1, shift)
            }
            Some(Key::PageDown) if alt && cfg!(target_os = "windows") => {
                self.move_page(false, 1, shift)
            }
            Some(Key::PageUp) => self.move_page(true, -1, shift),
            Some(Key::PageDown) => self.move_page(true, 1, shift),
            Some(Key::Space) if control && !shift => self.select_active_axis(false),
            Some(Key::Space) if shift && !control => self.select_active_axis(true),
            Some(Key::A) if command => {
                self.dispatch_command(AppCommand::SelectAll, InputOwner::Grid, false, None, ctx)
            }
            Some(Key::ArrowUp) => self.move_selection(-1, 0, shift),
            Some(Key::ArrowDown) => self.move_selection(1, 0, shift),
            Some(Key::ArrowLeft) => self.move_selection(0, -1, shift),
            Some(Key::ArrowRight) => self.move_selection(0, 1, shift),
            Some(Key::Enter) if self.move_waiting_enter_input(shift) => (),
            Some(Key::Enter) if self.cycle_selection_cell(shift) => (),
            Some(Key::Enter) => self.move_selection(if shift { -1 } else { 1 }, 0, false),
            Some(Key::Tab) if self.move_waiting_tab_input(shift) => (),
            Some(Key::Tab) => self.move_selection(0, if shift { -1 } else { 1 }, false),
            Some(Key::Delete | Key::Backspace) => self.dispatch_command(
                AppCommand::ClearSelection,
                InputOwner::Grid,
                false,
                None,
                ctx,
            ),
            Some(Key::N) if command && shift => {
                self.request_structure_action(StructureAction::AddTable)
            }
            Some(Key::D) if command && shift => {
                self.request_structure_action(StructureAction::DuplicateTable)
            }
            Some(Key::D) if command => self.request_structure_action(StructureAction::FillDown),
            Some(Key::R) if command => self.request_structure_action(StructureAction::FillRight),
            Some(Key::Z) if command && shift => {
                self.dispatch_command(AppCommand::Redo, InputOwner::Grid, false, None, ctx)
            }
            Some(Key::Y) if command && cfg!(target_os = "windows") => {
                self.dispatch_command(AppCommand::Redo, InputOwner::Grid, false, None, ctx)
            }
            Some(Key::Z) if command => {
                self.dispatch_command(AppCommand::Undo, InputOwner::Grid, false, None, ctx)
            }
            _ => (),
        }

        // Ignore only the text character produced by the handled axis shortcut.
        let new_axis_space_target = consume_axis_space_text
            .then(|| self.current_axis_space_target(ctx))
            .flatten();
        let mut pending_target = self.axis_space_text_pending;
        let typed_text = ctx.input_mut(|input| {
            let mut text = String::new();
            input.events.retain(|event| {
                let is_axis_space_key = new_axis_space_target.is_some()
                    && matches!(
                        event,
                        egui::Event::Key {
                            key: Key::Space,
                            pressed: true,
                            modifiers,
                            ..
                        } if (modifiers.ctrl && !modifiers.shift)
                            || (modifiers.shift && !modifiers.ctrl)
                    );
                if is_axis_space_key {
                    pending_target = new_axis_space_target;
                    return true;
                }
                let is_shortcut_space_text = pending_target.is_some()
                    && matches!(event, egui::Event::Text(part) if part == " ");
                pending_target = None;
                if is_shortcut_space_text {
                    return false;
                }
                if let egui::Event::Text(part) = event {
                    text.push_str(part);
                }
                true
            });
            (!text.is_empty()).then_some(text)
        });
        self.axis_space_text_pending = pending_target;
        if let Some(text) = typed_text {
            self.begin_selected_replacing(text);
        }
    }

    // Move or extend the selection to a row or column boundary.
    pub(super) fn select_boundary(&mut self, target: CellAddress, extend: bool) {
        let target = self.workspace.active_document().and_then(|document| {
            let table = document.active_table()?;
            document.view(table.id)?;
            Some(CellAddress {
                row: target.row.min(MAX_ROWS - 1),
                column: target.column.min(MAX_COLUMNS - 1),
            })
        });
        let Some(target) = target else {
            return;
        };
        let Some(document) = self.workspace.active_document_mut() else {
            return;
        };
        let Some(table) = document.active_table() else {
            return;
        };
        let Some(view) = document.view_mut(table.id) else {
            return;
        };
        if extend {
            if view.selection.anchor != view.active_cell() {
                view.selection.anchor = view.active_cell();
            }
            view.selection.head = target;
        } else {
            view.set_selection(CellRange::single(target));
        }
        self.request_grid_focus = true;
        if extend {
            let Some((document_id, table_id)) = self
                .workspace
                .active_document()
                .and_then(|document| document.active_table().map(|table| (document.id, table.id)))
            else {
                return;
            };
            self.request_scroll_to_selection = false;
            self.request_scroll_to_address = Some((document_id, table_id, target));
        } else {
            self.request_scroll_to_selection = true;
        }
    }

    // Select the active table contents.
    pub(super) fn select_table_all(&mut self) {
        let Some(table_id) = self.workspace.active_document().and_then(|document| {
            let table = document.active_table()?;
            Some(table.id)
        }) else {
            return;
        };
        if let Some(document) = self.workspace.active_document_mut()
            && let Some(view) = document.view_mut(table_id)
        {
            view.set_selection(CellRange {
                anchor: CellAddress::default(),
                head: CellAddress {
                    row: MAX_ROWS - 1,
                    column: MAX_COLUMNS - 1,
                },
            });
        }
        self.request_scroll_to_selection = true;
    }

    // Move the selection to the edge of a contiguous data region.
    pub(super) fn move_to_data_region(&mut self, row_delta: i32, column_delta: i32, extend: bool) {
        let Some(document) = self.workspace.active_document() else {
            return;
        };
        let Some(table) = document.active_table() else {
            return;
        };
        let Some(view) = document.view(table.id) else {
            return;
        };
        let target = table.data_region_edge(
            if extend {
                if view.selection.anchor == view.active_cell() {
                    view.selection.head
                } else {
                    view.active_cell()
                }
            } else {
                view.active_cell()
            },
            row_delta as isize,
            column_delta as isize,
        );
        self.select_boundary(target, extend);
    }
}

/// Which paste a Ctrl/Cmd+V chord asks for, read from the held modifier keys.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PasteShortcut {
    Normal,
    /// Ctrl/Cmd+Shift+V pastes plain text.
    Text,
    /// Ctrl+Alt+V, or Cmd+Ctrl+V on macOS, opens Paste Special.
    Special,
}

/// Classifies the modifiers held with V; egui reports every such chord as one paste.
pub(super) fn paste_shortcut_kind(modifiers: egui::Modifiers) -> PasteShortcut {
    let special = if cfg!(target_os = "macos") {
        modifiers.mac_cmd && modifiers.ctrl
    } else {
        modifiers.ctrl && modifiers.alt
    };
    if special {
        PasteShortcut::Special
    } else if modifiers.shift {
        PasteShortcut::Text
    } else {
        PasteShortcut::Normal
    }
}
