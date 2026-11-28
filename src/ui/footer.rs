use super::*;
#[path = "footer/selection_count.rs"]
mod selection_count;
#[path = "footer/sheet_tabs.rs"]
mod sheet_tabs;

#[cfg(test)]
#[path = "footer/selection_count_test.rs"]
mod selection_count_tests;

const STATUS_ACTIONS_WIDTH: f32 = 104.0;

enum FooterAction {
    CancelStructure(DocumentId),
    CancelSave(DocumentId),
    CancelCalculation(DocumentId),
    Recalculate(DocumentId),
    CancelAutoFit(DocumentId),
    ShowImportReport(DocumentId),
    DismissImportNotice,
}

impl TexcelApp {
    /// Keeps table switching above a separate, compact document status area.
    pub(super) fn draw_bottom(&mut self, root: &mut egui::Ui, navigation_blocked: bool) {
        self.refresh_status_lifetime(root.ctx());
        let mut sheet_action = None;
        let mut sheet_tab_shown = None;
        let mut footer_action = None;
        let panel_fill = root.visuals().panel_fill;
        egui::Panel::bottom("texcel_footer")
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(panel_fill)
                    .inner_margin(egui::Margin::ZERO)
                    .stroke(Stroke::NONE),
            )
            .show(root, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.vertical(|ui| {
                    (sheet_action, sheet_tab_shown) =
                        sheet_tabs::draw_sheet_band(self, ui, navigation_blocked);
                    ui.separator();
                    egui::Frame::new()
                        .fill(ui.visuals().faint_bg_color)
                        .inner_margin(egui::Margin::symmetric(CHROME_EDGE_INSET, 4))
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.set_min_height(CHROME_STATUS_HEIGHT - 8.0);
                            let status_action =
                                draw_fixed_status_line(ui, self, navigation_blocked);
                            if footer_action.is_none() {
                                footer_action = status_action;
                            }
                        });
                });
            });
        if sheet_tab_shown.is_some() {
            self.sheet_tab_shown_table = sheet_tab_shown;
        }
        if let Some(action) = sheet_action {
            match action {
                sheet_tabs::SheetBandAction::Select(table_id) if !navigation_blocked => {
                    self.select_table_from_chrome(table_id);
                }
                sheet_tabs::SheetBandAction::Manage(action, context_table_id)
                    if !navigation_blocked =>
                {
                    self.request_table_management(action, context_table_id, None);
                }
                sheet_tabs::SheetBandAction::ToggleMinimap => {
                    self.minimap_open = !self.minimap_open;
                }
                sheet_tabs::SheetBandAction::OpenZoom if !navigation_blocked => {
                    self.open_sheet_zoom_dialog();
                }
                _ => (),
            }
        }
        if let Some(action) = footer_action {
            match action {
                FooterAction::CancelStructure(document_id) => {
                    self.cancel_structure_for(document_id);
                }
                FooterAction::CancelSave(document_id) => self.request_cancel_save(document_id),
                FooterAction::CancelCalculation(document_id) => {
                    self.cancel_calculation(document_id);
                }
                FooterAction::Recalculate(document_id) => self.retry_calculation(document_id),
                FooterAction::CancelAutoFit(document_id) => {
                    self.cancel_auto_fit_for(document_id);
                }
                FooterAction::ShowImportReport(document_id) => {
                    self.reopen_import_report(document_id);
                }
                FooterAction::DismissImportNotice => self.import_notice = None,
            }
        }
    }
}

impl TexcelApp {
    /// Switches tables from a sheet tab, the table list, or the minimap after finishing
    /// the cell edit.
    pub(super) fn select_table_from_chrome(&mut self, table_id: TableId) {
        let document_id = self.workspace.active_document;
        if self.edit.is_editing() {
            self.commit_edit(None);
        }
        if self.edit.is_editing() {
            return;
        }
        if let Some(document_id) = document_id
            && self.pending_text_growth_sessions.contains_key(&document_id)
        {
            self.pending_growth_selection = Some(grid_events::PendingGrowthSelection::Table {
                document_id,
                table_id,
            });
        } else if let Some(document) = self.workspace.active_document_mut() {
            document.select_table(table_id);
        }
        self.grid_focused = false;
        self.request_grid_focus = true;
        self.request_scroll_to_selection = false;
    }
}

