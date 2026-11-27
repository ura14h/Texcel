use super::*;

impl TexcelApp {
    /// Applies app-wide native menu commands while every document window is closed.
    pub(in crate::ui) fn apply_native_global_menu_events(
        &mut self,
        events: Vec<crate::native_menu::NativeMenuEvent>,
        ctx: &egui::Context,
    ) -> bool {
        let global_events: Vec<crate::native_menu::NativeMenuEvent> = events
            .into_iter()
            .filter(|event| {
                matches!(
                    event.id.as_str(),
                    "file.new"
                        | "file.open"
                        | "app.settings"
                        | "app.quit"
                        | "help.open"
                        | "help.about"
                ) || event.id.starts_with("file.recent.")
            })
            .collect();
        let needs_visible_root = global_events.iter().any(|event| event.id != "app.quit");
        let mut state = TopActionState {
            command_owner: Some(InputOwner::Chrome),
            ..TopActionState::default()
        };
        let blocked = self.modal_input_blocked();
        self.apply_native_menu_events(global_events, &mut state, blocked, ctx);
        self.apply_top_actions(state, blocked, ctx);
        needs_visible_root
    }

    /// Keeps global actions enabled while the macOS menu bar has no document window.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub(in crate::ui) fn sync_native_menu_without_document(&mut self, ctx: &egui::Context) {
        let state = TopActionState {
            command_owner: Some(InputOwner::Chrome),
            ..TopActionState::default()
        };
        let blocked = self.modal_input_blocked();
        let states = self.native_menu_states(&state, blocked);
        let target = self.native_action_target(ctx, blocked);
        let windows = self.window_menu_items(ctx);
        let error = self.native_menu.as_mut().and_then(|menu| {
            menu.sync(self.language, &states, &self.recent_files, target, &windows)
                .err()
        });
        if let Some(detail) = error {
            self.status =
                Some(Message::new("status-native-menu-update-failed").with_text("detail", detail));
        }
    }

    /// Applies safe native events now and returns focused-modal text edits for the late pass.
    pub(in crate::ui) fn apply_native_menu_events(
        &self,
        events: Vec<crate::native_menu::NativeMenuEvent>,
        state: &mut TopActionState,
        navigation_blocked: bool,
        ctx: &egui::Context,
    ) -> Vec<crate::native_menu::NativeMenuEvent> {
        let mut late_text_events = Vec::new();
        for event in events {
            if !event.id.starts_with("window.activate.")
                && !self.native_event_target_is_current(&event, state, navigation_blocked, ctx)
            {
                continue;
            }
            let text_edit = matches!(
                event.id.as_str(),
                "edit.undo"
                    | "edit.redo"
                    | "edit.cut"
                    | "edit.copy"
                    | "edit.copy_display"
                    | "edit.paste"
                    | "edit.paste_text"
                    | "edit.clear"
                    | "edit.select_all"
            );
            if navigation_blocked && state.command_owner == Some(InputOwner::OtherText) && text_edit
            {
                state.preserve_text_focus = true;
                late_text_events.push(event);
            } else {
                self.apply_native_menu_event(event, state, navigation_blocked);
            }
        }
        late_text_events
    }

    /// Applies modal text commands after the active dialog has drawn its real text field.
    pub(in crate::ui) fn apply_late_native_text_events(
        &mut self,
        events: Vec<crate::native_menu::NativeMenuEvent>,
        navigation_blocked: bool,
        ctx: &egui::Context,
    ) {
        if events.is_empty() {
            return;
        }
        let focus = self.active_modal_text_input_focus;
        let mut state = TopActionState {
            command_owner: Some(if focus.is_some() {
                InputOwner::OtherText
            } else {
                InputOwner::Modal
            }),
            command_focus: focus,
            preserve_text_focus: focus.is_some(),
            ..TopActionState::default()
        };
        for event in events {
            if !super::native_targets_match(
                &event.target,
                &self.native_action_target(ctx, navigation_blocked),
            ) {
                continue;
            }
            self.apply_native_menu_event(event, &mut state, navigation_blocked);
        }
        self.apply_top_actions(state, navigation_blocked, ctx);
        let commands = std::mem::take(&mut self.deferred_commands);
        for deferred in commands {
            if let Some(target) = &deferred.native_target
                && !self.native_command_target_is_current(target, deferred.blocked, ctx)
            {
                continue;
            }
            self.dispatch_command(
                deferred.command,
                deferred.owner,
                deferred.blocked,
                deferred.focus,
                ctx,
            );
        }
    }

    /// Builds enabled menu state from the focused widget drawn in the current frame.
    pub(in crate::ui) fn native_menu_states_for_frame(
        &self,
        mut state: TopActionState,
        navigation_blocked: bool,
        ctx: &egui::Context,
    ) -> BTreeMap<String, crate::native_menu::ItemState> {
        if let Some(focus) = self.active_modal_text_input_focus {
            state.command_owner = Some(InputOwner::OtherText);
            state.command_focus = Some(focus);
        } else {
            let (owner, focus) = self.command_input_owner(ctx, navigation_blocked);
            state.command_owner = Some(owner);
            state.command_focus = focus;
        }
        self.native_menu_states(
            &state,
            navigation_blocked || self.active_modal_text_input_focus.is_some(),
        )
    }

    /// Updates native labels and enable states after current dialog fields have drawn.
    pub(in crate::ui) fn sync_native_menu_after_dialogs(
        &mut self,
        state: TopActionState,
        navigation_blocked: bool,
        ctx: &egui::Context,
    ) {
        let windows = self.window_menu_items(ctx);
        let focused = ctx.input(|input| input.viewport().focused == Some(true));
        if !focused {
            let error = self.native_menu.as_mut().and_then(|menu| {
                menu.sync_shared_lists(self.language, &self.recent_files, &windows, false)
                    .err()
            });
            if let Some(detail) = error {
                self.status = Some(
                    Message::new("status-native-menu-update-failed").with_text("detail", detail),
                );
            }
            return;
        }
        let states = self.native_menu_states_for_frame(state, navigation_blocked, ctx);
        let target = self.native_action_target(ctx, navigation_blocked);
        let error = self.native_menu.as_mut().and_then(|menu| {
            menu.sync(self.language, &states, &self.recent_files, target, &windows)
                .err()
        });
        if let Some(detail) = error {
            self.status =
                Some(Message::new("status-native-menu-update-failed").with_text("detail", detail));
        }
    }
}
