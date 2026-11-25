use super::*;

#[test]
// Keeps selection-local controls separate while both viewports share one workspace.
fn each_viewport_keeps_its_document_and_name_box_state() {
    let ctx = egui::Context::default();
    let mut app = crate::ui::tests::new_app(&ctx);
    let root_document = app.workspace.active_document.unwrap();
    let child_document = app.workspace.add_document("Child", &["Data"]);
    let child_viewport = egui::ViewportId::from_hash_of(("window-state-test", child_document));
    let original_active = app.workspace.active_document;
    app.multi_window_mode = true;
    app.enable_window_state(egui::ViewportId::ROOT);
    app.set_window_document(egui::ViewportId::ROOT, Some(root_document));
    app.set_window_document(child_viewport, Some(child_document));

    app.with_window_state(egui::ViewportId::ROOT, Some(root_document), |app| {
        assert_eq!(app.workspace.active_document, Some(root_document));
        app.name_box = "A1".to_owned();
        app.status = Some(Message::new("status-save-conflict"));
        app.set_sheet_zoom(2.0);
    });
    app.with_window_state(child_viewport, Some(child_document), |app| {
        assert_eq!(app.workspace.active_document, Some(child_document));
        app.name_box = "B2".to_owned();
        app.status = Some(Message::new("status-markdown-opened"));
    });

    assert_eq!(app.workspace.active_document, original_active);
    assert_eq!(
        app.window_document(egui::ViewportId::ROOT),
        Some(root_document)
    );
    assert_eq!(app.window_document(child_viewport), Some(child_document));
    assert_eq!(app.window_states[&egui::ViewportId::ROOT].name_box, "A1");
    assert_eq!(app.window_states[&child_viewport].name_box, "B2");
    assert_eq!(app.window_states[&egui::ViewportId::ROOT].sheet_zoom.0, 2.0);
    assert_eq!(
        app.window_states[&child_viewport].sheet_zoom.0, 1.0,
        "another window keeps its own sheet zoom"
    );
    assert_eq!(
        app.window_states[&egui::ViewportId::ROOT]
            .status
            .as_ref()
            .map(Message::key),
        Some("status-save-conflict")
    );
    assert_eq!(
        app.window_states[&child_viewport]
            .status
            .as_ref()
            .map(Message::key),
        Some("status-markdown-opened")
    );
}
