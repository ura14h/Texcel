use super::selection_count::{SelectionCount, count_selected_cells, grouped_number};
use crate::document::{CellAddress, CellRange, MAX_COLUMNS, MAX_ROWS};

/// Counts cells shared by overlapping ranges only once.
#[test]
fn overlapping_ranges_count_shared_cells_once() {
    let ranges = [rectangle(0, 0, 2, 2), rectangle(1, 1, 3, 3)];

    assert_eq!(count_selected_cells(&ranges), 9 + 9 - 4);
}

/// Counts a whole sheet from its dimensions instead of visiting its cells.
#[test]
fn whole_sheet_count_uses_dimensions_and_groups_large_numbers() {
    let ranges = [rectangle(0, 0, MAX_ROWS - 1, MAX_COLUMNS - 1)];

    let count = count_selected_cells(&ranges);
    assert_eq!(count, (MAX_ROWS as u64) * (MAX_COLUMNS as u64));
    assert_eq!(grouped_number(count), "17,179,869,184");
}

/// Reports the rows and columns that separate ranges cover together.
#[test]
fn selection_count_reports_covered_rows_and_columns() {
    let single = SelectionCount::of(&[rectangle(2, 1, 4, 2)]);
    assert_eq!(
        single,
        SelectionCount {
            cells: 6,
            rows: 3,
            columns: 2
        }
    );
    let separate = SelectionCount::of(&[rectangle(0, 0, 1, 0), rectangle(5, 3, 5, 4)]);
    assert_eq!((separate.cells, separate.rows, separate.columns), (4, 3, 3));
    let overlapping = SelectionCount::of(&[rectangle(0, 0, 2, 2), rectangle(1, 1, 3, 3)]);
    assert_eq!((overlapping.rows, overlapping.columns), (4, 4));
}

fn rectangle(top: usize, left: usize, bottom: usize, right: usize) -> CellRange {
    CellRange {
        anchor: CellAddress {
            row: top,
            column: left,
        },
        head: CellAddress {
            row: bottom,
            column: right,
        },
    }
}
