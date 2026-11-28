use super::*;

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
