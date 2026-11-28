//! The column fit run when a file is opened.
//!
//! Widths follow from the document's content, its formula results, and the display and font
//! settings. An opening document is not shown yet, so only the opening steps themselves
//! change those inputs: restoring the widths saved for the file, and the first calculation.
//! The fit therefore runs once, after both, as one worker job over every table, and its
//! result is applied only to the inputs it was measured from. Then the window appears.

use super::*;

/// Rows measured by the fit on opening; a long file's later rows are not read for it, so the
/// shared file worker is not held for long.
const OPENING_FIT_ROWS: usize = 1_000;

/// Where one opening document's fit stands.
pub(super) enum OpeningFit {
    /// Waiting until the saved widths are restored and the first calculation is done.
    Waiting,
    /// Sent to the worker, measured from this document and calculation revision.
    Measuring {
        token: JobToken,
        calculation_revision: u64,
    },
}

/// The step an opening fit is at, for the progress window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum OpeningFitStep {
    Calculating,
    Measuring,
}

impl TexcelApp {
    /// Queues a newly loaded document for its opening fit.
    pub(super) fn queue_opening_fit(&mut self, document_id: DocumentId) {
        self.opening_fits.insert(document_id, OpeningFit::Waiting);
    }

    /// Starts every opening fit whose inputs are settled. A busy worker leaves the fit
    /// waiting to be sent on a later frame.
    pub(super) fn advance_opening_fits(&mut self, ctx: &egui::Context) {
        let waiting = self
            .opening_fits
            .iter()
            .filter(|(_, fit)| matches!(fit, OpeningFit::Waiting))
            .map(|(document_id, _)| *document_id)
            .collect::<Vec<_>>();
        for document_id in waiting {
            if !self.opening_fit_inputs_settled(document_id) {
                continue;
            }
            let targets = self.opening_fit_targets(document_id);
            if targets.is_empty() {
                self.opening_fits.remove(&document_id);
                continue;
            }
            let Some(document) = self.workspace.documents.get(&document_id) else {
                self.opening_fits.remove(&document_id);
                continue;
            };
            let (revision, calculation_revision) =
                (document.revision, document.calculation_revision);
            let token = self.next_job_token(document_id, revision);
            let request = self.auto_fit_request(
                ctx,
                document_id,
                targets,
                Some(OPENING_FIT_ROWS),
                grid_character_width(ctx),
            );
            match request.map(|request| self.file_worker.auto_fit(token, request)) {
                Some(Ok(())) => {
                    self.opening_fits.insert(
                        document_id,
                        OpeningFit::Measuring {
                            token,
                            calculation_revision,
                        },
                    );
                }
                Some(Err(SubmitError::QueueFull)) => {
                    ctx.request_repaint_after(Duration::from_millis(50));
                }
                // Without a worker or a document the file opens at its default widths.
                Some(Err(SubmitError::Stopped)) | None => {
                    self.opening_fits.remove(&document_id);
                }
            }
        }
    }

    /// Tells whether the inputs of a document's widths are final: the widths saved for the
    /// file are restored and no calculation is running.
    fn opening_fit_inputs_settled(&self, document_id: DocumentId) -> bool {
        self.settings_applied_documents.contains(&document_id)
            && !self.pending_calculations.contains_key(&document_id)
            && !self.pending_initial_calculations.contains_key(&document_id)
            && !self.calculation_retries.contains(&document_id)
    }

    /// Lists every table with the used columns that have no saved width.
    fn opening_fit_targets(
        &self,
        document_id: DocumentId,
    ) -> Vec<crate::file_worker::AutoFitTarget> {
        let Some(document) = self.workspace.documents.get(&document_id) else {
            return Vec::new();
        };
        document
            .tables
            .iter()
            .filter_map(|table| {
                let view = document.view(table.id)?;
                let columns = (0..=table.extent_end().column)
                    .filter(|column| view.layout.column(*column).is_none())
                    .collect::<Vec<_>>();
                (!columns.is_empty()).then(|| crate::file_worker::AutoFitTarget {
                    table_id: table.id,
                    columns,
                    layout: view.layout.as_ref().clone(),
                    wrap_text: view.wrap_text,
                })
            })
            .collect()
    }

    /// Applies an opening fit's widths when they were measured from the document as it is
    /// now; if its content or formula results changed meanwhile, it is measured again.
    /// Returns whether the result belonged to an opening fit.
    pub(super) fn handle_opening_fit_result(
        &mut self,
        document_id: DocumentId,
        token: JobToken,
        widths: &crate::file_worker::TableColumnWidths,
    ) -> bool {
        let Some(OpeningFit::Measuring {
            token: expected,
            calculation_revision,
        }) = self.opening_fits.get(&document_id)
        else {
            return false;
        };
        if *expected != token {
            return false;
        }
        let calculation_revision = *calculation_revision;
        let Some(document) = self.workspace.documents.get_mut(&document_id) else {
            self.opening_fits.remove(&document_id);
            return true;
        };
        if document.revision != token.revision
            || document.calculation_revision != calculation_revision
        {
            self.opening_fits.insert(document_id, OpeningFit::Waiting);
            return true;
        }
        for (table_id, columns) in widths {
            let Some(view) = document.view_mut(*table_id) else {
                continue;
            };
            for (column, width) in columns {
                let mut layout = view.layout.column(*column).unwrap_or_default();
                layout.width = Some(*width);
                view.set_layout_value(
                    LayoutTarget::Column(*column),
                    LayoutValue::Column(Some(layout)),
                );
            }
        }
        self.opening_fits.remove(&document_id);
        for (table_id, _) in widths {
            self.mark_document_preference_change(
                document_id,
                *table_id,
                TablePreferenceField::ViewLayout,
            );
        }
        true
    }

    /// Returns the step a document's opening fit is at, if it is still opening.
    pub(super) fn opening_fit_step(&self, document_id: DocumentId) -> Option<OpeningFitStep> {
        let calculating = self.pending_calculations.contains_key(&document_id)
            || self.pending_initial_calculations.contains_key(&document_id)
            || self.calculation_retries.contains(&document_id);
        match self.opening_fits.get(&document_id)? {
            OpeningFit::Waiting if calculating => Some(OpeningFitStep::Calculating),
            // Waiting for the saved widths or a free worker comes right before measuring.
            OpeningFit::Waiting | OpeningFit::Measuring { .. } => Some(OpeningFitStep::Measuring),
        }
    }
}
