//! Updates formulas and validation references during table structure changes.

mod formulas;
mod operations;
mod paste_formulas;

pub(crate) use formulas::rewrite_structure_formulas;
pub(crate) use paste_formulas::PasteFormulaMover;
