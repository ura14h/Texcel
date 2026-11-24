use super::*;
#[path = "grid_segment/context_menu.rs"]
mod context_menu;
#[path = "formula_reference_paint.rs"]
mod formula_reference_paint;
#[path = "grid_segment_paint.rs"]
mod grid_segment_paint;

/// Grid-wide inputs one cell segment needs, borrowed from the frame for a single call.
pub(super) struct SegmentGrid<'f> {
    pub(super) grid: GridTable<'f>,
    pub(super) display: GridDisplay,
    pub(super) input: &'f GridInput<'f>,
    pub(super) selection: CellRange,
    pub(super) active_cell: CellAddress,
    pub(super) edit: &'f mut EditSession,
    pub(super) request_editor_focus: &'f mut bool,
    pub(super) request_grid_focus: &'f mut bool,
}

/// One visible cell and what to draw in it.
pub(super) struct CellSegment<'s> {
    pub(super) rect: Rect,
    pub(super) visible_rect: Rect,
    pub(super) address: CellAddress,
    pub(super) cell: Option<&'s Cell>,
    pub(super) calculation: Option<&'s crate::document::CalcResult>,
    pub(super) is_formula: bool,
    /// True when the cell holds a number or text that calculations read as a number.
    pub(super) semantic_numeric_input: bool,
    pub(super) selected: bool,
    /// Which sides of the selection outline, or of a fill-handle preview, pass along this
    /// cell: top, bottom, left, right.
    pub(super) selected_edges: [bool; 4],
    /// Which frozen pane holds the cell, used in the cell's widget id.
    pub(super) index: usize,
    pub(super) column_header: &'s str,
    pub(super) row_header: &'s str,
    pub(super) header_bold: bool,
    pub(super) markdown_display: Option<&'s MarkdownCellDisplay>,
    pub(super) reference_overlays: &'s [FormulaReferenceOverlay],
}

