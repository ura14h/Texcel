use super::*;

impl TexcelApp {
    // Handle the saved export result and update its report state.
    pub(super) fn handle_export_result(
        &mut self,
        document_id: DocumentId,
        token: JobToken,
        kind: FileResultKind,
    ) -> bool {
        match kind {
            FileResultKind::ExportPrepared(prepared) => {
                if self.pending_exports.get(&document_id) == Some(&token) {
                    self.pending_exports.remove(&document_id);
                    let cancelled = self.cancelling_exports.remove(&document_id) == Some(token);
                    if cancelled {
                        self.export_configs.remove(&document_id);
                        self.save_as_export_documents.remove(&document_id);
                        self.finish_save_intent_failed(document_id);
                        self.status = Some(Message::new("status-export-cancelled"));
                    } else {
                        self.begin_prepared_export(document_id, prepared);
                    }
                }
            }
            FileResultKind::Exported {
                written,
                failed,
                not_written,
            } => {
                if self.pending_export_writes.get(&document_id) == Some(&token) {
                    self.pending_export_writes.remove(&document_id);
                    self.cancelling_exports.remove(&document_id);
                    if written.is_empty() && failed.is_none() {
                        self.status = Some(Message::new("status-export-file-not-created"));
                    } else if failed.is_none() {
                        self.status = Some(
                            Message::new("status-done-exported")
                                .with_number("count", written.len() as i64),
                        );
                    } else if let Some((failed_path, message)) = failed {
                        let written_names = written
                            .iter()
                            .take(4)
                            .map(|path| path.display().to_string())
                            .collect::<Vec<_>>();
                        let written_paths = if written_names.is_empty() {
                            Message::new("status-export-no-successful-paths")
                        } else {
                            Message::new("status-export-successful-paths")
                                .with_text_list("paths", written_names)
                        };
                        let remaining = if written.len() > 4 {
                            Message::new("status-export-remaining")
                                .with_number("count", (written.len() - 4) as i64)
                        } else {
                            Message::new("dialog-empty-tail")
                        };
                        let omitted = if not_written.is_empty() {
                            Message::new("dialog-empty-tail")
                        } else {
                            Message::new("status-export-omitted")
                                .with_number("count", not_written.len() as i64)
                        };
                        self.status = Some(
                            Message::new("status-export-partial")
                                .with_number("written", written.len() as i64)
                                .with_message("remaining", remaining)
                                .with_text("failed_path", failed_path.display())
                                .with_message("detail", message)
                                .with_message("omitted", omitted)
                                .with_message("paths", written_paths),
                        );
                    }
                    if self.pending_save_intents.contains_key(&document_id) {
                        self.finish_save_intent_failed(document_id);
                    }
                }
            }
            _ => unreachable!("file result was routed to the wrong handler"),
        }
        false
    }
}
