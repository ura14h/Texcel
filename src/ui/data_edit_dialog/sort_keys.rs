use super::*;

/// Draws the sort key rows and returns the focused column field, if any.
pub(super) fn draw_sort_keys(
    ui: &mut egui::Ui,
    language: crate::i18n::Language,
    sort_keys: &mut Vec<SortKeyEditor>,
) -> Option<Id> {
    let mut focus = None;
    let mut remove_index = None;
    let sort_key_count = sort_keys.len();
    for (index, key) in sort_keys.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(
                Message::new("dialog-sort-key-order")
                    .with_number("order", (index + 1) as i64)
                    .resolve(language),
            );
            let column = ui.text_edit_singleline(&mut key.column);
            if column.has_focus() {
                focus = Some(column.id);
            }
            egui::ComboBox::from_id_salt(("sort_kind", index))
                .selected_text(match key.kind {
                    crate::structure::SortValueKind::Text => text(language, "dialog-text"),
                    crate::structure::SortValueKind::Number => text(language, "dialog-number"),
                    crate::structure::SortValueKind::Date => {
                        text(language, "dialog-sort-date-label")
                    }
                })
                .show_ui(ui, |ui| {
                    visual::prepare_context_menu(ui);
                    ui.selectable_value(
                        &mut key.kind,
                        crate::structure::SortValueKind::Text,
                        text(language, "dialog-text"),
                    );
                    ui.selectable_value(
                        &mut key.kind,
                        crate::structure::SortValueKind::Number,
                        text(language, "dialog-number"),
                    );
                    ui.selectable_value(
                        &mut key.kind,
                        crate::structure::SortValueKind::Date,
                        text(language, "dialog-sort-date-label"),
                    );
                });
            ui.checkbox(
                &mut key.descending,
                text(language, "dialog-sort-descending"),
            );
            if sort_key_count > 1 && ui.small_button("−").clicked() {
                remove_index = Some(index);
            }
        });
    }
    if let Some(index) = remove_index {
        sort_keys.remove(index);
    }
    if sort_keys.len() < 8 && ui.button(text(language, "dialog-sort-add-key")).clicked() {
        sort_keys.push(SortKeyEditor {
            column: sort_keys
                .last()
                .map_or_else(|| "A".to_owned(), |key| key.column.clone()),
            kind: crate::structure::SortValueKind::Text,
            descending: false,
        });
    }
    focus
}
