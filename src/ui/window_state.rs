use super::*;

macro_rules! define_window_state {
    ($( $(#[$meta:meta])* $field:ident: $kind:ty, )*) => {
        /// Stores input and dialog state that belongs to one native window.
        #[derive(Default)]
        pub(super) struct WindowState {
            pub(super) document_id: Option<DocumentId>,
            $(
                $(#[$meta])*
                pub(super) $field: $kind,
            )*
        }

        impl WindowState {
            /// Swaps one window's local state into the shared application methods.
            pub(super) fn swap_with(&mut self, app: &mut TexcelApp) {
                $(
                    $(#[$meta])*
                    std::mem::swap(&mut self.$field, &mut app.$field);
                )*
            }
        }
    };
}

define_window_state! {
    edit: EditSession,
    edit_document_id: Option<DocumentId>,
    ime_composition: Option<ImeCompositionState>,
    ime_interrupt_pending: bool,
    ime_interrupt_this_frame: bool,
    ime_escape_cancelled_this_frame: bool,
    modal_ime_composition: Option<Id>,
    late_ime_editor: Option<Id>,
    formula_references: FormulaReferenceState,
    pending_edit_input: Option<ordered_edit_input::PendingEditInput>,
    long_text_panel_open: bool,
    long_text_panel_dismissed_session: Option<u64>,
    settings_window_open: bool,
    about_dialog_open: bool,
    minimap_open: bool,
    minimap_shown_table: Option<(DocumentId, TableId)>,
    sheet_tab_shown_table: Option<(DocumentId, TableId)>,
    pending_accesskit_focus: Option<(DocumentId, u64, CellAddress)>,
    request_scroll_to_address: Option<(DocumentId, u64, CellAddress)>,
    font_layout_generation: u64,
    sheet_zoom: SheetZoom,
    last_pixels_per_point: f32,
    last_viewport_title: Option<String>,
    grid_focused: bool,
    text_dialog_open_previous_frame: bool,
    dialog_return_editor: Option<(DocumentId, u64, u64, crate::document::EditTarget)>,
    grid_keyboard_owner: Option<Id>,
    input_sequence_origin: Option<input_sequence::InputSequenceOrigin>,
    last_text_input_focus: Option<Id>,
    last_modal_text_input_focus: Option<Id>,
    active_modal_text_input_focus: Option<Id>,
    modal_menu_text_focus: Option<Id>,
    #[cfg(any(test, not(any(target_os = "macos", target_os = "windows"))))]
    shared_edit_menu_target: Option<SharedEditMenuTarget>,
    restore_modal_menu_focus_this_frame: bool,
    preserve_text_input_focus: bool,
    suppress_grid_keyboard: bool,
    axis_space_text_pending: Option<PendingAxisSpaceText>,
    request_editor_focus: bool,
    request_grid_focus: bool,
    request_scroll_to_selection: bool,
    status: Option<Message>,
    status_lifetime: Option<(Message, Instant)>,
    import_notice: Option<(DocumentId, Message)>,
    deferred_commands: Vec<commands::DeferredAppCommand>,
    deferred_text_commands: Vec<commands::DeferredTextCommand>,
    pending_close_document: Option<DocumentId>,
    close_after_save: BTreeSet<DocumentId>,
    close_after_save_edit: BTreeMap<DocumentId, EditSession>,
    save_conflict_dialog: Option<save_conflict::SaveConflictPrompt>,
    app_close_dialog_open: bool,
    active_import_report: Option<(DocumentId, Arc<ImportReport>)>,
    selected_import_group: usize,
    export_dialog: Option<ExportDialogState>,
    active_export_confirmation: Option<ExportConfirmation>,
    cell_link_dialog: Option<CellLinkDialog>,
    layout_dialog: Option<LayoutDialogState>,
    sheet_zoom_dialog: Option<sheet_zoom_dialog::SheetZoomDialogState>,
    search_dialog: Option<SearchDialogState>,
    paste_special_dialog: Option<PasteOptions>,
    copy_options_dialog: Option<CopyOptionsDialog>,
    data_edit_dialog: Option<DataEditDialogState>,
    pending_growth_selection: Option<grid_events::PendingGrowthSelection>,
    fill_handle_drag: Option<FillHandleDrag>,
    grid_selection_drag: Option<GridSelectionDrag>,
    header_resize_drag: Option<HeaderResizeDrag>,
    grid_page_extent: Vec2,
    text_selection: Option<TextSelection>,
    name_box: String,
    #[cfg(test)]
    test_save_path_selection: Option<PathBuf>,
    #[cfg(test)]
    test_close_button_rects: BTreeMap<&'static str, egui::Rect>,
    #[cfg(test)]
    test_app_close_button_rects: BTreeMap<&'static str, egui::Rect>,
}

impl TexcelApp {
    /// Moves the initial screen controls into the root window before the host starts.
    pub(super) fn enable_window_state(&mut self, viewport: egui::ViewportId) {
        let mut state = WindowState {
            document_id: self.workspace.active_document,
            ..WindowState::default()
        };
        state.swap_with(self);
        self.window_states.insert(viewport, state);
    }

    /// Runs the old screen methods with the local controls for one viewport.
    pub(super) fn with_window_state<R>(
        &mut self,
        viewport: egui::ViewportId,
        document_id: Option<DocumentId>,
        run: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let mut state = self.window_states.remove(&viewport).unwrap_or_default();
        if state.document_id.is_none() {
            state.document_id = document_id
                .filter(|document_id| self.workspace.documents.contains_key(document_id));
        }
        state.swap_with(self);
        let previous_document = self.workspace.active_document;
        self.workspace.active_document = state
            .document_id
            .filter(|id| self.workspace.documents.contains_key(id));
        self.current_viewport = Some(viewport);
        let result = run(self);
        state.document_id = if self.multi_window_mode {
            state
                .document_id
                .filter(|document_id| self.workspace.documents.contains_key(document_id))
        } else {
            self.workspace.active_document
        };
        self.workspace.active_document = previous_document;
        self.current_viewport = None;
        state.swap_with(self);
        self.window_states.insert(viewport, state);
        result
    }

    /// Associates one existing viewport state with a newly opened document.
    pub(super) fn set_window_document(
        &mut self,
        viewport: egui::ViewportId,
        document_id: Option<DocumentId>,
    ) {
        self.window_states.entry(viewport).or_default().document_id = document_id;
    }

    /// Returns the document that owns one stored viewport state.
    pub(super) fn window_document(&self, viewport: egui::ViewportId) -> Option<DocumentId> {
        self.window_states.get(&viewport)?.document_id
    }
}
