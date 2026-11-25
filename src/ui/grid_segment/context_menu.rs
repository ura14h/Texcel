use super::*;

/// Shows cell commands before structure and fill actions.
pub(super) fn show(
    response: &egui::Response,
    selection: CellRange,
    block_reason: Option<&str>,
    command_enabled: [bool; 6],
    outcome: &mut GridOutcome,
) {
    response.context_menu(|menu| {
        visual::prepare_context_menu(menu);
        for (index, (label, command)) in [
            (0, ("action-cut", AppCommand::CutAs(CopyFormat::DisplayTsv))),
            (
                1,
                ("action-copy", AppCommand::CopyAs(CopyFormat::DisplayTsv)),
            ),
            (
                2,
                (
                    "action-paste",
                    AppCommand::PasteSystem(PasteOptions::default()),
                ),
            ),
            (5, ("top-paste-special-open", AppCommand::OpenPasteSpecial)),
        ] {
            if menu
                .add_enabled(
                    command_enabled[index],
                    egui::Button::new(context_text(menu.ctx(), label)),
                )
                .clicked()
            {
                outcome.commands.push(command);
                menu.close();
            }
        }
        // One separator opens the structure group; a blocked reason heads that group.
        menu.separator();
        if let Some(reason) = block_reason {
            menu.label(reason);
        }
        let (selection_start, selection_end) = selection.normalized();
        let selected_rows = selection_end.row - selection_start.row + 1;
        let selected_columns = selection_end.column - selection_start.column + 1;
        let series_available = (selected_columns == 1 && selected_rows >= 3)
            || (selected_rows == 1 && selected_columns >= 3);
        menu.add_enabled_ui(block_reason.is_none(), |menu| {
            for (label, action) in [
                ("insert-row", StructureAction::InsertRows),
                ("insert-column", StructureAction::InsertColumns),
                ("delete-row", StructureAction::DeleteRows),
                ("delete-column", StructureAction::DeleteColumns),
            ] {
                if menu.button(context_text(menu.ctx(), label)).clicked() {
                    outcome.structure_action = Some(action);
                    menu.close();
                }
            }
        });
        menu.separator();
        for (index, label, command) in [
            (3, "action-clear-cells", AppCommand::ClearSelection),
            (4, "action-select-all", AppCommand::SelectAll),
        ] {
            if menu
                .add_enabled(
                    command_enabled[index],
                    egui::Button::new(context_text(menu.ctx(), label)),
                )
                .clicked()
            {
                outcome.commands.push(command);
                menu.close();
            }
        }
        menu.separator();
        menu.add_enabled_ui(block_reason.is_none(), |menu| {
            if menu
                .button(context_text(menu.ctx(), "grid-fill-down"))
                .clicked()
            {
                outcome.structure_action = Some(StructureAction::FillDown);
                menu.close();
            }
            if menu
                .button(context_text(menu.ctx(), "grid-fill-right"))
                .clicked()
            {
                outcome.structure_action = Some(StructureAction::FillRight);
                menu.close();
            }
            if menu
                .add_enabled(
                    series_available,
                    egui::Button::new(context_text(menu.ctx(), "grid-fill-series")),
                )
                .clicked()
            {
                outcome.structure_action = Some(StructureAction::FillSeries);
                menu.close();
            }
        });
    });
}
