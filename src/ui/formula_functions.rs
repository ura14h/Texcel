use super::*;
use std::sync::OnceLock;

/// One button press in the formula bar's cancel, confirm, and function controls.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum FormulaBarAction {
    Cancel,
    Confirm,
    InsertFunction(String),
}

/// The function list's width, enough for the longest built-in names.
const FUNCTION_LIST_WIDTH: f32 = 200.0;

/// The glyph painted on one square formula bar button.
#[derive(Clone, Copy)]
enum FormulaBarIcon {
    Cancel,
    Confirm,
    Function,
}

/// Returns the functions formulas calculate, sorted for the picker. The engine knows many
/// more names, but formulas that use them are kept as text and never calculated.
pub(super) fn builtin_function_names() -> &'static [String] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let mut names = crate::calc::SUPPORTED_FUNCTIONS
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>();
        names.sort();
        names
    })
}

/// Keeps the names that contain the typed filter, ignoring letter case.
pub(super) fn filter_function_names<'a>(names: &'a [String], filter: &str) -> Vec<&'a str> {
    let filter = filter.trim().to_uppercase();
    names
        .iter()
        .map(String::as_str)
        .filter(|name| filter.is_empty() || name.contains(filter.as_str()))
        .collect()
}

impl TexcelApp {
    /// Draws the cancel, confirm, and function buttons between the name box and the editor.
    pub(super) fn draw_formula_bar_actions(
        &mut self,
        ui: &mut egui::Ui,
        navigation_blocked: bool,
        has_multiple_ranges: bool,
    ) -> Option<FormulaBarAction> {
        let editing = self.edit.is_editing();
        let mut action = None;
        if formula_bar_icon_button(
            ui,
            FormulaBarIcon::Cancel,
            &text(self.language, "formula-cancel"),
            editing && !navigation_blocked,
        )
        .clicked()
        {
            action = Some(FormulaBarAction::Cancel);
        }
        if formula_bar_icon_button(
            ui,
            FormulaBarIcon::Confirm,
            &text(self.language, "formula-confirm"),
            editing && !navigation_blocked,
        )
        .clicked()
        {
            action = Some(FormulaBarAction::Confirm);
        }
        let function_button = formula_bar_icon_button(
            ui,
            FormulaBarIcon::Function,
            &text(self.language, "formula-insert-function"),
            !navigation_blocked && !has_multiple_ranges,
        );
        let popup_id = function_button.id.with("function_list");
        if function_button.clicked() {
            ui.data_mut(|data| data.remove::<String>(popup_id));
        }
        let language = self.language;
        egui::Popup::from_toggle_button_response(&function_button)
            .id(popup_id)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .width(FUNCTION_LIST_WIDTH)
            .show(|ui| {
                visual::prepare_context_menu(ui);
                if let Some(name) = function_list_contents(ui, popup_id, language) {
                    action = Some(FormulaBarAction::InsertFunction(name));
                    egui::Popup::close_id(ui.ctx(), popup_id);
                }
            });
        action
    }

    /// Runs one formula bar button after the bar has finished drawing.
    pub(super) fn apply_formula_bar_action(
        &mut self,
        action: FormulaBarAction,
        ctx: &egui::Context,
    ) {
        match action {
            FormulaBarAction::Cancel => {
                self.cancel_edit();
                self.grid_focused = true;
            }
            FormulaBarAction::Confirm => {
                self.commit_edit(None);
                if !self.edit.is_editing() {
                    self.request_grid_focus = true;
                    self.grid_focused = true;
                }
            }
            FormulaBarAction::InsertFunction(name) => self.insert_formula_function(&name, ctx),
        }
    }

    /// Puts `=NAME()` in the active cell and leaves the caret between the parentheses.
    pub(super) fn insert_formula_function(&mut self, name: &str, ctx: &egui::Context) {
        let call = format!("{name}()");
        if self.edit.is_editing() {
            // Insert into an open formula at the caret, or replace plain text with a formula.
            if self.edit.buffer.starts_with('=') {
                let start = clamp_byte_boundary(
                    &self.edit.buffer,
                    self.edit.anchor_byte.min(self.edit.cursor_byte),
                );
                let end = clamp_byte_boundary(
                    &self.edit.buffer,
                    self.edit.anchor_byte.max(self.edit.cursor_byte),
                );
                self.edit.buffer.replace_range(start..end, &call);
                self.edit.cursor_byte = start + call.len() - 1;
            } else {
                self.edit.buffer = format!("={call}");
                self.edit.cursor_byte = self.edit.buffer.len() - 1;
            }
            self.edit.anchor_byte = self.edit.cursor_byte;
            self.edit.mark_buffer_changed();
            if self.edit.target == Some(crate::document::EditTarget::DetailPanel) {
                self.edit
                    .switch_target(crate::document::EditTarget::FormulaBar);
                self.long_text_panel_open = false;
            }
        } else {
            self.begin_selected_replacing(format!("={call}"));
            if !self.edit.is_editing() {
                return;
            }
            self.edit.cursor_byte = self.edit.buffer.len() - 1;
            self.edit.anchor_byte = self.edit.cursor_byte;
        }
        // Grid clicks now insert references at the caret.
        self.edit.mode = EditMode::FormulaReference;
        self.text_selection = None;
        self.refresh_formula_references();
        self.sync_formula_editor_cursor(ctx);
        self.request_editor_focus = true;
    }
}

