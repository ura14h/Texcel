use super::*;

const OWNER: &str = "copy-1";

/// Builds an app whose clipboard mark covers A1 of the active table as a cut.
fn app_with_cut() -> TexcelApp {
    let ctx = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(ctx);
    let mut app = TexcelApp::new_with_settings_path(&creation, None);
    let (document_id, table_id) = active_ids(&app);
    let revision = app
        .workspace
        .active_document()
        .expect("a document")
        .revision;
    app.set_clipboard_mark(
        Some(OWNER.to_owned()),
        document_id,
        table_id,
        vec![CellRange::single(CellAddress::default())],
        revision,
    );
    app.mark_clipboard_as_cut();
    app
}

/// Returns the active document and table IDs.
fn active_ids(app: &TexcelApp) -> (DocumentId, TableId) {
    let document = app.workspace.active_document().expect("a document is open");
    (document.id, document.active_table().expect("a table").id)
}

#[test]
fn only_a_normal_paste_of_the_cut_data_moves_once() {
    let mut app = app_with_cut();
    let (document_id, table_id) = active_ids(&app);
    assert!(app.active_copy_marquee().is_some_and(|marquee| marquee.cut));
    let moved =
        app.cut_source_for_paste(Some(OWNER), document_id, table_id, PasteOptions::default());
    assert_eq!(moved, Some(CellRange::single(CellAddress::default())));
    // The cut is used up; the clipboard data now pastes as a copy.
    assert!(app.clipboard_mark.is_none());
    let again =
        app.cut_source_for_paste(Some(OWNER), document_id, table_id, PasteOptions::default());
    assert_eq!(again, None);
}

#[test]
fn special_pastes_and_foreign_clipboards_end_the_cut_without_moving() {
    let mut app = app_with_cut();
    let (document_id, table_id) = active_ids(&app);
    let values = PasteOptions {
        values_only: true,
        ..PasteOptions::default()
    };
    assert_eq!(
        app.cut_source_for_paste(Some(OWNER), document_id, table_id, values),
        None
    );
    assert!(app.clipboard_mark.is_none());

    let mut app = app_with_cut();
    let (document_id, table_id) = active_ids(&app);
    assert_eq!(
        app.cut_source_for_paste(None, document_id, table_id, PasteOptions::default()),
        None
    );
    assert!(app.clipboard_mark.is_none());
}

#[test]
fn editing_the_document_ends_the_outline_and_the_cut() {
    let mut app = app_with_cut();
    let (document_id, table_id) = active_ids(&app);
    app.workspace
        .active_document_mut()
        .expect("a document is open")
        .revision += 1;
    assert!(app.active_copy_marquee().is_none());
    assert_eq!(
        app.cut_source_for_paste(Some(OWNER), document_id, table_id, PasteOptions::default()),
        None
    );
    app.expire_clipboard_mark();
    assert!(app.clipboard_mark.is_none());
}
