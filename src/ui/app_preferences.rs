use super::*;

impl TexcelApp {
    // Capture current application and document preferences for saving.
    pub(super) fn snapshot_settings(&self) -> SettingsFile {
        let mut snapshot = SettingsFile {
            schema_version: self.settings_file.schema_version,
            app: self.current_app_preferences(),
            documents: self.settings_file.documents.clone(),
            recent_files: self.recent_files.iter().cloned().collect(),
        };
        let mut document_ids_by_path = BTreeMap::<PathBuf, Vec<DocumentId>>::new();
        for (document_id, backing) in &self.file_backings {
            document_ids_by_path
                .entry(normalize_recent_path(user_file_path(
                    self,
                    *document_id,
                    backing,
                )))
                .or_default()
                .push(*document_id);
        }
        let active_document_id = self.workspace.active_document;
        for (path, document_ids) in document_ids_by_path {
            let preferred_key = self
                .preferred_settings_identity_by_path
                .get(&path)
                .map(|identity| document_key(&path, identity));
            let saved_key = snapshot
                .documents
                .iter()
                .find(|entry| entry.key.path == path)
                .map(|entry| entry.key.clone());
            let key_to_match = preferred_key.as_ref().or(saved_key.as_ref());
            let matching_document = key_to_match.and_then(|key| {
                document_ids
                    .iter()
                    .copied()
                    .filter(|document_id| {
                        self.file_backings.get(document_id).is_some_and(|backing| {
                            document_key(
                                user_file_path(self, *document_id, backing),
                                &backing.identity,
                            ) == *key
                        })
                    })
                    .find(|document_id| Some(*document_id) == active_document_id)
                    .or_else(|| {
                        document_ids.iter().copied().find(|document_id| {
                            self.file_backings.get(document_id).is_some_and(|backing| {
                                document_key(
                                    user_file_path(self, *document_id, backing),
                                    &backing.identity,
                                ) == *key
                            })
                        })
                    })
            });
            let document_id = if preferred_key.is_some() {
                matching_document
            } else {
                matching_document.or_else(|| {
                    document_ids
                        .iter()
                        .copied()
                        .find(|document_id| Some(*document_id) == active_document_id)
                        .or_else(|| document_ids.first().copied())
                })
            };
            let Some(document_id) = document_id else {
                continue;
            };
            let Some(backing) = self.file_backings.get(&document_id) else {
                continue;
            };
            let Some(document) = self.workspace.documents.get(&document_id) else {
                continue;
            };
            let key = document_key(
                user_file_path(self, document_id, backing),
                &backing.identity,
            );
            let mut tables = snapshot_table_preferences(document);
            let coordinate_generation = self
                .coordinate_preference_generations
                .get(&document_id)
                .copied()
                .unwrap_or_default();
            let coordinate_preferences_deferred = self
                .frozen_coordinate_preference_generations
                .get(&document_id)
                .is_some_and(|frozen| coordinate_generation <= *frozen);
            if document.structure_revision != document.saved_structure_revision
                || coordinate_preferences_deferred
            {
                let saved = snapshot.document_shared(&key).cloned();
                preserve_saved_coordinate_preferences(&mut tables, saved.as_deref());
            }
            snapshot.upsert_document(DocumentPreferences {
                key,
                display_mode: document.display_mode,
                tables,
            });
        }
        snapshot
    }

    // Build application preferences from the active UI state.
    pub(super) fn current_app_preferences(&self) -> AppPreferences {
        AppPreferences {
            language: self.language_choice,
            theme: self.theme,
            font_preset: self.font_preset,
            ui_font_size: self.ui_font_size,
            show_invisibles: self.show_invisibles,
        }
    }

    // Keep saved per-document preferences while app settings are updated.
    pub(super) fn preserve_saved_document_preferences(&mut self, preferences: DocumentPreferences) {
        if self.settings_loaded {
            self.settings_file.upsert_document(preferences);
        } else {
            self.pending_saved_document_preferences.push(preferences);
        }
        self.mark_settings_write();
    }

    // Restore preferences that match the active document identity.
    pub(super) fn restore_document_preferences(&mut self, document_id: DocumentId) {
        if !self.settings_loaded || self.settings_applied_documents.contains(&document_id) {
            return;
        }
        let Some(backing) = self.file_backings.get(&document_id) else {
            return;
        };
        let key = document_key(
            user_file_path(self, document_id, backing),
            &backing.identity,
        );
        let preferences = self.settings_file.document_shared(&key).cloned();
        self.settings_applied_documents.insert(document_id);
        let Some(preferences) = preferences else {
            return;
        };
        let Some(document) = self.workspace.documents.get_mut(&document_id) else {
            return;
        };
        let display_mode_changed = self
            .document_preference_changes
            .get(&document_id)
            .is_some_and(|tables| {
                tables
                    .values()
                    .any(|fields| fields.contains(&TablePreferenceField::DisplayMode))
            });
        if !display_mode_changed {
            document.display_mode = preferences.display_mode;
        }
        let coordinate_preferences_match_saved =
            document.structure_revision == document.saved_structure_revision;
        for table_preferences in &preferences.tables {
            let matched_table = document
                .tables
                .get(table_preferences.table_index)
                .filter(|table| table.name == table_preferences.table_name);
            let Some(table) = matched_table else {
                continue;
            };
            let table_id = table.id;
            let changed = self
                .document_preference_changes
                .get(&document_id)
                .and_then(|tables| tables.get(&table_id));
            if let Some(view) = document.view_mut(table_id) {
                if coordinate_preferences_match_saved
                    && !changed
                        .is_some_and(|fields| fields.contains(&TablePreferenceField::WrapMode))
                {
                    view.wrap_text = table_preferences.wrap_mode == WrapMode::On;
                }
                if coordinate_preferences_match_saved
                    && !changed
                        .is_some_and(|fields| fields.contains(&TablePreferenceField::ViewLayout))
                {
                    view.layout = Arc::clone(&table_preferences.view_layout);
                }
            }
        }
    }
}
