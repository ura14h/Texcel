use super::*;
use egui_kittest::{
    Harness,
    kittest::{By, NodeT, Queryable},
};
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::scenario_tests as scenario;

/// Starts a normal app and waits for its settings before each interaction scenario.
fn scenario_harness(settings_path: PathBuf) -> Harness<'static, TexcelApp> {
    scenario::scenario_harness(settings_path)
}

/// Sends one grid key and checks the focused grid widget after both event frames.
fn key_keeps_grid_focus(
    harness: &mut Harness<'_, TexcelApp>,
    key: Key,
    modifiers: egui::Modifiers,
) {
    scenario::send_batch(
        harness,
        [
            egui::Event::ModifiersChanged(modifiers),
            egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            },
        ],
    );
    assert_grid_has_focus(harness, key, "press");
    scenario::send_batch(
        harness,
        [
            egui::Event::Key {
                key,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers,
            },
            egui::Event::ModifiersChanged(egui::Modifiers::NONE),
        ],
    );
    assert_grid_has_focus(harness, key, "release");
}

/// Checks that the active frame keeps focus on the selected grid widget.
fn assert_grid_has_focus(harness: &Harness<'_, TexcelApp>, key: Key, phase: &str) -> Id {
    let focused = harness
        .ctx
        .memory(|memory| memory.focused())
        .unwrap_or_else(|| panic!("focus was lost after {phase} of {key:?}"));
    assert!(
        !harness.ctx.text_edit_focused(),
        "a text editor took focus after {phase} of {key:?}"
    );
    let focused_cell = harness
        .query_all(By::new().role(egui::Role::GridCell))
        .find(|node| node.accesskit_node().is_focused())
        .unwrap_or_else(|| {
            panic!("focused widget {focused:?} is a grid cell after {phase} of {key:?}")
        });
    let focused_node_id = focused_cell.accesskit_node().id();
    assert_eq!(
        u128::from(focused_node_id) >> 64,
        u128::from(focused.accesskit_id().0),
        "the focused grid cell has the focused widget's id after {phase} of {key:?}"
    );
    assert!(
        harness.state().grid_focused,
        "grid focus state is false after {phase} of {key:?} while widget {focused:?} has focus"
    );
    assert_eq!(harness.state().grid_keyboard_owner, Some(focused));
    focused
}

/// Reads the active address after the normal navigation path updates the view.
fn active_cell(harness: &Harness<'_, TexcelApp>, document_id: DocumentId) -> CellAddress {
    let document = &harness.state().workspace.documents[&document_id];
    let table = document.active_table().unwrap();
    document.view(table.id).unwrap().active_cell()
}

#[test]
/// Commits direct input on arrows while F2 keeps arrows in the text editor.
fn direct_input_and_f2_use_one_edit_mode() {
    let directory = scenario::ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    scenario::click_cell(&mut harness, "A1");
    let document_id = harness.state().workspace.active_document.unwrap();

    scenario::send_event(&mut harness, egui::Event::Text("first".to_owned()));
    assert_eq!(harness.state().edit.mode, EditMode::Input);
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::ArrowRight, egui::Modifiers::NONE),
    );
    assert!(!harness.state().edit.is_editing());
    assert_eq!(
        scenario::cell_text(harness.state(), document_id, CellAddress::default()).as_deref(),
        Some("first")
    );
    assert_eq!(
        active_cell(&harness, document_id),
        CellAddress { row: 0, column: 1 }
    );

    scenario::send_event(&mut harness, egui::Event::Text("second".to_owned()));
    assert_eq!(harness.state().edit.mode, EditMode::Input);
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::F2, egui::Modifiers::NONE),
    );
    assert_eq!(harness.state().edit.mode, EditMode::Edit);
    let cursor_before = harness.state().edit.cursor_byte;
    harness.step();
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::ArrowLeft, egui::Modifiers::NONE),
    );
    assert!(harness.state().edit.is_editing());
    assert!(harness.state().edit.cursor_byte < cursor_before);
    assert_eq!(harness.state().edit.buffer, "second");
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::F2, egui::Modifiers::NONE),
    );
    assert_eq!(harness.state().edit.mode, EditMode::Input);
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::ArrowRight, egui::Modifiers::NONE),
    );
    // The new column is committed when its checked source is ready.
    for _ in 0..1000 {
        scenario::send_batch(&mut harness, []);
        if scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 0, column: 1 },
        )
        .as_deref()
            == Some("second")
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 0, column: 1 },
        )
        .as_deref(),
        Some("second"),
        "the edit and arrow commit finish; status={:?}, editor={:?}, mode={:?}, pending={:?}",
        harness.state().status,
        harness.state().edit,
        harness.state().edit.mode,
        harness.state().pending_text_growth_sessions,
    );
}

#[test]
/// Keeps formula text editing separate from the existing reference picker mode.
fn formula_f2_and_direct_equals_preserve_the_selected_mode() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("formula-mode.md");
    fs::write(
        &source,
        "| Value | Next |\n| --- | --- |\n| =1+2 | keep |\n",
    )
    .expect("write a formula source cell");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source.clone());
    scenario::click_cell(&mut harness, "A2");
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::F2, egui::Modifiers::NONE),
    );
    assert_eq!(harness.state().edit.mode, EditMode::Edit);
    let cursor_before = harness.state().edit.cursor_byte;
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::ArrowLeft, egui::Modifiers::NONE),
    );
    assert!(harness.state().edit.cursor_byte < cursor_before);
    assert_eq!(harness.state().edit.mode, EditMode::Edit);

    scenario::click_cell(&mut harness, "B2");
    scenario::send_event(&mut harness, egui::Event::Text("=".to_owned()));
    harness.step();
    assert_eq!(harness.state().edit.mode, EditMode::FormulaReference);
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::F2, egui::Modifiers::NONE),
    );
    assert_eq!(harness.state().edit.mode, EditMode::Edit);
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::ArrowLeft, egui::Modifiers::NONE),
    );
    assert_eq!(harness.state().edit.cursor_byte, 0);
    assert_eq!(harness.state().edit.mode, EditMode::Edit);
}

