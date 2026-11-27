use super::*;

pub(super) enum SheetBandAction {
    Select(TableId),
    Manage(StructureAction, Option<TableId>),
    OpenZoom,
    ToggleMinimap,
}

/// Draws the sheet strip with tabs and Add inside the scroller.
pub(super) fn draw_sheet_band(
    app: &TexcelApp,
    ui: &mut egui::Ui,
    navigation_blocked: bool,
) -> Option<SheetBandAction> {
    let document = app.workspace.active_document();
    let tabs = document.map_or_else(Vec::new, |document| {
        let mut counts = BTreeMap::<String, usize>::new();
        document
            .tables
            .iter()
            .map(|table| {
                let count = counts.entry(table.name.clone()).or_default();
                *count += 1;
                let label = if *count == 1 {
                    table.name.clone()
                } else {
                    format!("{} ({})", table.name, *count)
                };
                (table.id, label, table.id == document.active_table)
            })
            .collect::<Vec<_>>()
    });
    let can_manage_tables = document.is_some_and(|document| {
        matches!(
            document.source.as_deref(),
            Some(SourceDocument::Markdown(_) | SourceDocument::Csv(_))
        )
    });
    let management_enabled = can_manage_tables
        && !navigation_blocked
        && !document.is_some_and(|document| app.document_bulk_edit_pending(document.id));
    let mut action = None;
    let band_fill = GridThemePalette::from_visuals(ui.visuals()).header_background;
    egui::Frame::new()
        .fill(band_fill)
        .inner_margin(egui::Margin::ZERO)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_min_height(CHROME_TAB_HEIGHT);
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.spacing_mut().interact_size.y = CHROME_TAB_HEIGHT;
            ui.horizontal(|ui| {
                let zoom_label = format!(
                    "{}%",
                    super::super::sheet_zoom_dialog::zoom_percent(app.sheet_zoom.0)
                );
                let zoom_width = chrome::mode_tab_width(ui, &zoom_label);
                let list_visible = context_text(ui.ctx(), "dialog-table-list-button");
                let fixed_width = chrome::command_width(ui, &list_visible) + zoom_width + 8.0;
                let scroll_width = (ui.available_width() - fixed_width).max(0.0);
                egui::ScrollArea::horizontal()
                    .id_salt("table_tab_scroll")
                    .auto_shrink([false, false])
                    .max_width(scroll_width)
                    .max_height(CHROME_TAB_HEIGHT)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.horizontal(|ui| {
                            for (table_id, name, active) in &tabs {
                                let response = chrome::sheet_tab_button(ui, *active, name);
                                if response.clicked() {
                                    action = Some(SheetBandAction::Select(*table_id));
                                }
                                response.context_menu(|menu| {
                                    visual::prepare_context_menu(menu);
                                    menu.add_enabled_ui(management_enabled, |menu| {
                                        if menu
                                            .button(context_text(menu.ctx(), "dialog-add-table"))
                                            .clicked()
                                        {
                                            action = Some(SheetBandAction::Manage(
                                                StructureAction::AddTable,
                                                Some(*table_id),
                                            ));
                                            menu.close();
                                        }
                                        if menu
                                            .button(context_text(
                                                menu.ctx(),
                                                "dialog-duplicate-table",
                                            ))
                                            .clicked()
                                        {
                                            action = Some(SheetBandAction::Manage(
                                                StructureAction::DuplicateTable,
                                                Some(*table_id),
                                            ));
                                            menu.close();
                                        }
                                        if menu
                                            .add_enabled(
                                                tabs.len() > 1,
                                                egui::Button::new(context_text(
                                                    menu.ctx(),
                                                    "dialog-delete-table",
                                                )),
                                            )
                                            .clicked()
                                        {
                                            action = Some(SheetBandAction::Manage(
                                                StructureAction::DeleteTable,
                                                Some(*table_id),
                                            ));
                                            menu.close();
                                        }
                                    });
                                    if !can_manage_tables {
                                        menu.label(context_text(
                                            menu.ctx(),
                                            "dialog-csv-one-table",
                                        ));
                                    }
                                });
                            }
                            let add_label = context_text(ui.ctx(), "dialog-add-table");
                            if chrome::sheet_band_icon_button(
                                ui,
                                &add_label,
                                chrome::CommandIcon::AddTable,
                                management_enabled,
                            )
                            .clicked()
                            {
                                action =
                                    Some(SheetBandAction::Manage(StructureAction::AddTable, None));
                            }
                        });
                    });
                let list_label = context_text(ui.ctx(), "dialog-table-list");
                ui.add_space(8.0);
                // A Markdown file shows its minimap beside the sheet instead of a menu.
                let minimap_available =
                    document.is_some_and(|document| app.minimap_source(document.id).is_some());
                if minimap_available {
                    if chrome::sheet_minimap_button(
                        ui,
                        &list_visible,
                        &list_label,
                        app.minimap_open,
                    )
                    .clicked()
                    {
                        action = Some(SheetBandAction::ToggleMinimap);
                    }
                } else {
                    chrome::sheet_table_list_button(ui, &list_visible, &list_label, |menu| {
                        visual::prepare_context_menu(menu);
                        let max_height =
                            (menu.ctx().content_rect().height() - 48.0).clamp(120.0, 360.0);
                        // Summaries are built only while the list is open.
                        let empty = text(app.language, "dialog-empty-table");
                        egui::ScrollArea::vertical()
                            .id_salt("table_list_choices")
                            .auto_shrink([true, true])
                            .max_height(max_height)
                            .show(menu, |menu| {
                                let Some(document) = document else {
                                    return;
                                };
                                for ((table_id, name, active), table) in
                                    tabs.iter().zip(&document.tables)
                                {
                                    let summary =
                                        table_summary(table).unwrap_or_else(|| empty.clone());
                                    if menu
                                        .selectable_label(*active, format!("{name} — {summary}"))
                                        .clicked()
                                    {
                                        action = Some(SheetBandAction::Select(*table_id));
                                        menu.close();
                                    }
                                }
                            });
                    });
                }
                // The sheet zoom of this window sits at the right end.
                let zoom_name = context_message(
                    ui.ctx(),
                    &Message::new("view-sheet-zoom").with_text("zoom", zoom_label.clone()),
                );
                if ui
                    .add_enabled_ui(!navigation_blocked, |ui| {
                        chrome::mode_tab_button(ui, false, &zoom_label, &zoom_name)
                    })
                    .inner
                    .on_hover_text(zoom_name.clone())
                    .clicked()
                {
                    action = Some(SheetBandAction::OpenZoom);
                }
            });
        });
    action
}

/// Names a table by its value nearest to A1, shortened for the table list.
pub(super) fn table_summary(table: &crate::document::Table) -> Option<String> {
    let (_, cell) = table.nearest_filled_cell()?;
    let text = cell.input.source_text();
    let compact = text.chars().take(28).collect::<String>();
    Some(if text.chars().count() > 28 {
        format!("{compact}…")
    } else {
        compact
    })
}

#[cfg(test)]
#[path = "sheet_tabs_test.rs"]
mod tests;
