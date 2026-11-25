use super::*;
use std::collections::BTreeMap;

#[path = "native/accelerators.rs"]
mod accelerators;
#[path = "native/adapter.rs"]
mod adapter;
#[path = "native/target.rs"]
mod target;
use self::accelerators::native_accelerator_matches;
pub(in crate::ui) use self::target::native_targets_match;

impl TexcelApp {
    /// Removes key events already handled by the native menu accelerator table.
    pub(in crate::ui) fn suppress_native_menu_accelerators(
        &self,
        ctx: &egui::Context,
        events: &[crate::native_menu::NativeMenuEvent],
    ) {
        if events.is_empty() {
            return;
        }
        let is_mac = ctx.os().is_mac();
        ctx.input_mut(|input| {
            input.events.retain(|event| {
                let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = event
                else {
                    return true;
                };
                !events
                    .iter()
                    .any(|event| native_accelerator_matches(&event.id, *key, *modifiers, is_mac))
            });
        });
    }

    /// Applies a native action through the shared menu action handler.
    pub(in crate::ui) fn apply_native_menu_event(
        &self,
        event: crate::native_menu::NativeMenuEvent,
        state: &mut TopActionState,
        navigation_blocked: bool,
    ) {
        if event.id.starts_with("file.recent.") {
            if !navigation_blocked && let Some(path) = event.recent_path {
                state.open_recent_paths.push(path);
            }
            return;
        }
        if let Some(document_id) = event
            .id
            .strip_prefix("window.activate.")
            .and_then(|id| id.parse().ok())
        {
            state.focus_document = Some(document_id);
            return;
        }
        if !self.menu_item_enabled(&event.id, state, navigation_blocked) {
            return;
        }
        let command_count = state.commands.len();
        self.apply_menu_action_id(&event.id, state);
        if state.commands.len() > command_count {
            state.preserve_text_focus = matches!(
                state.command_owner,
                Some(InputOwner::OtherText | InputOwner::CellEditor)
            );
            state.native_command_target = Some(event.target);
        }
    }

