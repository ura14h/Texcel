use super::*;

use super::scenario_tests as scenario;
use egui_kittest::kittest::{By, Queryable};
use std::fs;
use std::time::{Duration, Instant};

/// Keeps ordinary in-source edits synchronous across Tab, Undo, and save.
#[test]
fn ordinary_text_edits_keep_the_fast_path_and_save_the_remaining_value() {
    let directory = scenario::ScenarioDirectory::new();
    let source_path = directory.0.join("ordinary.md");
    fs::write(
        &source_path,
        "| first | second |\n| --- | --- |\n| old-a | old-b |\n",
    )
    .expect("the small Markdown source is written");
    let mut harness = scenario::scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source_path.clone());
    let document_id = harness.state().workspace.active_document.unwrap();

    scenario::click_cell(&mut harness, "A2");
    scenario::send_event(&mut harness, egui::Event::Text("new-a".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Tab, egui::Modifiers::NONE),
    );
    assert!(harness.state().pending_structures.is_empty());
    assert!(harness.state().pending_text_growth_sessions.is_empty());
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 0 }
        )
        .as_deref(),
        Some("new-a")
    );
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .view(1)
            .unwrap()
            .active_cell(),
        CellAddress { row: 1, column: 1 }
    );

    scenario::send_event(&mut harness, egui::Event::Text("new-b".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    assert!(harness.state().pending_structures.is_empty());
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 1 }
        )
        .as_deref(),
        Some("new-b")
    );

    let undo_modifiers = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(&mut harness, scenario::key_event(Key::Z, undo_modifiers));
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 0 }
        )
        .as_deref(),
        Some("new-a")
    );
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 1 }
        )
        .as_deref(),
        Some("old-b")
    );

    let save_modifiers = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(&mut harness, scenario::key_event(Key::S, save_modifiers));
    scenario::wait_until(&mut harness, |app| {
        app.pending_saves.is_empty()
            && app
                .workspace
                .documents
                .get(&document_id)
                .is_some_and(|document| !document.is_dirty())
    });
    let saved = fs::read(&source_path).expect("the source file is saved");
    let reopened = crate::markdown_adapter::MarkdownDocument::parse(saved)
        .expect("the saved Markdown file can be reopened");
    assert_eq!(reopened.tables[0].rows[1][0].value, "new-a");
    assert_eq!(reopened.tables[0].rows[1][1].value, "old-b");
}

