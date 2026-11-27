//! Moves formula references for copied and cut cells when they are pasted.

use ironcalc_base::Model;
use ironcalc_base::expressions::types::{Area, CellReferenceIndex};

use crate::calc::engine::formula::{
    engine_error_message, formula_is_supported, one_based_engine_index,
};
use crate::document::{CellAddress, CellRange};
use crate::i18n::Message;

/// Rewrites pasted formulas on one scratch engine sheet, like Excel's copy and cut.
pub(crate) struct PasteFormulaMover {
    model: Model<'static>,
}

impl PasteFormulaMover {
    /// Creates an empty engine sheet used only to parse and print formulas.
    pub(crate) fn new() -> Result<Self, Message> {
        let model =
            Model::new_empty("texcel-paste", "en", "UTC", "en").map_err(engine_error_message)?;
        Ok(Self { model })
    }

    /// Returns a formula copied from `source` to `target`; relative references move with it.
    pub(crate) fn copy(
        &mut self,
        formula: &str,
        source: CellAddress,
        target: CellAddress,
    ) -> Result<String, Message> {
        if source == target {
            return Ok(formula.to_owned());
        }
        let (row, column) = self.parse(formula, source)?;
        let (target_row, target_column) = engine_position(target)?;
        self.model
            .extend_to(0, row, column, target_row, target_column)
            .map_err(engine_error_message)
    }

    /// Returns a formula cut from `source` to `target`. Only references into the cut
    /// area move; other references keep their cells.
    pub(crate) fn cut(
        &mut self,
        formula: &str,
        source: CellAddress,
        target: CellAddress,
        area: CellRange,
    ) -> Result<String, Message> {
        if source == target {
            return Ok(formula.to_owned());
        }
        let (row, column) = self.parse(formula, source)?;
        let (target_row, target_column) = engine_position(target)?;
        self.model
            .move_cell_value_to_area(
                formula,
                &CellReferenceIndex {
                    sheet: 0,
                    row,
                    column,
                },
                &CellReferenceIndex {
                    sheet: 0,
                    row: target_row,
                    column: target_column,
                },
                &engine_area(area)?,
            )
            .map_err(engine_error_message)
    }

    /// Returns the new text of a formula that stays in `cell` when its references into
    /// the cut `area` follow the area by the given offset, or `None` when nothing changes.
    pub(crate) fn follow_cut(
        &mut self,
        formula: &str,
        cell: CellAddress,
        area: CellRange,
        row_delta: i32,
        column_delta: i32,
    ) -> Result<Option<String>, Message> {
        let (row, column) = self.parse(formula, cell)?;
        let cell = CellReferenceIndex {
            sheet: 0,
            row,
            column,
        };
        let area = engine_area(area)?;
        // Compare with an unmoved print so that spelling-only differences keep the source.
        let unchanged = self
            .model
            .displace_references_to_area(formula, &cell, &area, 0, 0)
            .map_err(engine_error_message)?;
        let moved = self
            .model
            .displace_references_to_area(formula, &cell, &area, row_delta, column_delta)
            .map_err(engine_error_message)?;
        Ok((moved != unchanged).then_some(moved))
    }

    /// Parses a formula at its cell and rejects formulas whose references Texcel cannot
    /// update safely, the same rule that row and column operations use.
    fn parse(&mut self, formula: &str, address: CellAddress) -> Result<(i32, i32), Message> {
        let (row, column) = engine_position(address)?;
        let cannot_move = || {
            Message::new("status-paste-formula-cannot-move")
                .with_text("cell", crate::xlsx_adapter::cell_reference(address))
        };
        self.model
            .update_cell_with_formula(0, row, column, formula.to_owned())
            .map_err(|_| cannot_move())?;
        if formula_is_supported(&self.model, 0, row, column, false) {
            Ok((row, column))
        } else {
            Err(cannot_move())
        }
    }
}

/// Converts a zero-based Texcel cell into one-based engine coordinates.
fn engine_position(address: CellAddress) -> Result<(i32, i32), Message> {
    Ok((
        one_based_engine_index(address.row)?,
        one_based_engine_index(address.column)?,
    ))
}

/// Converts a Texcel range into an engine area on the scratch sheet.
fn engine_area(range: CellRange) -> Result<Area, Message> {
    let (start, end) = range.normalized();
    let (row, column) = engine_position(start)?;
    let (end_row, end_column) = engine_position(end)?;
    Ok(Area {
        sheet: 0,
        row,
        column,
        width: end_column - column + 1,
        height: end_row - row + 1,
    })
}

#[cfg(test)]
#[path = "paste_formulas_test.rs"]
mod tests;
