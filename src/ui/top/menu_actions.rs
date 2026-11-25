use super::*;

impl TexcelApp {
    /// Applies one stable menu action through the app's normal deferred action state.
    pub(super) fn apply_menu_action_id(&self, id: &str, state: &mut TopActionState) {
        match id {
            "file.new" => state.create_document = true,
            "file.open" => state.open_file = true,
            "file.save" => state.save_file = true,
            "file.save_as" => state.save_as = true,
            "file.close" => {
                state.close_document = self.workspace.active_document().map(|doc| doc.id)
            }
            "app.settings" => state.open_settings = true,
            "app.quit" => state.quit_app = true,
            "window.minimize" => state.minimize_window = true,
            "window.zoom" => state.toggle_window_zoom = true,
            "edit.undo" => state.commands.push(AppCommand::Undo),
            "edit.redo" => state.commands.push(AppCommand::Redo),
            "edit.cut" => state
                .commands
                .push(AppCommand::CutAs(CopyFormat::DisplayTsv)),
            "edit.copy" => state
                .commands
                .push(AppCommand::CopyAs(CopyFormat::DisplayTsv)),
            "edit.copy_as" => state.commands.push(AppCommand::OpenCopyOptions),
            "edit.paste" => state
                .commands
                .push(AppCommand::PasteSystem(PasteOptions::default())),
            "edit.clear" => state.commands.push(AppCommand::ClearSelection),
            "edit.select_all" => state.commands.push(AppCommand::SelectAll),
            "edit.search" => state.open_search_dialog = true,
            "edit.replace" => state.open_replace_dialog = true,
            "edit.paste_special" => state.commands.push(AppCommand::OpenPasteSpecial),
            "edit.delete_rows" => state.structure_action = Some(StructureAction::DeleteRows),
            "edit.delete_columns" => state.structure_action = Some(StructureAction::DeleteColumns),
            "edit.duplicate_rows" => state.structure_action = Some(StructureAction::DuplicateRows),
            "edit.duplicate_columns" => {
                state.structure_action = Some(StructureAction::DuplicateColumns)
            }
            "edit.move_rows_up" => state.structure_action = Some(StructureAction::MoveRowsUp),
            "edit.move_rows_down" => state.structure_action = Some(StructureAction::MoveRowsDown),
            "edit.move_columns_left" => {
                state.structure_action = Some(StructureAction::MoveColumnsLeft)
            }
            "edit.move_columns_right" => {
                state.structure_action = Some(StructureAction::MoveColumnsRight)
            }
            "edit.fill_copy" => state.structure_action = Some(StructureAction::FillCopy),
            "edit.fill_series" => state.structure_action = Some(StructureAction::FillSeries),
            "edit.table_duplicate" => {
                state.structure_action = Some(StructureAction::DuplicateTable)
            }
            "edit.table_delete" => state.structure_action = Some(StructureAction::DeleteTable),
            "view.live" => {
                state.table_display_mode = Some(crate::document::DocumentDisplayMode::Live)
            }
            "view.source" => {
                state.table_display_mode = Some(crate::document::DocumentDisplayMode::Source)
            }
            "view.zoom_in" => state.zoom_action = Some((self.zoom_factor + 0.05).min(1.5)),
            "view.zoom_out" => state.zoom_action = Some((self.zoom_factor - 0.05).max(0.75)),
            "view.zoom_reset" => state.zoom_action = Some(1.0),
            "view.wrap_on" => state.table_wrap_mode = Some(WrapMode::On),
            "view.wrap_off" => state.table_wrap_mode = Some(WrapMode::Off),
            "view.column_width" => state.layout_action = Some(LayoutAction::EditColumnWidth),
            "view.autofit_columns" => state.layout_action = Some(LayoutAction::AutoFitAllColumns),
            "view.freeze_rows" => state.layout_action = Some(LayoutAction::FreezeRows),
            "view.freeze_columns" => state.layout_action = Some(LayoutAction::FreezeColumns),
            "view.unfreeze" => state.layout_action = Some(LayoutAction::Unfreeze),
            "insert.rows" => state.structure_action = Some(StructureAction::InsertRows),
            "insert.columns" => state.structure_action = Some(StructureAction::InsertColumns),
            "edit.table_add" => state.structure_action = Some(StructureAction::AddTable),
            "format.bold" => state.format_action = Some(CellFormatAction::Bold),
            "format.italic" => state.format_action = Some(CellFormatAction::Italic),
            "format.strike" => state.format_action = Some(CellFormatAction::Strike),
            "format.code" => state.format_action = Some(CellFormatAction::Code),
            "format.link_edit" => state.edit_link = true,
            "format.link_open" => state.open_link = true,
            "format.align_left" => {
                state.structure_action = Some(StructureAction::MarkdownAlignLeft)
            }
            "format.align_center" => {
                state.structure_action = Some(StructureAction::MarkdownAlignCenter)
            }
            "format.align_right" => {
                state.structure_action = Some(StructureAction::MarkdownAlignRight)
            }
            "data.trim" => state.open_data_edit_dialog = Some(DataEditKind::TrimWhitespace),
            "data.split" => state.open_data_edit_dialog = Some(DataEditKind::SplitColumn),
            "data.join" => state.open_data_edit_dialog = Some(DataEditKind::JoinColumns),
            "data.transpose" => state.open_data_edit_dialog = Some(DataEditKind::Transpose),
            "data.sort" => state.open_data_edit_dialog = Some(DataEditKind::SortRows),
            "data.replace_results" => state.replace_formula_results = true,
            "data.detect_duplicates" => {
                state.open_data_edit_dialog = Some(DataEditKind::DetectDuplicates)
            }
            "data.remove_duplicates" => {
                state.open_data_edit_dialog = Some(DataEditKind::RemoveDuplicates)
            }
            "help.open" => state.open_help = true,
            _ => (),
        }
    }
}
