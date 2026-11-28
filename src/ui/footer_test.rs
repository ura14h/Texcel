use super::*;
use crate::ui::scenario_tests as scenario;
use egui::Key;
use egui_kittest::{
    Harness,
    kittest::{By, NodeT, Queryable},
};
use std::fs;

/// Keeps the status label in place when a save action appears and disappears.
#[test]
fn pending_save_uses_a_fixed_status_action_lane() {
    let mut harness = Harness::builder()
        .with_size([800.0, 560.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    harness.step();
    let document_id = harness
        .state()
        .workspace
        .active_document()
        .expect("the initial document is open")
        .id;
    let ctx = harness.ctx.clone();
    {
        let app = harness.state_mut();
        app.language = Language::English;
        app.apply_appearance(&ctx);
        app.status = Some(Message::new("status-settings-save-location-unavailable"));
        crate::i18n::set_context_language(&ctx, Language::English);
    }
    // Let the English font and translated sheet controls finish layout before measuring.
    harness.step();
    harness.step();
    let before = status_label_rect(&harness);
    assert!(
        harness
            .query_all(
                By::new()
                    .role(egui::Role::Button)
                    .label_contains("Cancel save")
            )
            .next()
            .is_none(),
        "the save control is hidden while no save is running"
    );

    harness.state_mut().pending_saves.insert(
        document_id,
        JobToken {
            document_id,
            revision: 0,
            job_id: 1,
        },
    );
    harness.step();
    let cancel = harness
        .query_all(
            By::new()
                .role(egui::Role::Button)
                .label_contains("Cancel save"),
        )
        .next()
        .expect("the status row exposes the active save control");
    assert_eq!(cancel.rect().height(), CHROME_STATUS_HEIGHT - 8.0);
    assert_eq!(status_label_rect(&harness), before);

    harness.state_mut().pending_saves.remove(&document_id);
    harness.step();
    assert_eq!(status_label_rect(&harness), before);
    assert!(
        harness
            .query_all(
                By::new()
                    .role(egui::Role::Button)
                    .label_contains("Cancel save")
            )
            .next()
            .is_none(),
        "the action disappears after its work ends"
    );
}

/// Shows the selected-cell count while a real pointer range has more than one cell.
#[test]
fn selection_count_tracks_shift_click_ranges() {
    let directory = scenario::ScenarioDirectory::new();
    let mut harness = scenario::scenario_harness(directory.0.join("settings.json"));
    harness.state_mut().language = Language::English;
    crate::i18n::set_context_language(&harness.ctx, Language::English);
    scenario::queue_open_and_wait(
        &mut harness,
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("samples")
            .join("interaction-workflow.md"),
    );
    scenario::click_cell(&mut harness, "D4");
    let end = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("C3"))
        .next()
        .expect("the range endpoint is visible")
        .rect()
        .center();
    let mut shift = egui::Modifiers::NONE;
    shift.shift = true;
    scenario::send_batch(
        &mut harness,
        [
            egui::Event::ModifiersChanged(shift),
            egui::Event::PointerMoved(end),
        ],
    );
    scenario::send_batch(
        &mut harness,
        [egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: shift,
        }],
    );
    scenario::send_batch(
        &mut harness,
        [
            egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: shift,
            },
            egui::Event::ModifiersChanged(egui::Modifiers::NONE),
        ],
    );
    scenario::send_batch(&mut harness, []);
    assert!(
        harness
            .query_all(By::new().label_contains("4 cells selected (2R × 2C)"))
            .next()
            .is_some(),
        "the status row displays the range area"
    );
    let count_rect = harness
        .query_all(By::new().label_contains("4 cells selected"))
        .next()
        .unwrap()
        .rect();
    let document_id = harness.state().workspace.active_document.unwrap();
    harness.state_mut().pending_saves.insert(
        document_id,
        JobToken {
            document_id,
            revision: 0,
            job_id: 2,
        },
    );
    harness.step();
    let count_during_save = harness
        .query_all(By::new().label_contains("4 cells selected"))
        .next()
        .unwrap()
        .rect();
    let cancel_rect = harness
        .query_all(
            By::new()
                .role(egui::Role::Button)
                .label_contains("Cancel save"),
        )
        .next()
        .expect("the save action shares the fixed status lane")
        .rect();
    assert_eq!(count_during_save, count_rect);
    assert!(count_during_save.left() > cancel_rect.right());
    harness.state_mut().pending_saves.remove(&document_id);
    harness.step();

    scenario::click_cell(&mut harness, "B2");
    let active_view = harness
        .state()
        .workspace
        .active_document()
        .and_then(|document| {
            document
                .active_table()
                .and_then(|table| document.view(table.id))
        })
        .expect("the active table view remains available");
    assert!(active_view.additional_selections.is_empty());
    assert_eq!(
        active_view.selection,
        CellRange::single(CellAddress { row: 1, column: 1 })
    );
    let lingering_counts = harness
        .query_all(By::new().label_contains("cells selected"))
        .filter_map(|node| node.accesskit_node().label())
        .collect::<Vec<_>>();
    assert!(
        lingering_counts.is_empty(),
        "a single selected cell does not need a count: {lingering_counts:?}"
    );
}

