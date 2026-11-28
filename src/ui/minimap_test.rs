use super::scenario_tests::{ScenarioDirectory, queue_open_and_wait, scenario_harness};
use super::*;
use egui_kittest::kittest::{By, Queryable};

#[test]
/// Shows a Markdown file's minimap from the table list button and switches tables from it.
fn markdown_file_minimap_switches_tables() {
    let directory = ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    harness.state_mut().language = crate::i18n::Language::English;
    crate::i18n::set_context_language(&harness.ctx, crate::i18n::Language::English);
    queue_open_and_wait(
        &mut harness,
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("samples")
            .join("tables.md"),
    );
    let document_id = harness.state().workspace.active_document.unwrap();
    assert!(harness.state().minimap_source(document_id).is_some());
    let list = crate::i18n::context_text(&harness.ctx, "dialog-table-list");
    harness
        .query_all(By::new().role(egui::Role::Button).label(&list))
        .next()
        .expect("the table list button is visible")
        .click();
    harness.step();
    assert!(harness.state().minimap_open);

    // Wait for the background parse, then pick the second table from the minimap.
    let second = harness.state().workspace.documents[&document_id].tables[1].clone();
    for _ in 0..200 {
        harness.step();
        if harness
            .query_all(By::new().role(egui::Role::Button).label(&second.name))
            .next()
            .is_some()
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let _ = harness.render().map(|image| {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("kittest-renders");
        let _ = std::fs::create_dir_all(&directory);
        image.save(directory.join("minimap.png"))
    });
    harness
        .query_all(By::new().role(egui::Role::Button).label(&second.name))
        .next()
        .expect("the minimap names the second table")
        .click();
    harness.run_steps(2);
    assert_eq!(
        harness.state().workspace.documents[&document_id].active_table,
        second.id
    );

    // An unsaved cell edit reaches the minimap after its background parse.
    let first_table = harness.state().workspace.documents[&document_id].tables[0].id;
    harness
        .state_mut()
        .workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .set_cell(
            first_table,
            CellAddress { row: 1, column: 0 },
            Some(Cell::text("minimap-edit")),
        )
        .expect("the cell accepts text");
    let mut seen = false;
    for _ in 0..200 {
        harness.step();
        seen = harness
            .query_all(By::new().label_contains("minimap-edit"))
            .next()
            .is_some();
        if seen {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(seen, "the minimap shows the unsaved edit");
}

#[test]
/// Keeps the minimap beside the sheet, with its text against its left edge, while inserted
/// columns widen a table.
fn minimap_stays_left_aligned_as_columns_are_inserted() {
    let directory = ScenarioDirectory::new();
    let path = directory.0.join("tables.md");
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("samples")
            .join("tables.md"),
        &path,
    )
    .expect("copy the sample");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    queue_open_and_wait(&mut harness, path);
    harness.state_mut().minimap_open = true;
    let heading_left = |harness: &mut egui_kittest::Harness<'_, TexcelApp>| {
        for _ in 0..100 {
            harness.step();
            std::thread::sleep(std::time::Duration::from_millis(3));
        }
        harness
            .query_all(By::new().label_contains("この文章はセル編集で変化しない"))
            .next()
            .expect("the minimap shows the paragraph")
            .rect()
            .left()
    };
    let before = heading_left(&mut harness);
    super::scenario_tests::click_cell(&mut harness, "B2");
    for _ in 0..12 {
        harness
            .state_mut()
            .request_structure_action(StructureAction::InsertColumns);
        for _ in 0..20 {
            harness.step();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    let after = heading_left(&mut harness);
    let _ = harness.render().map(|image| {
        image.save(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("kittest-renders")
                .join("minimap-inserted-columns.png"),
        )
    });
    assert!(
        (after - before).abs() < 0.5,
        "the paragraph stays at the left edge: {before} -> {after}"
    );
    // The sheet ends where the minimap starts, so the minimap is not drawn under it.
    let sheet_right = harness
        .query_all(By::new().role(egui::Role::GridCell))
        .map(|cell| cell.rect().right())
        .fold(f32::MIN, f32::max);
    assert!(
        sheet_right <= after,
        "the sheet ({sheet_right}) stays left of the minimap ({after})"
    );
}