/// Paints one visible cell: its background, text or editor, selection, and interactions.
pub(super) fn draw_grid_cell_segment(
    grid_ui: &mut egui::Ui,
    painter: &egui::Painter,
    context: SegmentGrid<'_>,
    segment: CellSegment<'_>,
    outcome: &mut GridOutcome,
) {
    let SegmentGrid {
        grid,
        display,
        input,
        selection: selection_range,
        active_cell,
        edit,
        request_editor_focus,
        request_grid_focus,
    } = context;
    let CellSegment {
        rect,
        visible_rect,
        address: cell_address,
        cell,
        calculation,
        is_formula,
        semantic_numeric_input,
        selected,
        selected_edges,
        index: segment_index,
        column_header,
        row_header,
        header_bold,
        markdown_display,
        reference_overlays: formula_reference_overlays,
    } = segment;
    let document_id = grid.document.id;
    let table_id = grid.table.id;
    let table_name = grid.table.name.as_str();
    let is_markdown = grid.is_markdown;
    let markdown_source = grid.markdown_source;
    let markdown_link_target =
        markdown_display.and_then(|display| display.first_link_target.as_deref());
    let markdown_column_alignment = markdown_source.and_then(|source| {
        source.column_alignment(grid.markdown_table_index?, cell_address.column)
    });
    let display_mode = display.mode;
    let navigation_blocked = input.navigation_blocked;
    let formula_reference_mode = input.formula_reference_mode;
    let structure_block_reason = input.structure_block_reason;
    let visuals = grid_ui.visuals().clone();
    let palette = GridThemePalette::from_visuals(&visuals);
    let mut cell_style = CellTextStyle {
        wrap_text: display.wrap_text,
        alignment: None,
    };
    apply_markdown_alignment(
        &mut cell_style,
        is_markdown.then_some(markdown_column_alignment).flatten(),
        semantic_numeric_input,
        is_formula,
        calculation,
    );
    let cell_background = grid_segment_paint::paint_cell_base(
        painter,
        rect,
        cell_address.row,
        selected,
        cell_address == active_cell,
        &palette,
    );
    // Remember where the active cell is so the OS can show IME candidates next to it.
    if cell_address == active_cell {
        outcome.active_cell_rect = Some(visible_rect);
    }
    let editing_this_cell = input.allow_edit
        && edit.target == Some(crate::document::EditTarget::Cell)
        && edit.table_id == Some(table_id)
        && edit.address == Some(cell_address);
    let mut fill_handle_rect = None;
    if editing_this_cell {
        let editor_rect = visible_rect.shrink(3.0);
        let editor_id = editor_id(document_id, edit.session_id);
        let output = grid_ui
            .scope_builder(egui::UiBuilder::new().max_rect(editor_rect), |editor_ui| {
                editor_ui.set_clip_rect(visible_rect);
                editor_ui.style_mut().visuals.override_text_color = Some(palette.foreground);
                let editor_ctx = editor_ui.ctx().clone();
                // Focus the new editor before it reads this frame's first IME event.
                // A repeated request would interrupt the platform IME, so skip a focused editor.
                if *request_editor_focus && !editor_ctx.memory(|memory| memory.has_focus(editor_id))
                {
                    editor_ctx.memory_mut(|memory| memory.request_focus(editor_id));
                }
                show_bounded_edit_text(
                    editor_ui,
                    &editor_ctx,
                    editor_id,
                    edit,
                    CellEditWidget::Singleline {
                        width: editor_rect.width() - 2.0,
                        frame_none: true,
                    },
                )
            })
            .inner;
        let response = output.response;
        let accessibility_label = grid_cell_accessibility_label(
            grid_ui.ctx(),
            table_name,
            cell_address,
            column_header,
            row_header,
            selected,
            true,
        );
        register_editing_cell_accessibility(
            grid_ui.ctx(),
            &response,
            &accessibility_label,
            cell_address,
            selected,
        );
        outcome.text_selection = output
            .selection_bytes
            .map(|selection| (cell_address, selection));
        if *request_editor_focus {
            if !response.has_focus() {
                response.request_focus();
            }
            *request_editor_focus = false;
        }
        outcome.focused |= response.has_focus();
    } else {
        let editing_same_cell = edit.is_editing()
            && edit.table_id == Some(table_id)
            && edit.address == Some(cell_address);
        let previewing_edit = editing_same_cell
            && (edit.target == Some(crate::document::EditTarget::FormulaBar)
                || edit.target == Some(crate::document::EditTarget::DetailPanel)
                || !input.allow_edit);
        let text = if previewing_edit {
            std::borrow::Cow::Borrowed(edit.buffer.as_str())
        } else {
            cell.map_or_else(
                || std::borrow::Cow::Borrowed(""),
                |cell| {
                    cell_display_text(
                        cell,
                        calculation,
                        display_mode,
                        is_markdown && !is_formula,
                        is_formula,
                    )
                },
            )
        };
        let text = bounded_grid_text(text);
        let calculation_status = calculation
            .map(|result| result.status)
            .or_else(|| (input.calculation_pending && is_formula).then_some(CalcStatus::Pending))
            .or_else(|| is_formula.then_some(CalcStatus::Stale));
        if !text.is_empty() {
            paint_cell_text(
                painter,
                visible_rect,
                CellTextPaint {
                    cell,
                    text: &text,
                    source_override: previewing_edit.then_some(edit.buffer.as_str()),
                    style: &cell_style,
                    color: if calculation_status == Some(CalcStatus::Error) {
                        palette.error
                    } else {
                        palette.foreground
                    },
                    background: cell_background,
                    bold: header_bold,
                    markdown: (is_markdown && !is_formula).then_some(CellMarkdown {
                        display: markdown_display,
                        source: markdown_source,
                    }),
                },
                display,
            );
        }
        let mut response = grid_ui.interact(
            visible_rect,
            input
                .accesskit_table_token
                .and_then(|token| grid_accesskit_cell_id(token, cell_address, segment_index))
                .unwrap_or_else(|| {
                    Id::unique((
                        "grid_cell",
                        document_id,
                        table_id,
                        cell_address.row,
                        cell_address.column,
                        segment_index,
                    ))
                }),
            Sense::click_and_drag(),
        );
        let accessibility_label = grid_cell_accessibility_label(
            grid_ui.ctx(),
            table_name,
            cell_address,
            column_header,
            row_header,
            selected,
            editing_same_cell,
        );
        register_grid_cell_accessibility(
            grid_ui.ctx(),
            &response,
            &accessibility_label,
            &grid_cell_accessibility_value(
                crate::i18n::context_language(grid_ui.ctx()),
                text.as_ref(),
                display_mode,
                is_markdown && !is_formula,
                markdown_display,
                markdown_source,
            ),
            cell_address,
            selected,
        );
        let indicator = match calculation_status {
            Some(CalcStatus::Pending) => Some(("…", "grid-calc-pending-hint")),
            Some(CalcStatus::Imported) => Some(("i", "grid-calc-imported-hint")),
            Some(CalcStatus::Unsupported) => Some(("?", "grid-calc-unsupported-hint")),
            Some(CalcStatus::Stale) => Some(("!", "grid-calc-stale-hint")),
            Some(CalcStatus::Fresh | CalcStatus::Error) | None => None,
        };
        if let Some((indicator, hint)) = indicator {
            painter.text(
                visible_rect.right_top() - Vec2::new(7.0, -7.0),
                Align2::RIGHT_TOP,
                indicator,
                FontId::monospace(10.0),
                palette.error,
            );
            response = response.on_hover_text(context_text(grid_ui.ctx(), hint));
        }
        let context_selection = if response.secondary_clicked() && !selected {
            outcome.selected_range = Some(CellRange::single(cell_address));
            CellRange::single(cell_address)
        } else {
            selection_range
        };
        context_menu::show(
            &response,
            context_selection,
            structure_block_reason,
            input.context_commands_enabled,
            outcome,
        );
        let (_, selection_end) = selection_range.normalized();
        if selected
            && cell_address == selection_end
            && !edit.is_editing()
            && !navigation_blocked
            && structure_block_reason.is_none()
        {
            let handle =
                Rect::from_min_size(rect.right_bottom() - Vec2::splat(8.0), Vec2::splat(8.0))
                    .intersect(visible_rect);
            if handle.width() > 0.0 && handle.height() > 0.0 {
                // Keep the fill handle draggable without adding it to keyboard focus order.
                let handle_response = grid_ui.interact(
                    handle,
                    Id::unique(egui::IdSalt::new((
                        "fill_handle",
                        document_id,
                        table_id,
                        selection_range.anchor.row,
                        selection_range.anchor.column,
                        selection_range.head.row,
                        selection_range.head.column,
                    ))),
                    Sense::DRAG,
                );
                if handle_response.drag_started() {
                    outcome.fill_handle_started = Some(selection_range);
                }
                let _ = handle_response
                    .on_hover_text(context_text(grid_ui.ctx(), "grid-fill-handle-help"));
                fill_handle_rect = Some(handle);
            }
        }
        let keyboard_focus_cell = if selection_range.anchor == selection_range.head {
            active_cell
        } else {
            selection_range.head
        };
        if cell_address == keyboard_focus_cell
            && *request_grid_focus
            && !navigation_blocked
            && !egui::Popup::is_any_open(grid_ui.ctx())
        {
            response.request_focus();
            if response.has_focus() {
                *request_grid_focus = false;
            }
        }
        if response.drag_started_by(egui::PointerButton::Primary)
            && !navigation_blocked
            && (!edit.is_editing() || formula_reference_mode)
        {
            let shift = grid_ui.input(|input| input.modifiers.shift);
            let anchor = if shift { active_cell } else { cell_address };
            outcome.selection_drag_started = Some(GridSelectionDrag {
                document_id,
                table_id,
                anchor,
                pointer_start: cell_address,
                extend: shift,
                formula_reference_session: formula_reference_mode.then_some(edit.session_id),
            });
            outcome.selected_range = Some(CellRange::single(anchor));
            outcome.selection_active_cell = Some(anchor);
            outcome.preserve_additional_selections = shift;
            outcome.focused = true;
            if !formula_reference_mode {
                response.request_focus();
            }
        }
        let keyboard_activation =
            grid_ui.input(|input| input.key_pressed(Key::Enter) || input.key_pressed(Key::Space));
        if (response.clicked() || response.double_clicked()) && !keyboard_activation {
            if !navigation_blocked && !formula_reference_mode {
                response.request_focus();
            }
            outcome.double_clicked = response.double_clicked();
            let command_click =
                !response.double_clicked() && grid_ui.input(|input| input.modifiers.command);
            let shift_click = !response.double_clicked()
                && !command_click
                && grid_ui.input(|input| input.modifiers.shift);
            outcome.open_link = is_markdown && markdown_link_target.is_some();
            outcome.open_link &=
                grid_ui.input(|input| input.modifiers.ctrl || input.modifiers.command);
            if command_click && !outcome.open_link {
                outcome.add_selection_range = Some(CellRange::single(cell_address));
            } else if shift_click && !outcome.open_link {
                let range = CellRange {
                    anchor: active_cell,
                    head: cell_address,
                };
                outcome.record_shift_range(range, active_cell, formula_reference_mode);
            } else {
                outcome.clicked = Some(cell_address);
            }
        }
        outcome.focused |= response.has_focus();
        super::grid_focus::lock_grid_navigation(grid_ui, &response);
    }
    grid_segment_paint::paint_selection_overlay(
        painter,
        rect,
        selected_edges,
        fill_handle_rect,
        &palette,
    );
    formula_reference_paint::paint(painter, rect, cell_address, formula_reference_overlays);
}
use super::commands::AppCommand;
