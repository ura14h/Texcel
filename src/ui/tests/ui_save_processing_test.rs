use super::*;
use std::fs;
use std::thread;
use std::time::{Duration, Instant};

#[path = "ui_save_processing_history_test.rs"]
mod history_and_source_tests;

#[test]
// Keep the document and close prompt when a save ends as cancelled.
fn cancelled_save_keeps_edit_undo_and_pending_close() {
    let directory = TestDirectory::new();
    let path = directory.0.join("cancelled.csv");
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let (document_id, table_id) = new_populated_csv_document(&mut app, "Keep on cancel");
    app.begin_cell_edit(table_id, CellAddress::default());
    app.edit.buffer = "keep draft".to_owned();
    app.edit.cursor_byte = app.edit.buffer.len();
    app.edit.anchor_byte = app.edit.buffer.len();
    let edit_before_save = app.edit.clone();
    let edit_revision = app.workspace.documents[&document_id].revision;
    app.close_after_save.insert(document_id);
    app.close_after_save_edit
        .insert(document_id, edit_before_save.clone());
    app.save_document_to_path(document_id, path.clone());
    let token = app.pending_saves[&document_id];

    app.request_cancel_save(document_id);

    assert_eq!(app.pending_saves.get(&document_id), Some(&token));
    assert_eq!(app.cancelling_saves.get(&document_id), Some(&token));
    assert!(app.close_after_save.contains(&document_id));
    app.deferred_file_results
        .push(crate::file_worker::FileResult {
            token,
            operation: crate::file_worker::FileOperation::Save,
            path,
            kind: crate::file_worker::FileResultKind::Cancelled,
        });
    app.process_file_results();

    let document = &app.workspace.documents[&document_id];
    assert!(document.revision > edit_revision);
    assert!(document.is_dirty());
    assert!(document.can_undo());
    assert_eq!(
        document.tables[0]
            .cell(CellAddress::default())
            .unwrap()
            .input,
        CellInput::Text("keep draft".to_owned())
    );
    assert!(app.workspace.documents.contains_key(&document_id));
    assert_eq!(app.pending_close_document, Some(document_id));
    assert!(!app.close_after_save.contains(&document_id));
    assert!(app.edit.is_editing());
    assert_eq!(app.edit.buffer, edit_before_save.buffer);
    assert_eq!(app.status, Some(Message::new("status-save-cancelled")));
    assert!(!app.pending_saves.contains_key(&document_id));
    assert!(!app.cancelling_saves.contains_key(&document_id));
}

#[test]
// Accept a save acknowledgement if replacement finished before cancellation arrived.
fn save_ack_after_cancel_request_still_marks_the_saved_revision() {
    let directory = TestDirectory::new();
    let path = directory.0.join("completed-before-cancel.csv");
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let (document_id, table_id) = new_populated_csv_document(&mut app, "Late acknowledgement");
    app.begin_cell_edit(table_id, CellAddress::default());
    app.edit.buffer = "finished write".to_owned();
    app.edit.cursor_byte = app.edit.buffer.len();
    app.edit.anchor_byte = app.edit.buffer.len();
    app.save_document_to_path(document_id, path.clone());
    let token = app.pending_saves[&document_id];
    wait_for_saved_bytes(&path, "finished write");

    app.request_cancel_save(document_id);

    assert_eq!(app.pending_saves.get(&document_id), Some(&token));
    assert_eq!(app.cancelling_saves.get(&document_id), Some(&token));
    let result = wait_for_file_result(&app, token);
    assert!(matches!(
        &result.kind,
        crate::file_worker::FileResultKind::Saved { .. }
    ));
    app.deferred_file_results.push(result);
    app.process_file_results();

    assert!(!app.pending_saves.contains_key(&document_id));
    assert!(!app.cancelling_saves.contains_key(&document_id));
    assert!(!app.workspace.documents[&document_id].is_dirty());
    assert_eq!(
        app.workspace.documents[&document_id].tables[0]
            .cell(CellAddress::default())
            .unwrap()
            .input,
        CellInput::Text("finished write".to_owned())
    );
    assert!(fs::read_to_string(path).unwrap().contains("finished write"));
}

#[test]
// Save acknowledgements keep the history state captured before later edits.
fn save_ack_tracks_its_undo_checkpoint_after_the_document_changes() {
    let directory = TestDirectory::new();
    let path = directory.0.join("saved-checkpoint.csv");
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let (document_id, table_id) = new_populated_csv_document(&mut app, "Save checkpoint");
    let address = CellAddress::default();
    app.workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .set_cell(
            table_id,
            address,
            Some(crate::document::Cell::text("saved")),
        )
        .expect("the first edit applies to the loaded CSV cell");

    app.save_document_to_path(document_id, path.clone());
    let token = app.pending_saves[&document_id];
    let saved_state = app.pending_save_history_states[&document_id].1;
    assert_eq!(
        saved_state,
        app.workspace.documents[&document_id].history_state_id()
    );
    app.workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .set_cell(
            table_id,
            address,
            Some(crate::document::Cell::text("later")),
        )
        .expect("editing may continue while the save worker runs");

    let result = wait_for_file_result(&app, token);
    assert!(matches!(
        result.kind,
        crate::file_worker::FileResultKind::Saved { .. }
    ));
    app.deferred_file_results.push(result);
    app.process_file_results();
    assert!(app.workspace.documents[&document_id].is_dirty());
    app.workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .undo()
        .unwrap();

    let document = &app.workspace.documents[&document_id];
    assert!(!document.is_dirty(), "undo returns to the version on disk");
    assert_eq!(
        document.tables[0].cell(address).unwrap().input,
        CellInput::Text("saved".to_owned())
    );
    let saved = crate::csv_adapter::CsvDocument::parse(
        fs::read(&path).expect("the worker wrote the captured version"),
        crate::csv_adapter::EncodingChoice::Auto,
        ',',
    )
    .expect("the saved CSV is valid");
    assert_eq!(saved.records[0].fields[0].value, "saved");
}

// Uses an existing CSV cell so cancellation begins at the file worker, not growth preparation.
fn new_populated_csv_document(app: &mut TexcelApp, name: &str) -> (DocumentId, u64) {
    let document_id = app.workspace.reserve_document_id();
    let source = crate::csv_adapter::CsvDocument::parse(
        b"base\n".to_vec(),
        crate::csv_adapter::EncodingChoice::Auto,
        ',',
    )
    .expect("one-cell CSV parses");
    let source = Arc::new(source);
    let mut document = source.to_document(document_id, name.to_owned());
    document.set_source(Some(Arc::new(SourceDocument::Csv(Arc::clone(&source)))));
    let table_id = document.active_table().unwrap().id;
    let calculation = CalculationSnapshot::from_document(&document, false);
    app.workspace.insert_loaded_document(document);
    let token = app.next_job_token(document_id, 0);
    app.initialize_calculation(token, calculation);
    (document_id, table_id)
}

// Waits until the atomic replacement makes the expected text visible.
fn wait_for_saved_bytes(path: &std::path::Path, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if fs::read_to_string(path).is_ok_and(|contents| contents.contains(expected)) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "save did not replace its destination"
        );
        thread::sleep(Duration::from_millis(2));
    }
}

// Waits for one terminal worker response without discarding its save token.
fn wait_for_file_result(app: &TexcelApp, token: JobToken) -> crate::file_worker::FileResult {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(result) = app
            .file_worker
            .poll()
            .into_iter()
            .find(|result| result.token == token)
        {
            return result;
        }
        assert!(Instant::now() < deadline, "save worker did not finish");
        thread::sleep(Duration::from_millis(2));
    }
}
