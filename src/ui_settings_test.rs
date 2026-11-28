use super::*;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

struct TestDirectory(std::path::PathBuf);

impl TestDirectory {
    /// Creates an isolated folder for asynchronous file-operation tests.
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("texcel-ui-settings-fix-{nonce}"));
        fs::create_dir_all(&path).expect("create test directory");
        Self(path)
    }
}

impl Drop for TestDirectory {
    /// Removes the test files after the assertions finish.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

// Verify that a disconnected settings queue fails the current generation and releases close handling.
#[test]
fn disconnected_settings_worker_fails_the_latest_generation_and_unblocks_close() {
    let ctx = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(ctx.clone());
    let mut app = TexcelApp::new_with_settings_path(&creation, None);
    app.settings_worker = Some(crate::settings::tests::disconnected_for_test());
    app.settings_loaded = true;
    app.settings_save_generation = 7;
    app.settings_write_requested = true;

    let mut output = ctx.run_ui(egui::RawInput::default(), |_| app.process_settings(&ctx));
    output.textures_delta.clear();

    assert_eq!(app.settings_failed_generation, Some(7));
    assert_eq!(app.settings_in_flight_generation, None);
    assert!(!app.settings_write_requested);
    assert!(app.settings_pending_save.is_none());
    assert!(!app.close_after_settings_save);
    assert!(app.status.as_ref().is_some_and(|status| {
        status
            .resolve(crate::i18n::Language::Japanese)
            .contains("設定を保存する処理")
    }));

    let mut raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
        ..egui::RawInput::default()
    };
    raw.viewports
        .get_mut(&egui::ViewportId::ROOT)
        .expect("raw input includes the root viewport")
        .events = vec![egui::ViewportEvent::Close];
    let mut output = ctx.run_ui(raw, |_| app.handle_app_close_request(&ctx));
    assert!(
        output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::CancelClose))
    );
    output.textures_delta.clear();
}

// Verify that late settings do not replace a user language choice or edit the document.
#[test]
fn late_settings_keep_the_selected_language_and_document_state() {
    let ctx = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(ctx.clone());
    let mut app = TexcelApp::new_with_settings_path(&creation, None);
    let document = app
        .workspace
        .active_document()
        .expect("the demo document opens");
    let document_id = document.id;
    let before_revision = document.revision;
    let before_saved_revision = document.saved_revision;
    let before_calculation_revision = document.calculation_revision;
    let before_dirty = document.is_dirty();
    let before_tables = serde_json::to_value(&document.tables).expect("snapshot table data");

    app.language_choice = crate::settings::LanguageChoice::Japanese;
    app.language = crate::i18n::Language::Japanese;
    app.mark_app_preference_change(AppPreferenceField::Language);
    app.apply_app_preferences_merged(
        &ctx,
        &AppPreferences {
            language: crate::settings::LanguageChoice::English,
            ..AppPreferences::default()
        },
    );
    crate::i18n::set_context_language(&ctx, app.language);

    assert_eq!(
        app.language_choice,
        crate::settings::LanguageChoice::Japanese
    );
    assert_eq!(app.language, crate::i18n::Language::Japanese);
    assert_eq!(
        crate::i18n::context_language(&ctx),
        crate::i18n::Language::Japanese
    );
    let document = &app.workspace.documents[&document_id];
    assert_eq!(document.revision, before_revision);
    assert_eq!(document.saved_revision, before_saved_revision);
    assert_eq!(document.calculation_revision, before_calculation_revision);
    assert_eq!(document.is_dirty(), before_dirty);
    assert_eq!(
        serde_json::to_value(&document.tables).expect("read table data"),
        before_tables
    );
}

