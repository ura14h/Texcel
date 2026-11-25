use super::*;

impl TexcelApp {
    // Apply a save acknowledgement to the matching document revision.
    pub(super) fn handle_save_result(
        &mut self,
        document_id: DocumentId,
        job_token: JobToken,
        path: PathBuf,
        kind: FileResultKind,
    ) -> bool {
        match kind {
            FileResultKind::Saved {
                identity,
                csv_source,
                markdown_source,
                text_overlay,
            } => {
                if self.pending_saves.get(&document_id) == Some(&job_token) {
                    let user_path = self
                        .file_backings
                        .get(&document_id)
                        .filter(|backing| backing.path == path)
                        .map(|backing| user_file_path(self, document_id, backing).to_path_buf())
                        .unwrap_or_else(|| path.clone());
                    self.pending_saves.remove(&document_id);
                    self.cancelling_saves.remove(&document_id);
                    if self
                        .recent_save_paths
                        .get(&document_id)
                        .is_some_and(|(token, _)| *token == job_token)
                    {
                        self.recent_save_paths.remove(&document_id);
                        self.remember_recent_file(path.clone());
                    }
                    let saved_recovery_generation = self
                        .pending_save_recovery_generations
                        .remove(&document_id)
                        .filter(|(token, _)| *token == job_token)
                        .map(|(_, generation)| generation);
                    let saved_history_state = self
                        .pending_save_history_states
                        .remove(&document_id)
                        .filter(|(token, _)| *token == job_token)
                        .map(|(_, state_id)| state_id);
                    let current_revision = self
                        .workspace
                        .documents
                        .get(&document_id)
                        .is_some_and(|document| document.revision == job_token.revision);
                    let saved_source = csv_source
                        .map(SourceDocument::Csv)
                        .or_else(|| markdown_source.map(SourceDocument::Markdown));
                    let retained_source = self
                        .workspace
                        .documents
                        .get(&document_id)
                        .and_then(|document| document.source.as_deref().cloned());
                    let saved_structure_revision = self
                        .pending_save_preferences
                        .get(&document_id)
                        .filter(|(token, _, _, _)| *token == job_token)
                        .map(|(_, _, structure_revision, _)| *structure_revision);
                    let saved_coordinate_preference_generation = self
                        .pending_save_preferences
                        .get(&document_id)
                        .filter(|(token, _, _, _)| *token == job_token)
                        .map(|(_, _, _, generation)| *generation);
                    if let Some(document) = self.workspace.documents.get_mut(&document_id) {
                        if let Some(structure_revision) = saved_structure_revision {
                            let state_id =
                                saved_history_state.unwrap_or_else(|| document.history_state_id());
                            document.mark_saved_at_structure(
                                job_token.revision,
                                structure_revision,
                                state_id,
                            );
                        } else if let Some(state_id) = saved_history_state {
                            document.mark_saved_at_state(job_token.revision, state_id);
                        } else {
                            document.mark_saved(job_token.revision);
                        }
                        if let Some(text_overlay) = text_overlay {
                            document.install_text_overlay_snapshot(text_overlay);
                        }
                        document.name = user_path.file_name().map_or_else(
                            || document.name.clone(),
                            |name| name.to_string_lossy().into_owned(),
                        );
                        if current_revision || document.source.is_none() {
                            document.set_source(saved_source.as_ref().cloned().map(Arc::new));
                        }
                    }
                    let source = if current_revision {
                        saved_source.or(retained_source)
                    } else {
                        retained_source.or(saved_source)
                    };
                    let saved_key = document_key(&user_path, &identity);
                    let saved_preferences = self
                        .pending_save_preferences
                        .remove(&document_id)
                        .filter(|(token, _, _, _)| *token == job_token)
                        .map(|(_, tables, _, _)| DocumentPreferences {
                            key: saved_key,
                            display_mode: self
                                .workspace
                                .documents
                                .get(&document_id)
                                .map_or_else(Default::default, |document| document.display_mode),
                            tables,
                        });
                    if let Some(saved_generation) = saved_coordinate_preference_generation {
                        let current_generation = self
                            .coordinate_preference_generations
                            .get(&document_id)
                            .copied()
                            .unwrap_or_default();
                        if current_generation > saved_generation {
                            self.frozen_coordinate_preference_generations
                                .insert(document_id, current_generation);
                        } else {
                            self.frozen_coordinate_preference_generations
                                .remove(&document_id);
                        }
                    }
                    self.preferred_settings_identity_by_path
                        .insert(normalize_recent_path(&user_path), identity.clone());
                    self.user_file_paths.insert(document_id, user_path);
                    self.file_backings.insert(
                        document_id,
                        FileBacking {
                            path,
                            identity,
                            source,
                            overwrite_confirmed: false,
                        },
                    );
                    self.recovered_documents_needing_save.remove(&document_id);
                    if self
                        .save_conflict_dialog
                        .is_some_and(|prompt| prompt.document_id == document_id)
                    {
                        self.save_conflict_dialog = None;
                    }
                    if let Some(generation) = saved_recovery_generation {
                        self.finish_recovery_after_save(
                            document_id,
                            job_token.revision,
                            generation,
                        );
                    }
                    if let Some(preferences) = saved_preferences {
                        self.preserve_saved_document_preferences(preferences);
                    } else {
                        self.mark_settings_write();
                    }
                    let saved_name = self
                        .user_file_paths
                        .get(&document_id)
                        .and_then(|path| path.file_name())
                        .map(|name| name.to_string_lossy().into_owned());
                    self.status = saved_name
                        .map(|name| Message::new("status-done-saved").with_text("name", name));
                    if self.close_after_save.remove(&document_id) {
                        let has_uncommitted_edit = self.edit.is_editing()
                            && self.edit_document_id.or_else(|| {
                                self.workspace.active_document().map(|document| document.id)
                            }) == Some(document_id);
                        self.close_after_save_edit.remove(&document_id);
                        if current_revision && !has_uncommitted_edit {
                            self.close_document_now(document_id);
                        } else {
                            self.pending_close_document = Some(document_id);
                            self.status = Some(if has_uncommitted_edit {
                                Message::new("dialog-save-uncommitted")
                            } else {
                                Message::new("dialog-save-pending-edit")
                            });
                        }
                    }
                    self.finish_app_close_save(document_id, job_token, true);
                }
            }
            FileResultKind::ReportSaved => {
                if self.pending_report_saves.get(&document_id) == Some(&job_token) {
                    self.pending_report_saves.remove(&document_id);
                    self.status = Some(Message::new("status-done-report-saved"));
                } else if self.pending_export_report_saves.get(&document_id) == Some(&job_token) {
                    self.pending_export_report_saves.remove(&document_id);
                    self.status = Some(Message::new("status-done-report-saved"));
                }
            }
            _ => unreachable!("file result was routed to the wrong handler"),
        }
        false
    }
}