#[test]
/// Returns Tab entry to its first column and ends that rule on arrows or clicks.
fn tab_entry_enter_returns_to_origin_column_until_navigation_changes_context() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("tab-entry.md");
    fs::write(
        &source,
        "| A | B | C | D |\n| --- | --- | --- | --- |\n| old | b | c | d |\n| row2 | b2 | c2 | d2 |\n",
    )
    .expect("write a table with two data rows");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source);
    let document_id = harness.state().workspace.active_document.unwrap();

    scenario::click_cell(&mut harness, "A2");
    scenario::send_event(&mut harness, egui::Event::Text("one".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Tab, egui::Modifiers::NONE),
    );
    scenario::send_event(&mut harness, egui::Event::Text("=".to_owned()));
    harness.step();
    assert_eq!(harness.state().edit.mode, EditMode::FormulaReference);
    scenario::send_event(&mut harness, egui::Event::Text("1+2".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    assert_eq!(
        active_cell(&harness, document_id),
        CellAddress { row: 2, column: 0 }
    );
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 0 }
        )
        .as_deref(),
        Some("one")
    );
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 1 }
        )
        .as_deref(),
        Some("=1+2")
    );

    scenario::click_cell(&mut harness, "A2");
    scenario::send_event(&mut harness, egui::Event::Text("one".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Tab, egui::Modifiers::NONE),
    );
    scenario::send_event(&mut harness, egui::Event::Text("two".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Tab, egui::Modifiers::NONE),
    );
    assert_eq!(
        active_cell(&harness, document_id),
        CellAddress { row: 1, column: 2 }
    );
    let origin = harness
        .state()
        .input_sequence_origin
        .expect("the untouched Tab destination keeps the entry origin");
    assert_eq!(origin.column, 0);
    assert_eq!(origin.expected_cell, CellAddress { row: 1, column: 2 });
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    assert_eq!(
        active_cell(&harness, document_id),
        CellAddress { row: 2, column: 0 },
        "Enter returns to the starting column even from an untouched Tab cell"
    );
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 1 }
        )
        .as_deref(),
        Some("two")
    );

    scenario::click_cell(&mut harness, "A2");
    scenario::send_event(&mut harness, egui::Event::Text("left".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Tab, egui::Modifiers::NONE),
    );
    scenario::send_event(&mut harness, egui::Event::Text("middle".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Tab, egui::Modifiers::NONE),
    );
    scenario::send_event(&mut harness, egui::Event::Text("three".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    assert_eq!(
        active_cell(&harness, document_id),
        CellAddress { row: 2, column: 0 },
        "an edited non-edge Tab cell returns directly to the first column"
    );
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 2 }
        )
        .as_deref(),
        Some("three")
    );

    scenario::click_cell(&mut harness, "C2");
    scenario::send_event(&mut harness, egui::Event::Text("origin".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Tab, egui::Modifiers::NONE),
    );
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Tab, egui::Modifiers::NONE),
    );
    scenario::send_event(&mut harness, egui::Event::Text("grown-edge".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    scenario::wait_until(&mut harness, |app| app.pending_structures.is_empty());
    assert_eq!(
        active_cell(&harness, document_id),
        CellAddress { row: 2, column: 2 },
        "Enter returns directly to the Tab sequence's starting column"
    );
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 4 }
        )
        .as_deref(),
        Some("grown-edge")
    );
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .active_table()
            .unwrap()
            .last_used_cell(),
        Some(CellAddress { row: 2, column: 4 }),
        "the sparse live extent keeps the greatest row and column independently"
    );

    scenario::click_cell(&mut harness, "A2");
    scenario::send_event(&mut harness, egui::Event::Text("again".to_owned()));
    for _ in 0..3 {
        scenario::send_event(
            &mut harness,
            scenario::key_event(Key::Tab, egui::Modifiers::NONE),
        );
    }
    assert_eq!(
        active_cell(&harness, document_id),
        CellAddress { row: 1, column: 3 }
    );
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    assert_eq!(
        active_cell(&harness, document_id),
        CellAddress { row: 2, column: 0 },
        "Tab can skip existing cells before Enter returns to the starting column"
    );

    scenario::click_cell(&mut harness, "A3");
    scenario::send_event(&mut harness, egui::Event::Text("start".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Tab, egui::Modifiers::NONE),
    );
    scenario::send_event(&mut harness, egui::Event::Text("middle".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::ArrowDown, egui::Modifiers::NONE),
    );
    assert_eq!(
        active_cell(&harness, document_id),
        CellAddress { row: 3, column: 1 }
    );
    scenario::send_event(&mut harness, egui::Event::Text("after-arrow".to_owned()));
    assert_eq!(
        harness.state().edit.address,
        Some(CellAddress { row: 3, column: 1 })
    );
    assert_eq!(harness.state().edit.buffer, "after-arrow");
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    scenario::wait_until(&mut harness, |app| app.pending_structures.is_empty());
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 3, column: 1 }
        )
        .as_deref(),
        Some("after-arrow"),
        "growth result status: {:?}",
        harness.state().status
    );
    assert_eq!(
        active_cell(&harness, document_id),
        CellAddress { row: 4, column: 1 }
    );

    scenario::click_cell(&mut harness, "A2");
    scenario::send_event(&mut harness, egui::Event::Text("click-start".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Tab, egui::Modifiers::NONE),
    );
    scenario::click_cell(&mut harness, "C2");
    scenario::send_event(&mut harness, egui::Event::Text("clicked".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    assert_eq!(
        active_cell(&harness, document_id),
        CellAddress { row: 2, column: 2 }
    );
}

#[test]
/// Keeps the grid focused while navigation enters blank cells past the source table.
fn grid_focus_stays_owned_through_blank_sheet_navigation() {
    let directory = scenario::ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(
        &mut harness,
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("samples")
            .join("interaction-workflow.md"),
    );
    scenario::click_cell(&mut harness, "A1");
    let document_id = harness.state().workspace.active_document.unwrap();
    let (table_id, row_count, column_count) = {
        let document = &harness.state().workspace.documents[&document_id];
        let table = document.active_table().unwrap();
        (table.id, table.row_count(), table.column_count())
    };
    assert!(row_count > 1 && column_count > 1);
    assert!(
        harness.ctx.memory(|memory| memory.focused()).is_some(),
        "the selected cell has keyboard focus"
    );
    assert!(!harness.ctx.text_edit_focused());
    assert!(harness.state().grid_focused);
    key_keeps_grid_focus(&mut harness, Key::ArrowUp, egui::Modifiers::NONE);
    key_keeps_grid_focus(&mut harness, Key::ArrowLeft, egui::Modifiers::NONE);
    scenario::send_event(&mut harness, egui::Event::Text("top-edge".to_owned()));
    assert_eq!(harness.state().edit.address, Some(CellAddress::default()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Escape, egui::Modifiers::NONE),
    );
    harness.step();
    assert_grid_has_focus(&harness, Key::Escape, "restore");
    key_keeps_grid_focus(&mut harness, Key::Tab, egui::Modifiers::NONE);
    let mut shift = egui::Modifiers::NONE;
    shift.shift = true;
    key_keeps_grid_focus(&mut harness, Key::Tab, shift);
    key_keeps_grid_focus(&mut harness, Key::Enter, egui::Modifiers::NONE);
    key_keeps_grid_focus(&mut harness, Key::ArrowUp, egui::Modifiers::NONE);
    key_keeps_grid_focus(&mut harness, Key::Escape, egui::Modifiers::NONE);

    for _ in 0..row_count.saturating_sub(1) {
        key_keeps_grid_focus(&mut harness, Key::ArrowDown, egui::Modifiers::NONE);
    }
    for _ in 0..column_count.saturating_sub(1) {
        key_keeps_grid_focus(&mut harness, Key::ArrowRight, egui::Modifiers::NONE);
    }
    key_keeps_grid_focus(&mut harness, Key::ArrowDown, egui::Modifiers::NONE);
    key_keeps_grid_focus(&mut harness, Key::ArrowRight, egui::Modifiers::NONE);
    key_keeps_grid_focus(&mut harness, Key::Tab, egui::Modifiers::NONE);
    key_keeps_grid_focus(&mut harness, Key::Tab, shift);
    key_keeps_grid_focus(&mut harness, Key::Enter, egui::Modifiers::NONE);
    key_keeps_grid_focus(&mut harness, Key::Escape, egui::Modifiers::NONE);
    key_keeps_grid_focus(&mut harness, Key::ArrowDown, egui::Modifiers::NONE);
    key_keeps_grid_focus(&mut harness, Key::ArrowRight, egui::Modifiers::NONE);

    let blank_cell = harness.state().workspace.documents[&document_id]
        .view(table_id)
        .unwrap()
        .active_cell();
    assert!(blank_cell.row >= row_count && blank_cell.column >= column_count);
    let edge_focus = harness
        .ctx
        .memory(|memory| memory.focused())
        .expect("a blank sheet cell keeps keyboard focus");
    scenario::send_event(&mut harness, egui::Event::Text("blank-sheet".to_owned()));
    assert_eq!(harness.state().edit.address, Some(blank_cell));
    assert_eq!(harness.state().edit.buffer, "blank-sheet");
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Escape, egui::Modifiers::NONE),
    );
    harness.step();
    let restored_grid_widget = harness
        .ctx
        .memory(|memory| memory.focused())
        .expect("Escape restores focus to the selected blank cell");
    assert_eq!(restored_grid_widget, edge_focus);
}

#[test]
/// Uses the real name box to reach the last sheet cell and edits it sparsely.
fn name_box_reaches_xfd1048576_but_rejects_unreadable_source_growth() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("far-cell.md");
    fs::write(&source, "| Value |\n| --- |\n| base |\n").expect("write a small source table");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source.clone());
    let document_id = harness.state().workspace.active_document.unwrap();
    let table_id = harness.state().workspace.documents[&document_id]
        .active_table()
        .unwrap()
        .id;

    let name_box_rect = harness
        .query_all(By::new().role(egui::Role::TextInput))
        .find(|node| node.value().as_deref() == Some("A1"))
        .expect("the name box shows the current cell")
        .rect();
    scenario::click_point(&mut harness, name_box_rect.center());
    let command = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(&mut harness, scenario::key_event(Key::A, command));
    scenario::send_event(&mut harness, egui::Event::Text("XFD1048576".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );

    let far = CellAddress {
        row: MAX_ROWS - 1,
        column: MAX_COLUMNS - 1,
    };
    assert_eq!(active_cell(&harness, document_id), far);
    scenario::wait_until(&mut harness, |app| {
        let page = app.grid_page_extent;
        app.workspace.documents[&document_id]
            .view(table_id)
            .is_some_and(|view| {
                view.scroll.x + page.x
                    >= far.column as f32 * crate::document::DEFAULT_COLUMN_WIDTH_PIXELS as f32
                    && view.scroll.y + page.y >= far.row as f32 * ROW_HEIGHT
            })
    });
    let name_box = harness
        .query_all(By::new().role(egui::Role::TextInput))
        .find(|node| node.value().as_deref() == Some("XFD1048576"))
        .expect("the name box keeps the full endpoint address visible");
    assert!(
        name_box.rect().width() > 84.0,
        "the name box expands for the longest sheet address"
    );
    scenario::send_batch(&mut harness, []);
    assert_grid_has_focus(&harness, Key::Enter, "name-box sheet jump");
    let before_revision = harness.state().workspace.documents[&document_id].revision;
    let before_history = harness.state().workspace.documents[&document_id].history_state_id();
    scenario::send_event(&mut harness, egui::Event::Text("sparse-edge".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::ArrowRight, egui::Modifiers::NONE),
    );
    // The growth check runs on the worker; wait for its rejection before checking the editor.
    scenario::wait_until(&mut harness, |app| {
        app.status
            .as_ref()
            .is_some_and(|status| status.resolve(app.language).contains("500,000"))
    });
    assert_eq!(active_cell(&harness, document_id), far);
    assert!(harness.state().edit.is_editing());
    assert_eq!(harness.state().edit.buffer, "sparse-edge");
    assert_eq!(
        harness.state().workspace.documents[&document_id].revision,
        before_revision
    );
    assert_eq!(
        harness.state().workspace.documents[&document_id].history_state_id(),
        before_history
    );
    assert!(
        harness
            .state()
            .status
            .as_ref()
            .unwrap()
            .resolve(harness.state().language)
            .contains("500,000")
    );
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Escape, egui::Modifiers::NONE),
    );
    assert_eq!(
        scenario::cell_text(harness.state(), document_id, far),
        None,
        "the rejected endpoint edit leaves the source-backed cell empty"
    );
    scenario::send_event(&mut harness, scenario::key_event(Key::S, command));
    scenario::wait_until(&mut harness, |app| {
        app.pending_saves.is_empty()
            && app
                .workspace
                .documents
                .get(&document_id)
                .is_some_and(|document| !document.is_dirty())
    });
    let reopened = crate::markdown_adapter::MarkdownDocument::parse(
        fs::read(&source).expect("the unchanged source remains readable"),
    )
    .expect("the save after rejection remains a valid Markdown file");
    assert_eq!(reopened.tables[0].rows[1][0].value, "base");
    assert_eq!(reopened.tables[0].rows.len(), 2);

    scenario::send_event(&mut harness, scenario::key_event(Key::Home, command));
    assert_eq!(active_cell(&harness, document_id), CellAddress::default());
    assert_eq!(
        scenario::cell_text(harness.state(), document_id, far),
        None,
        "navigation remains available after the rejected edit"
    );

    scenario::send_batch(&mut harness, []);
    scenario::send_event(
        &mut harness,
        egui::Event::Text("ordinary-after-edge".to_owned()),
    );
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    scenario::wait_until(&mut harness, |app| {
        scenario::cell_text(app, document_id, CellAddress::default()).as_deref()
            == Some("ordinary-after-edge")
    });
    assert_eq!(
        scenario::cell_text(harness.state(), document_id, far),
        None,
        "an ordinary edit after endpoint navigation does not materialize the far cell"
    );
}