// Verify that structure-save preferences follow the saved version, not later edits.
#[test]
fn structural_save_preferences_follow_the_saved_version_identity() {
    let directory = tests::TestDirectory::new();
    let path = directory.0.join("coordinate-settings.csv");
    let ctx = egui::Context::default();
    let (mut app, document_id, table_id) =
        tests::load_csv_test_app_after_confirming_default_settings(
            &ctx,
            path.clone(),
            b"name,value\nfirst,1\n",
        );
    let original_backing = app.file_backings[&document_id].clone();
    let original_address = CellAddress { row: 0, column: 1 };
    let moved_address = CellAddress { row: 0, column: 2 };
    let later_address = CellAddress { row: 0, column: 3 };
    {
        let document = app.workspace.documents.get_mut(&document_id).unwrap();
        let view = document.view_mut(table_id).unwrap();
        view.set_selection(CellRange::single(original_address));
        let mut layout = TableLayout::default();
        layout.apply_value(
            LayoutTarget::Column(original_address.column),
            LayoutValue::Column(Some(ColumnLayout {
                width: Some(31.0),
                ..Default::default()
            })),
        );
        view.layout = Arc::new(layout);
    }
    app.mark_document_preference_change(document_id, table_id, TablePreferenceField::ViewLayout);
    let old_preferences = DocumentPreferences {
        key: document_key(&path, &original_backing.identity),
        display_mode: crate::document::DocumentDisplayMode::Live,
        tables: snapshot_table_preferences(&app.workspace.documents[&document_id]),
    };
    app.settings_file.upsert_document(old_preferences.clone());

    app.request_structure_action(StructureAction::InsertColumns);
    tests::wait_for_condition(&mut app, |app| {
        !app.pending_structures.contains_key(&document_id)
    });
    let document = &app.workspace.documents[&document_id];
    let view = document.view(table_id).unwrap();
    assert_eq!(
        view.layout.column(moved_address.column).unwrap().width,
        Some(31.0)
    );
    assert!(view.layout.column(original_address.column).is_none());
    assert_ne!(document.revision, document.saved_revision);

    let snapshot = app.snapshot_settings();
    let original_saved = snapshot.document(&old_preferences.key).unwrap();
    assert_eq!(
        original_saved.tables[0]
            .view_layout
            .column(original_address.column)
            .unwrap()
            .width,
        Some(31.0)
    );
    assert!(
        original_saved.tables[0]
            .view_layout
            .column(moved_address.column)
            .is_none()
    );

    let mut old_file_app = tests::new_app(&ctx);
    old_file_app.settings_loaded = true;
    old_file_app.settings_file = snapshot.clone();
    let old_file_placeholder = old_file_app.workspace.active_document.unwrap();
    old_file_app.open_path(path.clone());
    let old_file_id = tests::confirm_pending_csv_settings_with_test_defaults(&mut old_file_app);
    assert_eq!(old_file_app.workspace.documents.len(), 1);
    assert!(
        !old_file_app
            .workspace
            .documents
            .contains_key(&old_file_placeholder)
    );
    assert_eq!(old_file_app.workspace.active_document, Some(old_file_id));
    let old_file_document =
        &old_file_app.workspace.documents[&old_file_app.workspace.active_document.unwrap()];
    let old_view = old_file_document.view(table_id).unwrap();
    assert_eq!(
        old_view
            .layout
            .column(original_address.column)
            .unwrap()
            .width,
        Some(31.0)
    );
    assert!(old_view.layout.column(moved_address.column).is_none());

    app.save_active(false);
    assert!(app.pending_saves.contains_key(&document_id));
    {
        let document = app.workspace.documents.get_mut(&document_id).unwrap();
        let view = document.view_mut(table_id).unwrap();
        let mut later_layout = (*view.layout).clone();
        later_layout.apply_value(
            LayoutTarget::Column(later_address.column),
            LayoutValue::Column(Some(ColumnLayout {
                width: Some(45.0),
                ..Default::default()
            })),
        );
        view.layout = Arc::new(later_layout);
        document
            .set_cell(
                table_id,
                CellAddress { row: 1, column: 0 },
                Some(Cell::text("edited after save submission")),
            )
            .expect("make a later source edit while the save is pending");
    }
    app.mark_document_preference_change(document_id, table_id, TablePreferenceField::ViewLayout);
    tests::wait_for_condition(&mut app, |app| {
        !app.pending_saves.contains_key(&document_id)
    });

    let saved_identity = FileIdentity::read(&path).expect("the structural CSV was saved");
    let saved_key = document_key(&path, &saved_identity);
    let saved_preferences = app
        .settings_file
        .document(&saved_key)
        .expect("save response bound its snapshot to the new identity");
    assert_eq!(
        saved_preferences.tables[0]
            .view_layout
            .column(moved_address.column)
            .unwrap()
            .width,
        Some(31.0)
    );
    assert!(
        saved_preferences.tables[0]
            .view_layout
            .column(later_address.column)
            .is_none()
    );
    assert!(app.workspace.documents[&document_id].is_dirty());
    assert!(
        !app.settings_file
            .documents
            .iter()
            .any(|entry| { entry.key.path == saved_key.path && entry.key != saved_key })
    );

    let mut saved_file_app = tests::new_app(&ctx);
    saved_file_app.settings_loaded = true;
    saved_file_app.settings_file = app.snapshot_settings();
    let saved_file_placeholder = saved_file_app.workspace.active_document.unwrap();
    saved_file_app.open_path(path);
    let saved_file_id = tests::confirm_pending_csv_settings_with_test_defaults(&mut saved_file_app);
    assert_eq!(saved_file_app.workspace.documents.len(), 1);
    assert!(
        !saved_file_app
            .workspace
            .documents
            .contains_key(&saved_file_placeholder)
    );
    assert_eq!(
        saved_file_app.workspace.active_document,
        Some(saved_file_id)
    );
    let reopened =
        &saved_file_app.workspace.documents[&saved_file_app.workspace.active_document.unwrap()];
    let reopened_view = reopened.view(table_id).unwrap();
    assert_eq!(
        reopened_view
            .layout
            .column(moved_address.column)
            .unwrap()
            .width,
        Some(31.0)
    );
    assert!(reopened_view.layout.column(later_address.column).is_none());
    assert_ne!(
        app.file_backings[&document_id].identity, original_backing.identity,
        "saving a structural version establishes a new content identity"
    );
}

