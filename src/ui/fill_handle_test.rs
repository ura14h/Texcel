use super::scenario_tests::{
    ScenarioDirectory, cell_text, click_cell, key_event, scenario_harness, select_cell_by_name_box,
    send_batch, send_event, wait_until,
};
use super::*;
use egui_kittest::kittest::{By, Queryable};

/// Finds the on-screen rectangle of one grid cell by its address label.
fn cell_rect(harness: &egui_kittest::Harness<'_, TexcelApp>, label: &str) -> Rect {
    harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains(label))
        .next()
        .unwrap_or_else(|| panic!("{label} is visible"))
        .rect()
}

/// Types 1 and 2 into A1:A2, selects them, and presses the fill handle without releasing it.
/// Returns the press point and the center of A4.
fn press_fill_handle_of_a1_a2(harness: &mut egui_kittest::Harness<'_, TexcelApp>) -> (Pos2, Pos2) {
    click_cell(harness, "A1");
    for value in ["1", "2"] {
        send_batch(harness, [egui::Event::Text(value.to_owned())]);
        send_event(harness, key_event(Key::Enter, egui::Modifiers::NONE));
    }
    select_cell_by_name_box(harness, "A1:A2");
    let start = cell_rect(harness, "A2").right_bottom() - Vec2::splat(3.0);
    let end = cell_rect(harness, "A4").center();
    send_batch(harness, [egui::Event::PointerMoved(start)]);
    send_batch(
        harness,
        [egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    (start, end)
}

/// Moves the pressed pointer toward the end point over several frames.
fn drag_pointer(harness: &mut egui_kittest::Harness<'_, TexcelApp>, start: Pos2, end: Pos2) {
    for step in 1..=4 {
        let point = start + (end - start) * (step as f32 / 4.0);
        send_batch(harness, [egui::Event::PointerMoved(point)]);
    }
}

/// Collects the horizontal selection outline segments painted in the latest frame.
fn horizontal_selection_lines(harness: &egui_kittest::Harness<'_, TexcelApp>) -> Vec<[Pos2; 2]> {
    // Walks nested shape lists so outlines inside grouped shapes are found too.
    fn collect(shape: &egui::epaint::Shape, color: Color32, lines: &mut Vec<[Pos2; 2]>) {
        match shape {
            egui::epaint::Shape::LineSegment { points, stroke }
                if stroke.width >= 2.0
                    && stroke.color == color
                    && (points[0].y - points[1].y).abs() < 0.01 =>
            {
                lines.push(*points);
            }
            egui::epaint::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, color, lines);
                }
            }
            _ => {}
        }
    }
    let color = GridThemePalette::from_visuals(&harness.ctx.global_style().visuals).selection;
    let mut lines = Vec::new();
    for clipped in &harness.output().shapes {
        collect(&clipped.shape, color, &mut lines);
    }
    lines
}

/// Reports whether a horizontal outline runs along the bottom edge of the cell rectangle.
fn has_bottom_outline(lines: &[[Pos2; 2]], cell: Rect) -> bool {
    lines.iter().any(|[from, to]| {
        (from.y - (cell.bottom() - 0.75)).abs() < 0.5
            && from.x.min(to.x) <= cell.center().x
            && from.x.max(to.x) >= cell.center().x
    })
}

#[test]
/// Outlines the range the fill handle would fill while the drag is still in progress.
fn dragging_the_fill_handle_outlines_the_target_before_release() {
    let directory = ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    let (start, end) = press_fill_handle_of_a1_a2(&mut harness);
    let source_bottom = cell_rect(&harness, "A2");
    let target_bottom = cell_rect(&harness, "A4");
    drag_pointer(&mut harness, start, end);

    let lines = horizontal_selection_lines(&harness);
    assert!(has_bottom_outline(&lines, target_bottom));
    assert!(!has_bottom_outline(&lines, source_bottom));
    let document_id = harness.state().workspace.active_document.unwrap();
    assert_eq!(
        cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 3, column: 0 }
        ),
        None
    );
}

#[test]
/// Drags the fill handle of 1 and 2 down two rows and gets the next numbers.
fn dragging_the_fill_handle_continues_a_number_sequence() {
    let directory = ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    let (start, end) = press_fill_handle_of_a1_a2(&mut harness);
    drag_pointer(&mut harness, start, end);
    send_batch(
        &mut harness,
        [egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    let document_id = harness.state().workspace.active_document.unwrap();
    wait_until(&mut harness, |app| {
        cell_text(app, document_id, CellAddress { row: 3, column: 0 }).is_some()
    });
    let values = (0..4)
        .map(|row| cell_text(harness.state(), document_id, CellAddress { row, column: 0 }))
        .collect::<Vec<_>>();
    assert_eq!(
        values,
        ["1", "2", "3", "4"].map(|value| Some(value.to_owned()))
    );
}
