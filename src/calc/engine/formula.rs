//!  Reads formulas, resolves references, and converts IronCalc values.

use ironcalc_base::Model;
use ironcalc_base::cell::CellValue as EngineValue;
use ironcalc_base::expressions::parser::Node;
use ironcalc_base::language::{Language, get_language};
use ironcalc_base::types::CellType;

use crate::calc::engine::FormulaReference;
use crate::calc::snapshot::CalculationSnapshot;
use crate::document::{CalcStatus, CalcValue, CellAddress, CellInput, TableId, literal_text_value};

/// Keeps a third-party calculation failure as detail under a translated cause.
pub(crate) fn engine_error_message(error: impl std::fmt::Display) -> crate::i18n::Message {
    crate::i18n::Message::new("status-calculation-engine-failed").with_text("detail", error)
}

/// Returns the formula source used by the calculation model.
pub(crate) fn formula_source(input: &CellInput, string_cell: bool) -> Option<String> {
    match input {
        CellInput::Formula(source) if source.starts_with('=') => Some(source.clone()),
        CellInput::Text(source) if !string_cell && source.starts_with('=') => Some(source.clone()),
        _ => None,
    }
}

/// Updates one calculation-model cell from document input.
pub(crate) fn update_model_cell(
    model: &mut Model<'static>,
    sheet_index: u32,
    address: CellAddress,
    input: &CellInput,
    string_cell: bool,
    allow_sheet_reference: bool,
) -> Result<bool, crate::i18n::Message> {
    let row = one_based_engine_index(address.row)?;
    let column = one_based_engine_index(address.column)?;
    if let Some(source) = formula_source(input, string_cell) {
        return match model.update_cell_with_formula(sheet_index, row, column, source.clone()) {
            Ok(()) => {
                let supported =
                    formula_is_supported(model, sheet_index, row, column, allow_sheet_reference);
                Ok(supported)
            }
            Err(_) => {
                // A failed replacement must not leave a previous formula's value in the model.
                model
                    .set_user_input(sheet_index, row, column, String::new())
                    .map_err(engine_error_message)?;
                Ok(false)
            }
        };
    }
    match input {
        CellInput::Empty => model
            .set_user_input(sheet_index, row, column, String::new())
            .map_err(engine_error_message)?,
        CellInput::Text(value) if !string_cell => {
            let value = literal_text_value(value);
            if let Some(number) = crate::numeric_text::parse_ascii_decimal(value) {
                model
                    .update_cell_with_number(sheet_index, row, column, number)
                    .map_err(engine_error_message)?;
            } else {
                model
                    .update_cell_with_text(sheet_index, row, column, value)
                    .map_err(engine_error_message)?;
            }
        }
        CellInput::Text(value) => model
            .update_cell_with_text(sheet_index, row, column, literal_text_value(value))
            .map_err(engine_error_message)?,
        CellInput::Number(value) => model
            .update_cell_with_number(sheet_index, row, column, *value)
            .map_err(engine_error_message)?,
        CellInput::Boolean(value) => model
            .update_cell_with_bool(sheet_index, row, column, *value)
            .map_err(engine_error_message)?,
        CellInput::Error(value) => model
            .set_user_input(sheet_index, row, column, value.clone())
            .map_err(engine_error_message)?,
        CellInput::Formula(_) => model
            .set_user_input(sheet_index, row, column, String::new())
            .map_err(engine_error_message)?,
    }
    Ok(true)
}

/// Converts a zero-based row or column to an engine index.
pub(crate) fn one_based_engine_index(index: usize) -> Result<i32, crate::i18n::Message> {
    index
        .checked_add(1)
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| {
            crate::i18n::Message::new("status-calculation-index-too-large")
                .with_number("index", index as i64)
        })
}

/// Checks whether a formula uses supported syntax and functions.
pub(crate) fn formula_is_supported(
    model: &Model<'static>,
    sheet_index: u32,
    row: i32,
    column: i32,
    allow_sheet_reference: bool,
) -> bool {
    let Some(node) = parsed_formula_node(model, sheet_index, row, column) else {
        return false;
    };
    let language = match get_language("en") {
        Ok(language) => language,
        Err(_) => return false,
    };
    node_is_supported(node, language, allow_sheet_reference)
}

/// Returns the parsed expression node for a formula.
pub(crate) fn parsed_formula_node<'a>(
    model: &'a Model<'static>,
    sheet_index: u32,
    row: i32,
    column: i32,
) -> Option<&'a Node> {
    let cell = model
        .workbook
        .worksheet(sheet_index)
        .ok()
        .and_then(|sheet| sheet.cell(row, column))?;
    let formula_index = cell.get_formula()?;
    model
        .parsed_formulas
        .get(sheet_index as usize)
        .and_then(|formulas| formulas.get(formula_index as usize))
        .map(|(node, _)| node.as_ref())
}

