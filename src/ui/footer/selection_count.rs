use super::super::*;

use std::collections::BTreeMap;

/// Cells, rows, and columns covered by the active table's selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct SelectionCount {
    pub(super) cells: u64,
    pub(super) rows: u64,
    pub(super) columns: u64,
}

impl SelectionCount {
    // Count one set of rectangles without visiting each cell.
    pub(super) fn of(ranges: &[CellRange]) -> Self {
        let (rows, columns): (Vec<_>, Vec<_>) = ranges
            .iter()
            .filter_map(|range| {
                let (start, end) = range.normalized();
                (start.row < MAX_ROWS && start.column < MAX_COLUMNS).then(|| {
                    (
                        (start.row, end.row.min(MAX_ROWS - 1) + 1),
                        (start.column, end.column.min(MAX_COLUMNS - 1) + 1),
                    )
                })
            })
            .unzip();
        Self {
            cells: count_selected_cells(ranges),
            rows: union_length(rows),
            columns: union_length(columns),
        }
    }
}

/// Counts cells, rows, and columns in the active table's selection.
pub(super) fn active_selection_count(app: &TexcelApp) -> Option<SelectionCount> {
    let document = app.workspace.active_document()?;
    let table = document.active_table()?;
    // While the fill handle is dragged, count the range it would fill.
    if let Some(preview) = app
        .fill_handle_drag
        .filter(|drag| drag.document_id == document.id && drag.table_id == table.id)
        .and_then(|drag| drag.preview)
    {
        return Some(SelectionCount::of(&[preview]));
    }
    let view = document.view(table.id)?;
    let ranges = view.selected_ranges().collect::<Vec<_>>();
    Some(SelectionCount::of(&ranges))
}

/// Counts the union of rectangles by row bands and column intervals.
pub(super) fn count_selected_cells(ranges: &[CellRange]) -> u64 {
    let mut events = BTreeMap::<usize, Vec<(usize, bool)>>::new();
    let mut column_ranges = BTreeMap::new();
    for (id, range) in ranges.iter().copied().enumerate() {
        let (start, end) = range.normalized();
        if start.row >= MAX_ROWS || start.column >= MAX_COLUMNS {
            continue;
        }
        let end_row = end.row.min(MAX_ROWS - 1);
        let end_column = end.column.min(MAX_COLUMNS - 1);
        column_ranges.insert(id, (start.column, end_column + 1));
        events.entry(start.row).or_default().push((id, true));
        events.entry(end_row + 1).or_default().push((id, false));
    }
    if events.is_empty() {
        return 0;
    }

    let mut active = BTreeMap::<usize, (usize, usize)>::new();
    let mut total = 0_u64;
    let mut previous_row = 0;
    for (row, changes) in events {
        if row > previous_row && !active.is_empty() {
            let rows = (row - previous_row) as u64;
            total = total.saturating_add(rows * union_column_count(&active));
        }
        for (id, add) in changes {
            if add {
                if let Some(columns) = column_ranges.get(&id) {
                    active.insert(id, *columns);
                }
            } else {
                active.remove(&id);
            }
        }
        previous_row = row;
    }
    total
}

/// Counts the columns covered by the union of the active column intervals.
fn union_column_count(active: &BTreeMap<usize, (usize, usize)>) -> u64 {
    union_length(active.values().copied().collect())
}

/// Counts the positions covered by the union of half-open intervals.
fn union_length(mut intervals: Vec<(usize, usize)>) -> u64 {
    intervals.sort_unstable();
    let mut total = 0_u64;
    let mut current = None;
    for (start, end) in intervals {
        match current {
            Some((left, right)) if start <= right => current = Some((left, right.max(end))),
            Some((left, right)) => {
                total += (right - left) as u64;
                current = Some((start, end));
            }
            None => current = Some((start, end)),
        }
    }
    if let Some((start, end)) = current {
        total += (end - start) as u64;
    }
    total
}

/// Adds comma separators to a count for the fixed status line.
pub(super) fn grouped_number(value: u64) -> String {
    let digits = value.to_string();
    let first_group = match digits.len() % 3 {
        0 => 3,
        remainder => remainder,
    };
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    grouped.push_str(&digits[..first_group]);
    for group in digits.as_bytes()[first_group..].chunks(3) {
        grouped.push(',');
        grouped.push_str(std::str::from_utf8(group).expect("digits are ASCII"));
    }
    grouped
}
