use super::commands::{AppCommand, InputOwner};
use super::*;

impl TexcelApp {
    /// Keeps the selected cell address beside its editable source text.
    pub(super) fn draw_formula_bar(&mut self, ui: &mut egui::Ui, navigation_blocked: bool) {
        let Some((
            document_id,
            table_id,
            address,
            selection,
            has_multiple_ranges,
            source_preview,
            source_is_long,
        )) = self.workspace.active_document().and_then(|document| {
            let table = document.active_table()?;
            let view = document.view(table.id)?;
            let address = view.active_cell();
            let input = table.cell(address).map(|cell| &cell.input);
            let source = input.map_or_else(|| std::borrow::Cow::Borrowed(""), cell_input_source);
            Some((
                document.id,
                table.id,
                address,
                view.selection,
                !view.additional_selections.is_empty(),
                text_preview(&source, 120).to_owned(),
                input.is_some_and(cell_input_needs_detail),
            ))
        })
        else {
            return;
        };
        let is_editing_bar = self.edit.target == Some(crate::document::EditTarget::FormulaBar);
        let is_editing_cell = self.edit.target == Some(crate::document::EditTarget::Cell);
        let is_editing_detail = self.edit.target == Some(crate::document::EditTarget::DetailPanel);
        if is_editing_bar || is_editing_detail {
            self.grid_focused = false;
        }
        let edit_preview = if self.edit.is_editing() {
            text_preview(&self.edit.buffer, 120).to_owned()
        } else {
            source_preview
        };
        let long_text_available = source_is_long
            || self.edit.buffer.len() > INLINE_EDIT_TEXT_BYTES
            || self.edit.buffer.contains('\n');
        let mut navigation_request = None;
        let mut begin_formula_edit = false;
        let mut begin_detail_edit = false;
        let mut switch_to_formula_bar = false;
        let mut formula_click_cursor = None;
        let mut bar_action = None;
        let mut text_menu_command = None;
        let name_box_id = name_box_id(document_id);
        let edit_ctx = ui.ctx().clone();
        let detail_label = if is_editing_detail {
            text(self.language, "formula-detail-editing")
        } else {
            text(self.language, "formula-detail-open")
        };
        // Measure the trailing control so the editor never pushes it offscreen.
        let detail_button_width = long_text_available.then(|| {
            let font_id = ui.style().text_styles[&egui::TextStyle::Button].clone();
            let text_color = ui.visuals().text_color();
            let text_width = ui.fonts_mut(|fonts| {
                fonts
                    .layout_no_wrap(detail_label.to_owned(), font_id, text_color)
                    .size()
                    .x
            });
            (text_width + ui.spacing().button_padding.x * 2.0).max(ui.spacing().interact_size.x)
        });
        egui::Frame::new()
            .fill(ui.visuals().extreme_bg_color)
            .inner_margin(egui::Margin::same(CHROME_EDGE_INSET))
            .show(ui, |ui| {
                let row_size = Vec2::new(ui.available_width(), CHROME_CONTROL_HEIGHT);
                ui.allocate_ui_with_layout(
                    row_size,
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let name_box_was_focused =
                            ui.memory(|memory| memory.has_focus(name_box_id));
                        // Keep the name box while its real target is opening the shared Edit menu.
                        #[cfg(any(test, not(any(target_os = "macos", target_os = "windows"))))]
                        let preserve_menu_name_box =
                            self.shared_edit_menu_target.is_some_and(|target| {
                                matches!(
                                    target,
                                    SharedEditMenuTarget::NameBox {
                                        document_id: target_document,
                                        table_id: target_table,
                                        ..
                                    } if target_document == document_id && target_table == table_id
                                ) && self.shared_edit_menu_target_is_current(target)
                                    && (egui::Popup::is_any_open(ui.ctx())
                                        || self.preserve_text_input_focus
                                        || ui.input(|input| {
                                            input.pointer.button_down(egui::PointerButton::Primary)
                                        }))
                            });
                        #[cfg(not(any(
                            test,
                            not(any(target_os = "macos", target_os = "windows"))
                        )))]
                        let preserve_menu_name_box = false;
                        if !name_box_was_focused && !preserve_menu_name_box {
                            self.name_box = selection_name(selection);
                        }
                        let name_box_text = self.name_box.clone();
                        let name_box_font = ui.style().text_styles[&egui::TextStyle::Body].clone();
                        let name_box_color = ui.visuals().text_color();
                        let name_box_text_width = ui.fonts_mut(|fonts| {
                            fonts
                                .layout_no_wrap(name_box_text, name_box_font, name_box_color)
                                .size()
                                .x
                        });
                        let name_box_width = (name_box_text_width + 24.0)
                            .clamp(84.0, 180.0)
                            .min(ui.available_width().max(84.0));
                        let enter_pressed = ui.input(|input| input.key_pressed(Key::Enter));
                        let name_box = ui
                            .add_enabled_ui(!navigation_blocked && !self.edit.is_editing(), |ui| {
                                ui.add_sized(
                                    [name_box_width, CHROME_CONTROL_HEIGHT],
                                    egui::TextEdit::singleline(&mut self.name_box)
                                        .id(name_box_id)
                                        .desired_width(name_box_width)
                                        .align(Align2::LEFT_CENTER)
                                        .hint_text("A1:D20"),
                                )
                            })
                            .inner;
                        let name_box_focused = name_box_was_focused || name_box.has_focus();
                        if name_box_focused {
                            self.grid_focused = false;
                            if enter_pressed {
                                navigation_request = Some(self.name_box.clone());
                                self.suppress_grid_keyboard = true;
                                name_box.surrender_focus();
                            }
                        }
                        if !self
                            .workspace
                            .active_document()
                            .and_then(|document| document.view(table_id))
                            .is_none_or(|view| view.additional_selections.is_empty())
                        {
                            let count = self
                                .workspace
                                .active_document()
                                .and_then(|document| document.view(table_id))
                                .map_or(0, |view| view.additional_selections.len() + 1);
                            ui.label(
                                Message::new("formula-selection-range-count")
                                    .with_number("count", count as i64)
                                    .resolve(self.language),
                            );
                        }
                        bar_action = self.draw_formula_bar_actions(
                            ui,
                            navigation_blocked,
                            has_multiple_ranges,
                        );
                        let edit_width = (ui.available_width()
                            - detail_button_width
                                .map_or(0.0, |width| width + ui.spacing().item_spacing.x))
                        .max(0.0);
                        let detail_button_width = detail_button_width.unwrap_or_default();
                        if is_editing_bar {
                            // A right-click opens the edit menu without moving the caret or selection.
                            let selection_before = (self.edit.anchor_byte, self.edit.cursor_byte);
                            let secondary_press = ui.input(|input| {
                                input.pointer.button_pressed(egui::PointerButton::Secondary)
                                    && !input.pointer.primary_pressed()
                            });
                            let output = ui
                                .add_enabled_ui(!navigation_blocked, |ui| {
                                    show_bounded_edit_text(
                                        ui,
                                        &edit_ctx,
                                        editor_id(document_id, self.edit.session_id),
                                        &mut self.edit,
                                        CellEditWidget::FormulaBarSingleline {
                                            width: edit_width,
                                            height: CHROME_CONTROL_HEIGHT,
                                        },
                                    )
                                })
                                .inner;
                            let response = output.response;
                            let mut selection_bytes = output.selection_bytes;
                            if secondary_press && response.hovered() {
                                (self.edit.anchor_byte, self.edit.cursor_byte) = selection_before;
                                self.sync_formula_editor_cursor(&edit_ctx);
                                selection_bytes = Some(
                                    selection_before.0.min(selection_before.1)
                                        ..selection_before.0.max(selection_before.1),
                                );
                            }
                            response.context_menu(|menu| {
                                text_menu_command = formula_bar_text_menu(menu);
                            });
                            self.capture_text_selection(
                                document_id,
                                table_id,
                                address,
                                selection_bytes,
                            );
                            if self.request_editor_focus {
                                // Requesting focus again would interrupt an IME composition.
                                if !response.has_focus() {
                                    response.request_focus();
                                }
                                self.request_editor_focus = false;
                            }
                        } else if is_editing_detail {
                            ui.add_sized(
                                [edit_width, CHROME_CONTROL_HEIGHT],
                                egui::Label::new(egui::RichText::new(edit_preview).monospace())
                                    .truncate(),
                            );
                        } else {
                            let preview_width = edit_width;
                            // Disable the painted preview itself when editing is blocked.
                            let (preview_rect, response) = ui
                                .add_enabled_ui(!navigation_blocked, |ui| {
                                    ui.allocate_exact_size(
                                        Vec2::new(preview_width, CHROME_CONTROL_HEIGHT),
                                        Sense::click(),
                                    )
                                })
                                .inner;
                            let preview_painter = ui.painter().with_clip_rect(preview_rect);
                            preview_painter.rect_filled(
                                preview_rect,
                                f32::from(FORMULA_BAR_CORNER_RADIUS),
                                ui.visuals().extreme_bg_color,
                            );
                            preview_painter.rect_stroke(
                                preview_rect,
                                f32::from(FORMULA_BAR_CORNER_RADIUS),
                                Stroke::new(
                                    0.6,
                                    ui.visuals().widgets.noninteractive.bg_stroke.color,
                                ),
                                egui::StrokeKind::Inside,
                            );
                            preview_painter.text(
                                preview_rect.left_center() + Vec2::new(8.0, 0.0),
                                Align2::LEFT_CENTER,
                                &edit_preview,
                                FontId::monospace(14.0),
                                ui.visuals().text_color(),
                            );
                            let response = response
                                .on_hover_text(text(self.language, "formula-source-tooltip"));
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::Role::Button,
                                    !navigation_blocked,
                                    &edit_preview,
                                )
                            });
                            if response.clicked() && !navigation_blocked {
                                formula_click_cursor = response
                                    .interact_pointer_pos()
                                    .map(|pointer| (pointer, preview_rect, edit_preview.clone()));
                                if has_multiple_ranges {
                                    self.status = Some(Message::new(
                                        "status-cell-edit-single-range-required",
                                    ));
                                } else if is_editing_cell {
                                    switch_to_formula_bar = true;
                                } else {
                                    begin_formula_edit = true;
                                }
                            }
                        }
                        if long_text_available
                            && ui
                                .add_enabled_ui(!navigation_blocked && !has_multiple_ranges, |ui| {
                                    ui.add_sized(
                                        [detail_button_width, CHROME_CONTROL_HEIGHT],
                                        egui::Button::new(detail_label),
                                    )
                                })
                                .inner
                                .clicked()
                        {
                            begin_detail_edit = true;
                        }
                    },
                );
            });
        if let Some(action) = bar_action {
            self.apply_formula_bar_action(action, &edit_ctx);
            return;
        }
        if let Some(command) = text_menu_command {
            // Give the editor its focus and the selection egui dropped while the menu had the
            // pointer, so the next frame delivers the text command to the right text.
            let focus = editor_id(document_id, self.edit.session_id);
            edit_ctx.memory_mut(|memory| memory.request_focus(focus));
            self.sync_formula_editor_cursor(&edit_ctx);
            self.dispatch_command(
                command,
                InputOwner::CellEditor,
                navigation_blocked,
                Some(focus),
                &edit_ctx,
            );
            return;
        }
        if switch_to_formula_bar {
            self.edit
                .switch_target(crate::document::EditTarget::FormulaBar);
            self.edit.mode = EditMode::Edit;
            if let Some((pointer, rect, preview)) = formula_click_cursor.as_ref() {
                self.place_formula_bar_caret(ui, *pointer, *rect, preview, &edit_ctx);
            }
            self.request_editor_focus = true;
            self.grid_focused = false;
        }
        if (begin_formula_edit || begin_detail_edit)
            && let original = self
                .workspace
                .documents
                .get(&document_id)
                .and_then(|document| document.tables.iter().find(|table| table.id == table_id))
                .and_then(|table| table.cell(address))
                .map_or_else(|| CellInput::Empty, |cell| cell.input.clone())
        {
            if self.edit.is_editing() {
                if begin_detail_edit {
                    self.edit
                        .switch_target(crate::document::EditTarget::DetailPanel);
                    self.long_text_panel_open = true;
                } else {
                    self.edit
                        .switch_target(crate::document::EditTarget::FormulaBar);
                }
            } else {
                let target = if begin_detail_edit {
                    crate::document::EditTarget::DetailPanel
                } else {
                    crate::document::EditTarget::FormulaBar
                };
                self.begin_edit_session(document_id, table_id, address, original, target);
            }
            if begin_formula_edit
                && let Some((pointer, rect, preview)) = formula_click_cursor.as_ref()
            {
                self.place_formula_bar_caret(ui, *pointer, *rect, preview, &edit_ctx);
            }
            self.request_editor_focus = true;
            self.grid_focused = begin_detail_edit;
        }
        if let Some(request) = navigation_request {
            if let Some(range) = parse_name_box_range(&request, MAX_ROWS, MAX_COLUMNS) {
                if let Some(document) = self.workspace.active_document_mut()
                    && let Some(view) = document.view_mut(table_id)
                {
                    view.set_selection(range);
                    self.request_grid_focus = true;
                    self.request_scroll_to_selection = true;
                    self.status = None;
                }
            } else {
                self.status = Some(Message::new("status-cell-reference-or-range-required"));
                self.name_box = selection_name(selection);
            }
        }
    }

    /// Places the new formula editor cursor where the painted preview was clicked.
    fn place_formula_bar_caret(
        &mut self,
        ui: &mut egui::Ui,
        pointer: Pos2,
        rect: Rect,
        preview: &str,
        ctx: &egui::Context,
    ) {
        let galley = ui.fonts_mut(|fonts| {
            fonts.layout_no_wrap(
                preview.to_owned(),
                FontId::monospace(14.0),
                ui.visuals().text_color(),
            )
        });
        let text_origin = Pos2::new(
            rect.left() + 8.0,
            rect.center().y - galley.rect.height() * 0.5,
        );
        let local_pointer = pointer - text_origin + Vec2::new(galley.rect.left(), 0.0);
        let character = galley.cursor_from_pos(local_pointer).index.0;
        let character = character.min(self.edit.buffer.chars().count());
        let byte = byte_index_for_char(&self.edit.buffer, character);
        self.edit.anchor_byte = byte;
        self.edit.cursor_byte = byte;
        self.sync_formula_editor_cursor(ctx);
    }
}

/// Shows the standard text editing commands for the formula bar editor; returns the clicked one.
fn formula_bar_text_menu(menu: &mut egui::Ui) -> Option<AppCommand> {
    visual::prepare_context_menu(menu);
    let groups: [&[(&str, AppCommand)]; 3] = [
        &[
            ("top-undo", AppCommand::Undo),
            ("top-redo", AppCommand::Redo),
        ],
        &[
            ("action-cut", AppCommand::CutAs(CopyFormat::DisplayTsv)),
            ("action-copy", AppCommand::CopyAs(CopyFormat::DisplayTsv)),
            (
                "action-paste",
                AppCommand::PasteSystem(PasteOptions::default()),
            ),
        ],
        &[("action-select-all", AppCommand::SelectAll)],
    ];
    let mut chosen = None;
    for (index, group) in groups.into_iter().enumerate() {
        if index > 0 {
            menu.separator();
        }
        for (label, command) in group {
            if menu
                .button(crate::i18n::context_text(menu.ctx(), label))
                .clicked()
            {
                chosen = Some(command.clone());
                menu.close();
            }
        }
    }
    chosen
}