/// Accepts the exact byte limit, then rejects the next edit without changing history.
#[test]
fn reaching_markdown_byte_limit_warns_and_blocks_only_the_overflowing_edit() {
    let directory = scenario::ScenarioDirectory::new();
    let source_path = directory.0.join("limit.md");
    let base = b"| value |\n| --- |\n| x |\n";
    let prefix = b"<!--";
    let suffix = b"-->";
    let filler_len =
        crate::markdown_adapter::MAX_MARKDOWN_BYTES - 1 - base.len() - prefix.len() - suffix.len();
    let mut bytes = Vec::with_capacity(crate::markdown_adapter::MAX_MARKDOWN_BYTES - 1);
    bytes.extend_from_slice(base);
    bytes.extend_from_slice(prefix);
    bytes.resize(bytes.len() + filler_len, b'x');
    bytes.extend_from_slice(suffix);
    assert_eq!(bytes.len(), crate::markdown_adapter::MAX_MARKDOWN_BYTES - 1);
    fs::write(&source_path, bytes).expect("the source is written just below the reader limit");

    let mut harness = scenario::scenario_harness(directory.0.join("settings.json"));
    harness.state_mut().queue_open_paths([source_path.clone()]);
    let deadline = Instant::now() + Duration::from_secs(30);
    while harness.state().active_queued_open.is_some()
        || !harness.state().queued_open_paths.is_empty()
        || !harness.state().pending_markdown_loads.is_empty()
        // The window shows once its columns are fitted on opening.
        || !harness.state().opening_fits.is_empty()
    {
        scenario::send_batch(&mut harness, []);
        assert!(
            Instant::now() < deadline,
            "large Markdown source load timed out"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    let document_id = harness.state().workspace.active_document.unwrap();
    assert!(matches!(
        harness.state().workspace.documents[&document_id]
            .source
            .as_deref(),
        Some(SourceDocument::Markdown(_))
    ));

    scenario::click_cell(&mut harness, "A2");
    scenario::send_event(&mut harness, egui::Event::Text("xy".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    let document = &harness.state().workspace.documents[&document_id];
    assert_eq!(
        document.check_text_cell_resource_limit(
            1,
            CellAddress { row: 1, column: 0 },
            Some(&Cell::text("xy"))
        ),
        crate::document::TextCellResourceCheck::ReachedLimit(crate::i18n::ResourceLimit::FileSize)
    );
    assert!(
        harness
            .state()
            .status
            .as_ref()
            .unwrap()
            .resolve(harness.state().language)
            .contains("64 MiB")
    );
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 0 }
        )
        .as_deref(),
        Some("xy")
    );

    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Escape, egui::Modifiers::NONE),
    );
    let save_modifiers = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(&mut harness, scenario::key_event(Key::S, save_modifiers));
    let save_deadline = Instant::now() + Duration::from_secs(30);
    while !harness.state().pending_saves.is_empty()
        || harness.state().workspace.documents[&document_id].is_dirty()
    {
        scenario::send_batch(&mut harness, []);
        assert!(
            Instant::now() < save_deadline,
            "the exact-limit save timed out"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    let saved = fs::read(&source_path).expect("the exact-limit edit is saved");
    assert_eq!(saved.len(), crate::markdown_adapter::MAX_MARKDOWN_BYTES);
    let reopened = crate::markdown_adapter::MarkdownDocument::parse(saved)
        .expect("the exact-limit output can be reopened");
    assert_eq!(reopened.tables[0].rows[1][0].value, "xy");

    scenario::click_cell(&mut harness, "A2");
    scenario::send_event(&mut harness, egui::Event::Text("xyz".to_owned()));
    let before_revision = harness.state().workspace.documents[&document_id].revision;
    let before_state = harness.state().workspace.documents[&document_id].history_state_id();
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    let document = &harness.state().workspace.documents[&document_id];
    assert!(harness.state().edit.is_editing());
    assert_eq!(harness.state().edit.buffer, "xyz");
    assert_eq!(document.revision, before_revision);
    assert_eq!(document.history_state_id(), before_state);
    assert!(
        harness
            .state()
            .status
            .as_ref()
            .unwrap()
            .resolve(harness.state().language)
            .contains("64 MiB")
    );
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 0 }
        )
        .as_deref(),
        Some("xy")
    );

    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Backspace, egui::Modifiers::NONE),
    );
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Backspace, egui::Modifiers::NONE),
    );
    assert_eq!(harness.state().edit.buffer, "x");
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 0 }
        )
        .as_deref(),
        Some("x")
    );
    assert_eq!(
        harness.state().workspace.documents[&document_id].check_text_cell_resource_limit(
            1,
            CellAddress { row: 1, column: 0 },
            Some(&Cell::text("x"))
        ),
        crate::document::TextCellResourceCheck::WithinLimit
    );

    let undo = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(&mut harness, scenario::key_event(Key::Z, undo));
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 0 }
        )
        .as_deref(),
        Some("xy"),
        "the rejected edit added no extra history entry"
    );
    scenario::send_event(&mut harness, scenario::key_event(Key::Z, undo));
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 0 }
        )
        .as_deref(),
        Some("x")
    );
}