/// Selects every cell from the corner above the row numbers.
#[test]
fn header_corner_click_selects_every_cell() {
    let directory = scenario::ScenarioDirectory::new();
    let mut harness = scenario::scenario_harness(directory.0.join("settings.json"));
    scenario::click_cell(&mut harness, "B2");
    let label = text(harness.state().language, "grid-select-all-corner");
    let corner = harness
        .query_all(By::new().role(egui::Role::Button).label(label.as_str()))
        .next()
        .expect("the header corner is a button")
        .rect()
        .center();
    scenario::click_point(&mut harness, corner);
    harness.step();
    let view = harness
        .state()
        .workspace
        .active_document()
        .and_then(|document| {
            document
                .active_table()
                .and_then(|table| document.view(table.id))
        })
        .expect("the active table view remains available");
    assert_eq!(
        view.selection.normalized(),
        (
            CellAddress::default(),
            CellAddress {
                row: MAX_ROWS - 1,
                column: MAX_COLUMNS - 1,
            }
        )
    );
}

/// Cancels a real worker-backed row insertion and confirms cell editing can resume.
#[test]
fn cancel_preparation_stays_enabled_and_restores_editing() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("many-rows.md");
    let mut contents = String::from("| value |\n| --- |\n");
    for row in 0..30_000 {
        contents.push_str(&format!("| row {row} |\n"));
    }
    let original_contents = contents.clone();
    fs::write(&source, contents).expect("write a table large enough to keep preparation pending");
    let mut harness = scenario::scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source.clone());
    let document_id = harness.state().workspace.active_document.unwrap();

    harness
        .state_mut()
        .request_structure_action(StructureAction::InsertRows);
    assert!(
        harness
            .state()
            .pending_structures
            .contains_key(&document_id)
    );
    harness.step();
    let cancel = harness
        .query_all(
            By::new()
                .role(egui::Role::Button)
                .label_contains("Cancel Preparation"),
        )
        .next()
        .expect("the pending structure exposes its cancellation action");
    assert!(
        !cancel.accesskit_node().is_disabled(),
        "pending structure work does not disable its own cancel action"
    );
    assert_eq!(
        harness.state().status,
        None,
        "background preparation is not announced in the status line"
    );
    let cancel_point = cancel.rect().center();
    scenario::send_batch(
        &mut harness,
        [
            egui::Event::PointerMoved(cancel_point),
            egui::Event::PointerButton {
                pos: cancel_point,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos: cancel_point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    // The cancel request is applied by the next frames, so wait instead of sampling once.
    for _ in 0..200 {
        if !harness
            .state()
            .pending_structures
            .contains_key(&document_id)
        {
            break;
        }
        scenario::send_batch(&mut harness, []);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(
        !harness
            .state()
            .pending_structures
            .contains_key(&document_id),
        "the real row insertion request is cancelled"
    );
    assert_eq!(
        fs::read(&source).expect("the source remains readable"),
        original_contents.as_bytes(),
        "cancelling preparation does not change the original file"
    );

    scenario::click_cell(&mut harness, "A2");
    scenario::send_event(&mut harness, egui::Event::Text("resumed".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    scenario::wait_until(&mut harness, |app| {
        app.pending_structures.is_empty()
            && scenario::cell_text(app, document_id, CellAddress { row: 1, column: 0 }).as_deref()
                == Some("resumed")
    });
    let command = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(&mut harness, scenario::key_event(Key::S, command));
    scenario::wait_until(&mut harness, |app| {
        app.pending_saves.is_empty() && !app.workspace.documents[&document_id].is_dirty()
    });
    let reopened = crate::markdown_adapter::MarkdownDocument::parse(
        fs::read(source).expect("the later cell edit is saved"),
    )
    .expect("the source remains parseable");
    assert_eq!(reopened.tables[0].rows[1][0].value, "resumed");
}

/// Checks the sheet band keeps its fixed actions beside working table tabs.
#[test]
fn sheet_band_table_list_selects_and_add_button_creates_a_table() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("interaction-workflow.md");
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("samples")
            .join("interaction-workflow.md"),
        &source,
    )
    .expect("copy the production Markdown fixture");
    let mut harness = scenario::scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source);
    let document_id = harness.state().workspace.active_document.unwrap();
    // A Markdown file shows the minimap; without its file the document keeps the menu, as
    // untitled and imported documents do.
    harness.state_mut().file_backings.remove(&document_id);
    harness.step();
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .tables
            .len(),
        3
    );

    let first_tab = button_rect(&harness, "Input");
    let second_tab = button_rect(&harness, "Sales");
    let add_label = crate::i18n::context_text(&harness.ctx, "dialog-add-table");
    let list_label = crate::i18n::context_text(&harness.ctx, "dialog-table-list");
    let add_rect = button_rect(&harness, &add_label);
    let list_rect = button_rect(&harness, &list_label);
    assert_eq!(first_tab.height(), CHROME_TAB_HEIGHT);
    assert_eq!(second_tab.height(), CHROME_TAB_HEIGHT);
    assert_eq!(add_rect.height(), CHROME_TAB_HEIGHT);
    assert_eq!(list_rect.height(), CHROME_TAB_HEIGHT);
    assert!(second_tab.right() <= add_rect.left());
    assert!(add_rect.right() <= list_rect.left());

    scenario::click_point(&mut harness, list_rect.center());
    for _ in 0..4 {
        if harness
            .query_all(By::new().label_contains("Sales —"))
            .next()
            .is_some()
        {
            break;
        }
        scenario::send_batch(&mut harness, []);
    }
    let second_choice = harness
        .query_all(By::new().label_contains("Sales —"))
        .next()
        .unwrap_or_else(|| {
            let labels = harness
                .query_all(By::new())
                .filter_map(|node| node.accesskit_node().label())
                .collect::<Vec<_>>();
            panic!("Table List shows each table as a choice; labels: {labels:?}")
        })
        .rect()
        .center();
    scenario::click_point(&mut harness, second_choice);
    assert_eq!(
        harness.state().workspace.documents[&document_id].active_table,
        harness.state().workspace.documents[&document_id].tables[1].id,
        "Table List selects the named table"
    );

    let add_rect = button_rect(&harness, &add_label);
    scenario::click_point(&mut harness, add_rect.center());
    scenario::wait_until(&mut harness, |app| {
        !app.pending_structures.contains_key(&document_id)
    });
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .tables
            .len(),
        4,
        "the adjacent plus control runs the existing Add Table action"
    );
    assert!(
        harness.state().grid_focused,
        "the completed Add Table action returns keyboard ownership to the grid"
    );
    let focused = harness
        .ctx
        .memory(|memory| memory.focused())
        .expect("the new table has a focused grid cell");
    let focused_cell = harness
        .query_all(By::new().role(egui::Role::GridCell))
        .find(|node| node.accesskit_node().is_focused())
        .expect("the grid cell owns focus after the add acknowledgement");
    assert_eq!(
        u128::from(focused_cell.accesskit_node().id()) >> 64,
        u128::from(focused.accesskit_id().0),
        "the focused widget is the selected grid cell"
    );

    scenario::send_event(&mut harness, egui::Event::Text("ready".to_owned()));
    scenario::send_event(
        &mut harness,
        scenario::key_event(Key::Enter, egui::Modifiers::NONE),
    );
    scenario::wait_until(&mut harness, |app| {
        app.workspace.documents[&document_id].tables[3]
            .cell(CellAddress { row: 0, column: 0 })
            .is_some_and(|cell| cell.input.source_text() == "ready")
    });
    let document = &harness.state().workspace.documents[&document_id];
    assert!(document.tables[..3].iter().all(|table| {
        table
            .cell(CellAddress { row: 0, column: 0 })
            .is_none_or(|cell| cell.input.source_text() != "ready")
    }));
}