#[test]
/// Extends an empty sheet for a ten-screen wheel gesture and edits a visible cell.
fn empty_sheet_grows_canvas_for_a_ten_screen_wheel_gesture() {
    let directory = scenario::ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    let document_id = harness.state().workspace.active_document.unwrap();
    let table_id = harness.state().workspace.documents[&document_id]
        .active_table()
        .unwrap()
        .id;
    let page = harness.state().grid_page_extent;
    let first_cell = harness
        .query_all(By::new().role(egui::Role::GridCell))
        .next()
        .expect("the initial sheet fills the grid viewport");
    let wheel_target = first_cell.rect().center();
    scenario::send_batch(
        &mut harness,
        [
            egui::Event::PointerMoved(wheel_target),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(-page.x * 10.0, -page.y * 10.0),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    scenario::wait_until(&mut harness, |app| {
        app.workspace.documents[&document_id]
            .view(table_id)
            .is_some_and(|view| view.scroll.x >= page.x * 8.0 && view.scroll.y >= page.y * 8.0)
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut previous = harness.state().workspace.documents[&document_id]
        .view(table_id)
        .unwrap()
        .scroll;
    let mut stable_frames = 0;
    while stable_frames < 3 {
        scenario::send_batch(&mut harness, []);
        let current = harness.state().workspace.documents[&document_id]
            .view(table_id)
            .unwrap()
            .scroll;
        if (current.x - previous.x).abs() < 1.0 && (current.y - previous.y).abs() < 1.0 {
            stable_frames += 1;
        } else {
            stable_frames = 0;
        }
        previous = current;
        assert!(
            Instant::now() < deadline,
            "the scroll settles after the wheel input"
        );
    }
    let scroll = harness.state().workspace.documents[&document_id]
        .view(table_id)
        .unwrap()
        .scroll;
    assert!(scroll.x < f32::MAX / 2.0 && scroll.y < f32::MAX / 2.0);

    let visible_cell = harness
        .query_all(By::new().role(egui::Role::GridCell))
        .next()
        .expect("cells remain available after a long wheel gesture")
        .rect()
        .center();
    scenario::click_point(&mut harness, visible_cell);
    assert!(
        harness.state().grid_focused,
        "the visible cell at {visible_cell:?} owns grid focus after the long scroll"
    );
    let address = active_cell(&harness, document_id);
    scenario::send_event(
        &mut harness,
        egui::Event::Text("after ten screens".to_owned()),
    );
    assert_eq!(harness.state().edit.address, Some(address));
    assert_eq!(harness.state().edit.buffer, "after ten screens");
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    // A distant edit becomes visible after the source worker accepts the new rectangle.
    scenario::wait_until(&mut harness, |app| {
        scenario::cell_text(app, document_id, address).as_deref() == Some("after ten screens")
    });
}

#[test]
/// Keeps the opened table extent across two real Markdown file saves.
fn markdown_save_retrims_a_cleared_far_cell_in_the_same_session() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("extent.md");
    let original = "# Data\n\n| Name | Value |\n| --- | ---: |\n| A | keep |\n";
    fs::write(&source, original).expect("write a small Markdown source");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source.clone());
    let document_id = harness.state().workspace.active_document.unwrap();
    scenario::click_cell(&mut harness, "F8");
    scenario::send_event(&mut harness, egui::Event::Text("edge".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    scenario::wait_until(&mut harness, |app| {
        scenario::cell_text(app, document_id, CellAddress { row: 7, column: 5 }).as_deref()
            == Some("edge")
    });
    save_active_document(&mut harness, document_id);
    let first = fs::read_to_string(&source).expect("the first save writes the expanded table");
    let first_source = crate::markdown_adapter::MarkdownDocument::parse(first.as_bytes().to_vec())
        .expect("the expanded Markdown remains readable");
    assert_eq!(first_source.tables[0].rows.len(), 8);
    assert_eq!(first_source.tables[0].rows[0].len(), 6);
    assert_eq!(first_source.tables[0].rows[7][5].value, "edge");

    scenario::click_cell(&mut harness, "F8");
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Delete, egui::Modifiers::NONE),
    );
    scenario::wait_until(&mut harness, |app| {
        scenario::cell_text(app, document_id, CellAddress { row: 7, column: 5 }).as_deref()
            == Some("")
    });
    save_active_document(&mut harness, document_id);
    assert_eq!(
        fs::read(&source).expect("the second save writes the trimmed table"),
        original.as_bytes(),
        "the original byte shape remains the lower bound across saves"
    );
}

#[test]
/// Keeps temporary column alignment through a normal save and one value Undo.
fn virtual_alignment_survives_real_save_growth_and_undo() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("virtual-alignment.md");
    let original = b"| A | B |\n| --- | --- |\n| x | y |\n";
    fs::write(&source, original).expect("write a small Markdown source");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source.clone());
    let document_id = harness.state().workspace.active_document.unwrap();

    scenario::click_cell(&mut harness, "C2");
    harness
        .state_mut()
        .request_table_management(StructureAction::MarkdownAlignRight, None, None);
    scenario::wait_until(&mut harness, |app| {
        !app.pending_structures.contains_key(&document_id)
    });
    assert_eq!(
        harness.state().status.as_ref().map(Message::key),
        Some("status-markdown-column-alignment-temporary")
    );
    save_active_document(&mut harness, document_id);
    assert_eq!(
        fs::read(&source).expect("the no-value save completes"),
        original
    );
    assert_eq!(
        markdown_alignment(harness.state(), document_id, 2),
        Some(crate::markdown_adapter::MarkdownTableAlignment::Right),
        "the virtual alignment remains available after saving"
    );

    scenario::click_cell(&mut harness, "C2");
    scenario::send_event(&mut harness, egui::Event::Text("far".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    scenario::wait_until(&mut harness, |app| {
        scenario::cell_text(app, document_id, CellAddress { row: 1, column: 2 }).as_deref()
            == Some("far")
    });
    save_active_document(&mut harness, document_id);
    let expanded = fs::read(&source).expect("the value save includes its aligned column");
    let expanded_text = std::str::from_utf8(&expanded).expect("the saved Markdown is UTF-8");
    assert!(
        expanded_text
            .lines()
            .nth(1)
            .is_some_and(|line| line.contains("---:"))
    );
    assert!(
        expanded_text
            .lines()
            .nth(2)
            .is_some_and(|line| line.contains("far"))
    );

    let command = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(&mut harness, scenario::key_event(Key::Z, command));
    scenario::wait_until(&mut harness, |app| {
        !app.pending_history_restores.contains_key(&document_id)
    });
    save_active_document(&mut harness, document_id);
    assert_eq!(
        fs::read(&source).expect("Undo removes the grown column from the saved file"),
        original
    );
    assert_eq!(
        markdown_alignment(harness.state(), document_id, 2),
        Some(crate::markdown_adapter::MarkdownTableAlignment::Right),
        "Undoing the value keeps the still-open virtual alignment"
    );

    scenario::send_event(&mut harness, scenario::key_event(Key::Z, command));
    scenario::wait_until(&mut harness, |app| {
        !app.pending_history_restores.contains_key(&document_id)
    });
    save_active_document(&mut harness, document_id);
    assert_eq!(
        fs::read(&source).expect("Undo of the setting saves"),
        original
    );
    assert_eq!(markdown_alignment(harness.state(), document_id, 2), None);
}

#[test]
/// Promotes CSV for a virtual alignment and restores its exact source with one Undo.
fn csv_virtual_alignment_promotes_and_undo_restores_the_csv_source() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("alignment.csv");
    let original = b"Name,Amount\r\nTea,00123\r\n";
    fs::write(&source, original).expect("write a UTF-8 CSV source");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source.clone());
    let document_id = harness.state().workspace.active_document.unwrap();
    scenario::click_cell(&mut harness, "C2");
    harness
        .state_mut()
        .request_table_management(StructureAction::MarkdownAlignRight, None, None);
    scenario::wait_until(&mut harness, |app| {
        !app.pending_structures.contains_key(&document_id)
    });
    assert!(matches!(
        harness.state().workspace.documents[&document_id]
            .source
            .as_deref(),
        Some(SourceDocument::Markdown(_))
    ));
    assert_eq!(
        markdown_alignment(harness.state(), document_id, 2),
        Some(crate::markdown_adapter::MarkdownTableAlignment::Right)
    );

    let command = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(&mut harness, scenario::key_event(Key::Z, command));
    scenario::wait_until(&mut harness, |app| {
        !app.pending_history_restores.contains_key(&document_id)
    });
    let Some(SourceDocument::Csv(csv)) = harness.state().workspace.documents[&document_id]
        .source
        .as_deref()
    else {
        panic!("Undo restores the original CSV source kind");
    };
    assert_eq!(csv.source_bytes(), original);
    assert_eq!(
        fs::read(&source).expect("promotion leaves the source file untouched"),
        original
    );
}

/// Reads one Markdown source alignment from the adopted application document.
fn markdown_alignment(
    app: &TexcelApp,
    document_id: DocumentId,
    column: usize,
) -> Option<crate::markdown_adapter::MarkdownTableAlignment> {
    let Some(SourceDocument::Markdown(markdown)) =
        app.workspace.documents[&document_id].source.as_deref()
    else {
        return None;
    };
    markdown.column_alignment(0, column)
}