/// Collects formula references from the same public AST used by the support
/// allowlist, resolving relative references against the formula's own cell.
pub(crate) fn formula_references(
    model: &Model<'static>,
    model_index: usize,
    sheet_index: u32,
    address: CellAddress,
) -> Vec<FormulaReference> {
    let row = address
        .row
        .checked_add(1)
        .and_then(|value| i32::try_from(value).ok());
    let column = address
        .column
        .checked_add(1)
        .and_then(|value| i32::try_from(value).ok());
    let (Some(row), Some(column)) = (row, column) else {
        return Vec::new();
    };
    let Some(node) = parsed_formula_node(model, sheet_index, row, column) else {
        return Vec::new();
    };
    let mut references = Vec::new();
    collect_formula_references(node, model_index, address, &mut references);
    references
}

/// Resolves a formula reference to a document cell address.
pub(crate) fn resolve_formula_coordinate(
    index: i32,
    absolute: bool,
    origin: usize,
) -> Option<usize> {
    let one_based = if absolute {
        i64::from(index)
    } else {
        i64::try_from(origin)
            .ok()?
            .checked_add(1)?
            .checked_add(i64::from(index))?
    };
    usize::try_from(one_based.checked_sub(1)?).ok()
}

/// Collects supported cell references from a formula tree.
pub(crate) fn collect_formula_references(
    node: &Node,
    model_index: usize,
    origin: CellAddress,
    output: &mut Vec<FormulaReference>,
) {
    match node {
        Node::ReferenceKind {
            sheet_index,
            absolute_row,
            absolute_column,
            row,
            column,
            ..
        } => {
            if let (Some(row), Some(column)) = (
                resolve_formula_coordinate(*row, *absolute_row, origin.row),
                resolve_formula_coordinate(*column, *absolute_column, origin.column),
            ) {
                let address = CellAddress { row, column };
                output.push(FormulaReference {
                    model_index,
                    sheet_index: *sheet_index,
                    first: address,
                    last: address,
                });
            }
        }
        Node::RangeKind {
            sheet_index,
            absolute_row1,
            absolute_column1,
            row1,
            column1,
            absolute_row2,
            absolute_column2,
            row2,
            column2,
            ..
        } => {
            if let (Some(row1), Some(column1), Some(row2), Some(column2)) = (
                resolve_formula_coordinate(*row1, *absolute_row1, origin.row),
                resolve_formula_coordinate(*column1, *absolute_column1, origin.column),
                resolve_formula_coordinate(*row2, *absolute_row2, origin.row),
                resolve_formula_coordinate(*column2, *absolute_column2, origin.column),
            ) {
                output.push(FormulaReference {
                    model_index,
                    sheet_index: *sheet_index,
                    first: CellAddress {
                        row: row1.min(row2),
                        column: column1.min(column2),
                    },
                    last: CellAddress {
                        row: row1.max(row2),
                        column: column1.max(column2),
                    },
                });
            }
        }
        Node::OpRangeKind { left, right }
        | Node::OpConcatenateKind { left, right }
        | Node::OpSumKind { left, right, .. }
        | Node::OpProductKind { left, right, .. }
        | Node::OpPowerKind { left, right }
        | Node::CompareKind { left, right, .. } => {
            collect_formula_references(left, model_index, origin, output);
            collect_formula_references(right, model_index, origin, output);
        }
        Node::FunctionKind { args, .. } | Node::NamedFunctionKind { args, .. } => {
            for argument in args {
                collect_formula_references(argument, model_index, origin, output);
            }
        }
        Node::UnaryKind { right, .. }
        | Node::ImplicitIntersection { child: right, .. }
        | Node::SpillRangeOperator { child: right } => {
            collect_formula_references(right, model_index, origin, output);
        }
        Node::LambdaDefKind { body, .. } => {
            collect_formula_references(body, model_index, origin, output);
        }
        Node::LambdaCallKind { lambda, args } => {
            collect_formula_references(lambda, model_index, origin, output);
            for argument in args {
                collect_formula_references(argument, model_index, origin, output);
            }
        }
        Node::BooleanKind(_)
        | Node::NumberKind(_)
        | Node::StringKind(_)
        | Node::WrongReferenceKind { .. }
        | Node::WrongRangeKind { .. }
        | Node::ArrayKind(_)
        | Node::DefinedNameKind(_)
        | Node::TableNameKind(_)
        | Node::NamedVariableKind { .. }
        | Node::ErrorKind(_)
        | Node::ParseErrorKind { .. }
        | Node::EmptyArgKind => (),
    }
}

