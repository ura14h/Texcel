//! Completion notices for user commands that finish on the background worker.
use super::*;

#[cfg(test)]
#[path = "completion_notice_test.rs"]
mod tests;

impl TexcelApp {
    /// Remembers what to report when this user command's background job succeeds.
    pub(super) fn expect_completion(
        &mut self,
        document_id: DocumentId,
        token: JobToken,
        notice: Option<Message>,
    ) {
        match notice {
            Some(notice) => {
                self.completion_notices.insert(document_id, (token, notice));
            }
            None => {
                self.completion_notices.remove(&document_id);
            }
        }
    }

    /// Takes the notice for one finished job; a notice for another job is dropped.
    pub(super) fn take_completion(
        &mut self,
        document_id: DocumentId,
        token: JobToken,
    ) -> Option<Message> {
        self.completion_notices
            .remove(&document_id)
            .filter(|(expected, _)| *expected == token)
            .map(|(_, notice)| notice)
    }
}

/// Names a finished row, column, or fill command; text edits report nothing.
pub(super) fn structure_completion_notice(operation: &StructureOperation) -> Option<Message> {
    let pick = |axis: &StructureAxis, rows: &'static str, columns: &'static str| {
        if *axis == StructureAxis::Rows {
            rows
        } else {
            columns
        }
    };
    let (key, count) = match operation {
        StructureOperation::Insert { axis, count, .. } => (
            pick(
                axis,
                "status-done-rows-inserted",
                "status-done-columns-inserted",
            ),
            *count,
        ),
        StructureOperation::Delete { axis, count, .. } => (
            pick(
                axis,
                "status-done-rows-deleted",
                "status-done-columns-deleted",
            ),
            *count,
        ),
        StructureOperation::Duplicate { axis, count, .. } => (
            pick(
                axis,
                "status-done-rows-duplicated",
                "status-done-columns-duplicated",
            ),
            *count,
        ),
        StructureOperation::Move { axis, count, .. } => (
            pick(axis, "status-done-rows-moved", "status-done-columns-moved"),
            *count,
        ),
        StructureOperation::Fill { .. } => return Some(Message::new("status-done-filled")),
        StructureOperation::ReorderRows { .. }
        | StructureOperation::SetTextCell { .. }
        | StructureOperation::SetTextCells { .. } => return None,
    };
    Some(Message::new(key).with_number("count", count as i64))
}

/// Names a finished table command; follow-up conversions report nothing.
pub(super) fn table_completion_notice(action: StructureAction) -> Option<Message> {
    let key = match action {
        StructureAction::AddTable => "status-done-table-added",
        StructureAction::DuplicateTable => "status-done-table-duplicated",
        StructureAction::DeleteTable => "status-done-table-deleted",
        StructureAction::RenameTable => "status-done-table-renamed",
        StructureAction::MarkdownAlignLeft
        | StructureAction::MarkdownAlignCenter
        | StructureAction::MarkdownAlignRight => "status-done-alignment-changed",
        _ => return None,
    };
    Some(Message::new(key))
}