/// Finds one button by its accessible name for sheet-band geometry checks.
fn button_rect(harness: &Harness<'_, TexcelApp>, label: &str) -> egui::Rect {
    harness
        .query_all(By::new().role(egui::Role::Button).label(label))
        .next()
        .unwrap_or_else(|| panic!("button {label:?} is visible"))
        .rect()
}

/// Keeps the bottom table reachable when the list is taller than the viewport.
#[test]
fn sheet_list_scroll_reaches_the_last_table() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("many-tables.md");
    let mut contents = String::from("# Many tables\n\n");
    for index in 1..=40 {
        contents.push_str(&format!(
            "## Sheet {index}\n\n| Key | Value |\n| --- | --- |\n| k{index} | v{index} |\n\n"
        ));
    }
    fs::write(&source, contents).expect("write a long production-loadable table list");
    let mut harness = scenario::scenario_harness(directory.0.join("settings.json"));
    scenario::queue_open_and_wait(&mut harness, source);
    let document = harness.state().workspace.active_document().unwrap();
    assert_eq!(document.tables.len(), 40);
    let document_id = document.id;
    let last_table_id = document.tables[39].id;
    // A Markdown file shows the minimap; without its file the document keeps the menu.
    harness.state_mut().file_backings.remove(&document_id);
    harness.step();

    let list_label = crate::i18n::context_text(&harness.ctx, "dialog-table-list");
    let list_rect = button_rect(&harness, &list_label);
    scenario::click_point(&mut harness, list_rect.center());
    for _ in 0..4 {
        if harness
            .query_all(By::new().label_contains("Sheet 1 —"))
            .next()
            .is_some()
        {
            break;
        }
        scenario::send_batch(&mut harness, []);
    }
    let first_item = harness
        .query_all(By::new().label_contains("Sheet 1 —"))
        .next()
        .expect("the list opens at its first table")
        .rect()
        .center();
    for _ in 0..12 {
        scenario::send_batch(
            &mut harness,
            [
                egui::Event::PointerMoved(first_item),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: Vec2::new(0.0, -800.0),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    let last_item = harness
        .query_all(By::new().label_contains("Sheet 40 —"))
        .next()
        .unwrap_or_else(|| {
            let labels = harness
                .query_all(By::new())
                .filter_map(|node| node.accesskit_node().label())
                .collect::<Vec<_>>();
            panic!("the scrollable table list exposes its last item: {labels:?}")
        })
        .rect()
        .center();
    scenario::click_point(&mut harness, last_item);
    assert_eq!(
        harness.state().workspace.documents[&document_id].active_table,
        last_table_id,
        "the last table can be selected after scrolling the popup"
    );
}

/// Finds the stable status label rectangle used by the layout comparison.
fn status_label_rect(harness: &Harness<'_, TexcelApp>) -> egui::Rect {
    harness
        .query_all(By::new().label_contains("operating system settings location"))
        .next()
        .expect("the status notice is visible")
        .rect()
}

/// Opens the zoom dialog from the status band and applies a preset to this window only.
#[test]
fn zoom_button_applies_a_preset_from_the_dialog() {
    let directory = scenario::ScenarioDirectory::new();
    let mut harness = scenario::scenario_harness(directory.0.join("settings.json"));
    harness.state_mut().language = Language::English;
    crate::i18n::set_context_language(&harness.ctx, Language::English);
    harness.step();
    let zoom_button = harness
        .query_all(By::new().label_contains("Sheet zoom 100%"))
        .next()
        .expect("the status band shows the sheet zoom")
        .rect()
        .center();
    scenario::click_point(&mut harness, zoom_button);
    harness.step();
    assert!(harness.state().sheet_zoom_dialog.is_some());
    let preset = harness
        .query_all(By::new().label("50%"))
        .next()
        .expect("the dialog offers 50%")
        .rect()
        .center();
    scenario::click_point(&mut harness, preset);
    let apply = harness
        .query_all(By::new().role(egui::Role::Button).label("Apply"))
        .next()
        .expect("the dialog has Apply")
        .rect()
        .center();
    scenario::click_point(&mut harness, apply);
    harness.step();
    assert!(harness.state().sheet_zoom_dialog.is_none());
    assert_eq!(harness.state().sheet_zoom.0, 0.5);
    assert_eq!(
        harness.state().snapshot_settings().app.ui_font_points,
        crate::settings::default_ui_font_points(),
        "the sheet zoom is not an app setting"
    );
    assert!(
        harness
            .query_all(By::new().label_contains("Sheet zoom 50%"))
            .next()
            .is_some()
    );
}

#[test]
/// Keeps a sheet tab the same height while the pointer is over it.
fn hovered_sheet_tab_keeps_its_height() {
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    harness
        .state_mut()
        .workspace
        .add_document("Tabs", &["First", "Second"]);
    harness.run_steps(2);
    let tab = button_rect(&harness, "Second");
    harness.event(egui::Event::PointerMoved(tab.center()));
    harness.run_steps(2);
    // The hover fill covers the whole height of the tab band, leaving no gap above or below.
    let band_height = CHROME_TAB_HEIGHT;
    let fill_height = harness
        .output()
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Rect(rect)
                if rect.rect.contains(tab.center())
                    && rect.fill.a() > 0
                    && rect.rect.width() <= tab.width() + 0.5 =>
            {
                Some(rect.rect.height())
            }
            _ => None,
        })
        .fold(0.0_f32, f32::max);
    assert!(
        (fill_height - band_height).abs() < 0.01 && (tab.height() - band_height).abs() < 0.01,
        "the hovered tab fill is {fill_height} tall in a {} tall tab; the band is {band_height}",
        tab.height()
    );
}
