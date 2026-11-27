use super::scenario_tests::{
    ScenarioDirectory, click_point, queue_open_and_wait, scenario_harness, send_batch,
};
use super::*;
use egui::{Event, Modifiers};
use egui_kittest::kittest::{By, NodeT, Queryable};

type AppHarness = egui_kittest::Harness<'static, TexcelApp>;

/// Returns the screen rectangle of one visible grid cell.
fn cell_rect(harness: &AppHarness, address: CellAddress) -> egui::Rect {
    harness
        .query_all(By::new().role(egui::Role::GridCell))
        .find(|node| {
            crate::ui::grid_accessibility::decode_grid_cell_node_id(
                node.accesskit_node().locate().0,
            )
            .is_some_and(|(_, actual, _)| actual == address)
        })
        .unwrap_or_else(|| panic!("cell {address:?} is visible"))
        .rect()
}

/// Points at the column letter above a cell.
fn column_header(harness: &AppHarness, column: usize) -> Pos2 {
    let rect = cell_rect(harness, CellAddress { row: 0, column });
    Pos2::new(rect.center().x, rect.top() - HEADER_HEIGHT / 2.0)
}

/// Points at the row number left of a cell.
fn row_header(harness: &AppHarness, row: usize) -> Pos2 {
    let rect = cell_rect(harness, CellAddress { row, column: 0 });
    Pos2::new(rect.left() - 12.0, rect.center().y)
}

/// Sends one press or release of a pointer button with modifiers.
fn button(pos: Pos2, button: egui::PointerButton, pressed: bool, modifiers: Modifiers) -> Event {
    Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers,
    }
}

/// Returns the normalized selected ranges of the active table.
fn selected(harness: &AppHarness) -> Vec<(CellAddress, CellAddress)> {
    let document = harness.state().workspace.active_document().unwrap();
    document
        .view(document.active_table)
        .unwrap()
        .selected_ranges()
        .map(|range| range.normalized())
        .collect()
}

/// Opens the interaction sample in a fresh harness.
fn open_sample() -> (ScenarioDirectory, AppHarness) {
    let directory = ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    queue_open_and_wait(
        &mut harness,
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("samples")
            .join("interaction-workflow.md"),
    );
    (directory, harness)
}

#[test]
/// Shift+click and a drag across headers select several whole columns or rows, and a
/// right-click inside them keeps the selection for the menu.
fn header_shift_click_and_drag_select_several_columns_and_rows() {
    let (_directory, mut harness) = open_sample();
    let last_row = MAX_ROWS - 1;
    let last_column = MAX_COLUMNS - 1;

    let first = column_header(&harness, 0);
    click_point(&mut harness, first);
    let target = column_header(&harness, 2);
    let mut shift = Modifiers::NONE;
    shift.shift = true;
    send_batch(
        &mut harness,
        [Event::ModifiersChanged(shift), Event::PointerMoved(target)],
    );
    send_batch(
        &mut harness,
        [button(target, egui::PointerButton::Primary, true, shift)],
    );
    send_batch(
        &mut harness,
        [
            button(target, egui::PointerButton::Primary, false, shift),
            Event::ModifiersChanged(Modifiers::NONE),
        ],
    );
    assert_eq!(
        selected(&harness),
        vec![(
            CellAddress { row: 0, column: 0 },
            CellAddress {
                row: last_row,
                column: 2
            }
        )]
    );

    let start = row_header(&harness, 1);
    let end = row_header(&harness, 3);
    send_batch(&mut harness, [Event::PointerMoved(start)]);
    send_batch(
        &mut harness,
        [button(
            start,
            egui::PointerButton::Primary,
            true,
            Modifiers::NONE,
        )],
    );
    for step in 1..=4 {
        let point = start + (end - start) * (step as f32 / 4.0);
        send_batch(&mut harness, [Event::PointerMoved(point)]);
    }
    send_batch(
        &mut harness,
        [button(
            end,
            egui::PointerButton::Primary,
            false,
            Modifiers::NONE,
        )],
    );
    let rows = vec![(
        CellAddress { row: 1, column: 0 },
        CellAddress {
            row: 3,
            column: last_column,
        },
    )];
    assert_eq!(selected(&harness), rows);

    let inside = row_header(&harness, 2);
    send_batch(&mut harness, [Event::PointerMoved(inside)]);
    send_batch(
        &mut harness,
        [button(
            inside,
            egui::PointerButton::Secondary,
            true,
            Modifiers::NONE,
        )],
    );
    send_batch(
        &mut harness,
        [button(
            inside,
            egui::PointerButton::Secondary,
            false,
            Modifiers::NONE,
        )],
    );
    assert_eq!(
        selected(&harness),
        rows,
        "right-click keeps the selected rows"
    );
}
