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

/// Returns the positions of the black frozen-edge lines: vertical ones by x, others by y.
fn frozen_edges(harness: &egui_kittest::Harness<'_, TexcelApp>) -> (Vec<f32>, Vec<f32>) {
    let mut vertical = Vec::new();
    let mut horizontal = Vec::new();
    for clipped in &harness.output().shapes {
        if let egui::Shape::LineSegment { points, stroke } = &clipped.shape
            && stroke.color == egui::Color32::BLACK
        {
            if points[0].x == points[1].x {
                vertical.push(points[0].x);
            } else if points[0].y == points[1].y {
                horizontal.push(points[0].y);
            }
        }
    }
    (vertical, horizontal)
}

#[test]
/// Shows the scrolling columns right after freezing columns, and draws the edge beside the
/// frozen cells rather than over them.
fn freezing_columns_keeps_the_scrolling_columns_and_frozen_cells_whole() {
    let harness = frozen_through_b3(LayoutAction::FreezeColumns);
    let b1 = cell_rect(&harness, "B1").expect("frozen B1 is visible");
    let c1 = cell_rect(&harness, "C1").expect("C1 is visible right after freezing");
    assert!(c1.left() >= b1.right() - 0.5);
    let (vertical, _) = frozen_edges(&harness);
    assert_eq!(vertical.len(), 1);
    assert!(vertical[0] > b1.right(), "the edge does not cover column B");
}

#[test]
/// Draws the edge of frozen rows below the frozen cells, so they keep their full height.
fn freezing_rows_keeps_frozen_cells_whole() {
    let harness = frozen_through_b3(LayoutAction::FreezeRows);
    let b3 = cell_rect(&harness, "B3").expect("frozen B3 is visible");
    let b4 = cell_rect(&harness, "B4").expect("B4 is visible");
    assert!(b4.top() >= b3.bottom() - 0.5);
    let (_, horizontal) = frozen_edges(&harness);
    assert_eq!(horizontal.len(), 1);
    assert!(horizontal[0] > b3.bottom(), "the edge does not cover row 3");
}