// Verify that dirty value edits still snapshot view and calculation preferences.
#[test]
fn dirty_value_edits_keep_document_preferences_in_the_settings_snapshot() {
    let directory = TestDirectory::new();
    let path = directory.0.join("dirty-values.csv");
    let ctx = egui::Context::default();
    let (mut app, document_id, table_id) =
        tests::load_csv_test_app_after_confirming_default_settings(
            &ctx,
            path,
            b"name,value\nfirst,1\n",
        );
    let backing = app.file_backings[&document_id].clone();
    let string_address = CellAddress { row: 1, column: 1 };
    {
        let document = app.workspace.documents.get_mut(&document_id).unwrap();
        document
            .set_string_cell_local(table_id, string_address, true)
            .expect("mark the local string cell");
        document
            .set_cell(
                table_id,
                CellAddress { row: 1, column: 0 },
                Some(Cell::text("edited")),
            )
            .expect("edit a source value");
        document.display_mode = crate::document::DocumentDisplayMode::Source;
        let view = document.view_mut(table_id).unwrap();
        let mut layout = TableLayout::default();
        layout.apply_value(
            LayoutTarget::Column(0),
            LayoutValue::Column(Some(ColumnLayout {
                width: Some(37.0),
                ..Default::default()
            })),
        );
        view.layout = Arc::new(layout);
    }
    for field in [
        TablePreferenceField::DisplayMode,
        TablePreferenceField::ViewLayout,
    ] {
        app.mark_document_preference_change(document_id, table_id, field);
    }
    let document = &app.workspace.documents[&document_id];
    assert!(document.is_dirty());
    assert_eq!(
        document.structure_revision,
        document.saved_structure_revision
    );

    let snapshot = app.snapshot_settings();
    let preferences = snapshot
        .document(&document_key(
            &app.user_file_paths[&document_id],
            &backing.identity,
        ))
        .expect("dirty value edits keep local preferences");
    let table = &preferences.tables[0];
    assert_eq!(
        preferences.display_mode,
        crate::document::DocumentDisplayMode::Source
    );
    assert_eq!(table.view_layout.column(0).unwrap().width, Some(37.0));
}

// Verifies that reopening a file keeps one document and saving updates its preferences.
#[test]
fn reopening_the_same_path_keeps_one_set_of_saved_preferences() {
    let directory = TestDirectory::new();
    let path = directory.0.join("same-path.csv");
    let ctx = egui::Context::default();
    let (mut app, first_document_id, _) =
        tests::load_csv_test_app_after_confirming_default_settings(
            &ctx,
            path.clone(),
            b"name,value\nfirst,1\n",
        );
    let original_backing = app.file_backings[&first_document_id].clone();
    let old_tables = snapshot_table_preferences(&app.workspace.documents[&first_document_id]);
    app.settings_file.upsert_document(DocumentPreferences {
        key: document_key(&path, &original_backing.identity),
        display_mode: crate::document::DocumentDisplayMode::Source,
        tables: old_tables,
    });

    app.open_path(path.clone());
    tests::wait_for_condition(&mut app, |app| app.pending_csv_opens.is_empty());
    assert_eq!(app.workspace.active_document, Some(first_document_id));
    assert_eq!(app.file_backings.len(), 1);
    let table_id = app.workspace.documents[&first_document_id].active_table;
    {
        let document = app.workspace.documents.get_mut(&first_document_id).unwrap();
        document
            .set_cell(
                table_id,
                CellAddress { row: 1, column: 0 },
                Some(Cell::text("saved after reopening")),
            )
            .expect("edit the existing document before saving");
        document.display_mode = crate::document::DocumentDisplayMode::Live;
    }
    app.mark_document_preference_change(
        first_document_id,
        table_id,
        TablePreferenceField::DisplayMode,
    );
    app.workspace.active_document = Some(first_document_id);
    app.save_active(false);
    tests::wait_for_condition(&mut app, |app| {
        !app.pending_saves.contains_key(&first_document_id)
    });
    let saved_backing = &app.file_backings[&first_document_id];
    let saved_key = document_key(&path, &saved_backing.identity);
    assert_ne!(saved_key, document_key(&path, &original_backing.identity));

    let snapshot = app.snapshot_settings();
    let saved_preferences = snapshot
        .document(&saved_key)
        .expect("the newest identity keeps its saved preferences");
    assert_eq!(
        saved_preferences.display_mode,
        crate::document::DocumentDisplayMode::Live
    );
    assert_eq!(
        snapshot
            .documents
            .iter()
            .filter(|entry| entry.key.path == normalize_settings_path(&path))
            .count(),
        1
    );
}

