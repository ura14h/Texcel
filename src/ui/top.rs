use super::commands::{AppCommand, InputOwner};

#[derive(Clone, Default)]
pub(super) struct TopActionState {
    has_table: bool,
    table_count: usize,
    source_draft_active: bool,
    can_markdown_modifiers: bool,
    has_source: bool,
    display_state: Option<(DocumentId, u64, crate::document::DocumentDisplayMode)>,
    table_display_mode: Option<crate::document::DocumentDisplayMode>,
    wrap_state: Option<(DocumentId, u64, WrapMode)>,
    table_wrap_mode: Option<WrapMode>,
    command_owner: Option<InputOwner>,
    command_focus: Option<Id>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    native_command_target: Option<crate::native_menu::NativeActionTarget>,
    preserve_text_focus: bool,
    table_management_available: bool,
    structure_busy: bool,
    document_edit_locked: bool,
    create_document: bool,
    open_file: bool,
    open_recent_paths: Vec<std::path::PathBuf>,
    focus_document: Option<DocumentId>,
    save_file: bool,
    save_as: bool,
    replace_formula_results: bool,
    close_document: Option<DocumentId>,
    open_settings: bool,
    open_help: bool,
    quit_app: bool,
    commands: Vec<AppCommand>,
    format_action: Option<CellFormatAction>,
    layout_action: Option<LayoutAction>,
    structure_action: Option<StructureAction>,
    edit_link: bool,
    open_link: bool,
    zoom_action: Option<f32>,
    minimize_window: bool,
    toggle_window_zoom: bool,
    local_preference_changes: Vec<(DocumentId, u64, TablePreferenceField)>,
    open_search_dialog: bool,
    open_replace_dialog: bool,
    open_data_edit_dialog: Option<DataEditKind>,
}

impl TopActionState {
    /// Reports whether a header action should wait until text widgets finish this frame.
    fn has_deferred_action(&self) -> bool {
        !self.commands.is_empty()
            || self.create_document
            || self.open_file
            || !self.open_recent_paths.is_empty()
            || self.save_file
            || self.save_as
            || self.close_document.is_some()
            || self.replace_formula_results
            || self.open_search_dialog
            || self.open_replace_dialog
            || self.open_data_edit_dialog.is_some()
            || self.format_action.is_some()
            || self.layout_action.is_some()
            || self.edit_link
            || self.open_link
            || self.structure_action.is_some()
            || self.open_settings
            || self.open_help
            || self.quit_app
            || self.zoom_action.is_some()
            || self.minimize_window
            || self.toggle_window_zoom
            || self.focus_document.is_some()
            || self.display_state.is_some_and(|(_, _, current)| {
                self.table_display_mode.is_some_and(|next| next != current)
            })
            || !self.local_preference_changes.is_empty()
    }
}

use super::*;

#[path = "top/actions.rs"]
mod actions;
#[path = "top/menu_actions.rs"]
mod menu_actions;
#[cfg(any(test, not(any(target_os = "macos", target_os = "windows"))))]
#[path = "top/menu_bar.rs"]
mod menu_bar;
#[path = "top/native.rs"]
mod native;
#[path = "top/shortcuts.rs"]
mod shortcuts;

impl TexcelApp {
    /// Collects the current document names for the Window menu.
    pub(super) fn window_menu_items(&self, ctx: &egui::Context) -> Vec<(u64, String, bool)> {
        self.workspace
            .order
            .iter()
            .filter_map(|document_id| {
                self.workspace.documents.get(document_id)?;
                let mut label = self.document_display_name(ctx, *document_id);
                if self.document_has_unsaved_changes(*document_id) {
                    label.push_str(" •");
                }
                Some((
                    *document_id,
                    label,
                    self.workspace.active_document == Some(*document_id),
                ))
            })
            .collect()
    }

    /// Applies a collected shortcut through the header's existing action handlers.
    pub(super) fn apply_app_shortcut(
        &mut self,
        shortcut: Option<shortcuts::AppShortcut>,
        navigation_blocked: bool,
        ctx: &egui::Context,
    ) {
        self.apply_collected_app_shortcut(shortcut, navigation_blocked, ctx);
    }

