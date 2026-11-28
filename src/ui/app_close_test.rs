use super::*;
use egui_kittest::{
    Harness,
    kittest::{By, Queryable},
};

#[test]
/// Lists only the documents with unsaved changes, under the same headed columns as the
/// import report.
fn quit_confirmation_lists_only_changed_documents() {
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    harness.state_mut().language = crate::i18n::Language::English;
    crate::i18n::set_context_language(&harness.ctx, crate::i18n::Language::English);
    {
        let app = harness.state_mut();
        app.workspace.add_document("Kept as saved", &["Sheet"]);
        let edited = app.workspace.add_document("Edited notes", &["Sheet"]);
        let document = app.workspace.documents.get_mut(&edited).unwrap();
        let table_id = document.tables[0].id;
        document
            .set_cell(table_id, CellAddress::default(), Some(Cell::text("x")))
            .expect("the cell accepts text");
        app.app_close_dialog_open = true;
    }
    harness.run_steps(2);
    let listed = |harness: &Harness<'_, TexcelApp>, name: &str| {
        harness
            .query_all(By::new().label_contains(name))
            .next()
            .is_some()
    };
    assert!(listed(&harness, "Edited notes Unsaved changes"));
    assert!(!listed(&harness, "Kept as saved"));
    assert!(listed(&harness, "Document"));
    assert!(listed(&harness, "Status"));
}