#[test]
/// Expands a saved Markdown table when a pasted rectangle crosses its right and bottom edges.
fn out_of_extent_paste_saves_and_reopens_its_full_rectangle() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("paste-extent.md");
    let original = b"| A | B |\n| --- | --- |\n| x | y |\n";
    fs::write(&source, original).expect("write a small Markdown source");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source.clone());
    let document_id = harness.state().workspace.active_document.unwrap();
    scenario::click_cell(&mut harness, "B2");
    scenario::send_event(&mut harness, egui::Event::Paste("p\tq\nr\ts".to_owned()));
    scenario::wait_until(&mut harness, |app| {
        scenario::cell_text(app, document_id, CellAddress { row: 2, column: 2 }).as_deref()
            == Some("s")
    });
    save_active_document(&mut harness, document_id);
    let bytes = fs::read(&source).expect("the expanded Markdown was saved");
    let saved = crate::markdown_adapter::MarkdownDocument::parse(bytes)
        .expect("the expanded Markdown reopens");
    assert_eq!(saved.tables[0].rows.len(), 3);
    assert_eq!(saved.tables[0].rows.iter().map(Vec::len).max(), Some(3));
    assert_eq!(saved.tables[0].rows[1][1].value, "p");
    assert_eq!(saved.tables[0].rows[1][2].value, "q");
    assert_eq!(saved.tables[0].rows[2][1].value, "r");
    assert_eq!(saved.tables[0].rows[2][2].value, "s");

    let command = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(&mut harness, scenario::key_event(Key::Z, command));
    scenario::wait_until(&mut harness, |app| {
        !app.pending_history_restores.contains_key(&document_id)
    });
    save_active_document(&mut harness, document_id);
    assert_eq!(
        fs::read(&source).expect("Undo restores the original file bytes"),
        original
    );
}

#[test]
/// Saves and reopens a cleared source table while keeping its original rows and columns.
fn clearing_all_cells_keeps_the_source_table_shape_and_is_undoable() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("all-clear.md");
    let original = b"| Name | Value |\n| --- | --- |\n| keep | one |\n| other | two |\n";
    fs::write(&source, original).expect("write a small Markdown table");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source.clone());
    let document_id = harness.state().workspace.active_document.unwrap();
    scenario::click_cell(&mut harness, "Name");
    let command = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(&mut harness, scenario::key_event(Key::A, command));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Delete, egui::Modifiers::NONE),
    );
    save_active_document(&mut harness, document_id);
    let cleared_bytes = fs::read(&source).expect("the cleared table is saved");
    let cleared = crate::markdown_adapter::MarkdownDocument::parse(cleared_bytes.clone())
        .expect("the cleared table reopens");
    assert_eq!(cleared.tables[0].rows.len(), 3);
    assert_eq!(cleared.parser_width(0), Some(2));
    assert!(
        cleared.tables[0]
            .rows
            .iter()
            .flatten()
            .all(|cell| cell.value.is_empty())
    );

    scenario::send_event(&mut harness, scenario::key_event(Key::Z, command));
    scenario::wait_until(&mut harness, |app| {
        !app.pending_history_restores.contains_key(&document_id)
    });
    save_active_document(&mut harness, document_id);
    let restored = crate::markdown_adapter::MarkdownDocument::parse(
        fs::read(&source).expect("Undo saves the source cells"),
    )
    .expect("the restored table reopens");
    assert_eq!(restored.tables[0].rows[0][0].value, "Name");
    assert_eq!(restored.tables[0].rows[0][1].value, "Value");
    assert_eq!(restored.tables[0].rows[1][0].value, "keep");
    assert_eq!(restored.tables[0].rows[1][1].value, "one");
    assert_eq!(restored.tables[0].rows[2][0].value, "other");
    assert_eq!(restored.tables[0].rows[2][1].value, "two");
}

#[test]
/// Saves a cleared new document as the smallest valid Markdown table.
fn cleared_new_document_saves_a_minimal_markdown_table() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("empty-new-document.md");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    let document_id = harness.state().workspace.active_document.unwrap();
    scenario::click_cell(&mut harness, "A1");
    scenario::send_event(&mut harness, egui::Event::Text("temporary".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    scenario::wait_until(&mut harness, |app| {
        scenario::cell_text(app, document_id, CellAddress::default()).as_deref()
            == Some("temporary")
    });
    let command = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(&mut harness, scenario::key_event(Key::A, command));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Delete, egui::Modifiers::NONE),
    );
    harness.state_mut().test_save_path_selection = Some(source.clone());
    scenario::send_event(&mut harness, scenario::key_event(Key::S, command));
    scenario::wait_until(&mut harness, |app| app.export_dialog.is_some());
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    scenario::wait_until(&mut harness, |app| {
        source.exists()
            && !app.pending_saves.contains_key(&document_id)
            && !app.document_has_unsaved_changes(document_id)
    });
    let bytes = fs::read(&source).expect("the cleared new document is saved");
    assert_eq!(bytes, b"|  |\n| --- |\n");
    let reopened = crate::markdown_adapter::MarkdownDocument::parse(bytes)
        .expect("the minimal table is valid Markdown");
    assert_eq!(reopened.tables.len(), 1);
    assert_eq!(reopened.parser_width(0), Some(1));
    assert_eq!(reopened.tables[0].rows.len(), 1);
}

#[test]
/// Keeps the saved source unchanged when row and column edits are entirely past its extent.
fn row_and_column_operations_outside_source_extent_do_not_change_saved_bytes() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("outside-structure.md");
    let original = b"| Formula | Note |\n| --- | --- |\n| =A10 | keep |\n";
    fs::write(&source, original).expect("write a small Markdown source");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source.clone());
    let document_id = harness.state().workspace.active_document.unwrap();
    let command = scenario::command_modifiers(&harness.ctx);

    scenario::select_cell_by_name_box(&mut harness, "A10");
    assert_eq!(
        active_cell(&harness, document_id),
        CellAddress { row: 9, column: 0 }
    );
    harness
        .state_mut()
        .request_structure_action(StructureAction::InsertRows);
    scenario::wait_until(&mut harness, |app| {
        !app.pending_structures.contains_key(&document_id)
    });
    save_active_document(&mut harness, document_id);
    let mut formula_shifted = original.to_vec();
    let formula_start = formula_shifted
        .windows(4)
        .position(|window| window == b"A10 ")
        .expect("the source formula points to the inserted row");
    formula_shifted[formula_start + 2] = b'1';
    assert_eq!(
        fs::read(&source).expect("off-extent row insertion saves"),
        formula_shifted,
        "the reference shifts without adding blank source rows"
    );
    scenario::send_event(&mut harness, scenario::key_event(Key::Z, command));
    scenario::wait_until(&mut harness, |app| {
        !app.pending_history_restores.contains_key(&document_id)
    });
    save_active_document(&mut harness, document_id);
    assert_eq!(fs::read(&source).expect("row Undo saves"), original);

    scenario::select_cell_by_name_box(&mut harness, "D1");
    assert_eq!(
        active_cell(&harness, document_id),
        CellAddress { row: 0, column: 3 }
    );
    harness
        .state_mut()
        .request_structure_action(StructureAction::InsertColumns);
    scenario::wait_until(&mut harness, |app| {
        !app.pending_structures.contains_key(&document_id)
    });
    save_active_document(&mut harness, document_id);
    assert_eq!(
        fs::read(&source).expect("off-extent column insertion saves"),
        original,
        "inserting a column beyond the source table does not add blank source columns"
    );
}

#[test]
/// Preserves a ragged CSV row across repeated saves and Undo in one real app session.
fn csv_ragged_row_survives_continuous_saves_and_undo() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("ragged.csv");
    let original = b"a,b\r\nx\r\n";
    fs::write(&source, original).expect("write a ragged CSV source");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source.clone());
    let document_id = harness.state().workspace.active_document.unwrap();
    assert_eq!(
        scenario::cell_text(harness.state(), document_id, CellAddress::default()).as_deref(),
        Some("a"),
        "the CSV source is adopted before grid input"
    );

    select_b2(&mut harness);
    scenario::send_event(&mut harness, egui::Event::Text("y".to_owned()));
    assert_eq!(
        harness.state().edit.address,
        Some(CellAddress { row: 1, column: 1 })
    );
    assert_eq!(harness.state().edit.buffer, "y");
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    scenario::wait_until(&mut harness, |app| {
        scenario::cell_text(app, document_id, CellAddress { row: 1, column: 1 }).as_deref()
            == Some("y")
    });
    save_active_document(&mut harness, document_id);
    assert_eq!(
        fs::read(&source).expect("first CSV save completes"),
        b"a,b\r\nx,y\r\n"
    );

    select_b2(&mut harness);
    scenario::send_event(&mut harness, egui::Event::Text("changed".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    scenario::wait_until(&mut harness, |app| {
        scenario::cell_text(app, document_id, CellAddress { row: 1, column: 1 }).as_deref()
            == Some("changed")
    });
    save_active_document(&mut harness, document_id);
    assert_eq!(
        fs::read(&source).expect("second CSV save completes"),
        b"a,b\r\nx,changed\r\n"
    );

    let command = scenario::command_modifiers(&harness.ctx);
    for _ in 0..2 {
        scenario::send_event(&mut harness, scenario::key_event(Key::Z, command));
    }
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 1 },
        ),
        None,
        "two Undo commands restore the source row's absent trailing field"
    );
    save_active_document(&mut harness, document_id);
    let saved = fs::read(&source).expect("restored ragged row saves again");
    assert_eq!(saved, original);
    let reopened = crate::csv_adapter::CsvDocument::parse(
        saved,
        crate::csv_adapter::EncodingChoice::Auto,
        ',',
    )
    .expect("the final saved CSV reopens");
    assert_eq!(reopened.records[1].fields.len(), 1);
}

