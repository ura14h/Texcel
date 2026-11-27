//!  Connects sparse document inputs, the calculation worker, and structure rewrites.

mod engine;
mod snapshot;
mod structure;
mod worker;

#[cfg(test)]
use crate::document::{CalcStatus, CalcValue, Cell, CellAddress, CellInput, Document, TableId};
#[cfg(test)]
use crate::file_worker::JobToken;
#[cfg(test)]
pub(crate) use crate::numeric_text::parse_ascii_decimal as ascii_number;
pub(crate) use engine::formula_source;
#[cfg(test)]
use ironcalc_base::Model;
pub(crate) use snapshot::calculation_cell_for_document;
pub(crate) use snapshot::decode_markdown_formula_source;
pub use snapshot::{CalculationChange, CalculationSnapshot, FormulaResultSnapshot};
#[cfg(test)]
pub(crate) use snapshot::{CalculationResult, FormulaResultRoot};
pub(crate) use snapshot::{MarkdownCalculationSource, project_markdown_changes};
#[cfg(test)]
use std::sync::mpsc::{self, Receiver, SyncSender};
#[cfg(test)]
use std::thread;
pub(crate) use structure::{PasteFormulaMover, rewrite_structure_formulas};
#[cfg(test)]
pub(crate) use worker::Message;
pub use worker::{CalculationSubmitError, CalculationWorker};

#[cfg(test)]
#[path = "calc_test.rs"]
pub(crate) mod tests;
