use super::*;

#[cfg(test)]
#[path = "alignment_test.rs"]
mod tests;

impl MarkdownDocument {
    /// Changes selected separator cells while leaving the rest of the source bytes untouched.
    #[cfg(test)]
    pub fn set_column_alignment(
        &self,
        table_index: usize,
        columns: std::ops::RangeInclusive<usize>,
        alignment: MarkdownTableAlignment,
    ) -> Result<Self, MarkdownError> {
        self.set_column_alignment_ranges(table_index, &[columns], alignment)
    }

    /// Changes the separator cells of several column ranges in one edit.
    pub fn set_column_alignment_ranges(
        &self,
        table_index: usize,
        ranges: &[std::ops::RangeInclusive<usize>],
        alignment: MarkdownTableAlignment,
    ) -> Result<Self, MarkdownError> {
        let mut columns = std::collections::BTreeSet::new();
        for range in ranges {
            if range.start() > range.end() || *range.end() >= MAX_MARKDOWN_COLUMNS {
                return Err(MarkdownError::ResourceLimit(
                    crate::i18n::ResourceLimit::ColumnCount,
                ));
            }
            columns.extend(range.clone());
        }
        self.set_column_alignments(
            table_index,
            columns
                .into_iter()
                .map(|column| (column, alignment))
                .collect(),
        )
    }

    /// Stores alignment for source columns and keeps virtual columns only in memory.
    fn set_column_alignments(
        &self,
        table_index: usize,
        assignments: Vec<(usize, MarkdownTableAlignment)>,
    ) -> Result<Self, MarkdownError> {
        let text = std::str::from_utf8(&self.bytes).map_err(|_| MarkdownError::InvalidUtf8)?;
        let table = self
            .tables
            .get(table_index)
            .ok_or(MarkdownError::OutputMismatch)?;
        let syntax = row_syntax(text, table.separator_range.clone());
        let mut replacements = Vec::new();
        for (column, alignment) in &assignments {
            let Some(source_range) = syntax.cell_ranges.get(*column) else {
                continue;
            };
            let range = trim_cell_range(text, source_range.clone());
            let existing = text
                .get(range.clone())
                .ok_or(MarkdownError::OutputMismatch)?;
            let hyphens = existing
                .chars()
                .filter(|character| *character == '-')
                .count()
                .max(3);
            let marker = match alignment {
                MarkdownTableAlignment::Left => format!(":{}", "-".repeat(hyphens)),
                MarkdownTableAlignment::Center => {
                    format!(":{}:", "-".repeat(hyphens))
                }
                MarkdownTableAlignment::Right => format!("{}:", "-".repeat(hyphens)),
            };
            replacements.push((range, marker.into_bytes()));
        }
        replacements.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
        let has_replacements = !replacements.is_empty();
        let mut bytes = self.bytes.clone();
        for (range, replacement) in replacements {
            bytes.splice(range, replacement);
        }
        let mut updated = if !has_replacements {
            self.clone()
        } else {
            let mut updated = Self::parse(bytes)?;
            updated.source_extents.clone_from(&self.source_extents);
            copy_pending_alignment_metadata(self, &mut updated)?;
            updated
        };
        if updated.tables.len() != self.tables.len()
            || updated.tables.get(table_index).is_none_or(|after| {
                after.rows.len() != table.rows.len()
                    || after
                        .rows
                        .iter()
                        .zip(&table.rows)
                        .any(|(after, before)| after.len() != before.len())
            })
        {
            return Err(MarkdownError::OutputMismatch);
        }
        let table = updated
            .tables
            .get_mut(table_index)
            .ok_or(MarkdownError::OutputMismatch)?;
        if let Some((last_column, _)) = assignments.last() {
            let new_len = table.alignments.len().max(last_column.saturating_add(1));
            table.alignments.resize(new_len, None);
        }
        for (column, alignment) in assignments {
            table.alignments[column] = Some(alignment);
        }
        Ok(updated)
    }

    /// Remaps temporary alignments while keeping every untouched table's positions.
    pub(crate) fn carry_structure_alignments(
        &self,
        table_index: usize,
        updated: &mut Self,
        operation: &crate::document::StructureOperation,
        column_sources: &[Option<usize>],
    ) -> Result<(), MarkdownError> {
        if self.tables.len() != updated.tables.len() {
            return Err(MarkdownError::OutputMismatch);
        }
        for index in 0..self.tables.len() {
            let target_width = updated
                .tables
                .get(index)
                .ok_or(MarkdownError::OutputMismatch)?
                .parser_width;
            updated.tables[index].alignments.truncate(target_width);
            let mapping = if index == table_index {
                column_sources_with_pending_alignments(self, index, operation, column_sources)
            } else {
                let width = self.tables[index]
                    .alignments
                    .len()
                    .max(target_width)
                    .max(self.tables[index].parser_width);
                (0..width).map(Some).collect()
            };
            carry_pending_alignments(self, index, updated, index, &mapping)?;
        }
        Ok(())
    }
}