/// Selects B2 through the visible grid and keyboard navigation.
fn select_b2(harness: &mut Harness<'_, TexcelApp>) {
    let first_cell = harness
        .query_all(By::new().role(egui::Role::GridCell))
        .next()
        .expect("the loaded CSV grid has a visible first cell")
        .rect()
        .center();
    scenario::click_point(harness, first_cell);
    for key in [Key::ArrowDown, Key::ArrowRight] {
        scenario::send_event(harness, scenario::key_event(key, egui::Modifiers::NONE));
    }
}

#[test]
/// Reopens Markdown after row and column insertion without losing the far edge.
fn markdown_row_and_column_insertions_reopen_with_all_cells() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("structure.md");
    let original = b"| A | B |\n| --- | --- |\n| x | y |\n";
    fs::write(&source, original).expect("write a Markdown table");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source.clone());
    let document_id = harness.state().workspace.active_document.unwrap();
    let command = scenario::command_modifiers(&harness.ctx);

    select_b2(&mut harness);
    harness
        .state_mut()
        .request_structure_action(StructureAction::InsertRows);
    scenario::wait_until(&mut harness, |app| {
        !app.pending_structures.contains_key(&document_id)
    });
    // Whole-table commands must use the same rectangle that will be saved.
    let document = &harness.state().workspace.documents[&document_id];
    assert_eq!(
        document.extent_end(document.active_table),
        Some(CellAddress { row: 2, column: 1 })
    );
    save_active_document(&mut harness, document_id);
    let saved_rows = crate::markdown_adapter::MarkdownDocument::parse(
        fs::read(&source).expect("read Markdown after row insertion"),
    )
    .expect("the row-inserted Markdown reopens");
    assert_eq!(saved_rows.tables[0].rows[2][1].value, "y");
    scenario::send_event(&mut harness, scenario::key_event(Key::Z, command));
    scenario::wait_until(&mut harness, |app| {
        !app.pending_history_restores.contains_key(&document_id)
    });
    save_active_document(&mut harness, document_id);
    assert_eq!(fs::read(&source).expect("row Undo saves"), original);

    select_b2(&mut harness);
    harness
        .state_mut()
        .request_structure_action(StructureAction::InsertColumns);
    scenario::wait_until(&mut harness, |app| {
        !app.pending_structures.contains_key(&document_id)
    });
    // Whole-table commands must use the same rectangle that will be saved.
    let document = &harness.state().workspace.documents[&document_id];
    assert_eq!(
        document.extent_end(document.active_table),
        Some(CellAddress { row: 1, column: 2 })
    );
    save_active_document(&mut harness, document_id);
    let saved_columns = crate::markdown_adapter::MarkdownDocument::parse(
        fs::read(&source).expect("read Markdown after column insertion"),
    )
    .expect("the column-inserted Markdown reopens");
    assert_eq!(saved_columns.tables[0].rows[1][2].value, "y");
    scenario::send_event(&mut harness, scenario::key_event(Key::Z, command));
    scenario::wait_until(&mut harness, |app| {
        !app.pending_history_restores.contains_key(&document_id)
    });
    save_active_document(&mut harness, document_id);
    assert_eq!(fs::read(&source).expect("column Undo saves"), original);
}

#[test]
/// Keeps one inserted row and column without serializing spare model dimensions.
fn row_then_column_insertions_do_not_save_phantom_edges() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("row-then-column.md");
    let original = b"| A | B |\n| --- | --- |\n| x | y |\n";
    fs::write(&source, original).expect("write a two-column source table");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source.clone());
    let document_id = harness.state().workspace.active_document.unwrap();
    let command = scenario::command_modifiers(&harness.ctx);

    select_b2(&mut harness);
    harness
        .state_mut()
        .request_structure_action(StructureAction::InsertRows);
    scenario::wait_until(&mut harness, |app| {
        !app.pending_structures.contains_key(&document_id)
    });
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .extent_end(harness.state().workspace.documents[&document_id].active_table),
        Some(CellAddress { row: 2, column: 1 }),
        "row insertion extends data through B3 only"
    );
    select_b2(&mut harness);
    harness
        .state_mut()
        .request_structure_action(StructureAction::InsertColumns);
    scenario::wait_until(&mut harness, |app| {
        !app.pending_structures.contains_key(&document_id)
    });
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .extent_end(harness.state().workspace.documents[&document_id].active_table),
        Some(CellAddress { row: 2, column: 2 }),
        "column insertion extends data through C3 only"
    );
    save_active_document(&mut harness, document_id);

    let saved = crate::markdown_adapter::MarkdownDocument::parse(
        fs::read(&source).expect("both structure changes save to the source"),
    )
    .expect("the transformed table remains valid Markdown");
    assert_eq!(saved.tables[0].rows.len(), 3);
    assert_eq!(saved.parser_width(0), Some(3));
    assert!(saved.tables[0].rows.iter().all(|row| row.len() == 3));
    assert_eq!(saved.tables[0].rows[2][2].value, "y");

    scenario::send_event(&mut harness, scenario::key_event(Key::Z, command));
    scenario::wait_until(&mut harness, |app| {
        !app.pending_history_restores.contains_key(&document_id)
    });
    save_active_document(&mut harness, document_id);
    let after_column_undo = crate::markdown_adapter::MarkdownDocument::parse(
        fs::read(&source).expect("column Undo saves"),
    )
    .expect("the row-only table remains valid Markdown");
    assert_eq!(after_column_undo.tables[0].rows.len(), 3);
    assert_eq!(after_column_undo.parser_width(0), Some(2));

    scenario::send_event(&mut harness, scenario::key_event(Key::Z, command));
    scenario::wait_until(&mut harness, |app| {
        !app.pending_history_restores.contains_key(&document_id)
    });
    save_active_document(&mut harness, document_id);
    assert_eq!(
        fs::read(&source).expect("row Undo restores original bytes"),
        original
    );
}

#[test]
/// Saves and undoes row and column insertion through the normal structure worker.
fn csv_row_and_column_insertions_save_and_undo() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("structure.csv");
    let original = b"a,b\r\nx,y\r\n";
    fs::write(&source, original).expect("write a rectangular CSV source");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source.clone());
    let document_id = harness.state().workspace.active_document.unwrap();
    let command = scenario::command_modifiers(&harness.ctx);

    select_b2(&mut harness);
    harness
        .state_mut()
        .request_structure_action(StructureAction::InsertRows);
    scenario::wait_until(&mut harness, |app| {
        !app.pending_structures.contains_key(&document_id)
    });
    save_active_document(&mut harness, document_id);
    assert_eq!(
        fs::read(&source).expect("row insertion saves"),
        b"a,b\r\n,\r\nx,y\r\n"
    );
    let saved_rows = crate::csv_adapter::CsvDocument::parse(
        fs::read(&source).expect("read the saved row insertion"),
        crate::csv_adapter::EncodingChoice::Auto,
        ',',
    )
    .expect("the saved row insertion reopens");
    let reopened_rows = saved_rows.to_document(9001, "reopened.csv");
    assert_eq!(
        reopened_rows.tables[0]
            .cell(CellAddress { row: 2, column: 1 })
            .map(|cell| cell.input.source_text()),
        Some("y".to_owned()),
        "row insertion keeps the last column after reopening"
    );
    scenario::send_event(&mut harness, scenario::key_event(Key::Z, command));
    scenario::wait_until(&mut harness, |app| {
        !app.pending_history_restores.contains_key(&document_id)
    });
    save_active_document(&mut harness, document_id);
    assert_eq!(fs::read(&source).expect("row Undo saves"), original);

    select_b2(&mut harness);
    harness
        .state_mut()
        .request_structure_action(StructureAction::InsertColumns);
    scenario::wait_until(&mut harness, |app| {
        !app.pending_structures.contains_key(&document_id)
    });
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 0 },
        )
        .as_deref(),
        Some("x"),
        "column insertion keeps the first data column"
    );
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 2 },
        )
        .as_deref(),
        Some("y"),
        "column insertion moves the second data column"
    );
    let Some(SourceDocument::Csv(csv)) = harness.state().workspace.documents[&document_id]
        .source
        .as_deref()
    else {
        panic!("the inserted column keeps a CSV source");
    };
    assert_eq!(
        csv.source_bytes(),
        b"a,,b\r\nx,,y\r\n",
        "the worker source matches the transformed document before saving"
    );
    save_active_document(&mut harness, document_id);
    assert_eq!(
        fs::read(&source).expect("column insertion saves"),
        b"a,,b\r\nx,,y\r\n"
    );
    let saved_columns = crate::csv_adapter::CsvDocument::parse(
        fs::read(&source).expect("read the saved column insertion"),
        crate::csv_adapter::EncodingChoice::Auto,
        ',',
    )
    .expect("the saved column insertion reopens");
    let reopened_columns = saved_columns.to_document(9002, "reopened.csv");
    assert_eq!(
        reopened_columns.tables[0]
            .cell(CellAddress { row: 1, column: 2 })
            .map(|cell| cell.input.source_text()),
        Some("y".to_owned()),
        "column insertion keeps the last row after reopening"
    );
    scenario::send_event(&mut harness, scenario::key_event(Key::Z, command));
    scenario::wait_until(&mut harness, |app| {
        !app.pending_history_restores.contains_key(&document_id)
    });
    save_active_document(&mut harness, document_id);
    assert_eq!(fs::read(&source).expect("column Undo saves"), original);
}

