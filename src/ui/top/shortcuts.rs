use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ui) enum AppShortcut {
    NewDocument,
    OpenDocument,
    Save,
    SaveAs,
    CloseDocument,
    Search,
    Replace,
    ToggleDisplayMode,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Format(CellFormatAction),
}

impl TexcelApp {
    /// Collects one app shortcut without taking keys from editors or IME.
    pub(super) fn take_app_shortcut(
        &self,
        ctx: &egui::Context,
        navigation_blocked: bool,
    ) -> Option<AppShortcut> {
        // Leaves modal and composing input with the editor that owns it.
        if navigation_blocked || self.edit.composing {
            return None;
        }
        let focused = ctx.memory(|memory| memory.focused());
        let app_editor_focused = self.edit.is_editing()
            && self.edit_document_id.is_some_and(|document_id| {
                focused == Some(editor_id(document_id, self.edit.session_id))
            });
        // App shortcuts may commit Texcel's own draft, but not another text field.
        if ctx.text_edit_focused() && !app_editor_focused {
            return None;
        }

        let is_mac = ctx.os().is_mac();
        // Consume exact shortcut events and discard repeats before widgets see them.
        ctx.input_mut(|input| {
            let shortcut = input.events.iter().find_map(|event| {
                shortcut_event(event, is_mac)
                    .and_then(|(shortcut, repeat)| (!repeat).then_some(shortcut))
            });
            input
                .events
                .retain(|event| !is_shortcut_event(event, is_mac));
            shortcut
        })
    }

    /// Applies a collected shortcut after this frame has updated active editors.
    pub(super) fn apply_collected_app_shortcut(
        &mut self,
        shortcut: Option<AppShortcut>,
        navigation_blocked: bool,
        ctx: &egui::Context,
    ) {
        let Some(shortcut) = shortcut else {
            return;
        };
        let mut state = TopActionState::default();
        match shortcut {
            AppShortcut::NewDocument => state.create_document = true,
            AppShortcut::OpenDocument => state.open_file = true,
            AppShortcut::Save => state.save_file = true,
            AppShortcut::SaveAs => state.save_as = true,
            AppShortcut::CloseDocument => {
                state.close_document = self.workspace.active_document().map(|doc| doc.id);
            }
            AppShortcut::Search => state.open_search_dialog = true,
            AppShortcut::Replace => state.open_replace_dialog = true,
            AppShortcut::ToggleDisplayMode => {
                if let Some(document) = self.workspace.active_document()
                    && let Some(table) = document.active_table()
                {
                    state.display_state = Some((document.id, table.id, document.display_mode));
                    state.table_display_mode = Some(match document.display_mode {
                        crate::document::DocumentDisplayMode::Live => {
                            crate::document::DocumentDisplayMode::Source
                        }
                        crate::document::DocumentDisplayMode::Source => {
                            crate::document::DocumentDisplayMode::Live
                        }
                    });
                }
            }
            AppShortcut::ZoomIn => {
                state.zoom_action = Some(step_sheet_zoom(self.sheet_zoom.0, true))
            }
            AppShortcut::ZoomOut => {
                state.zoom_action = Some(step_sheet_zoom(self.sheet_zoom.0, false))
            }
            AppShortcut::ZoomReset => state.zoom_action = Some(1.0),
            AppShortcut::Format(action) => {
                let can_format = self.workspace.active_document().is_some_and(|document| {
                    matches!(
                        document.source.as_deref(),
                        Some(SourceDocument::Markdown(_) | SourceDocument::Csv(_))
                    ) && !self.cell_edit_locked(document.id)
                        && !self.close_after_save.contains(&document.id)
                });
                if can_format {
                    state.can_markdown_modifiers = true;
                    state.format_action = Some(action);
                }
            }
        }
        self.apply_top_actions(state, navigation_blocked, ctx);
    }
}

/// Maps a supported command combination and reports whether it is a key repeat.
fn shortcut_event(event: &egui::Event, is_mac: bool) -> Option<(AppShortcut, bool)> {
    let egui::Event::Key {
        key,
        pressed: true,
        repeat,
        modifiers,
        ..
    } = event
    else {
        return None;
    };
    let shortcut = match key {
        Key::S if exact_command_modifiers(*modifiers, true, is_mac) => Some(AppShortcut::SaveAs),
        Key::N if exact_command_modifiers(*modifiers, false, is_mac) => {
            Some(AppShortcut::NewDocument)
        }
        Key::O if exact_command_modifiers(*modifiers, false, is_mac) => {
            Some(AppShortcut::OpenDocument)
        }
        Key::S if exact_command_modifiers(*modifiers, false, is_mac) => Some(AppShortcut::Save),
        Key::W if exact_command_modifiers(*modifiers, false, is_mac) => {
            Some(AppShortcut::CloseDocument)
        }
        Key::F if exact_command_modifiers(*modifiers, false, is_mac) => Some(AppShortcut::Search),
        Key::H if exact_command_modifiers(*modifiers, is_mac, is_mac) => Some(AppShortcut::Replace),
        Key::Plus
            if exact_command_modifiers(*modifiers, false, is_mac)
                || exact_command_modifiers(*modifiers, true, is_mac) =>
        {
            Some(AppShortcut::ZoomIn)
        }
        Key::Equals if exact_command_modifiers(*modifiers, true, is_mac) => {
            Some(AppShortcut::ZoomIn)
        }
        Key::Minus if exact_command_modifiers(*modifiers, false, is_mac) => {
            Some(AppShortcut::ZoomOut)
        }
        Key::Num0 if exact_command_modifiers(*modifiers, false, is_mac) => {
            Some(AppShortcut::ZoomReset)
        }
        Key::Slash if exact_command_modifiers(*modifiers, false, is_mac) => {
            Some(AppShortcut::ToggleDisplayMode)
        }
        Key::B if exact_command_modifiers(*modifiers, false, is_mac) => {
            Some(AppShortcut::Format(CellFormatAction::Bold))
        }
        Key::I if exact_command_modifiers(*modifiers, false, is_mac) => {
            Some(AppShortcut::Format(CellFormatAction::Italic))
        }
        Key::X if exact_command_modifiers(*modifiers, true, is_mac) => {
            Some(AppShortcut::Format(CellFormatAction::Strike))
        }
        Key::E if exact_command_modifiers(*modifiers, false, is_mac) => {
            Some(AppShortcut::Format(CellFormatAction::Code))
        }
        _ => None,
    }?;
    Some((shortcut, *repeat))
}

/// Requires the platform command key and rejects every extra modifier.
fn exact_command_modifiers(modifiers: egui::Modifiers, shift: bool, is_mac: bool) -> bool {
    if modifiers.alt || modifiers.shift != shift || !modifiers.command {
        return false;
    }
    if is_mac {
        modifiers.mac_cmd && !modifiers.ctrl
    } else {
        modifiers.ctrl && !modifiers.mac_cmd
    }
}

/// Identifies any supported command event, including OS key repeats.
fn is_shortcut_event(event: &egui::Event, is_mac: bool) -> bool {
    shortcut_event(event, is_mac).is_some()
}

#[cfg(test)]
#[path = "shortcuts_test.rs"]
mod tests;