    /// Builds current enable, check, and recent-path labels for the OS menu.
    pub(in crate::ui) fn native_menu_states(
        &self,
        state: &TopActionState,
        navigation_blocked: bool,
    ) -> BTreeMap<String, crate::native_menu::ItemState> {
        let mut result = crate::command_menu::action_ids()
            .iter()
            .map(|id| {
                let checked = self.menu_item_checked(id, state);
                (
                    (*id).to_owned(),
                    crate::native_menu::ItemState {
                        enabled: self.menu_item_enabled(id, state, navigation_blocked),
                        checked,
                        label: None,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        for (index, path) in self.recent_files.iter().enumerate() {
            let id = format!("file.recent.{index}");
            result.insert(
                id,
                crate::native_menu::ItemState {
                    enabled: !navigation_blocked,
                    checked: None,
                    label: Some(path.file_name().map_or_else(
                        || path.display().to_string(),
                        |name| name.to_string_lossy().into_owned(),
                    )),
                },
            );
        }
        result
    }

    /// Applies the same command eligibility checks used by the in-window menus.
    pub(super) fn menu_item_enabled(
        &self,
        id: &str,
        state: &TopActionState,
        blocked: bool,
    ) -> bool {
        let owner = state.command_owner.unwrap_or(InputOwner::Chrome);
        let command_enabled = |command: AppCommand| {
            self.can_execute_command(&command, owner, blocked)
                && self.native_owner_is_live(state, blocked)
        };
        match id {
            "file.open" => !blocked,
            "window.minimize" | "window.zoom" => true,
            "file.new" => !blocked,
            "file.save" | "file.save_as" => !blocked && self.workspace.active_document().is_some(),
            "file.close" => !blocked && self.workspace.active_document().is_some(),
            "app.settings" | "help.open" | "app.quit" => !blocked,
            "edit.undo" => command_enabled(AppCommand::Undo),
            "edit.redo" => command_enabled(AppCommand::Redo),
            "edit.cut" => command_enabled(AppCommand::CutAs(CopyFormat::DisplayTsv)),
            "edit.copy" => command_enabled(AppCommand::CopyAs(CopyFormat::DisplayTsv)),
            "edit.copy_as" => command_enabled(AppCommand::OpenCopyOptions),
            "edit.paste" => command_enabled(AppCommand::PasteSystem(PasteOptions::default())),
            "edit.clear" => command_enabled(AppCommand::ClearSelection),
            "edit.select_all" => command_enabled(AppCommand::SelectAll),
            "edit.paste_special" => command_enabled(AppCommand::OpenPasteSpecial),
            "edit.search" => !blocked && !state.document_edit_locked && state.has_table,
            "edit.replace" => !blocked && !state.document_edit_locked && state.has_table,
            "data.replace_results" => !blocked && state.has_table && !state.document_edit_locked,
            "view.live" | "view.source" | "view.wrap_on" | "view.wrap_off" => {
                state.has_table && !blocked && !state.document_edit_locked
            }
            "view.zoom_in" => !blocked && self.sheet_zoom.0 < MAX_SHEET_ZOOM,
            "view.zoom_out" => !blocked && self.sheet_zoom.0 > MIN_SHEET_ZOOM,
            "view.zoom_reset" => !blocked && (self.sheet_zoom.0 - 1.0).abs() > f32::EPSILON,
            "view.column_width"
            | "view.autofit_columns"
            | "view.freeze_rows"
            | "view.freeze_columns"
            | "view.unfreeze" => {
                state.has_table && !blocked && !state.document_edit_locked && !state.structure_busy
            }
            id if id.starts_with("edit.table_") => {
                state.table_management_available
                    && !blocked
                    && !state.document_edit_locked
                    && !state.structure_busy
                    && (id != "edit.table_delete" || state.table_count > 1)
            }
            "insert.rows" | "insert.columns" => {
                state.has_table
                    && state.has_source
                    && !blocked
                    && !state.document_edit_locked
                    && !state.structure_busy
            }
            "edit.table_add" => {
                state.table_management_available
                    && !blocked
                    && !state.document_edit_locked
                    && !state.structure_busy
            }
            "edit.delete_rows"
            | "edit.delete_columns"
            | "edit.duplicate_rows"
            | "edit.duplicate_columns"
            | "edit.move_rows_up"
            | "edit.move_rows_down"
            | "edit.move_columns_left"
            | "edit.move_columns_right"
            | "edit.fill_copy"
            | "edit.fill_series" => {
                state.has_table
                    && state.has_source
                    && !blocked
                    && !state.document_edit_locked
                    && !state.structure_busy
            }
            id if id.starts_with("format.") => {
                state.can_markdown_modifiers
                    && !state.source_draft_active
                    && !state.document_edit_locked
                    && !blocked
            }
            "data.sort" => {
                state.has_table
                    && !blocked
                    && !state.source_draft_active
                    && !state.document_edit_locked
            }
            id if id.starts_with("data.") => {
                state.has_table
                    && !blocked
                    && !state.source_draft_active
                    && !state.document_edit_locked
            }
            _ => false,
        }
    }

    /// Rejects input ownership from a replaced editor or a closed modal field.
    fn native_owner_is_live(&self, state: &TopActionState, blocked: bool) -> bool {
        match state.command_owner.unwrap_or(InputOwner::Chrome) {
            InputOwner::CellEditor => {
                self.edit_document_id == self.workspace.active_document
                    && state.command_focus
                        == self
                            .edit_document_id
                            .map(|id| editor_id(id, self.edit.session_id))
            }
            InputOwner::OtherText if blocked => {
                let Some(focus) = state.command_focus else {
                    return false;
                };
                self.last_modal_text_input_focus == Some(focus)
                    && self.active_modal_text_input_focus == Some(focus)
            }
            InputOwner::OtherText => state.command_focus.is_some(),
            InputOwner::Ime | InputOwner::Modal => false,
            InputOwner::Grid | InputOwner::Chrome => true,
        }
    }

    // Returns the checked state for native menu items that show one.
    pub(super) fn menu_item_checked(&self, id: &str, state: &TopActionState) -> Option<bool> {
        match id {
            "view.live" | "view.source" => {
                let mode = state.table_display_mode?;
                Some(if id == "view.live" {
                    mode == crate::document::DocumentDisplayMode::Live
                } else {
                    mode == crate::document::DocumentDisplayMode::Source
                })
            }
            "view.wrap_on" => Some(state.table_wrap_mode == Some(WrapMode::On)),
            "view.wrap_off" => Some(state.table_wrap_mode == Some(WrapMode::Off)),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "native_test.rs"]
mod tests;