#[test]
/// Clears a full-sheet selection through sparse cells and restores it with one Undo.
fn select_all_delete_clears_sparse_cells_and_undo_restores_them() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("sparse-delete.md");
    fs::write(
        &source,
        "| Name | Value |\n| --- | --- |\n| keep | one |\n| other | two |\n",
    )
    .expect("write a small Markdown table");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source);
    let document_id = harness.state().workspace.active_document.unwrap();
    scenario::click_cell(&mut harness, "keep");
    let original = harness.state().workspace.documents[&document_id].tables[0]
        .stored_cells()
        .map(|(address, cell)| (address, cell.clone()))
        .collect::<Vec<_>>();
    assert!(!original.is_empty());

    let command = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(&mut harness, scenario::key_event(Key::A, command));
    let document = &harness.state().workspace.documents[&document_id];
    assert_eq!(
        document.views[&document.active_table].selection,
        CellRange {
            anchor: CellAddress::default(),
            head: CellAddress {
                row: MAX_ROWS - 1,
                column: MAX_COLUMNS - 1,
            },
        }
    );
    let revision_before_clear = document.revision;

    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Delete, egui::Modifiers::NONE),
    );
    let document = &harness.state().workspace.documents[&document_id];
    assert!(document.revision > revision_before_clear);
    assert!(original.iter().all(|(address, _)| {
        document.tables[0]
            .cell(*address)
            .is_none_or(|cell| cell.input == CellInput::Empty)
    }));

    scenario::send_event(&mut harness, scenario::key_event(Key::Z, command));
    let document = &harness.state().workspace.documents[&document_id];
    assert!(
        original
            .iter()
            .all(|(address, cell)| { document.tables[0].cell(*address) == Some(cell) })
    );
}

/// Saves through the normal command and worker result path.
fn save_active_document(harness: &mut Harness<'_, TexcelApp>, document_id: DocumentId) {
    let command = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(harness, scenario::key_event(Key::S, command));
    scenario::wait_until(harness, |app| {
        !app.pending_saves.contains_key(&document_id)
            && app.workspace.documents[&document_id].saved_revision
                >= app.workspace.documents[&document_id].revision
    });
}

#[test]
/// Keeps normal growth feedback, undo, and saved Markdown aligned with one source table.
fn adjacent_growth_is_one_undo_and_has_no_status_notice() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("growth.md");
    fs::write(&source, "| A | B |\n| --- | --- |\n| old | keep |\n").expect("write a source table");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source.clone());
    let document_id = harness.state().workspace.active_document.unwrap();
    scenario::click_cell(&mut harness, "A2");
    scenario::send_event(&mut harness, egui::Event::Text("typed".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::ArrowRight, egui::Modifiers::NONE),
    );
    scenario::send_event(&mut harness, egui::Event::Text("moved".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Tab, egui::Modifiers::NONE),
    );
    scenario::send_event(&mut harness, egui::Event::Text("grown".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    assert!(
        harness
            .state()
            .pending_structures
            .contains_key(&document_id)
            || scenario::cell_text(
                harness.state(),
                document_id,
                CellAddress { row: 1, column: 2 }
            )
            .as_deref()
                == Some("grown"),
        "the adjacent growth is pending or already applied"
    );
    scenario::wait_until(&mut harness, |app| {
        app.pending_structures.is_empty()
            && scenario::cell_text(app, document_id, CellAddress { row: 1, column: 2 }).as_deref()
                == Some("grown")
    });
    // Growing the saved range is not announced in the status bar.
    assert!(harness.state().status.is_none());
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 2 }
        )
        .as_deref(),
        Some("grown")
    );

    let command = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(&mut harness, scenario::key_event(Key::Z, command));
    scenario::wait_until(&mut harness, |app| {
        app.pending_structures.is_empty()
            && scenario::cell_text(app, document_id, CellAddress { row: 1, column: 2 }).is_none()
    });
    assert!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 2 }
        )
        .is_none()
    );
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 0 }
        )
        .as_deref(),
        Some("typed")
    );
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 1 }
        )
        .as_deref(),
        Some("moved")
    );

    scenario::send_event(&mut harness, scenario::key_event(Key::S, command));
    scenario::wait_until(&mut harness, |app| app.pending_saves.is_empty());
    let saved = crate::markdown_adapter::MarkdownDocument::parse(
        fs::read(&source).expect("the original Markdown path receives the save"),
    )
    .expect("the saved table remains valid Markdown");
    assert_eq!(saved.tables[0].rows[1][0].value, "typed");
    assert_eq!(saved.tables[0].rows[1][1].value, "moved");
    assert_eq!(saved.tables[0].rows[1].len(), 2);

    let mut reopened = scenario_harness(directory.0.join("reopened-settings.json"));
    scenario::queue_open_and_wait(&mut reopened, source);
    let reopened_id = reopened.state().workspace.active_document.unwrap();
    assert_eq!(
        scenario::cell_text(
            reopened.state(),
            reopened_id,
            CellAddress { row: 1, column: 0 }
        )
        .as_deref(),
        Some("typed")
    );
    assert_eq!(
        scenario::cell_text(
            reopened.state(),
            reopened_id,
            CellAddress { row: 1, column: 1 }
        )
        .as_deref(),
        Some("moved")
    );
}

#[test]
/// Starts formula-bar editing at the clicked source position instead of the first character.
fn formula_bar_click_keeps_the_pointer_caret_position() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("formula-bar-caret.md");
    fs::write(&source, "| Value |\n| --- |\n| abcd |\n").expect("write one editable cell value");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source);
    scenario::click_cell(&mut harness, "A2");
    let document_id = harness.state().workspace.active_document.unwrap();
    let document = &harness.state().workspace.documents[&document_id];
    let table = document.active_table().unwrap();
    assert_eq!(
        document.view(table.id).unwrap().active_cell(),
        CellAddress { row: 1, column: 0 }
    );
    assert_eq!(
        table
            .cell(CellAddress { row: 1, column: 0 })
            .unwrap()
            .input
            .source_text(),
        "abcd"
    );
    let previews = harness
        .ctx
        .interactive_rects_last_pass()
        .into_iter()
        .filter(|rect| rect.width() > 300.0 && (rect.height() - CHROME_CONTROL_HEIGHT).abs() < 1.0)
        .collect::<Vec<_>>();
    assert_eq!(
        previews.len(),
        1,
        "the formula-bar preview has one wide hit area"
    );
    let preview = previews[0];
    scenario::click_point(
        &mut harness,
        Pos2::new(preview.left() + 24.0, preview.center().y),
    );
    harness.step();
    assert_eq!(
        harness.state().edit.target,
        Some(crate::document::EditTarget::FormulaBar)
    );
    assert!(
        harness.state().edit.cursor_byte > 0
            && harness.state().edit.cursor_byte < harness.state().edit.buffer.len()
    );
    scenario::send_event(&mut harness, egui::Event::Text("X".to_owned()));
    assert_eq!(harness.state().edit.buffer, "abXcd");
}

#[test]
/// Makes the formula bar use text-caret arrows without starting a new edit session.
fn formula_bar_switches_input_and_reference_modes_to_edit() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("formula-bar-mode.md");
    fs::write(&source, "| Value | Next |\n| --- | --- |\n| abc | =1+2 |\n")
        .expect("write a text cell and a formula cell");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source);
    let document_id = harness.state().workspace.active_document.unwrap();

    scenario::click_cell(&mut harness, "A2");
    scenario::send_event(&mut harness, egui::Event::Text("draft".to_owned()));
    let text_session = harness.state().edit.session_id;
    assert_eq!(harness.state().edit.mode, EditMode::Input);
    click_active_formula_bar_preview(&mut harness);
    harness.step();
    assert_eq!(harness.state().edit.mode, EditMode::Edit);
    assert_eq!(harness.state().edit.session_id, text_session);
    let cursor_before = harness.state().edit.cursor_byte;
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::ArrowLeft, egui::Modifiers::NONE),
    );
    assert!(harness.state().edit.cursor_byte < cursor_before);
    assert_eq!(harness.state().edit.buffer, "draft");
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 0 }
        )
        .as_deref(),
        Some("abc"),
        "switching editors does not commit the draft"
    );

    scenario::click_cell(&mut harness, "B2");
    assert_eq!(
        active_cell(&harness, document_id),
        CellAddress { row: 1, column: 1 },
        "the second cell click selects B2"
    );
    assert!(
        !harness.state().edit.is_editing(),
        "the previous formula-bar edit finishes before B2 input: {:?}",
        harness.state().edit
    );
    assert!(harness.state().grid_focused);
    scenario::send_event(&mut harness, egui::Event::Text("=".to_owned()));
    harness.step();
    assert_eq!(
        harness.state().edit.address,
        Some(CellAddress { row: 1, column: 1 }),
        "the new input belongs to B2: {:?}",
        harness.state().edit
    );
    assert!(harness.state().edit.buffer.starts_with('='));
    assert_eq!(harness.state().edit.mode, EditMode::FormulaReference);
    let formula_session = harness.state().edit.session_id;
    click_active_formula_bar_preview(&mut harness);
    harness.step();
    assert_eq!(harness.state().edit.mode, EditMode::Edit);
    assert_eq!(harness.state().edit.session_id, formula_session);
    let cursor_before = harness.state().edit.cursor_byte;
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::ArrowLeft, egui::Modifiers::NONE),
    );
    assert!(harness.state().edit.cursor_byte < cursor_before);
    assert_eq!(harness.state().edit.session_id, formula_session);
}

#[test]
/// Runs select-all, cut, and undo from the formula bar editor's context menu.
fn formula_bar_context_menu_edits_the_draft() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("formula-bar-menu.md");
    fs::write(&source, "| Value |\n| --- |\n| abcd |\n").expect("write one editable cell value");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source);
    scenario::click_cell(&mut harness, "A2");
    click_active_formula_bar_preview(&mut harness);
    harness.step();
    assert_eq!(
        harness.state().edit.target,
        Some(crate::document::EditTarget::FormulaBar)
    );
    let session = harness.state().edit.session_id;
    for (label, expected) in [
        ("action-select-all", "abcd"),
        ("action-cut", ""),
        ("top-undo", "abcd"),
    ] {
        choose_formula_bar_menu_item(&mut harness, label);
        assert_eq!(harness.state().edit.buffer, expected, "after {label}");
        assert_eq!(harness.state().edit.session_id, session);
    }
    assert_eq!(
        harness.state().edit.target,
        Some(crate::document::EditTarget::FormulaBar),
        "the menu keeps the formula bar edit open"
    );
}

