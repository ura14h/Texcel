use super::scenario_tests::{ScenarioDirectory, click_cell, queue_open_and_wait, scenario_harness};
use super::*;
use egui_kittest::kittest::{By, Queryable};

/// Opens the sample tables, selects B3, and freezes rows or columns through it.
fn frozen_through_b3(action: LayoutAction) -> egui_kittest::Harness<'static, TexcelApp> {
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
    click_cell(&mut harness, "B3");
    let ctx = harness.ctx.clone();
    harness.state_mut().apply_layout_action(action, &ctx);
    harness.run_steps(3);
    harness
}

/// Returns the rectangle of one visible grid cell.
fn cell_rect(harness: &egui_kittest::Harness<'_, TexcelApp>, address: &str) -> Option<Rect> {
    harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains(address))
        .next()
        .map(|node| node.rect())
}

#[test]
/// Shows the scrolling columns right after freezing columns.
fn freezing_columns_keeps_the_scrolling_columns_and_frozen_cells_whole() {
    let harness = frozen_through_b3(LayoutAction::FreezeColumns);
    let b1 = cell_rect(&harness, "B1").expect("frozen B1 is visible");
    let c1 = cell_rect(&harness, "C1").expect("C1 is visible right after freezing");
    assert!(c1.left() >= b1.right() - 0.5);
}