// Walks the AST and exposes results only for supported operators, references, and functions.
pub(crate) fn node_is_supported(
    node: &Node,
    language: &Language,
    allow_sheet_reference: bool,
) -> bool {
    match node {
        Node::BooleanKind(_) | Node::NumberKind(_) | Node::StringKind(_) | Node::ErrorKind(_) => {
            true
        }
        Node::ReferenceKind { sheet_name, .. } | Node::RangeKind { sheet_name, .. } => {
            allow_sheet_reference || sheet_name.is_none()
        }
        Node::WrongReferenceKind { .. } | Node::WrongRangeKind { .. } => true,
        Node::OpRangeKind { left, right }
        | Node::OpConcatenateKind { left, right }
        | Node::OpSumKind { left, right, .. }
        | Node::OpProductKind { left, right, .. }
        | Node::OpPowerKind { left, right }
        | Node::CompareKind { left, right, .. } => {
            node_is_supported(left, language, allow_sheet_reference)
                && node_is_supported(right, language, allow_sheet_reference)
        }
        Node::FunctionKind { kind, args } => {
            let name = kind.to_localized_name(language).to_ascii_uppercase();
            is_supported_function(&name)
                && args
                    .iter()
                    .all(|argument| node_is_supported(argument, language, allow_sheet_reference))
        }
        Node::UnaryKind { right, .. }
        | Node::ImplicitIntersection { child: right, .. }
        | Node::SpillRangeOperator { child: right } => {
            node_is_supported(right, language, allow_sheet_reference)
                && !matches!(
                    node,
                    Node::ImplicitIntersection { .. } | Node::SpillRangeOperator { .. }
                )
        }
        Node::ParseErrorKind { .. }
        | Node::LambdaDefKind { .. }
        | Node::LambdaCallKind { .. }
        | Node::NamedFunctionKind { .. }
        | Node::ArrayKind(_)
        | Node::DefinedNameKind(_)
        | Node::TableNameKind(_)
        | Node::NamedVariableKind { .. }
        | Node::EmptyArgKind => false,
    }
}

/// Functions the calculation adapter evaluates; the Help appendix describes each one.
pub(crate) const SUPPORTED_FUNCTIONS: [&str; 28] = [
    "SUM",
    "AVERAGE",
    "MIN",
    "MAX",
    "COUNT",
    "COUNTA",
    "COUNTIF",
    "SUMIF",
    "IF",
    "IFERROR",
    "AND",
    "OR",
    "NOT",
    "ABS",
    "ROUND",
    "ROUNDUP",
    "ROUNDDOWN",
    "LEN",
    "LEFT",
    "RIGHT",
    "MID",
    "TRIM",
    "CONCATENATE",
    "VALUE",
    "DATE",
    "YEAR",
    "MONTH",
    "DAY",
];

/// Checks whether a function is supported by the calculation adapter.
pub(crate) fn is_supported_function(name: &str) -> bool {
    SUPPORTED_FUNCTIONS.contains(&name)
}

/// Reads a calculated value and maps it to the document result type.
pub(crate) fn read_result(
    model: &Model<'static>,
    sheet_index: u32,
    address: CellAddress,
) -> Result<(CalcStatus, Option<CalcValue>), crate::i18n::Message> {
    let row = one_based_engine_index(address.row)?;
    let column = one_based_engine_index(address.column)?;
    let cell_type = model
        .get_cell_type(sheet_index, row, column)
        .map_err(engine_error_message)?;
    let value = model
        .get_cell_value_by_index(sheet_index, row, column)
        .map_err(engine_error_message)?;
    let error = cell_type == CellType::ErrorValue;
    let value = match value {
        EngineValue::None => None,
        EngineValue::String(value) if error => Some(CalcValue::Error(value)),
        EngineValue::String(value) => Some(CalcValue::Text(value)),
        EngineValue::Number(value) => Some(CalcValue::Number(value)),
        EngineValue::Boolean(value) => Some(CalcValue::Boolean(value)),
    };
    Ok((
        if error {
            CalcStatus::Error
        } else {
            CalcStatus::Fresh
        },
        value,
    ))
}

/// Finds imported formula results the engine cannot verify.
pub(crate) fn unsupported_initial_results(
    snapshot: &CalculationSnapshot,
) -> Vec<(TableId, CellAddress, CalcStatus, Option<CalcValue>)> {
    snapshot
        .tables
        .iter()
        .flat_map(|table| {
            table.cells.iter().filter_map(move |(address, cell)| {
                formula_source(&cell.input, table.string_cells.contains(address)).map(|_| {
                    (
                        table.id,
                        *address,
                        CalcStatus::Unsupported,
                        cell.imported_result.clone(),
                    )
                })
            })
        })
        .collect()
}
