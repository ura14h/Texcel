use super::*;
use crate::ui::scenario_tests::{
    ScenarioDirectory, cell_text, click_cell, click_point, key_event, scenario_harness, send_batch,
    send_event,
};
use egui_kittest::kittest::{By, Queryable};

/// Clicks the first visible button whose accessible name is exactly `label`.
fn click_named_button(harness: &mut egui_kittest::Harness<'_, TexcelApp>, label: &str) {
    let point = harness
        .query_all(By::new().role(egui::Role::Button).label(label))
        .next()
        .unwrap_or_else(|| panic!("button {label:?} is visible"))
        .rect()
        .center();
    click_point(harness, point);
}

/// Returns the active document id and the active cell address.
fn active_cell(app: &TexcelApp) -> (DocumentId, CellAddress) {
    let document = app.workspace.active_document().expect("active document");
    let table = document.active_table().expect("active table");
    (
        document.id,
        document.view(table.id).expect("view").active_cell(),
    )
}

#[test]
/// Lists only the functions formulas calculate, in order, and filters them without regard
/// to case.
fn function_names_are_the_calculated_ones_sorted_and_filterable() {
    let names = builtin_function_names();
    assert_eq!(names.len(), crate::calc::SUPPORTED_FUNCTIONS.len());
    assert!(
        names
            .iter()
            .all(|name| crate::calc::SUPPORTED_FUNCTIONS.contains(&name.as_str()))
    );
    assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(!names.iter().any(|name| name == "VLOOKUP"));
    let matches = filter_function_names(names, " round ");
    assert_eq!(matches, vec!["ROUND", "ROUNDDOWN", "ROUNDUP"]);
}

#[test]
/// Inserts `=SUM()` from the function list and lets a grid click add the reference.
fn function_list_inserts_a_call_and_accepts_a_grid_reference() {
    let directory = ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    click_cell(&mut harness, "A1");
    click_named_button(&mut harness, "Insert function");
    send_batch(&mut harness, [egui::Event::Text("sum".to_owned())]);
    click_named_button(&mut harness, "SUM");
    let app = harness.state();
    assert_eq!(app.edit.buffer, "=SUM()");
    assert_eq!(app.edit.cursor_byte, "=SUM(".len());
    assert_eq!(app.edit.mode, EditMode::FormulaReference);

    let b2 = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("B2"))
        .next()
        .expect("B2 is visible")
        .rect()
        .center();
    click_point(&mut harness, b2);
    assert_eq!(harness.state().edit.buffer, "=SUM(B2)");
    send_event(&mut harness, key_event(Key::Enter, egui::Modifiers::NONE));
    let document_id = harness.state().workspace.active_document.unwrap();
    assert_eq!(
        cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 0, column: 0 }
        )
        .as_deref(),
        Some("=SUM(B2)")
    );
}

#[test]
/// Confirms with the check button without moving, so arrow keys move from the edited cell.
fn confirm_button_commits_in_place_and_returns_keys_to_the_grid() {
    let directory = ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    click_cell(&mut harness, "A1");
    send_batch(&mut harness, [egui::Event::Text("abc".to_owned())]);
    assert!(harness.state().edit.is_editing());
    click_named_button(&mut harness, "Confirm input");
    assert!(!harness.state().edit.is_editing());
    let (document_id, address) = active_cell(harness.state());
    assert_eq!(address, CellAddress { row: 0, column: 0 });
    assert_eq!(
        cell_text(harness.state(), document_id, address).as_deref(),
        Some("abc")
    );
    send_event(
        &mut harness,
        key_event(Key::ArrowDown, egui::Modifiers::NONE),
    );
    assert_eq!(
        active_cell(harness.state()).1,
        CellAddress { row: 1, column: 0 }
    );
}

#[test]
/// Cancels with the cross button and keeps the cell's previous value.
fn cancel_button_discards_the_edit() {
    let directory = ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    click_cell(&mut harness, "A1");
    send_batch(&mut harness, [egui::Event::Text("abc".to_owned())]);
    click_named_button(&mut harness, "Cancel input");
    assert!(!harness.state().edit.is_editing());
    let (document_id, address) = active_cell(harness.state());
    assert_eq!(
        cell_text(harness.state(), document_id, address).unwrap_or_default(),
        ""
    );
    assert!(!harness.state().document_has_unsaved_changes(document_id));
}
