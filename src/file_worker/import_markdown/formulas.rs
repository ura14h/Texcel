//!  Converts XLSX and ODS formulas into Texcel formulas and reports the ones that cannot be kept.

use super::*;

use ironcalc_base::expressions::{
    parser::{Node, new_parser_english, stringify::to_english_string},
    types::CellReferenceRC,
};

/// Describes whether an unsupported formula kept its cached value or became empty.
pub(super) fn formula_fallback_entry(
    fallback: FormulaImport,
    has_cached_value: bool,
    sheet_index: usize,
    sheet_name: &str,
    address: crate::document::CellAddress,
) -> ConversionEntry {
    let (class, reason, message_key) = match (fallback, has_cached_value) {
        (FormulaImport::External, true) => (
            ConversionClass::Converted,
            "A formula referring to another sheet was replaced with its saved value.",
            "report-reason-xlsx-external-formula-value",
        ),
        (FormulaImport::External, false) => (
            ConversionClass::Omitted,
            "A formula referring to another sheet had no saved value and became empty.",
            "report-reason-xlsx-external-formula-empty",
        ),
        (FormulaImport::Invalid, true) => (
            ConversionClass::Converted,
            "An invalid local formula was replaced with its saved value.",
            "report-reason-xlsx-invalid-formula-value",
        ),
        (FormulaImport::Invalid, false) => (
            ConversionClass::Omitted,
            "An invalid local formula had no saved value and became empty.",
            "report-reason-xlsx-invalid-formula-empty",
        ),
        (FormulaImport::Local(_), _) => unreachable!("local formulas are not fallbacks"),
    };
    ConversionEntry::new(
        class,
        Some(sheet_index + 1),
        Some(sheet_name.to_owned()),
        Some(crate::xlsx_adapter::cell_reference(address)),
        reason,
    )
    .with_message(crate::i18n::Message::new(message_key))
}

/// Parses references with IronCalc so exclamation marks inside strings stay ordinary text.
pub(super) fn normalize_import_formula(
    formula: &str,
    sheet_names: &[String],
    sheet_index: usize,
    address: crate::document::CellAddress,
) -> FormulaImport {
    let Some(sheet_name) = sheet_names.get(sheet_index) else {
        return FormulaImport::Invalid;
    };
    let context = CellReferenceRC {
        sheet: sheet_name.clone(),
        column: address.column as i32 + 1,
        row: address.row as i32 + 1,
    };
    let mut parser = new_parser_english(sheet_names.to_vec(), Vec::new(), HashMap::new());
    let parsed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        parser.parse(formula, &context)
    }));
    let Ok(mut node) = parsed else {
        return FormulaImport::Invalid;
    };
    let mut changed = false;
    if let Err(classification) =
        rewrite_local_sheet_references(&mut node, sheet_index as u32, &mut changed)
    {
        return classification;
    }
    if changed {
        FormulaImport::Local(to_english_string(&node, &context))
    } else {
        FormulaImport::Local(formula.to_owned())
    }
}

/// Removes explicit same-sheet qualifiers and rejects real cross-sheet references.
pub(super) fn rewrite_local_sheet_references(
    node: &mut Node,
    current_sheet: u32,
    changed: &mut bool,
) -> Result<(), FormulaImport> {
    match node {
        Node::ReferenceKind {
            sheet_index,
            sheet_name,
            ..
        }
        | Node::RangeKind {
            sheet_index,
            sheet_name,
            ..
        } => {
            if *sheet_index != current_sheet {
                return Err(FormulaImport::External);
            }
            if sheet_name.take().is_some() {
                *changed = true;
            }
        }
        Node::WrongReferenceKind { sheet_name, .. } | Node::WrongRangeKind { sheet_name, .. } => {
            return Err(if sheet_name.is_some() {
                FormulaImport::External
            } else {
                FormulaImport::Invalid
            });
        }
        Node::OpRangeKind { left, right }
        | Node::OpConcatenateKind { left, right }
        | Node::OpSumKind { left, right, .. }
        | Node::OpProductKind { left, right, .. }
        | Node::OpPowerKind { left, right }
        | Node::CompareKind { left, right, .. } => {
            rewrite_local_sheet_references(left, current_sheet, changed)?;
            rewrite_local_sheet_references(right, current_sheet, changed)?;
        }
        Node::FunctionKind { args, .. } | Node::NamedFunctionKind { args, .. } => {
            for argument in args {
                rewrite_local_sheet_references(argument, current_sheet, changed)?;
            }
        }
        Node::UnaryKind { right, .. }
        | Node::ImplicitIntersection { child: right, .. }
        | Node::SpillRangeOperator { child: right } => {
            rewrite_local_sheet_references(right, current_sheet, changed)?;
        }
        Node::LambdaDefKind { body, .. } => {
            rewrite_local_sheet_references(body, current_sheet, changed)?;
        }
        Node::LambdaCallKind { lambda, args } => {
            rewrite_local_sheet_references(lambda, current_sheet, changed)?;
            for argument in args {
                rewrite_local_sheet_references(argument, current_sheet, changed)?;
            }
        }
        Node::ArrayKind(_) => (),
        Node::ParseErrorKind { .. } => return Err(FormulaImport::Invalid),
        Node::BooleanKind(_)
        | Node::NumberKind(_)
        | Node::StringKind(_)
        | Node::ErrorKind(_)
        | Node::DefinedNameKind(_)
        | Node::TableNameKind(_)
        | Node::NamedVariableKind { .. }
        | Node::EmptyArgKind => (),
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum FormulaImport {
    Local(String),
    External,
    Invalid,
}
