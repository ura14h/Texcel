use super::*;

impl TexcelApp {
    /// Shows the original extension and import source for imported workbooks and pages.
    pub(super) fn document_display_name(
        &self,
        ctx: &egui::Context,
        document_id: DocumentId,
    ) -> String {
        let Some(document) = self.workspace.documents.get(&document_id) else {
            return String::new();
        };
        let imported_name = self.file_backings.get(&document_id).and_then(|backing| {
            let path = user_file_path(self, document_id, backing);
            let extension = path.extension()?.to_str()?.to_ascii_lowercase();
            if matches!(extension.as_str(), "xlsx" | "ods" | "html" | "htm") {
                Some(path.file_name()?.to_string_lossy().into_owned())
            } else {
                None
            }
        });
        imported_name.map_or_else(
            || document.name.clone(),
            |name| {
                crate::i18n::context_message(
                    ctx,
                    &Message::new("top-imported-document-name").with_text("name", name),
                )
            },
        )
    }

    /// Updates the native window title only when the selected document or table changes.
    pub(super) fn update_viewport_title(&mut self, ctx: &egui::Context) {
        #[cfg(target_os = "macos")]
        crate::macos_app::sync_document_edited_states(
            self.workspace
                .order
                .iter()
                .filter_map(|id| {
                    self.workspace
                        .documents
                        .contains_key(id)
                        .then_some((*id, self.document_has_unsaved_changes(*id)))
                })
                .collect::<Vec<_>>()
                .as_slice(),
            ctx.input(|input| input.viewport().focused == Some(true))
                .then_some(self.workspace.active_document)
                .flatten(),
        );
        let title = self.workspace.active_document().map_or_else(
            || "Texcel".to_owned(),
            |document| {
                let name = self.document_display_name(ctx, document.id);
                let marker = if self.document_has_unsaved_changes(document.id) {
                    " •"
                } else {
                    ""
                };
                document.active_table().map_or_else(
                    || format!("{name}{marker} — Texcel"),
                    |table| format!("{name}{marker} — {} — Texcel", table.name),
                )
            },
        );
        if self.last_viewport_title.as_deref() != Some(title.as_str()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_viewport_title = Some(title);
        }
    }
}