    /// Keeps primary actions and the formula bar in one compact header.
    pub(super) fn draw_top(
        &mut self,
        root: &mut egui::Ui,
        ctx: &egui::Context,
        navigation_blocked: bool,
        formula_ime_active: bool,
        document_edit_locked: bool,
    ) -> (Option<shortcuts::AppShortcut>, TopActionState) {
        let mut state = TopActionState::default();
        let (command_owner, command_focus) = self.command_input_owner(ctx, navigation_blocked);
        state.command_owner = Some(command_owner);
        state.command_focus = command_focus;
        let has_table = self
            .workspace
            .active_document()
            .and_then(|document| document.active_table())
            .is_some();
        state.table_count = self
            .workspace
            .active_document()
            .map_or(0, |document| document.tables.len());
        let source_draft_active = false;
        let can_markdown_modifiers = self.workspace.active_document().is_some_and(|document| {
            matches!(
                document.source.as_deref(),
                Some(SourceDocument::Markdown(_) | SourceDocument::Csv(_))
            )
        });
        let has_source = self
            .workspace
            .active_document()
            .is_some_and(|document| document.source.is_some());
        let display_state = self.workspace.active_document().and_then(|document| {
            let table = document.active_table()?;
            Some((document.id, table.id, document.display_mode))
        });
        let table_display_mode = display_state.map(|state| state.2);
        let wrap_state = self.workspace.active_document().and_then(|document| {
            let table = document.active_table()?;
            let view = document.view(table.id)?;
            let wrap_mode = if view.wrap_text {
                WrapMode::On
            } else {
                WrapMode::Off
            };
            Some((document.id, table.id, wrap_mode))
        });
        let table_management_available = self.workspace.active_document().is_some_and(|document| {
            matches!(
                document.source.as_deref(),
                Some(SourceDocument::Markdown(_) | SourceDocument::Csv(_))
            )
        });
        state.has_table = has_table;
        state.source_draft_active = source_draft_active;
        state.can_markdown_modifiers = can_markdown_modifiers;
        state.has_source = has_source;
        state.display_state = display_state;
        state.table_display_mode = table_display_mode;
        state.wrap_state = wrap_state;
        state.table_wrap_mode = wrap_state.map(|state| state.2);
        state.table_management_available = table_management_available;
        state.document_edit_locked = document_edit_locked;
        state.structure_busy =
            self.active_structure_pending() || self.active_history_restore_pending();
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let show_shared_menu = true;
        #[cfg(all(test, any(target_os = "macos", target_os = "windows")))]
        let show_shared_menu = shared_menu_bar_for_test(ctx);
        #[cfg(all(not(test), any(target_os = "macos", target_os = "windows")))]
        let show_shared_menu = false;
        egui::Panel::top("texcel_header").show(root, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
            ui.spacing_mut().button_padding = Vec2::new(8.0, 2.0);
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(CHROME_EDGE_INSET, 0))
                .show(ui, |ui| {
                    if show_shared_menu {
                        ui.scope(|ui| {
                            ui.horizontal_wrapped(|_ui| {
                                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                                self.draw_shared_menu_bar(_ui, navigation_blocked, &mut state);
                                #[cfg(all(test, any(target_os = "macos", target_os = "windows")))]
                                self.draw_shared_menu_bar(_ui, navigation_blocked, &mut state);
                            });
                        });
                        ui.separator();
                    }
                });
            let formula_bar_blocked = navigation_blocked && !formula_ime_active
                || self.workspace.active_document().is_some_and(|document| {
                    self.cell_edit_locked(document.id)
                        || self.close_after_save.contains(&document.id)
                });
            // The formula bar takes the width left of the narrow live/source bar.
            let mode_bar_width = self.display_mode_bar_width(ui);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.scope(|ui| {
                    ui.set_max_width((ui.available_width() - mode_bar_width).max(0.0));
                    self.draw_formula_bar(ui, formula_bar_blocked);
                });
                if mode_bar_width > 0.0 {
                    ui.add_space(display_mode_bar::DISPLAY_MODE_BAR_GAP);
                    self.draw_display_mode_bar(ui, navigation_blocked);
                }
            });
        });

        self.restore_modal_menu_focus_this_frame = false;
        if navigation_blocked && state.preserve_text_focus {
            if let Some(focus) = state
                .command_focus
                .filter(|focus| self.last_modal_text_input_focus == Some(*focus))
            {
                self.modal_menu_text_focus = Some(focus);
            }
            self.restore_modal_menu_focus_this_frame = self.modal_menu_text_focus.is_some();
        } else {
            self.modal_menu_text_focus = None;
        }
        if state.preserve_text_focus
            || (matches!(
                command_owner,
                InputOwner::CellEditor | InputOwner::OtherText
            ) && state.has_deferred_action())
        {
            self.preserve_text_input_focus = true;
            if let Some(focus) = state.command_focus
                && !(navigation_blocked && self.restore_modal_menu_focus_this_frame)
                && !egui::Popup::is_any_open(ctx)
            {
                ctx.memory_mut(|memory| memory.request_focus(focus));
            }
        }
        // Delays shortcuts until every editor has consumed this frame's text events.
        (self.take_app_shortcut(ctx, navigation_blocked), state)
    }
}

/// Selects the in-window renderer in a real app Harness on native-menu hosts.
#[cfg(test)]
pub(super) fn force_shared_menu_bar_for_test(ctx: &egui::Context) {
    ctx.data_mut(|data| data.insert_temp(Id::NULL.with("test_shared_menu_bar"), true));
}

/// Reads the test-only renderer choice without changing production menu selection.
#[cfg(all(test, any(target_os = "macos", target_os = "windows")))]
fn shared_menu_bar_for_test(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<bool>(Id::NULL.with("test_shared_menu_bar")))
        .unwrap_or(false)
}