/// Reserves a fixed row so long names cannot push status or selection details offscreen.
fn draw_fixed_status_line(
    ui: &mut egui::Ui,
    app: &TexcelApp,
    navigation_blocked: bool,
) -> Option<FooterAction> {
    let total_width = ui.available_width().max(0.0);
    let row_height = CHROME_STATUS_HEIGHT - 8.0;
    let (_, row_rect) = ui.allocate_space(Vec2::new(total_width, row_height));
    let selection_label = selection_count::active_selection_count(app)
        .filter(|count| count.cells > 1)
        .map(|count| {
            Message::new("status-selection-count")
                .with_text("count", selection_count::grouped_number(count.cells))
                .with_text("rows", selection_count::grouped_number(count.rows))
                .with_text("columns", selection_count::grouped_number(count.columns))
                .resolve(app.language)
        });

    let mut status_parts = Vec::new();
    let mut hover_parts = Vec::new();
    let active_import_notice = app.workspace.active_document().and_then(|document| {
        app.import_notice
            .as_ref()
            .filter(|(owner, _)| *owner == document.id)
    });
    // The import notice offers its report only while no newer status hides it.
    let notice_document = active_import_notice
        .filter(|_| app.status.is_none())
        .map(|(owner, _)| *owner);
    if let Some(status) = app
        .status
        .as_ref()
        .or(active_import_notice.map(|(_, notice)| notice))
    {
        let status = status.resolve(app.language);
        status_parts.push(status.clone());
        hover_parts.push(status);
    }
    let visible_status = status_parts.join(" · ");
    let full_status = hover_parts.join(" · ");

    let count_color = ui.visuals().text_color();
    let count_width = selection_label.as_ref().map_or(0.0, |label| {
        ui.fonts_mut(|fonts| {
            fonts
                .layout_no_wrap(label.clone(), egui::FontId::proportional(12.0), count_color)
                .size()
                .x
                + 12.0
        })
    });
    let control_width = STATUS_ACTIONS_WIDTH + count_width;
    let details_label = context_text(ui.ctx(), "action-import-details");
    let close_notice_label = context_text(ui.ctx(), "action-close-notice");
    let notice_width =
        notice_document.map_or(0.0, |_| chrome::command_width(ui, &details_label) + 32.0);
    let status_width = (total_width - control_width - notice_width).max(0.0);
    let status_rect = egui::Rect::from_min_size(row_rect.min, Vec2::new(status_width, row_height));
    let controls_rect = egui::Rect::from_min_max(
        egui::pos2(
            (row_rect.right() - control_width).max(row_rect.left()),
            row_rect.top(),
        ),
        row_rect.right_bottom(),
    );
    let mut left = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(status_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    left.add(
        egui::Label::new(visible_status)
            .truncate()
            .halign(egui::Align::Min),
    )
    .on_hover_text(full_status);

    let mut action = None;
    // Keep the notice buttons right after the message so they never cover the controls.
    if let Some(owner) = notice_document {
        let notice_rect = egui::Rect::from_min_size(
            egui::pos2(status_rect.right(), row_rect.top()),
            Vec2::new(notice_width, row_height),
        );
        let mut notice = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(notice_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        notice.spacing_mut().item_spacing.x = 2.0;
        if notice
            .add(egui::Button::new(details_label).small())
            .clicked()
        {
            action = Some(FooterAction::ShowImportReport(owner));
        }
        if chrome::status_action_button(
            &mut notice,
            &close_notice_label,
            chrome::CommandIcon::Cancel,
            true,
        )
        .clicked()
        {
            action = Some(FooterAction::DismissImportNotice);
        }
    }
    let mut controls = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(controls_rect)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    controls.spacing_mut().item_spacing.x = 2.0;
    let document_id = app.workspace.active_document().map(|document| document.id);
    let blocked = navigation_blocked
        || document_id
            .is_some_and(|id| app.cell_edit_locked(id) || app.close_after_save.contains(&id));
    let cancel_blocked =
        navigation_blocked || document_id.is_some_and(|id| app.close_after_save.contains(&id));
    if let Some(label) = selection_label {
        controls.add(egui::Label::new(label).truncate());
    }
    if let Some(document_id) = document_id {
        if app.pending_auto_fits.contains_key(&document_id)
            || app.deferred_auto_fits.contains_key(&document_id)
        {
            let label = context_text(controls.ctx(), "action-cancel-autofit");
            let enabled = !cancel_blocked;
            if chrome::status_action_button(
                &mut controls,
                &label,
                chrome::CommandIcon::Cancel,
                enabled,
            )
            .clicked()
            {
                action = Some(FooterAction::CancelAutoFit(document_id));
            }
        }
        if app.pending_calculations.contains_key(&document_id)
            || app.pending_initial_calculations.contains_key(&document_id)
            || app.calculation_retries.contains(&document_id)
        {
            let label = context_text(controls.ctx(), "action-cancel-calculation");
            if chrome::status_action_button(
                &mut controls,
                &label,
                chrome::CommandIcon::Cancel,
                !cancel_blocked,
            )
            .clicked()
            {
                action = Some(FooterAction::CancelCalculation(document_id));
            }
        } else if app.calculation_retry_available.contains(&document_id) {
            let label = context_text(controls.ctx(), "action-recalculate");
            if chrome::status_action_button(
                &mut controls,
                &label,
                chrome::CommandIcon::Recalculate,
                !blocked,
            )
            .clicked()
            {
                action = Some(FooterAction::Recalculate(document_id));
            }
        }
        if app.pending_saves.contains_key(&document_id) {
            let label = context_text(controls.ctx(), "action-cancel-save");
            let enabled = !cancel_blocked && !app.cancelling_saves.contains_key(&document_id);
            if chrome::status_action_button(
                &mut controls,
                &label,
                chrome::CommandIcon::Cancel,
                enabled,
            )
            .clicked()
            {
                action = Some(FooterAction::CancelSave(document_id));
            }
        }
        if app.pending_structures.contains_key(&document_id) {
            let label = context_text(controls.ctx(), "top-cancel-preparation");
            if chrome::status_action_button(
                &mut controls,
                &label,
                chrome::CommandIcon::Cancel,
                !cancel_blocked,
            )
            .clicked()
            {
                action = Some(FooterAction::CancelStructure(document_id));
            }
        }
    }
    action
}
