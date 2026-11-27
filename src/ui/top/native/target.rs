use super::*;

impl TexcelApp {
    /// Captures the current document, selection, and text owner for native events.
    pub(in crate::ui) fn native_action_target(
        &self,
        ctx: &egui::Context,
        blocked: bool,
    ) -> crate::native_menu::NativeActionTarget {
        let current_focus = ctx.memory(|memory| memory.focused());
        let retained_modal_focus = (blocked
            && self.modal_text_session().is_some()
            && self
                .last_modal_text_input_focus
                .is_some_and(|focus| current_focus == Some(focus)))
        .then_some(self.last_modal_text_input_focus)
        .flatten();
        let (owner, focus) =
            if let Some(focus) = self.active_modal_text_input_focus.or(retained_modal_focus) {
                (InputOwner::OtherText, Some(focus))
            } else {
                self.command_input_owner(ctx, blocked)
            };
        let document = self.workspace.active_document();
        let table = document.and_then(|document| document.active_table());
        let ranges = document
            .zip(table)
            .and_then(|(document, table)| document.view(table.id))
            .map_or_else(Vec::new, |view| view.selected_ranges().collect());
        let active_cell = document
            .zip(table)
            .and_then(|(document, table)| document.view(table.id))
            .map(|view| view.active_cell());
        let edit_session = if self.edit.is_editing() {
            self.edit_document_id
                .map(|document_id| (document_id, self.edit.session_id, self.edit.address))
        } else {
            None
        };
        crate::native_menu::NativeActionTarget {
            document_id: document.map(|document| document.id),
            viewport: self.current_viewport,
            table_id: table.map(|table| table.id),
            ranges,
            active_cell,
            owner: native_input_owner(owner),
            focus,
            edit_session,
            modal_session: self.modal_text_session(),
        }
    }

    /// Rejects an event if its synchronized document target changed before dispatch.
    pub(in crate::ui) fn native_event_target_is_current(
        &self,
        event: &crate::native_menu::NativeMenuEvent,
        state: &TopActionState,
        navigation_blocked: bool,
        ctx: &egui::Context,
    ) -> bool {
        if state.create_document || state.close_document.is_some() {
            return false;
        }
        let current = self.native_action_target(ctx, navigation_blocked);
        if matches!(event.id.as_str(), "file.save" | "file.save_as") {
            return native_save_target_matches(&event.target, &current, navigation_blocked);
        }
        if matches!(
            event.id.as_str(),
            "file.open" | "file.new" | "app.settings" | "app.quit" | "help.open" | "help.about"
        ) || event.id.starts_with("file.recent.")
        {
            return true;
        }
        native_targets_match(&event.target, &current)
    }

    /// Keeps a queued document command from reaching a later active tab.
    pub(in crate::ui) fn native_command_target_is_current(
        &self,
        target: &crate::native_menu::NativeActionTarget,
        blocked: bool,
        ctx: &egui::Context,
    ) -> bool {
        native_targets_match(target, &self.native_action_target(ctx, blocked))
    }
}

/// Keeps native event snapshots independent of the UI command enum.
fn native_input_owner(owner: InputOwner) -> crate::native_menu::NativeInputOwner {
    match owner {
        InputOwner::Grid => crate::native_menu::NativeInputOwner::Grid,
        InputOwner::CellEditor => crate::native_menu::NativeInputOwner::CellEditor,
        InputOwner::OtherText => crate::native_menu::NativeInputOwner::OtherText,
        InputOwner::Ime => crate::native_menu::NativeInputOwner::Ime,
        InputOwner::Modal => crate::native_menu::NativeInputOwner::Modal,
        InputOwner::Chrome => crate::native_menu::NativeInputOwner::Chrome,
    }
}

/// Keeps a same-frame cell-editor commit valid while rejecting a new target.
pub(in crate::ui) fn native_targets_match(
    expected: &crate::native_menu::NativeActionTarget,
    current: &crate::native_menu::NativeActionTarget,
) -> bool {
    let same_selection = expected.document_id == current.document_id
        && expected.viewport == current.viewport
        && expected.table_id == current.table_id
        && expected.ranges == current.ranges
        && expected.active_cell == current.active_cell
        && expected.modal_session == current.modal_session;
    let same_owner = expected.owner == current.owner
        && expected.focus == current.focus
        && expected.edit_session == current.edit_session;
    let editor_committed = expected.owner == crate::native_menu::NativeInputOwner::CellEditor
        && current.owner == crate::native_menu::NativeInputOwner::Grid
        && expected.edit_session.is_some()
        && current.edit_session.is_none();
    same_selection && (same_owner || editor_committed)
}

/// Lets Save follow a same-document commit while retaining modal and close guards.
pub(in crate::ui) fn native_save_target_matches(
    expected: &crate::native_menu::NativeActionTarget,
    current: &crate::native_menu::NativeActionTarget,
    blocked: bool,
) -> bool {
    !blocked
        && expected.document_id.is_some()
        && expected.document_id == current.document_id
        && expected.viewport == current.viewport
        && expected.modal_session == current.modal_session
        && !matches!(
            current.owner,
            crate::native_menu::NativeInputOwner::Ime | crate::native_menu::NativeInputOwner::Modal
        )
}
