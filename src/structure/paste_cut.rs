//! Worker-side changes that turn a paste of cut cells into a move.

use super::*;

/// Returns the extra cell changes of moving the cut `area` to the pasted cells: the
/// cut cells that were not pasted over become empty, and formulas elsewhere in the
/// table that refer into the area follow it, like Excel's cut and paste.
pub(super) fn cut_move_changes(
    table: &crate::document::Table,
    area: CellRange,
    pasted: &[(CellAddress, Option<crate::document::Cell>)],
) -> Result<Vec<(CellAddress, Option<crate::document::Cell>)>, Message> {
    let (start, end) = area.normalized();
    let area_cells = (end.row - start.row + 1).saturating_mul(end.column - start.column + 1);
    let Some(target_start) = pasted.iter().map(|(address, _)| *address).min() else {
        return Ok(Vec::new());
    };
    if pasted.len() != area_cells {
        return Err(Message::new("status-paste-cut-size-mismatch"));
    }
    let row_delta = signed_offset(start.row, target_start.row)?;
    let column_delta = signed_offset(start.column, target_start.column)?;
    let targets = pasted
        .iter()
        .map(|(address, _)| *address)
        .collect::<BTreeSet<_>>();
    let in_area = |address: CellAddress| {
        (start.row..=end.row).contains(&address.row)
            && (start.column..=end.column).contains(&address.column)
    };
    let mut changes = Vec::new();
    let mut mover = None;
    // Formulas that stay where they are follow references into the moved area.
    for (address, cell) in table.stored_cells() {
        if targets.contains(&address) || in_area(address) {
            continue;
        }
        let Some(formula) = crate::calc::formula_source(&cell.input, table.is_string_cell(address))
        else {
            continue;
        };
        let mover = match &mut mover {
            Some(mover) => mover,
            None => mover.insert(crate::calc::PasteFormulaMover::new()?),
        };
        if let Some(moved) = mover.follow_cut(&formula, address, area, row_delta, column_delta)? {
            let input = match cell.input {
                crate::document::CellInput::Formula(_) => {
                    crate::document::CellInput::Formula(moved)
                }
                _ => crate::document::CellInput::Text(moved),
            };
            changes.push((
                address,
                Some(crate::document::Cell {
                    input,
                    imported_result: None,
                    calculation: None,
                }),
            ));
        }
    }
    // The cut cells left uncovered by the paste become empty.
    for (address, _) in table.stored_cells_in_range(area) {
        if !targets.contains(&address) {
            changes.push((address, None));
        }
    }
    Ok(changes)
}

/// Returns `to - from` as an engine offset.
fn signed_offset(from: usize, to: usize) -> Result<i32, Message> {
    let from = i64::try_from(from).ok();
    let to = i64::try_from(to).ok();
    from.zip(to)
        .and_then(|(from, to)| i32::try_from(to - from).ok())
        .ok_or_else(|| Message::new("status-paste-cut-size-mismatch"))
}

#[cfg(test)]
#[path = "paste_cut_test.rs"]
mod tests;