/// Rejects a multi-cell paste atomically when the reader would cross its cell cap.
#[test]
fn paste_crossing_markdown_cell_limit_does_not_apply_its_first_cell() {
    let directory = scenario::ScenarioDirectory::new();
    let source_path = directory.0.join("cell-limit.md");
    let source_text = "| header |\n| --- |\n| base |\n";
    fs::write(&source_path, source_text).expect("the source table is written below the cell cap");

    let mut harness = scenario::scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source_path.clone());
    let document_id = harness.state().workspace.active_document.unwrap();
    let document = &harness.state().workspace.documents[&document_id];
    assert_eq!(document.reached_text_source_limit(), None);
    assert_eq!(document.tables[0].row_count(), 2);
    let table_id = document.active_table().unwrap().id;
    let target = CellAddress {
        row: crate::markdown_adapter::MAX_MARKDOWN_CELLS / 2,
        column: 0,
    };
    let name_box_point = harness
        .query_all(By::new().role(egui::Role::TextInput))
        .find(|node| node.value().as_deref() == Some("A1"))
        .expect("the name box is available before paste")
        .rect()
        .center();
    scenario::click_point(&mut harness, name_box_point);
    let command = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(&mut harness, scenario::key_event(Key::A, command));
    scenario::send_event(
        &mut harness,
        egui::Event::Text(crate::xlsx_adapter::cell_reference(target)),
    );
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .view(table_id)
            .unwrap()
            .active_cell(),
        target
    );
    let before_revision = harness.state().workspace.documents[&document_id].revision;
    let before_state = harness.state().workspace.documents[&document_id].history_state_id();
    let source_before = fs::read(&source_path).expect("the source bytes are captured");

    scenario::send_event(&mut harness, egui::Event::Paste("first\tsecond".to_owned()));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let rejected = harness.state().status.as_ref().is_some_and(|message| {
            message
                .resolve(harness.state().language)
                .contains("500,000")
        });
        if harness.state().pending_structures.is_empty() && rejected {
            break;
        }
        scenario::send_batch(&mut harness, []);
        assert!(Instant::now() < deadline, "the over-limit paste timed out");
        std::thread::sleep(Duration::from_millis(2));
    }

    assert_eq!(
        harness.state().workspace.documents[&document_id].revision,
        before_revision,
        "a rejected paste does not create a partial document edit"
    );
    assert_eq!(
        harness.state().workspace.documents[&document_id].history_state_id(),
        before_state
    );
    assert_eq!(
        scenario::cell_text(harness.state(), document_id, target).as_deref(),
        None,
        "the first pasted cell was not partially applied"
    );
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress {
                row: target.row,
                column: target.column + 1,
            }
        ),
        None,
        "the second pasted cell was not added"
    );
    assert_eq!(
        fs::read(&source_path).expect("a rejected paste leaves the file untouched"),
        source_before
    );
}

/// Completes a clear command on the worker when one pending source field is not mapped.
#[test]
fn clearing_text_cells_uses_worker_validation_for_an_unmapped_source_overlay() {
    let directory = scenario::ScenarioDirectory::new();
    let source_path = directory.0.join("clear-overlay.md");
    fs::write(&source_path, "| Value |\n| --- |\n| base |\n")
        .expect("the small Markdown source is written");
    let mut harness = scenario::scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source_path.clone());
    let document_id = harness.state().workspace.active_document.unwrap();
    let table_id = harness.state().workspace.documents[&document_id]
        .active_table()
        .unwrap()
        .id;
    let address = CellAddress { row: 2, column: 0 };
    {
        let document = &mut harness
            .state_mut()
            .workspace
            .documents
            .get_mut(&document_id)
            .unwrap();
        let table = document
            .tables
            .iter_mut()
            .find(|table| table.id == table_id)
            .unwrap();
        table.set_dimensions(3, 1);
        table.set_loaded_cell(address, Cell::text("staged"));
        document.record_changed_cells([(table_id, address)]);
    }

    scenario::click_cell(&mut harness, "A3");
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Backspace, egui::Modifiers::NONE),
    );
    scenario::wait_until(&mut harness, |app| {
        app.pending_structures.is_empty()
            && app.workspace.documents[&document_id]
                .source
                .as_deref()
                .is_some_and(|source| matches!(source, SourceDocument::Markdown(_)))
    });
    assert_eq!(
        scenario::cell_text(harness.state(), document_id, address),
        Some(String::new()),
        "the requested clear is applied after worker validation"
    );

    let save = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(&mut harness, scenario::key_event(Key::S, save));
    scenario::wait_until(&mut harness, |app| {
        app.pending_saves.is_empty()
            && app.workspace.documents[&document_id]
                .source
                .as_deref()
                .is_some_and(|source| matches!(source, SourceDocument::Markdown(_)))
            && !app.workspace.documents[&document_id].is_dirty()
    });
    let reopened = crate::markdown_adapter::MarkdownDocument::parse(
        fs::read(&source_path).expect("the cleared source is saved"),
    )
    .expect("the saved Markdown can be reopened");
    assert_eq!(reopened.tables[0].rows[1][0].value, "base");
    assert!(
        !reopened.tables[0]
            .rows
            .iter()
            .flatten()
            .any(|cell| cell.value == "staged")
    );

    let undo = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(&mut harness, scenario::key_event(Key::Z, undo));
    scenario::wait_until(&mut harness, |app| {
        app.pending_history_restores.is_empty()
            && scenario::cell_text(app, document_id, address).as_deref() == Some("staged")
    });
}

