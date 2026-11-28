use super::scenario_tests::{ScenarioDirectory, queue_open_and_wait, scenario_harness};
use super::*;

/// Opens one Markdown table with text wrapping on and returns the settled height of row 2
/// in each display mode.
fn wrapped_row_heights(cell: &str) -> (f32, f32) {
    let directory = ScenarioDirectory::new();
    let path = directory.0.join("wrap.md");
    std::fs::write(&path, format!("| A |\n| --- |\n| {cell} |\n")).expect("write a table");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    queue_open_and_wait(&mut harness, path);
    let (document_id, table_id) = {
        let document = harness.state_mut().workspace.active_document_mut().unwrap();
        let table_id = document.active_table().unwrap().id;
        document.view_mut(table_id).unwrap().wrap_text = true;
        (document.id, table_id)
    };
    let settle = |harness: &mut egui_kittest::Harness<'_, TexcelApp>| {
        for _ in 0..40 {
            harness.step();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        harness.state().grid_row_layouts[&(document_id, table_id)].row_height(1)
    };
    let live = settle(&mut harness);
    harness
        .state_mut()
        .workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .display_mode = crate::document::DocumentDisplayMode::Source;
    let source = settle(&mut harness);
    (live, source)
}

#[test]
/// Sizes a wrapped row from what the current mode shows: a link with a long address takes
/// one line in live editing and several in source editing.
fn wrapped_row_height_follows_the_displayed_text() {
    let (plain_live, _) = wrapped_row_heights("example");
    let long_link = format!("[example](https://example.com/{})", "path/".repeat(20));
    let (live, source) = wrapped_row_heights(&long_link);
    assert_eq!(live, plain_live, "live editing shows only the link text");
    assert!(source > live, "source editing shows the whole address");
}
