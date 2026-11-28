use super::*;

const PREVIEW_ROW_HEIGHT: f32 = 26.0;
const PREVIEW_HEADER_HEIGHT: f32 = 28.0;
const PREVIEW_ROW_HEADER_WIDTH: f32 = 52.0;
const PREVIEW_COLUMN_WIDTH: f32 = 180.0;

impl TexcelApp {
    /// Shows one active load without inserting its partial data into the workspace.
    pub(super) fn draw_load_preview(&mut self, ctx: &egui::Context) {
        let Some(document_id) = self
            .pending_load_previews
            .keys()
            .copied()
            .find(|document_id| self.load_request_belongs_to_current_viewport(*document_id))
        else {
            return;
        };
        let Some(state) = self.pending_load_previews.get_mut(&document_id) else {
            return;
        };
        let token = state.token;
        let title = state.path.file_name().map_or_else(
            || context_text(ctx, "load-preview-title"),
            |name| name.to_string_lossy().into_owned(),
        );
        let mut cancel = false;
        egui::Window::new("")
            .id(Id::unique(("load_preview", document_id)))
            .title_bar(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .default_size(Vec2::new(640.0, 520.0))
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                let width = (ctx.content_rect().width() - 40.0).clamp(320.0, 760.0);
                ui.set_min_width(width.min(520.0));
                ui.set_max_width(width);
                ui.set_max_height((ctx.content_rect().height() - 64.0).max(200.0));
                visual::dialog_surface(ui, |ui| {
                    visual::prepare_dialog_body(ui);
                    visual::dialog_title(ui, &title);
                    visual::group_gap(ui);
                    let footer_height = visual::BUTTON_HEIGHT + 2.0 * visual::SPACE_8;
                    let body_height = (ui.available_height() - footer_height).max(120.0);
                    egui::ScrollArea::vertical()
                        .id_salt(("load_preview_body", document_id))
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                        )
                        .max_height(body_height)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.label(context_text(ui.ctx(), stage_message(state.stage)));
                            visual::caption(ui, &context_text(ui.ctx(), "load-preview-read-only"));
                            draw_preview_table_choice(ui, state, document_id);
                            let grid_height =
                                (ui.available_height() - 2.0 * visual::BUTTON_HEIGHT).max(120.0);
                            draw_preview_grid(ui, state, grid_height);
                        });
                    ui.separator();
                    visual::dialog_footer(ui, |ui| {
                        if ui.button(context_text(ui.ctx(), "action-cancel")).clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        if cancel {
            self.cancel_load_preview(document_id, token);
        }
    }

    /// Cancels one load and restores any external-change prompt it was serving.
    fn cancel_load_preview(&mut self, document_id: DocumentId, token: JobToken) {
        self.file_worker.cancel(token);
        self.pending_load_previews.remove(&document_id);
        self.load_request_viewports.remove(&document_id);
        self.pending_file_load_paths.remove(&document_id);
        if self
            .pending_csv_opens
            .get(&document_id)
            .is_some_and(|pending| pending.job_id == token.job_id)
        {
            self.pending_csv_opens.remove(&document_id);
        }
        if self.pending_markdown_loads.get(&document_id) == Some(&token) {
            self.pending_markdown_loads.remove(&document_id);
        }
        if self.pending_html_loads.get(&document_id) == Some(&token) {
            self.pending_html_loads.remove(&document_id);
        }
        if self.pending_workbook_loads.get(&document_id) == Some(&token) {
            self.pending_workbook_loads.remove(&document_id);
        }
    }
}

/// Returns the current localized phase name for one loader.
fn stage_message(stage: LoadStage) -> &'static str {
    match stage {
        LoadStage::ReadingSource => "load-preview-stage-reading",
        LoadStage::ParsingSource => "load-preview-stage-parsing",
        LoadStage::ParsingWorkbook => "load-preview-stage-workbook",
    }
}

/// Draws a selector only when the worker has confirmed more than one table.
fn draw_preview_table_choice(
    ui: &mut egui::Ui,
    state: &mut LoadPreviewState,
    document_id: DocumentId,
) {
    let tables = state
        .tables
        .iter()
        .map(|(index, table)| (*index, table.name.clone()))
        .collect::<Vec<_>>();
    if tables.is_empty() {
        return;
    }
    if !tables
        .iter()
        .any(|(index, _)| *index == state.selected_table)
    {
        state.selected_table = tables[0].0;
    }
    if tables.len() > 1 {
        ui.horizontal(|ui| {
            ui.label(context_text(ui.ctx(), "load-preview-table"));
            egui::ComboBox::from_id_salt(("load_preview_table", document_id))
                .selected_text(
                    tables
                        .iter()
                        .find(|(index, _)| *index == state.selected_table)
                        .map_or_else(String::new, |(_, name)| name.clone()),
                )
                .show_ui(ui, |ui| {
                    visual::prepare_context_menu(ui);
                    for (index, name) in &tables {
                        ui.selectable_value(&mut state.selected_table, *index, name);
                    }
                });
        });
    } else if let Some((_, name)) = tables.first() {
        ui.label(name);
    }
}

/// Paints only rows and columns intersecting the two-dimensional viewport.
fn draw_preview_grid(ui: &mut egui::Ui, state: &LoadPreviewState, max_height: f32) {
    let Some(table) = state.tables.get(&state.selected_table) else {
        return;
    };
    let total_width = PREVIEW_ROW_HEADER_WIDTH + table.column_count as f32 * PREVIEW_COLUMN_WIDTH;
    let total_height = PREVIEW_HEADER_HEIGHT + table.row_count as f32 * PREVIEW_ROW_HEIGHT;
    egui::ScrollArea::both()
        .id_salt((
            "load_preview_grid",
            state.token.document_id,
            state.selected_table,
        ))
        .auto_shrink([false, false])
        .max_height(max_height)
        .show_viewport(ui, |canvas, viewport| {
            canvas.set_min_size(Vec2::new(total_width, total_height));
            let origin = canvas.max_rect().min;
            let clip = Rect::from_min_size(origin + viewport.min.to_vec2(), viewport.size());
            let painter = canvas.painter().with_clip_rect(clip);
            draw_preview_headers(canvas, &painter, origin, viewport, table);
            let rows = visible_range(
                viewport.min.y,
                viewport.max.y,
                PREVIEW_HEADER_HEIGHT,
                PREVIEW_ROW_HEIGHT,
                table.row_count,
            );
            let columns = visible_range(
                viewport.min.x,
                viewport.max.x,
                PREVIEW_ROW_HEADER_WIDTH,
                PREVIEW_COLUMN_WIDTH,
                table.column_count,
            );
            for row_index in rows {
                let y = origin.y + PREVIEW_HEADER_HEIGHT + row_index as f32 * PREVIEW_ROW_HEIGHT;
                let row_header = Rect::from_min_size(
                    Pos2::new(origin.x, y),
                    Vec2::new(PREVIEW_ROW_HEADER_WIDTH, PREVIEW_ROW_HEIGHT),
                );
                paint_preview_cell(
                    canvas,
                    &painter,
                    row_header,
                    &(row_index + 1).to_string(),
                    true,
                );
                let Some(row) = table.row(row_index) else {
                    continue;
                };
                for column in columns.clone() {
                    let x =
                        origin.x + PREVIEW_ROW_HEADER_WIDTH + column as f32 * PREVIEW_COLUMN_WIDTH;
                    let rect = Rect::from_min_size(
                        Pos2::new(x, y),
                        Vec2::new(PREVIEW_COLUMN_WIDTH, PREVIEW_ROW_HEIGHT),
                    );
                    let text = row
                        .cells
                        .binary_search_by_key(&column, |cell| cell.column)
                        .ok()
                        .map(|index| row.cells[index].text.as_str())
                        .unwrap_or("");
                    paint_preview_cell(canvas, &painter, rect, text, false);
                }
            }
        });
}

/// Draws the visible column headings and the row-header corner.
fn draw_preview_headers(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    origin: Pos2,
    viewport: Rect,
    table: &LoadPreviewTable,
) {
    let corner = Rect::from_min_size(
        origin,
        Vec2::new(PREVIEW_ROW_HEADER_WIDTH, PREVIEW_HEADER_HEIGHT),
    );
    paint_preview_cell(ui, painter, corner, "#", true);
    for column in visible_range(
        viewport.min.x,
        viewport.max.x,
        PREVIEW_ROW_HEADER_WIDTH,
        PREVIEW_COLUMN_WIDTH,
        table.column_count,
    ) {
        let x = origin.x + PREVIEW_ROW_HEADER_WIDTH + column as f32 * PREVIEW_COLUMN_WIDTH;
        let rect = Rect::from_min_size(
            Pos2::new(x, origin.y),
            Vec2::new(PREVIEW_COLUMN_WIDTH, PREVIEW_HEADER_HEIGHT),
        );
        paint_preview_cell(ui, painter, rect, &(column + 1).to_string(), true);
    }
}

/// Draws one bordered preview cell and clips long text to the normal grid limit.
fn paint_preview_cell(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    rect: Rect,
    text: &str,
    header: bool,
) {
    painter.rect_filled(
        rect,
        0.0,
        if header {
            ui.visuals().faint_bg_color
        } else {
            ui.visuals().extreme_bg_color
        },
    );
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(0.75, ui.visuals().widgets.noninteractive.bg_stroke.color),
        egui::StrokeKind::Inside,
    );
    let layout = if header {
        egui::Layout::centered_and_justified(egui::Direction::LeftToRight)
    } else {
        egui::Layout::left_to_right(egui::Align::Center)
    };
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(6.0, 2.0)))
            .layout(layout),
        |cell_ui| {
            cell_ui.add(
                egui::Label::new(text_preview(text, MAX_GRID_TEXT_BYTES))
                    .truncate()
                    .halign(egui::Align::LEFT),
            );
        },
    );
}

/// Computes the visible range without visiting off-screen rows or columns.
fn visible_range(
    viewport_start: f32,
    viewport_end: f32,
    leading_space: f32,
    item_size: f32,
    total: usize,
) -> std::ops::Range<usize> {
    let start = ((viewport_start - leading_space).max(0.0) / item_size).floor() as usize;
    let end = ((viewport_end - leading_space).max(0.0) / item_size).ceil() as usize;
    start.min(total)..end.min(total).max(start.min(total))
}

#[cfg(test)]
#[path = "load_preview_test.rs"]
mod tests;