/// Completes a formula result replacement on the worker without losing its typed value.
#[test]
fn replacing_formula_results_uses_worker_validation_for_an_unmapped_source_overlay() {
    let directory = scenario::ScenarioDirectory::new();
    let source_path = directory.0.join("formula-overlay.md");
    fs::write(&source_path, "| Formula |\n| --- |\n| =1+2 |\n")
        .expect("the small formula source is written");
    let mut harness = scenario::scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source_path.clone());
    let document_id = harness.state().workspace.active_document.unwrap();
    let table_id = harness.state().workspace.documents[&document_id]
        .active_table()
        .unwrap()
        .id;
    let formula_address = CellAddress { row: 1, column: 0 };
    let overlay_address = CellAddress { row: 2, column: 0 };
    {
        let document = &mut harness
            .state_mut()
            .workspace
            .documents
            .get_mut(&document_id)
            .unwrap();
        document.apply_calculation_results(
            document.calculation_revision,
            [(
                table_id,
                formula_address,
                CalcStatus::Fresh,
                Some(crate::document::CalcValue::Number(3.0)),
            )],
        );
        let table = document
            .tables
            .iter_mut()
            .find(|table| table.id == table_id)
            .unwrap();
        table.set_dimensions(3, 1);
        table.set_loaded_cell(overlay_address, Cell::text("staged"));
        document.record_changed_cells([(table_id, overlay_address)]);
    }

    let replacement = harness
        .state_mut()
        .workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .replace_formula_results_in_ranges(table_id, &[CellRange::single(formula_address)]);
    assert!(
        replacement.is_err(),
        "the exact check requests worker validation"
    );
    let pending = harness
        .state_mut()
        .workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .take_pending_text_source_changes()
        .expect("the replacement remains queued for validation");
    harness.state_mut().submit_text_source_changes(
        pending,
        Some(Message::new("status-formula-replaced").with_number("count", 1)),
    );
    scenario::wait_until(&mut harness, |app| {
        app.pending_structures.is_empty()
            && app.workspace.documents[&document_id].tables[0]
                .cell(formula_address)
                .is_some_and(|cell| cell.input == CellInput::Number(3.0))
    });
    assert_eq!(
        scenario::cell_text(harness.state(), document_id, formula_address).as_deref(),
        Some("3")
    );

    let save = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(&mut harness, scenario::key_event(Key::S, save));
    scenario::wait_until(&mut harness, |app| {
        app.pending_saves.is_empty() && !app.workspace.documents[&document_id].is_dirty()
    });
    let reopened = crate::markdown_adapter::MarkdownDocument::parse(
        fs::read(&source_path).expect("the typed formula result is saved"),
    )
    .expect("the saved Markdown can be reopened");
    assert_eq!(reopened.tables[0].rows[1][0].value, "3");
}

/// Counts fields present in a ragged CSV when a user adds one field and saves it.
#[test]
fn ragged_csv_growth_counts_saved_fields_instead_of_the_bounding_rectangle() {
    let directory = scenario::ScenarioDirectory::new();
    let path = directory.0.join("ragged.csv");
    let header = format!("{}last\r\n", "h,".repeat(500));
    let rows = "old,keep\r\n".repeat(999);
    fs::write(&path, format!("{header}{rows}")).expect("write 1,000 rows with one wide header");

    // The input has 2,499 fields. Filling C2 adds just one field to the second row.
    let mut harness = scenario::scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, path.clone());
    let document_id = harness.state().workspace.active_document.unwrap();
    assert_eq!(
        scenario::cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 1 }
        )
        .as_deref(),
        Some("keep")
    );
    scenario::click_cell(&mut harness, "C2");
    scenario::send_event(&mut harness, egui::Event::Text("added".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    scenario::wait_until(&mut harness, |app| {
        app.pending_structures.is_empty()
            && scenario::cell_text(app, document_id, CellAddress { row: 1, column: 2 }).as_deref()
                == Some("added")
    });

    // Saving must leave every shorter row and its original line ending intact.
    let command = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(&mut harness, scenario::key_event(Key::S, command));
    scenario::wait_until(&mut harness, |app| {
        app.pending_saves.is_empty() && !app.workspace.documents[&document_id].is_dirty()
    });
    let expected = format!("{header}old,keep,added\r\n{}", "old,keep\r\n".repeat(998));
    assert_eq!(fs::read(&path).unwrap(), expected.as_bytes());
}
