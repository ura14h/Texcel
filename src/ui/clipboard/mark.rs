use super::*;

/// The ranges of the last Texcel copy or cut, outlined until the user moves on.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::ui) struct ClipboardMark {
    /// Owner ID of the clipboard data this copy wrote.
    pub(in crate::ui) owner_id: String,
    pub(in crate::ui) document_id: DocumentId,
    pub(in crate::ui) table_id: TableId,
    pub(in crate::ui) ranges: Vec<CellRange>,
    /// Document revision at copy time; any later edit ends the mark.
    pub(in crate::ui) revision: u64,
    /// True when the next paste in the same table moves the cells.
    pub(in crate::ui) cut: bool,
}

/// What the grid outlines for one copy or cut.
#[derive(Clone, Copy, Debug)]
pub(in crate::ui) struct CopyMarquee<'a> {
    pub(in crate::ui) ranges: &'a [CellRange],
    pub(in crate::ui) cut: bool,
}

impl TexcelApp {
    /// Remembers the ranges just written to the clipboard under `owner_id`.
    pub(super) fn set_clipboard_mark(
        &mut self,
        owner_id: Option<String>,
        document_id: DocumentId,
        table_id: TableId,
        ranges: Vec<CellRange>,
        revision: u64,
    ) {
        self.clipboard_mark = owner_id.map(|owner_id| ClipboardMark {
            owner_id,
            document_id,
            table_id,
            ranges,
            revision,
            cut: false,
        });
    }

    /// Turns the last copy into a cut that the next paste in the same table moves.
    pub(in crate::ui) fn mark_clipboard_as_cut(&mut self) {
        if let Some(mark) = self.clipboard_mark.as_mut() {
            mark.cut = true;
        }
    }

    /// Ends the copy or cut outline; the clipboard itself keeps its data.
    pub(in crate::ui) fn clear_clipboard_mark(&mut self) -> bool {
        self.clipboard_mark.take().is_some()
    }

    /// Returns the outline for the active table while its document is unchanged.
    pub(in crate::ui) fn active_copy_marquee(&self) -> Option<CopyMarquee<'_>> {
        let mark = self.clipboard_mark.as_ref()?;
        let document = self.workspace.active_document()?;
        let table = document.active_table()?;
        (mark.document_id == document.id
            && mark.table_id == table.id
            && mark.revision == document.revision)
            .then_some(CopyMarquee {
                ranges: &mark.ranges,
                cut: mark.cut,
            })
    }

    /// Drops a mark whose document was edited after the copy or cut.
    pub(in crate::ui) fn expire_clipboard_mark(&mut self) {
        let stale = self.clipboard_mark.as_ref().is_some_and(|mark| {
            self.workspace
                .documents
                .get(&mark.document_id)
                .is_none_or(|document| document.revision != mark.revision)
        });
        if stale {
            self.clipboard_mark = None;
        }
    }

    /// Decides whether a paste moves the cut cells, and ends the cut otherwise.
    /// Only a normal paste of this process's own cut data into the same table moves.
    /// `owner_id` names the Texcel data on the clipboard now, if this process wrote it.
    pub(super) fn cut_source_for_paste(
        &mut self,
        owner_id: Option<&str>,
        document_id: DocumentId,
        table_id: TableId,
        options: PasteOptions,
    ) -> Option<CellRange> {
        let mark = self.clipboard_mark.as_ref()?;
        if owner_id != Some(mark.owner_id.as_str()) {
            // Another app or copy replaced the clipboard.
            self.clipboard_mark = None;
            return None;
        }
        if !mark.cut {
            return None;
        }
        let revision = self
            .workspace
            .documents
            .get(&document_id)
            .map(|document| document.revision);
        let moves = mark.document_id == document_id
            && mark.table_id == table_id
            && Some(mark.revision) == revision
            && mark.ranges.len() == 1
            && options == PasteOptions::default();
        let range = mark.ranges.first().copied();
        // A cut is used once: a move or any other paste ends it.
        self.clipboard_mark = None;
        moves.then_some(range).flatten()
    }
}

#[cfg(test)]
#[path = "mark_test.rs"]
mod tests;