/// Shows the filter field and the matching names; returns the clicked name.
fn function_list_contents(ui: &mut egui::Ui, popup_id: Id, language: Language) -> Option<String> {
    let stored = ui.data_mut(|data| data.get_temp::<String>(popup_id));
    let just_opened = stored.is_none();
    let mut filter = stored.unwrap_or_default();
    let filter_response = ui.add(
        egui::TextEdit::singleline(&mut filter)
            .id(popup_id.with("filter"))
            .desired_width(FUNCTION_LIST_WIDTH)
            .hint_text(text(language, "formula-function-search")),
    );
    // Typing goes to the filter as soon as the list opens.
    if just_opened {
        filter_response.request_focus();
    }
    ui.data_mut(|data| data.insert_temp(popup_id, filter.clone()));
    let names = filter_function_names(builtin_function_names(), &filter);
    let enter = filter_response.has_focus() && ui.input(|input| input.key_pressed(Key::Enter));
    let mut chosen = (enter && names.len() == 1).then(|| names[0].to_owned());
    // Only the visible rows are laid out, since the engine has hundreds of functions.
    let row_height = ui.spacing().interact_size.y;
    egui::ScrollArea::vertical()
        .max_height(280.0)
        .auto_shrink([false, true])
        .show_rows(ui, row_height, names.len().max(1), |ui, rows| {
            ui.set_width(FUNCTION_LIST_WIDTH);
            for name in names.iter().skip(rows.start).take(rows.len()) {
                if ui
                    .add(
                        egui::Button::new(*name)
                            .frame(false)
                            .min_size(Vec2::new(FUNCTION_LIST_WIDTH, row_height)),
                    )
                    .clicked()
                {
                    chosen = Some((*name).to_owned());
                }
            }
            if names.is_empty() {
                ui.weak(text(language, "formula-function-none"));
            }
        });
    chosen
}

/// Paints one square, frameless icon button with an accessible name and tooltip.
fn formula_bar_icon_button(
    ui: &mut egui::Ui,
    icon: FormulaBarIcon,
    label: &str,
    enabled: bool,
) -> egui::Response {
    let size = Vec2::splat(CHROME_CONTROL_HEIGHT);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
    let response = response.on_hover_text(label);
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let visuals = ui.visuals();
    if enabled && response.hovered() {
        ui.painter().rect_filled(
            rect.shrink(2.0),
            f32::from(FORMULA_BAR_CORNER_RADIUS),
            visuals.widgets.hovered.weak_bg_fill,
        );
    }
    let color = if enabled {
        visuals.text_color()
    } else {
        visuals.weak_text_color().gamma_multiply(0.6)
    };
    let center = rect.center();
    let half = CHROME_CONTROL_HEIGHT * 0.17;
    let stroke = Stroke::new(1.6, color);
    match icon {
        FormulaBarIcon::Cancel => {
            ui.painter().line_segment(
                [
                    center + Vec2::new(-half, -half),
                    center + Vec2::new(half, half),
                ],
                stroke,
            );
            ui.painter().line_segment(
                [
                    center + Vec2::new(-half, half),
                    center + Vec2::new(half, -half),
                ],
                stroke,
            );
        }
        FormulaBarIcon::Confirm => {
            ui.painter().add(egui::Shape::line(
                vec![
                    center + Vec2::new(-half * 1.1, 0.0),
                    center + Vec2::new(-half * 0.3, half * 0.85),
                    center + Vec2::new(half * 1.1, -half * 0.9),
                ],
                stroke,
            ));
        }
        FormulaBarIcon::Function => {
            ui.painter().text(
                center,
                Align2::CENTER_CENTER,
                "fx",
                FontId::proportional(14.0),
                color,
            );
        }
    }
    response
}

#[cfg(test)]
#[path = "formula_functions_test.rs"]
mod tests;
