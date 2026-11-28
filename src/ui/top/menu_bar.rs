use super::*;
use crate::command_menu::{MENUS, MenuAction, MenuCheck, MenuEntry};

impl TexcelApp {
    /// Draws Linux menus from the same action tree used by native menu adapters.
    pub(super) fn draw_shared_menu_bar(
        &mut self,
        ui: &mut egui::Ui,
        navigation_blocked: bool,
        state: &mut TopActionState,
    ) {
        for menu in MENUS {
            let response = ui.menu_button(crate::i18n::context_text(ui.ctx(), menu.label), |ui| {
                self.draw_menu_entries(ui, menu.entries, navigation_blocked, state);
            });
            if menu.id == "menu.edit" {
                let edit_menu_interacting = response.response.clicked()
                    || response.response.is_pointer_button_down_on()
                    || response.inner.is_some();
                if edit_menu_interacting
                    && matches!(
                        state.command_owner,
                        Some(InputOwner::OtherText | InputOwner::CellEditor)
                    )
                {
                    state.preserve_text_focus = true;
                    let captured =
                        self.capture_shared_edit_menu_target(state, navigation_blocked, ui.ctx());
                    if let Some(existing) = self.shared_edit_menu_target
                        && !self.shared_edit_menu_target_is_current(existing)
                    {
                        self.shared_edit_menu_target = None;
                    }
                    if let Some(captured) = captured
                        && self
                            .shared_edit_menu_target
                            .is_none_or(|existing| !existing.same_widget(captured))
                    {
                        self.shared_edit_menu_target = Some(captured);
                    }
                } else if !edit_menu_interacting && state.commands.is_empty() {
                    if ui.input(|input| input.key_released(egui::Key::Escape))
                        && !navigation_blocked
                        && self.modal_text_session().is_none()
                        && !context_has_ime_event(ui.ctx())
                        && let Some(target) = self.shared_edit_menu_target
                        && self.shared_edit_menu_target_is_current(target)
                    {
                        // Return keyboard ownership only after Escape closes this exact menu.
                        ui.ctx()
                            .memory_mut(|memory| memory.request_focus(target.focus()));
                        state.preserve_text_focus = true;
                    }
                    self.shared_edit_menu_target = None;
                }
            }
        }
    }

    /// Captures a real cell editor or name box while the shared Edit menu is active.
    fn capture_shared_edit_menu_target(
        &self,
        state: &TopActionState,
        navigation_blocked: bool,
        ctx: &egui::Context,
    ) -> Option<SharedEditMenuTarget> {
        if navigation_blocked
            || self.edit.composing
            || self.modal_ime_composition.is_some()
            || context_has_ime_event(ctx)
        {
            return None;
        }
        let focus = state.command_focus?;
        match state.command_owner? {
            InputOwner::CellEditor => {
                let document_id = self.edit_document_id?;
                let table_id = self.edit.table_id?;
                let target = SharedEditMenuTarget::CellEditor {
                    document_id,
                    table_id,
                    session_id: self.edit.session_id,
                    address: self.edit.address,
                    focus,
                    cursor_range: egui::TextEdit::load_state(ctx, focus)
                        .and_then(|state| state.cursor.char_range()),
                };
                self.shared_edit_menu_target_is_current(target)
                    .then_some(target)
            }
            InputOwner::OtherText => {
                let document = self.workspace.active_document()?;
                let table_id = document.active_table()?.id;
                let target = SharedEditMenuTarget::NameBox {
                    document_id: document.id,
                    table_id,
                    focus,
                    cursor_range: egui::TextEdit::load_state(ctx, focus)
                        .and_then(|state| state.cursor.char_range()),
                };
                (focus == name_box_id(document.id)
                    && self.modal_text_session().is_none()
                    && self.shared_edit_menu_target_is_current(target))
                .then_some(target)
            }
            _ => None,
        }
    }

    /// Renders one menu level while keeping every leaf tied to its shared ID.
    fn draw_menu_entries(
        &self,
        ui: &mut egui::Ui,
        entries: &'static [MenuEntry],
        blocked: bool,
        state: &mut TopActionState,
    ) {
        // Menu rows match the context menus: compact height and no gaps.
        visual::prepare_context_menu(ui);
        for entry in entries {
            match entry {
                MenuEntry::Action(action) => {
                    self.draw_menu_action(ui, action, blocked, state);
                }
                MenuEntry::Submenu(submenu) => {
                    ui.menu_button(crate::i18n::context_text(ui.ctx(), submenu.label), |ui| {
                        self.draw_menu_entries(ui, submenu.entries, blocked, state);
                    });
                }
                MenuEntry::RecentFiles => self.draw_recent_files_menu(ui, blocked, state),
                MenuEntry::WindowList => self.draw_window_list_menu(ui, state),
                MenuEntry::Separator => {
                    ui.separator();
                }
            }
        }
    }

    /// Shows the same document targets that native Window menus expose.
    fn draw_window_list_menu(&self, ui: &mut egui::Ui, state: &mut TopActionState) {
        ui.menu_button(
            crate::i18n::context_text(ui.ctx(), "menu-window-open"),
            |ui| {
                visual::prepare_context_menu(ui);
                for (document_id, label, active) in self.window_menu_items(ui.ctx()) {
                    if ui.selectable_label(active, label).clicked() {
                        state.focus_document = Some(document_id);
                        ui.close();
                    }
                }
            },
        );
    }

    /// Shows recent paths as dynamic children of the shared placeholder.
    fn draw_recent_files_menu(&self, ui: &mut egui::Ui, blocked: bool, state: &mut TopActionState) {
        ui.menu_button(
            crate::i18n::context_text(ui.ctx(), "menu-file-open-recent"),
            |ui| {
                visual::prepare_context_menu(ui);
                if self.recent_files.is_empty() {
                    ui.add_enabled(
                        false,
                        egui::Button::new(crate::i18n::context_text(
                            ui.ctx(),
                            "menu-file-recent-empty",
                        )),
                    );
                }
                for path in &self.recent_files {
                    let label = path.file_name().map_or_else(
                        || path.display().to_string(),
                        |name| name.to_string_lossy().into_owned(),
                    );
                    if ui
                        .add_enabled(!blocked, egui::Button::new(label))
                        .on_hover_text(path.display().to_string())
                        .clicked()
                    {
                        state.open_recent_paths.push(path.clone());
                        ui.close();
                    }
                }
            },
        );
    }

    /// Applies the shared enabled and check state before drawing one leaf.
    fn draw_menu_action(
        &self,
        ui: &mut egui::Ui,
        action: &'static MenuAction,
        blocked: bool,
        state: &mut TopActionState,
    ) {
        let enabled = self.menu_item_enabled(action.id, state, blocked);
        let checked = self.menu_item_checked(action.id, state).unwrap_or(false);
        let label = self
            .menu_item_label(action.id)
            .unwrap_or_else(|| crate::i18n::context_text(ui.ctx(), action.label));
        let clicked = match action.check {
            MenuCheck::Radio => {
                let label = if checked {
                    format!("✓ {label}")
                } else {
                    label
                };
                ui.add_enabled(enabled, egui::Button::selectable(checked, label))
                    .clicked()
            }
            MenuCheck::None => ui
                .add_enabled(
                    enabled,
                    egui::Button::new(label).shortcut_text(crate::command_menu::shortcut_text(
                        ui.ctx(),
                        action.shortcut,
                    )),
                )
                .clicked(),
        };
        if clicked {
            self.apply_menu_action_id(action.id, state);
            ui.close();
        }
    }
}
