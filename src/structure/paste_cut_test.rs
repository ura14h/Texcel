use super::*;
use crate::document::{Cell, CellInput, Table};

/// Builds a zero-based cell address.
fn at(row: usize, column: usize) -> CellAddress {
    CellAddress { row, column }
}

/// Builds a table from A1-style text inputs.
fn table_with(cells: &[(CellAddress, &str)]) -> Table {
    let mut table = Table::new(1, "Table");
    for (address, input) in cells {
        table.set_loaded_cell(*address, Cell::text(*input));
    }
    table
}

/// Returns the input that a change writes, or `None` for a cleared cell.
fn change_at(
    changes: &[(CellAddress, Option<Cell>)],
    address: CellAddress,
) -> Option<Option<CellInput>> {
    changes
        .iter()
        .find(|(changed, _)| *changed == address)
        .map(|(_, cell)| cell.as_ref().map(|cell| cell.input.clone()))
}

#[test]
fn moving_a_cut_range_clears_it_and_other_formulas_follow() {
    let table = table_with(&[
        (at(0, 0), "1"),
        (at(1, 0), "2"),
        (at(0, 2), "=A1+A2"),
        (at(0, 3), "=SUM(A1:A2)+$A$1"),
        (at(0, 4), "=sum(A1:A3)"),
    ]);
    let area = CellRange {
        anchor: at(0, 0),
        head: at(1, 0),
    };
    let pasted = vec![
        (at(4, 1), Some(Cell::text("1"))),
        (at(5, 1), Some(Cell::text("2"))),
    ];
    let changes = cut_move_changes(&table, area, &pasted).expect("move");
    assert_eq!(change_at(&changes, at(0, 0)), Some(None));
    assert_eq!(change_at(&changes, at(1, 0)), Some(None));
    assert_eq!(
        change_at(&changes, at(0, 2)),
        Some(Some(CellInput::Text("=B5+B6".to_owned())))
    );
    assert_eq!(
        change_at(&changes, at(0, 3)),
        Some(Some(CellInput::Text("=SUM(B5:B6)+$B$5".to_owned())))
    );
    // A range only partly inside the cut keeps its cells and its spelling.
    assert_eq!(change_at(&changes, at(0, 4)), None);
}

#[test]
fn moving_onto_an_overlapping_range_keeps_the_pasted_cells() {
    let table = table_with(&[(at(0, 0), "a"), (at(1, 0), "b")]);
    let area = CellRange {
        anchor: at(0, 0),
        head: at(1, 0),
    };
    let pasted = vec![
        (at(1, 0), Some(Cell::text("a"))),
        (at(2, 0), Some(Cell::text("b"))),
    ];
    let changes = cut_move_changes(&table, area, &pasted).expect("move");
    assert_eq!(changes.len(), 1);
    assert_eq!(change_at(&changes, at(0, 0)), Some(None));
}

#[test]
fn a_cut_pasted_into_a_different_size_is_refused() {
    let table = table_with(&[(at(0, 0), "a")]);
    let area = CellRange {
        anchor: at(0, 0),
        head: at(1, 0),
    };
    let pasted = vec![(at(3, 0), Some(Cell::text("a")))];
    assert_eq!(
        cut_move_changes(&table, area, &pasted).map_err(|error| error.key().to_owned()),
        Err("status-paste-cut-size-mismatch".to_owned())
    );
}