/// Opens the formula bar editor's context menu and clicks one command in it.
fn choose_formula_bar_menu_item(harness: &mut Harness<'_, TexcelApp>, label: &str) {
    let editor = harness
        .ctx
        .interactive_rects_last_pass()
        .into_iter()
        .find(|rect| rect.width() > 300.0 && (rect.height() - CHROME_CONTROL_HEIGHT).abs() < 1.0)
        .expect("the formula-bar editor has a wide hit area");
    let point = Pos2::new(editor.right() - 20.0, editor.center().y);
    scenario::send_event(harness, egui::Event::PointerMoved(point));
    for pressed in [true, false] {
        scenario::send_event(
            harness,
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        );
    }
    harness.step();
    let text = text(harness.state().language, label);
    let item = harness.get_by_label(&text).rect().center();
    scenario::click_point(harness, item);
    harness.step();
    harness.step();
}

/// Clicks the formula-bar preview while a cell editor still owns the draft.
fn click_active_formula_bar_preview(harness: &mut Harness<'_, TexcelApp>) {
    let preview = harness
        .ctx
        .interactive_rects_last_pass()
        .into_iter()
        .find(|rect| rect.width() > 300.0 && (rect.height() - CHROME_CONTROL_HEIGHT).abs() < 1.0)
        .expect("the formula-bar preview has a wide click area");
    scenario::click_point(
        harness,
        Pos2::new(preview.right() - 20.0, preview.center().y),
    );
}

#[test]
/// Routes same-frame text and navigation through their cells in event order.
fn ordered_input_batch_moves_before_its_following_text_and_undoes_by_cell() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("ordered-input.md");
    fs::write(
        &source,
        "| A | B | C |\n| --- | --- | --- |\n| old-a | old-b | old-c |\n| old-a3 | old-b3 | old-c3 |\n",
    )
    .expect("write cells with values to protect from misplaced text");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source.clone());
    let document_id = harness.state().workspace.active_document.unwrap();
    scenario::click_cell(&mut harness, "A2");

    scenario::send_batch(
        &mut harness,
        [
            egui::Event::Text("77".to_owned()),
            scenario::key_event(Key::ArrowRight, egui::Modifiers::NONE),
            scenario::key_event(Key::ArrowDown, egui::Modifiers::NONE),
        ],
    );
    assert_eq!(
        harness.state().edit.address,
        Some(CellAddress { row: 1, column: 0 })
    );
    assert_eq!(harness.state().edit.buffer, "77");
    wait_for_ordered_input(&mut harness, |app| {
        !app.edit.is_editing()
            && app.pending_edit_input.is_none()
            && active_cell_from_app(app, document_id) == CellAddress { row: 2, column: 1 }
    });
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 0 }
        )
        .as_deref(),
        Some("77")
    );
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 1 }
        )
        .as_deref(),
        Some("old-b"),
        "text before the arrow stays out of its destination cell"
    );

    scenario::click_cell(&mut harness, "B2");
    scenario::send_event(&mut harness, egui::Event::Text("prefix".to_owned()));
    scenario::send_batch(
        &mut harness,
        [
            egui::Event::Text("a".to_owned()),
            scenario::key_event(Key::ArrowRight, egui::Modifiers::NONE),
            egui::Event::Text("b".to_owned()),
        ],
    );
    wait_for_ordered_input(&mut harness, |app| {
        app.edit.address == Some(CellAddress { row: 1, column: 2 })
            && app.edit.mode == EditMode::Input
            && app.edit.buffer == "b"
            && app.pending_edit_input.is_none()
    });
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 1 }
        )
        .as_deref(),
        Some("prefixa")
    );
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    let command = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(&mut harness, scenario::key_event(Key::Z, command));
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 2 }
        )
        .as_deref(),
        Some("old-c"),
        "Undo reverts the destination cell without reverting the prefix cell"
    );
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 1 }
        )
        .as_deref(),
        Some("prefixa")
    );
    let mut redo = command;
    redo.shift = true;
    scenario::send_event(&mut harness, scenario::key_event(Key::Z, redo));
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 2 }
        )
        .as_deref(),
        Some("b")
    );
    scenario::send_event(&mut harness, scenario::key_event(Key::S, command));
    scenario::wait_until(&mut harness, |app| app.pending_saves.is_empty());
    let saved = crate::markdown_adapter::MarkdownDocument::parse(
        fs::read(&source).expect("the source path receives both ordered cell edits"),
    )
    .expect("the saved ordered edits remain valid Markdown");
    assert_eq!(saved.tables[0].rows[1][0].value, "77");
    assert_eq!(saved.tables[0].rows[1][1].value, "prefixa");
    assert_eq!(saved.tables[0].rows[1][2].value, "b");
}

#[test]
/// Commits text at successive sparse cells before saving and reopening the source.
fn ordered_input_batch_commits_sparse_cells_and_saves_in_the_next_cell() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("ordered-growth.md");
    fs::write(&source, "A|B\n---|---\na|b\n").expect("write a pipe-free table");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source.clone());
    let document_id = harness.state().workspace.active_document.unwrap();
    scenario::click_cell(&mut harness, "C2");
    assert_eq!(
        active_cell(&harness, document_id),
        CellAddress { row: 1, column: 2 }
    );
    scenario::send_batch(
        &mut harness,
        [
            egui::Event::Text("edge".to_owned()),
            scenario::key_event(Key::ArrowRight, egui::Modifiers::NONE),
            egui::Event::Key {
                key: Key::ArrowRight,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Text("next".to_owned()),
            scenario::key_event(Key::Enter, egui::Modifiers::NONE),
            egui::Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::ModifiersChanged(egui::Modifiers::NONE),
        ],
    );
    wait_for_ordered_input(&mut harness, |app| {
        !app.edit.is_editing()
            && app.pending_structures.is_empty()
            && app.pending_text_growth_sessions.is_empty()
            && app.pending_edit_input.is_none()
            && scenario::cell_text(app, document_id, CellAddress { row: 1, column: 2 }).as_deref()
                == Some("edge")
            && scenario::cell_text(app, document_id, CellAddress { row: 1, column: 3 }).as_deref()
                == Some("next")
    });
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 2 }
        )
        .as_deref(),
        Some("edge")
    );
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 3 }
        )
        .as_deref(),
        Some("next")
    );
    assert!(
        harness.state().workspace.documents[&document_id].is_dirty(),
        "the sparse edits remain dirty before Save"
    );
    let command = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(&mut harness, scenario::key_event(Key::S, command));
    wait_for_ordered_input(&mut harness, |app| {
        app.pending_saves.is_empty()
            && app.pending_edit_input.is_none()
            && app
                .workspace
                .documents
                .get(&document_id)
                .is_some_and(|document| !document.is_dirty())
    });
    let saved = crate::markdown_adapter::MarkdownDocument::parse(
        fs::read(&source).expect("the source path receives both grown cells"),
    )
    .expect("the expanded table remains valid Markdown");
    assert_eq!(
        saved.tables[0].rows[1]
            .get(2)
            .map(|cell| cell.value.as_str()),
        Some("edge"),
        "the saved table rows are {:?}",
        saved.tables[0].rows
    );
    assert_eq!(
        saved.tables[0].rows[1]
            .get(3)
            .map(|cell| cell.value.as_str()),
        Some("next"),
        "the saved table rows are {:?}",
        saved.tables[0].rows
    );
    assert_eq!(saved.tables[0].rows[1].len(), 4);

    let mut reopened = scenario_harness(directory.0.join("reopened-settings.json"));
    scenario::queue_open_and_wait(&mut reopened, source);
    let reopened_id = reopened.state().workspace.active_document.unwrap();
    assert_eq!(
        scenario::cell_text(
            reopened.state(),
            reopened_id,
            CellAddress { row: 1, column: 2 }
        )
        .as_deref(),
        Some("edge")
    );
    assert_eq!(
        scenario::cell_text(
            reopened.state(),
            reopened_id,
            CellAddress { row: 1, column: 3 }
        )
        .as_deref(),
        Some("next")
    );

    scenario::send_event(&mut harness, scenario::key_event(Key::Z, command));
    assert!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 3 }
        )
        .is_none()
    );
    scenario::send_event(&mut harness, scenario::key_event(Key::Z, command));
    assert!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 2 }
        )
        .is_none()
    );
}

#[test]
/// Keeps a separate-frame text event until an adjacent-cell expansion is acknowledged.
fn input_after_a_tab_growth_boundary_reaches_the_next_cell() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("growth-next-frame.md");
    fs::write(&source, "A|B\n---|---\na|b\n").expect("write a small table");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source.clone());
    let document_id = harness.state().workspace.active_document.unwrap();
    scenario::click_cell(&mut harness, "C2");
    assert_eq!(
        active_cell(&harness, document_id),
        CellAddress { row: 1, column: 2 }
    );

    scenario::send_event(&mut harness, egui::Event::Text("edge".to_owned()));
    scenario::send_batch(
        &mut harness,
        [
            egui::Event::ModifiersChanged(egui::Modifiers::NONE),
            scenario::key_event(Key::Tab, egui::Modifiers::NONE),
        ],
    );
    assert!(
        harness
            .state()
            .pending_text_growth_sessions
            .contains_key(&document_id)
    );
    scenario::send_event(&mut harness, egui::Event::Text("next".to_owned()));
    scenario::send_batch(
        &mut harness,
        [
            egui::Event::Key {
                key: Key::Tab,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::ModifiersChanged(egui::Modifiers::NONE),
        ],
    );
    scenario::wait_until(&mut harness, |app| {
        app.pending_structures.is_empty()
            && app.pending_text_growth_sessions.is_empty()
            && app.pending_edit_input.is_none()
            && app.edit.is_editing()
            && app.edit.buffer == "next"
            && app
                .workspace
                .active_document()
                .and_then(|document| document.active_table())
                .and_then(|table| {
                    app.workspace
                        .active_document()
                        .and_then(|document| document.view(table.id))
                })
                .is_some_and(|view| view.active_cell() == CellAddress { row: 1, column: 3 })
    });

    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    scenario::wait_until(&mut harness, |app| {
        app.pending_structures.is_empty()
            && app.pending_text_growth_sessions.is_empty()
            && app.pending_edit_input.is_none()
            && scenario::cell_text(app, document_id, CellAddress { row: 1, column: 2 }).as_deref()
                == Some("edge")
            && scenario::cell_text(app, document_id, CellAddress { row: 1, column: 3 }).as_deref()
                == Some("next")
    });

    let command = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(&mut harness, scenario::key_event(Key::S, command));
    scenario::wait_until(&mut harness, |app| {
        app.pending_saves.is_empty() && !app.workspace.documents[&document_id].is_dirty()
    });
    let saved = crate::markdown_adapter::MarkdownDocument::parse(
        fs::read(&source).expect("the source path receives both entries"),
    )
    .expect("the expanded table remains valid Markdown");
    assert_eq!(saved.tables[0].rows[1][2].value, "edge");
    assert_eq!(saved.tables[0].rows[1][3].value, "next");
}

