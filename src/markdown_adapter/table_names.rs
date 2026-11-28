//! Names Markdown tables after the section they sit in.
//!
//! The section a table belongs to is opened by the last heading before it. That heading is
//! both the nearest one and the deepest one still open at the table: any deeper heading
//! further up has already been closed by it. A table with no heading between it and the
//! previous table therefore shares that table's section, and tables sharing a section are
//! told apart by their order within it.

use super::*;
use std::collections::BTreeMap;

/// The name a table takes from its section, and the heading that goes with it on deletion.
#[derive(Debug, Eq, PartialEq)]
pub(super) struct TableName {
    /// The section heading, numbered when the section holds several tables; `None` when no
    /// titled heading comes before the table.
    pub(super) name: Option<String>,
    /// The heading to delete with the table: only one that names no other table and has
    /// nothing but blank lines between it and the table.
    pub(super) heading_range: Option<Range<usize>>,
}

/// Names each table, in document order, from the headings before it. `headings` and
/// `tables` are byte ranges in `text`, both in document order.
pub(super) fn name_tables(
    text: &str,
    headings: &[(Range<usize>, String)],
    tables: &[Range<usize>],
) -> Vec<TableName> {
    // Each table's section: the index of the last heading that ends before it.
    let sections = tables
        .iter()
        .map(|table| {
            headings
                .partition_point(|(range, _)| range.end <= table.start)
                .checked_sub(1)
        })
        .collect::<Vec<_>>();
    let mut section_sizes = BTreeMap::<usize, usize>::new();
    for section in sections.iter().flatten() {
        *section_sizes.entry(*section).or_default() += 1;
    }
    let mut positions = BTreeMap::<usize, usize>::new();
    sections
        .iter()
        .zip(tables)
        .map(|(section, table)| {
            let titled = section.and_then(|index| {
                let heading = headings.get(index)?;
                (!heading.1.is_empty()).then_some((index, heading))
            });
            let Some((index, (range, title))) = titled else {
                return TableName {
                    name: None,
                    heading_range: None,
                };
            };
            let size = section_sizes.get(&index).copied().unwrap_or(1);
            let position = positions.entry(index).or_default();
            *position += 1;
            let shared = size > 1;
            let adjacent = text
                .get(range.end..table.start)
                .is_some_and(|between| between.trim().is_empty());
            TableName {
                name: Some(if shared {
                    format!("{title} ({position})")
                } else {
                    title.clone()
                }),
                heading_range: (!shared && adjacent).then(|| range.clone()),
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "table_names_test.rs"]
mod tests;
