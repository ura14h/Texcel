//! The column fit run when a file is opened: every table of the document is fitted before
//! its window appears, while the opening progress window reports how far it has got.

use super::*;

/// Rows measured by the fit on opening; a long file's later rows are not read for it, so the
/// shared file worker is not held for long.
const INITIAL_AUTO_FIT_ROWS: usize = 1_000;

/// The tables of one opened document that still need their first fit.
pub(super) struct InitialAutoFit {
    remaining: VecDeque<TableId>,
    total: usize,
    /// Whether the table at the front has been sent to the worker.
    requested: bool,
}

impl InitialAutoFit {
    /// Returns how many tables are fitted and how many there are.
    pub(super) fn progress(&self) -> (usize, usize) {
        (self.total - self.remaining.len(), self.total)
    }

    /// Share of the tables already fitted, from 0 to 1.
    pub(super) fn fraction_done(&self) -> f32 {
        if self.total == 0 {
            return 1.0;
        }
        (self.total - self.remaining.len()) as f32 / self.total as f32
    }
}

impl TexcelApp {
    /// Queues every table of a newly loaded document for its first fit, the shown one first.
    pub(super) fn queue_initial_auto_fit(&mut self, document_id: DocumentId) {
        let Some(document) = self.workspace.documents.get(&document_id) else {
            return;
        };
        let mut remaining = std::iter::once(document.active_table)
            .chain(
                document
                    .tables
                    .iter()
                    .map(|table| table.id)
                    .filter(|id| *id != document.active_table),
            )
            .filter(|id| document.tables.iter().any(|table| table.id == *id))
            .collect::<VecDeque<_>>();
        remaining.make_contiguous();
        let total = remaining.len();
        self.initial_auto_fits.insert(
            document_id,
            InitialAutoFit {
                remaining,
                total,
                requested: false,
            },
        );
    }

    /// Moves the opening fit forward: waits for the table being measured, then sends the next
    /// table's used columns that have no saved width. The entry goes away when every table is
    /// done, which lets the document's window appear.
    pub(super) fn advance_initial_auto_fit(
        &mut self,
        ctx: &egui::Context,
        document_id: DocumentId,
    ) {
        self.start_ready_auto_fits(ctx);
        // Each pass of the loop finishes one table or sends one to the worker.
        loop {
            let running = self.pending_auto_fits.contains_key(&document_id)
                || self.deferred_auto_fits.contains_key(&document_id);
            let Some(state) = self.initial_auto_fits.get_mut(&document_id) else {
                return;
            };
            if running {
                return;
            }
            if state.requested {
                state.remaining.pop_front();
                state.requested = false;
            }
            let Some(table_id) = state.remaining.front().copied() else {
                self.initial_auto_fits.remove(&document_id);
                return;
            };
            let columns = self.unsized_used_columns(document_id, table_id);
            let Some(state) = self.initial_auto_fits.get_mut(&document_id) else {
                return;
            };
            if columns.is_empty() {
                state.remaining.pop_front();
                continue;
            }
            // A request the worker could not take counts as done, so opening never stalls.
            state.requested = true;
            self.request_columns_auto_fit_within(
                ctx,
                document_id,
                table_id,
                columns,
                Some(INITIAL_AUTO_FIT_ROWS),
            );
            if self.pending_auto_fits.contains_key(&document_id)
                || self.deferred_auto_fits.contains_key(&document_id)
            {
                return;
            }
        }
    }

    /// Lists a table's used columns whose width was not saved earlier.
    fn unsized_used_columns(&self, document_id: DocumentId, table_id: TableId) -> Vec<usize> {
        let Some(document) = self.workspace.documents.get(&document_id) else {
            return Vec::new();
        };
        document
            .tables
            .iter()
            .find(|table| table.id == table_id)
            .zip(document.view(table_id))
            .map(|(table, view)| {
                (0..=table.extent_end().column)
                    .filter(|column| view.layout.column(*column).is_none())
                    .collect()
            })
            .unwrap_or_default()
    }
}