/// Reads the active cell after a queued UI frame updates the document view.
fn active_cell_from_app(app: &TexcelApp, document_id: DocumentId) -> CellAddress {
    let document = &app.workspace.documents[&document_id];
    let table = document.active_table().unwrap();
    document.view(table.id).unwrap().active_cell()
}

/// Advances queued ordered input through the production raw-input hook.
fn wait_for_ordered_input(
    harness: &mut Harness<'_, TexcelApp>,
    ready: impl Fn(&TexcelApp) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready(harness.state()) {
        scenario::send_batch(harness, []);
        assert!(
            Instant::now() < deadline,
            "ordered input scenario timed out: edit={:?}, active={:?}, pending={:?}, table={:?}, growth={}, structures={}, saves={}, dirty={:?}, grid_focus={}, tab={:?}, status={:?}",
            harness.state().edit,
            harness
                .state()
                .workspace
                .active_document()
                .and_then(|document| document.active_table())
                .and_then(|table| harness.state().workspace.active_document()?.view(table.id))
                .map(|view| view.active_cell()),
            harness
                .state()
                .pending_edit_input
                .as_ref()
                .map(|pending| format!("{pending:?}")),
            harness
                .state()
                .workspace
                .active_document()
                .and_then(|document| document.active_table().map(|table| (
                    table.column_count(),
                    table
                        .cell(CellAddress { row: 1, column: 2 })
                        .map(|cell| &cell.input),
                    table
                        .cell(CellAddress { row: 1, column: 3 })
                        .map(|cell| &cell.input)
                ))),
            harness.state().pending_text_growth_sessions.len(),
            harness.state().pending_structures.len(),
            harness.state().pending_saves.len(),
            harness
                .state()
                .workspace
                .active_document()
                .map(|document| document.is_dirty()),
            harness.state().grid_focused,
            harness.state().input_sequence_origin,
            harness.state().status
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
/// Saves representative English and Japanese light and dark grid and formula-bar images.
#[ignore = "Run on a host with a usable WGPU adapter for visual review."]
fn render_grid_input_mode_matrix() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("appearance.md");
    fs::write(
        &source,
        "| ID | Name | Value |\n| --- | --- | --- |\n| 00123 | **東京** | =1+2 |\n| 7 | Plain | 4 |\n",
    )
    .expect("write a table with an explicit header and source values");
    let images = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("kittest-renders");
    fs::create_dir_all(&images).expect("create the review image directory");

    for (language, language_name) in [(Language::English, "en"), (Language::Japanese, "ja")] {
        for (theme, theme_name) in [(ThemeChoice::Light, "light"), (ThemeChoice::Dark, "dark")] {
            let settings = directory
                .0
                .join(format!("settings-{language_name}-{theme_name}.json"));
            let mut harness = scenario_harness(settings);
            scenario::queue_open_and_wait(&mut harness, source.clone());
            let ctx = harness.ctx.clone();
            {
                let app = harness.state_mut();
                app.language = language;
                app.language_choice = match language {
                    Language::English => LanguageChoice::English,
                    Language::Japanese => LanguageChoice::Japanese,
                };
                app.theme = theme;
                crate::i18n::set_context_language(&ctx, language);
                app.apply_appearance(&ctx);
            }
            scenario::click_cell(&mut harness, "B2");
            harness.step();
            let prefix = format!("grid-input-{language_name}-{theme_name}");
            match catch_unwind(AssertUnwindSafe(|| harness.render())) {
                Ok(Ok(image)) => image
                    .save(images.join(format!("{prefix}-idle.png")))
                    .expect("save the idle formula-bar and grid image"),
                Ok(Err(error)) => panic!("grid render is unavailable: {error}"),
                Err(_) => panic!("grid render failed before producing an image"),
            }

            let preview = harness
                .ctx
                .interactive_rects_last_pass()
                .into_iter()
                .find(|rect| {
                    rect.width() > 300.0 && (rect.height() - CHROME_CONTROL_HEIGHT).abs() < 1.0
                })
                .expect("the formula-bar preview has one wide click area");
            scenario::click_point(
                &mut harness,
                Pos2::new(preview.right() - 20.0, preview.center().y),
            );
            harness.step();
            assert_eq!(
                harness.state().edit.target,
                Some(crate::document::EditTarget::FormulaBar)
            );
            // Capture the focused editor after its focus request has reached the widget.
            let document_id = harness.state().workspace.active_document.unwrap();
            let editor = editor_id(document_id, harness.state().edit.session_id);
            for _ in 0..6 {
                harness.step();
                if harness.ctx.memory(|memory| memory.has_focus(editor)) {
                    break;
                }
            }
            assert!(harness.ctx.memory(|memory| memory.has_focus(editor)));
            harness.step();
            match catch_unwind(AssertUnwindSafe(|| harness.render())) {
                Ok(Ok(image)) => image
                    .save(images.join(format!("{prefix}-focused.png")))
                    .expect("save the focused formula-bar and grid image"),
                Ok(Err(error)) => panic!("focused grid render is unavailable: {error}"),
                Err(_) => panic!("focused grid render failed before producing an image"),
            }
        }
    }

    let mut grown = scenario_harness(directory.0.join("settings-growth.json"));
    scenario::queue_open_and_wait(&mut grown, source);
    let grown_ctx = grown.ctx.clone();
    {
        let app = grown.state_mut();
        app.language = Language::English;
        app.language_choice = LanguageChoice::English;
        app.theme = ThemeChoice::Dark;
        crate::i18n::set_context_language(&grown_ctx, Language::English);
        app.apply_appearance(&grown_ctx);
    }
    scenario::click_cell(&mut grown, "D2");
    scenario::send_event(&mut grown, egui::Event::Text("new column".to_owned()));
    scenario::send_event(
        &mut grown,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    let (document_id, table_id) = {
        let document = grown.state().workspace.active_document().unwrap();
        (document.id, document.active_table().unwrap().id)
    };
    scenario::wait_until(&mut grown, |app| {
        let document = &app.workspace.documents[&document_id];
        app.pending_structures.is_empty()
            && !app.pending_initial_calculations.contains_key(&document_id)
            && !app.pending_calculations.contains_key(&document_id)
            && !app.calculation_retries.contains(&document_id)
            && document
                .calculation_result_for(table_id, CellAddress { row: 1, column: 2 })
                .is_some_and(|result| result.status == CalcStatus::Fresh)
            && scenario::cell_text(app, document_id, CellAddress { row: 1, column: 3 }).as_deref()
                == Some("new column")
    });
    grown.step();
    let output = match catch_unwind(AssertUnwindSafe(|| grown.render())) {
        Ok(Ok(image)) => image,
        Ok(Err(error)) => panic!("grown grid render is unavailable: {error}"),
        Err(_) => panic!("grown grid render failed before producing an image"),
    };
    output
        .save(images.join("grid-input-growth-en-dark.png"))
        .expect("save the grown table boundary image");
}

#[test]
/// Saves one real range selection for review of its fill, outer edge, and headers.
#[ignore = "Run on a host with a usable WGPU adapter for visual review."]
fn render_range_selection_fill_and_outer_edge() {
    let directory = scenario::ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(
        &mut harness,
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("samples")
            .join("interaction-workflow.md"),
    );
    scenario::click_cell(&mut harness, "B2");
    let target = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("C3"))
        .next()
        .expect("the lower corner cell is visible")
        .rect()
        .center();
    let mut modifiers = egui::Modifiers::NONE;
    modifiers.shift = true;
    // A native Shift-click reports the held key before the pointer button goes down.
    scenario::send_batch(
        &mut harness,
        [
            egui::Event::ModifiersChanged(modifiers),
            egui::Event::PointerMoved(target),
        ],
    );
    scenario::send_batch(
        &mut harness,
        [egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers,
        }],
    );
    scenario::send_batch(
        &mut harness,
        [egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers,
        }],
    );
    let document = harness.state().workspace.active_document().unwrap();
    let selection = document
        .view(document.active_table().unwrap().id)
        .unwrap()
        .selection
        .normalized();
    assert_eq!(
        selection,
        (
            CellAddress { row: 1, column: 1 },
            CellAddress { row: 2, column: 2 }
        )
    );
    // Paint the selection that was committed at the end of the pointer frame.
    scenario::send_batch(
        &mut harness,
        [egui::Event::ModifiersChanged(egui::Modifiers::NONE)],
    );
    let image = harness.render().expect("render the selected range");
    let images = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("kittest-renders");
    fs::create_dir_all(&images).expect("create the review image directory");
    image
        .save(images.join("range-selection-en-light.png"))
        .expect("save the selected range image");
}
