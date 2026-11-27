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