// Verify that settings arriving late do not clear cells already marked as kept text,
// so a text result such as "=A1" is still saved with its literal marker.
#[test]
fn late_settings_restore_keeps_text_marked_cells() {
    let directory = TestDirectory::new();
    let path = directory.0.join("late-string-cell.csv");
    let ctx = egui::Context::default();
    let (mut app, document_id, table_id) =
        tests::load_csv_test_app_after_confirming_default_settings(
            &ctx,
            path,
            b"name,value\nfirst,1\n",
        );
    let backing = app.file_backings[&document_id].clone();
    let address = CellAddress { row: 1, column: 1 };
    app.workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .set_string_cell_local(table_id, address, true)
        .expect("mark the cell as kept text");
    let tables = snapshot_table_preferences(&app.workspace.documents[&document_id]);
    app.settings_file.upsert_document(DocumentPreferences {
        key: document_key(&app.user_file_paths[&document_id], &backing.identity),
        display_mode: crate::document::DocumentDisplayMode::Live,
        tables,
    });
    app.settings_applied_documents.remove(&document_id);

    app.restore_document_preferences(document_id);

    assert!(app.workspace.documents[&document_id].tables[0].is_string_cell(address));
}

// Verify that late preferences restore table-wide fields without applying stale coordinates.
#[test]
fn late_settings_restore_skips_coordinates_after_a_structure_edit() {
    let directory = TestDirectory::new();
    let path = directory.0.join("late-settings.csv");
    let ctx = egui::Context::default();
    let (mut app, document_id, table_id) =
        tests::load_csv_test_app_after_confirming_default_settings(
            &ctx,
            path,
            b"name,value\nfirst,1\n",
        );
    let backing = app.file_backings[&document_id].clone();
    let stale_address = CellAddress { row: 1, column: 0 };
    let mut stale_table =
        snapshot_table_preferences(&app.workspace.documents[&document_id]).remove(0);
    stale_table.wrap_mode = WrapMode::On;
    let mut saved_layout = TableLayout::default();
    saved_layout.apply_value(
        LayoutTarget::Column(0),
        LayoutValue::Column(Some(ColumnLayout {
            width: Some(41.0),
            ..Default::default()
        })),
    );
    stale_table.view_layout = Arc::new(saved_layout);
    app.settings_file.upsert_document(DocumentPreferences {
        key: document_key(&app.user_file_paths[&document_id], &backing.identity),
        display_mode: crate::document::DocumentDisplayMode::Live,
        tables: vec![stale_table],
    });
    app.settings_applied_documents.remove(&document_id);
    app.workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .display_mode = crate::document::DocumentDisplayMode::Source;
    app.mark_document_preference_change(document_id, table_id, TablePreferenceField::DisplayMode);

    app.request_structure_action(StructureAction::InsertRows);
    tests::wait_for_condition(&mut app, |app| {
        !app.pending_structures.contains_key(&document_id)
    });
    assert_ne!(
        app.workspace.documents[&document_id].structure_revision,
        app.workspace.documents[&document_id].saved_structure_revision
    );

    app.restore_document_preferences(document_id);
    let inserted_revision = {
        let document = &app.workspace.documents[&document_id];
        assert!(!document.tables[0].is_string_cell(stale_address));
        let view = document.view(table_id).unwrap();
        assert_eq!(
            document.display_mode,
            crate::document::DocumentDisplayMode::Source
        );
        assert!(!view.wrap_text);
        assert!(
            view.layout
                .column(0)
                .is_none_or(|column| column.width != Some(41.0))
        );
        document.structure_revision
    };
    app.apply_undo(false);
    tests::wait_for_condition(&mut app, |app| {
        app.workspace.documents[&document_id].structure_revision > inserted_revision
    });
    let undone_revision = app.workspace.documents[&document_id].structure_revision;
    app.apply_undo(true);
    tests::wait_for_condition(&mut app, |app| {
        app.workspace.documents[&document_id].structure_revision > undone_revision
    });
    assert_eq!(
        app.workspace.documents[&document_id].display_mode,
        crate::document::DocumentDisplayMode::Source,
        "restoring or undoing structure does not replace document display mode"
    );
}