/// Keeps unmaterialized alignment values while reparsing another table's source bytes.
fn copy_pending_alignment_metadata(
    source: &MarkdownDocument,
    updated: &mut MarkdownDocument,
) -> Result<(), MarkdownError> {
    if source.tables.len() != updated.tables.len() {
        return Err(MarkdownError::OutputMismatch);
    }
    for (source_table, target_table) in source.tables.iter().zip(&mut updated.tables) {
        for (column, alignment) in source_table
            .alignments
            .iter()
            .copied()
            .enumerate()
            .skip(source_table.parser_width)
        {
            if let Some(alignment) = alignment {
                target_table
                    .alignments
                    .resize(column.saturating_add(1), None);
                target_table.alignments[column] = Some(alignment);
            }
        }
    }
    Ok(())
}

/// Carries temporary alignments through edits that map columns to new positions.
pub(super) fn carry_pending_alignments(
    source_document: &MarkdownDocument,
    source_table_index: usize,
    updated: &mut MarkdownDocument,
    table_index: usize,
    column_sources: &[Option<usize>],
) -> Result<(), MarkdownError> {
    let source = source_document
        .tables
        .get(source_table_index)
        .ok_or(MarkdownError::OutputMismatch)?;
    let source_floor = source_document
        .source_extents
        .get(source_table_index)
        .map_or(source.parser_width, |(_, columns)| *columns);
    let target_width = updated
        .tables
        .get(table_index)
        .ok_or(MarkdownError::OutputMismatch)?
        .parser_width;
    let assignments = column_sources
        .iter()
        .enumerate()
        .filter_map(|(target, source_column)| {
            let source_column = (*source_column)?;
            (source_column >= source.parser_width
                || target >= target_width && source_column >= source_floor)
                .then(|| source.alignments.get(source_column).copied().flatten())
                .flatten()
                .map(|alignment| (target, alignment))
        })
        .collect::<Vec<_>>();
    if !assignments.is_empty() {
        *updated = updated.set_column_alignments(table_index, assignments)?;
    }
    Ok(())
}

/// Extends a column map so virtual alignment columns follow sheet structure edits.
pub(super) fn column_sources_with_pending_alignments(
    source_document: &MarkdownDocument,
    table_index: usize,
    operation: &crate::document::StructureOperation,
    column_sources: &[Option<usize>],
) -> Vec<Option<usize>> {
    let Some(source) = source_document.tables.get(table_index) else {
        return column_sources.to_vec();
    };
    let mut extended = column_sources.to_vec();
    let source_width = match operation {
        crate::document::StructureOperation::Insert {
            axis: crate::document::StructureAxis::Columns,
            count,
            ..
        }
        | crate::document::StructureOperation::Duplicate {
            axis: crate::document::StructureAxis::Columns,
            count,
            ..
        } => column_sources.len().saturating_sub(*count),
        crate::document::StructureOperation::Delete {
            axis: crate::document::StructureAxis::Columns,
            count,
            ..
        } => column_sources.len().saturating_add(*count),
        _ => column_sources.len(),
    };
    for source_column in source_width..source.alignments.len() {
        let target_column = match operation {
            crate::document::StructureOperation::Insert {
                axis: crate::document::StructureAxis::Columns,
                index,
                count,
                ..
            } if source_column >= *index => source_column.saturating_add(*count),
            crate::document::StructureOperation::Delete {
                axis: crate::document::StructureAxis::Columns,
                index,
                count,
                ..
            } => {
                let after_deleted = index.saturating_add(*count);
                if source_column < after_deleted {
                    continue;
                }
                source_column.saturating_sub(*count)
            }
            crate::document::StructureOperation::Duplicate {
                axis: crate::document::StructureAxis::Columns,
                index,
                count,
                ..
            } if source_column >= index.saturating_add(*count) => {
                source_column.saturating_add(*count)
            }
            _ => source_column,
        };
        extended.resize(target_column.saturating_add(1), None);
        extended[target_column] = Some(source_column);
    }
    extended
}
