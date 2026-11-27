use super::*;

#[path = "virtual_grid/canvas.rs"]
mod canvas;
#[path = "virtual_grid/cells.rs"]
mod cells;
#[path = "virtual_grid/frame.rs"]
mod frame;
#[path = "virtual_grid/headers.rs"]
mod headers;
#[path = "virtual_grid/measure.rs"]
mod measure;
#[path = "virtual_grid/overlays.rs"]
mod overlays;
#[path = "virtual_grid/selection_drag.rs"]
mod selection_drag;
use self::frame::GridViewportFrame;
use self::selection_drag::update_selection_drag;

// These inputs are borrowed only for an egui frame to preserve TextEdit, selection, and scroll state.

// Draw visible grid cells, headers, and overlays in their established order.
pub(super) fn draw_virtual_grid(
    ui: &mut egui::Ui,
    grid: GridTable<'_>,
    display: GridDisplay,
    row_layout: &mut GridRowLayout,
    input: GridInput<'_>,
    state: GridState<'_>,
) -> GridOutcome {
    let document = grid.document;
    let document_id = document.id;
    let table_id = grid.table.id;
    let layout = grid.layout;
    let view = document.view(table_id);
    let selection = view.map_or(CellRange::default(), |view| view.selection);
    let active_cell = view.map_or(CellAddress::default(), |view| view.active_cell());
    let additional_selections = view.map_or(&[][..], |view| view.additional_selections.as_slice());
    let scroll = view.map_or(crate::document::ScrollPosition::default(), |view| {
        view.scroll
    });
    let header_row = document.header_row(table_id);
    let row_count = MAX_ROWS;
    let column_count = MAX_COLUMNS;
    row_layout.sync_layout_revision(layout.revision());
    row_layout.sync_display_generation(display.font_layout_generation);
    row_layout.sync_text_display(display.mode, display.show_invisibles);
    row_layout.sync_scale(display.scale);
    let header_height = grid_header_height(display.scale);
    row_layout.ensure_row_count(row_count);
    let viewport_size = ui.available_size();
    let frozen = layout.frozen();
    let frozen_row_count = frozen.rows.min(row_count);
    let frozen_column_count = frozen.columns.min(column_count);
    let (adjusted_scroll, row_header_width) = measure::adjust_visible_row_heights(
        ui,
        grid,
        display,
        row_layout,
        scroll,
        viewport_size,
        state.markdown_cell_displays,
    );
    let wheel_delta = ui.input(|input| input.smooth_scroll_delta());
    let pending_scroll_ahead = Vec2::new((-wheel_delta.x).max(0.0), (-wheel_delta.y).max(0.0));
    // The saved rectangle: its headers are drawn stronger than the rest of the sheet.
    let extent_end = document.extent_end(table_id).unwrap_or_default();
    let total_size = canvas::canvas_size(
        extent_end,
        row_layout,
        layout,
        viewport_size,
        row_header_width,
        canvas::CanvasScroll {
            offset: Vec2::new(adjusted_scroll.x, adjusted_scroll.y),
            pending_ahead: pending_scroll_ahead,
            target: input
                .scroll_to_address
                .or_else(|| (*state.request_scroll_to_selection).then_some(active_cell)),
        },
    );
    grid_theme::prepare_grid_scrollbars(ui);
    let output = egui::ScrollArea::both()
        .id_salt(("texcel_grid", document_id, table_id))
        .scroll_offset(Vec2::new(adjusted_scroll.x, adjusted_scroll.y))
        .auto_shrink([false, false])
        .show_viewport(ui, |grid_ui, viewport| {
            grid_ui.set_min_size(total_size);
            let origin = grid_ui.max_rect().min;
            let painter = grid_ui.painter().clone();
            let frozen = layout.frozen();
            let viewport_origin = origin + viewport.min.to_vec2();
            let frozen_height = row_layout.prefix_height(frozen_row_count);
            let frozen_width = layout.prefix_column_width_pixels(frozen_column_count) as f32;
            // The scroll offset belongs to the full canvas (including headers and frozen bands).
            // Fixed bands are overlaid at the viewport origin; scrolling bands use the raw offset.
            let effective_scroll_y = viewport.min.y;
            let effective_scroll_x = viewport.min.x;
            let pinned_rows = visible_rows_in_pixels(
                row_layout,
                0.0,
                (viewport.height() - header_height)
                    .max(0.0)
                    .min(frozen_height),
                frozen_row_count,
            );
            let scrolling_rows = visible_rows_in_pixels(
                row_layout,
                frozen_height + effective_scroll_y,
                frozen_height
                    + effective_scroll_y
                    + (viewport.height() - header_height - frozen_height).max(0.0),
                row_count,
            )
            .into_iter()
            .filter(|row| *row >= frozen_row_count)
            .collect::<Vec<_>>();
            let pinned_columns = visible_columns_in_pixels(
                layout,
                column_count,
                0.0,
                (viewport.width() - row_header_width).max(0.0),
            )
            .into_iter()
            .filter(|column| *column < frozen_column_count)
            .collect::<Vec<_>>();
            let scrolling_columns = visible_columns_in_pixels(
                layout,
                column_count,
                frozen_width + effective_scroll_x,
                frozen_width
                    + effective_scroll_x
                    + (viewport.width() - row_header_width - frozen_width).max(0.0),
            )
            .into_iter()
            .filter(|column| *column >= frozen_column_count)
            .collect::<Vec<_>>();
            let viewport_rect = Rect::from_min_size(viewport_origin, viewport.size());
            let body_min = Pos2::new(
                (viewport_origin.x + row_header_width).min(viewport_rect.max.x),
                (viewport_origin.y + header_height).min(viewport_rect.max.y),
            );
            let frozen_x = (body_min.x + frozen_width).clamp(body_min.x, viewport_rect.max.x);
            let frozen_y = (body_min.y + frozen_height).clamp(body_min.y, viewport_rect.max.y);
            let clip = |min_x: f32, max_x: f32, min_y: f32, max_y: f32| {
                Rect::from_min_max(
                    Pos2::new(
                        min_x.clamp(body_min.x, viewport_rect.max.x),
                        min_y.clamp(body_min.y, viewport_rect.max.y),
                    ),
                    Pos2::new(
                        max_x.clamp(body_min.x, viewport_rect.max.x),
                        max_y.clamp(body_min.y, viewport_rect.max.y),
                    ),
                )
            };
            let pane_zones = [
                GridPaneZone {
                    rows: 0..frozen_row_count,
                    columns: 0..frozen_column_count,
                    clip_rect: clip(body_min.x, frozen_x, body_min.y, frozen_y),
                    frozen_rows: true,
                    frozen_columns: true,
                },
                GridPaneZone {
                    rows: 0..frozen_row_count,
                    columns: frozen_column_count..column_count,
                    clip_rect: clip(frozen_x, viewport_rect.max.x, body_min.y, frozen_y),
                    frozen_rows: true,
                    frozen_columns: false,
                },
                GridPaneZone {
                    rows: frozen_row_count..row_count,
                    columns: 0..frozen_column_count,
                    clip_rect: clip(body_min.x, frozen_x, frozen_y, viewport_rect.max.y),
                    frozen_rows: false,
                    frozen_columns: true,
                },
                GridPaneZone {
                    rows: frozen_row_count..row_count,
                    columns: frozen_column_count..column_count,
                    clip_rect: clip(frozen_x, viewport_rect.max.x, frozen_y, viewport_rect.max.y),
                    frozen_rows: false,
                    frozen_columns: false,
                },
            ];
            let mut visible_rows = pinned_rows;
            visible_rows.extend(scrolling_rows);
            visible_rows.sort_unstable();
            visible_rows.dedup();
            let mut visible_columns = pinned_columns;
            visible_columns.extend(scrolling_columns);
            visible_columns.sort_unstable();
            visible_columns.dedup();
            let render_cells = visible_rows
                .iter()
                .flat_map(|row| visible_columns.iter().map(move |column| (*row, *column)))
                .collect::<BTreeSet<_>>();
            if *state.request_scroll_to_selection {
                let selected_cell = active_cell;
                let selected_visible = visible_rows.contains(&selected_cell.row)
                    && visible_columns.contains(&selected_cell.column);
                if !selected_visible {
                    let x = if selected_cell.column < frozen.columns {
                        viewport.min.x
                            + row_header_width
                            + layout.prefix_column_width_pixels(selected_cell.column) as f32
                    } else {
                        viewport.min.x
                            + row_header_width
                            + layout.prefix_column_width_pixels(selected_cell.column) as f32
                            - effective_scroll_x
                    };
                    let y = if selected_cell.row < frozen.rows {
                        viewport.min.y + header_height + row_layout.prefix_height(selected_cell.row)
                    } else {
                        viewport.min.y + header_height + row_layout.prefix_height(selected_cell.row)
                            - effective_scroll_y
                    };
                    let selected_rect = Rect::from_min_size(
                        origin + Vec2::new(x, y),
                        Vec2::new(
                            layout.column_width_pixels(selected_cell.column) as f32,
                            row_layout.row_height(selected_cell.row),
                        ),
                    );
                    grid_ui.scroll_to_rect(selected_rect, Some(egui::Align::Center));
                } else {
                    *state.request_scroll_to_selection = false;
                }
            }
            if let Some(target) = input.scroll_to_address
                && target.row < row_count
                && target.column < column_count
                && (!visible_rows.contains(&target.row)
                    || !visible_columns.contains(&target.column))
            {
                let x = if target.column < frozen.columns {
                    viewport.min.x
                        + row_header_width
                        + layout.prefix_column_width_pixels(target.column) as f32
                } else {
                    viewport.min.x
                        + row_header_width
                        + layout.prefix_column_width_pixels(target.column) as f32
                        - effective_scroll_x
                };
                let y = if target.row < frozen.rows {
                    viewport.min.y + header_height + row_layout.prefix_height(target.row)
                } else {
                    viewport.min.y + header_height + row_layout.prefix_height(target.row)
                        - effective_scroll_y
                };
                let target_rect = Rect::from_min_size(
                    origin + Vec2::new(x, y),
                    Vec2::new(
                        layout.column_width_pixels(target.column) as f32,
                        row_layout.row_height(target.row),
                    ),
                );
                grid_ui.scroll_to_rect(target_rect, Some(egui::Align::Center));
            }
            let visuals = grid_ui.visuals().clone();
            let palette = GridThemePalette::from_visuals(&visuals);
            let mut outcome = GridOutcome {
                table_id,
                ..GridOutcome::default()
            };
            let row_corner =
                Rect::from_min_size(viewport_origin, Vec2::new(row_header_width, header_height));
            painter.rect_filled(row_corner, 0.0, palette.header_background);
            painter.rect_stroke(
                row_corner,
                0.0,
                Stroke::new(0.75, palette.grid),
                egui::StrokeKind::Inside,
            );
            // The corner above the row numbers selects every cell, as Ctrl/Cmd+A does.
            let corner = grid_ui.interact(
                row_corner,
                Id::unique(egui::IdSalt::new((
                    "grid_select_all_corner",
                    document_id,
                    table_id,
                ))),
                Sense::click(),
            );
            let corner_label = crate::i18n::context_text(grid_ui.ctx(), "grid-select-all-corner");
            corner
                .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &corner_label));
            if corner.clicked() {
                outcome.selected_range = Some(CellRange {
                    anchor: CellAddress::default(),
                    head: CellAddress {
                        row: row_count - 1,
                        column: column_count - 1,
                    },
                });
                corner.request_focus();
            }
            outcome.focused |= corner.has_focus();
            grid_focus::lock_grid_navigation(grid_ui, &corner);
            let selected_ranges = std::iter::once(selection)
                .chain(additional_selections.iter().copied())
                .map(CellRange::normalized)
                .collect::<Vec<_>>();
            let selection_drag = input.selection_drag;
            let mut frame = GridViewportFrame {
                grid,
                display,
                input,
                state,
                document_id,
                table_id,
                row_layout,
                row_header_width,
                selection,
                active_cell,
                header_row,
                frozen_row_count,
                frozen_column_count,
                row_count,
                column_count,
                extent_end,
                effective_scroll_x,
                effective_scroll_y,
                viewport_rect,
                viewport_origin,
                body_min,
                frozen_x,
                frozen_y,
                frozen_width,
                frozen_height,
                visible_rows,
                visible_columns,
                render_cells,
                pane_zones,
                selected_ranges,
                fill_preview: None,
                visuals,
                palette,
                header_height,
            };
            outcome.fill_handle_preview = frame.fill_handle_preview(grid_ui);
            frame.fill_preview = outcome.fill_handle_preview.map(CellRange::normalized);
            frame.draw_column_headers(grid_ui, &painter, &mut outcome);
            frame.draw_row_headers(grid_ui, &painter, &mut outcome);
            frame.draw_visible_cells(grid_ui, &painter, &mut outcome);
            frame.draw_frozen_guides_and_fill(grid_ui, &painter, &mut outcome);
            let body_page = Vec2::new(
                (viewport.width() - row_header_width - frozen_width).max(0.0),
                (viewport.height() - header_height - frozen_height).max(0.0),
            );
            outcome.page_extent = Some(body_page);
            update_selection_drag(
                grid_ui,
                &mut outcome,
                selection_drag,
                GridPointerGeometry {
                    viewport: viewport_rect,
                    body_min,
                    scroll_x: effective_scroll_x,
                    scroll_y: effective_scroll_y,
                    frozen_width,
                    frozen_height,
                    layout,
                    row_layout,
                    rows: row_count,
                    columns: column_count,
                },
                document_id,
                table_id,
            );
            outcome
        });
    let mut outcome = output.inner;
    let offset = output.state.clamped_offset();
    outcome.scroll = Some(crate::document::ScrollPosition {
        x: offset.x,
        y: offset.y,
    });
    outcome
}
