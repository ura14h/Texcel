//! The progress window shown while files are being opened: reading, the first calculation,
//! and fitting every table's columns, until each document's own window appears.

use super::*;

/// How long an opening may take before the progress window appears.
pub(super) const OPENING_PROGRESS_DELAY: Duration = Duration::from_millis(300);

/// One file being opened, as the progress window shows it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct OpeningEntry {
    pub(super) name: String,
    pub(super) stage: Message,
    /// Share of the opening done, from 0 to 1.
    pub(super) fraction: f32,
}

/// Returns the viewport of the app-wide opening progress window.
pub(super) fn opening_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("texcel-opening-progress")
}

impl TexcelApp {
    /// Notes when each document began opening and forgets those whose window is ready.
    pub(super) fn refresh_opening_progress(&mut self) {
        let opening = self.opening_documents();
        self.opening_since
            .retain(|document_id, _| opening.contains(document_id));
        for document_id in opening {
            self.opening_since
                .entry(document_id)
                .or_insert_with(Instant::now);
        }
    }

    /// Lists documents still opening: loading (not waiting for the user's CSV answer), in
    /// their first calculation, or having their tables fitted.
    fn opening_documents(&self) -> BTreeSet<DocumentId> {
        let waiting_for_user = |document_id: &DocumentId| {
            self.pending_csv_opens
                .get(document_id)
                .is_some_and(|pending| pending.prompt.is_some())
        };
        self.pending_file_load_paths
            .keys()
            .filter(|document_id| !waiting_for_user(document_id))
            .chain(self.initial_auto_fits.keys())
            .copied()
            .collect()
    }

    /// Returns the files that have been opening long enough to show their progress.
    pub(super) fn opening_entries(&self) -> Vec<OpeningEntry> {
        self.opening_since
            .iter()
            .filter(|(_, since)| since.elapsed() >= OPENING_PROGRESS_DELAY)
            .filter_map(|(document_id, _)| self.opening_entry(*document_id))
            .collect()
    }

    /// Tells whether any file is opening, so the app checks again when the delay passes.
    pub(super) fn has_opening_documents(&self) -> bool {
        !self.opening_since.is_empty()
    }

    /// Describes one opening file: its name, its current step, and how far it has got.
    /// Reading and parsing count for the first 40%, the first calculation for 50%, and
    /// fitting the tables for the rest.
    fn opening_entry(&self, document_id: DocumentId) -> Option<OpeningEntry> {
        if let Some(path) = self.pending_file_load_paths.get(&document_id) {
            let name = path.file_name()?.to_string_lossy().into_owned();
            let (stage, fraction) = match self
                .pending_load_previews
                .get(&document_id)
                .map(|preview| preview.stage)
            {
                Some(LoadStage::ParsingSource | LoadStage::ParsingWorkbook) => {
                    (Message::new("opening-stage-parsing"), 0.2)
                }
                _ => (Message::new("opening-stage-reading"), 0.05),
            };
            return Some(OpeningEntry {
                name,
                stage,
                fraction,
            });
        }
        let name = self.workspace.documents.get(&document_id)?.name.clone();
        let fitting = self.initial_auto_fits.get(&document_id)?;
        if self.pending_initial_calculations.contains_key(&document_id) {
            return Some(OpeningEntry {
                name,
                stage: Message::new("opening-stage-calculating"),
                fraction: 0.5,
            });
        }
        let (done, total) = fitting.progress();
        Some(OpeningEntry {
            name,
            stage: Message::new("opening-stage-fitting")
                .with_number("done", done as i64)
                .with_number("total", total as i64),
            fraction: 0.6 + 0.4 * fitting.fraction_done(),
        })
    }

    /// Draws the progress window: one row per opening file with its step and a bar.
    pub(super) fn draw_opening_progress_contents(&self, ui: &mut egui::Ui) {
        if ui.input(|input| input.viewport().close_requested()) {
            // Opening cannot be stopped from here; the window closes when it finishes.
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        let entries = self.opening_entries();
        egui::CentralPanel::default().show(ui, |ui| {
            visual::prepare_dialog_body(ui);
            for entry in &entries {
                ui.strong(&entry.name);
                ui.label(entry.stage.resolve(self.language));
                ui.add(
                    egui::ProgressBar::new(entry.fraction)
                        .show_percentage()
                        .desired_width(ui.available_width()),
                );
                ui.add_space(visual::SPACE_8);
            }
        });
        ui.ctx().request_repaint_after(Duration::from_millis(100));
    }

    /// Describes the progress window's native frame for the given number of files.
    pub(super) fn opening_viewport_builder(&self, rows: usize) -> egui::ViewportBuilder {
        egui::ViewportBuilder::default()
            .with_title(text(self.language, "opening-window-title"))
            .with_inner_size([420.0, 32.0 + 92.0 * rows.max(1) as f32])
            .with_resizable(false)
            .with_minimize_button(false)
            .with_maximize_button(false)
    }
}
