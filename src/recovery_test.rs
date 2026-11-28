use super::storage::{cleanup_saved, load_items, restore_item, save_snapshot};
use super::*;

// Verify that a durable sparse snapshot keeps its base until the saved version covers it.
#[test]
fn recovery_cleanup_keeps_edits_newer_than_the_successful_save() {
    let root = std::env::temp_dir().join(format!(
        "texcel-recovery-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let base = markdown_base("base");
    let snapshot = RecoverySnapshot {
        key: "1-a1".to_owned(),
        document_id: 1,
        name: "Book".to_owned(),
        original_path: None,
        original_identity: None,
        requires_initial_save: false,
        revision: 2,
        saved_revision: 1,
        generation: 2,
        structure_revision: 0,
        base,
        cells: vec![(1, CellAddress::default(), Some(Cell::text("edit")))],
        tables: markdown_base_tables(),
        text_overlay_revision: 0,
        text_overlay_cells: Arc::new(Vec::new()),
        cell_edit: None,
    };
    save_snapshot(&root, snapshot).unwrap();

    cleanup_saved(&root, "1-a1", 1, 1).unwrap();
    let (loaded, skipped) = load_items(&root).unwrap();
    assert_eq!(skipped, 0);
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].record.revision, 2);
    assert!(matches!(
        loaded[0].base,
        RecoveryBasePayload::Markdown { .. }
    ));

    cleanup_saved(&root, "1-a1", 2, 2).unwrap();
    assert!(load_items(&root).unwrap().0.is_empty());
    let _ = fs::remove_dir_all(root);
}

// Verify that a missing recovery directory loads as an empty startup result.
#[test]
fn recovery_worker_loads_missing_directory_as_empty() {
    let root = test_root("missing-directory");
    assert!(!root.exists());

    let worker = super::storage::RecoveryWorker::new(root.clone());
    let RecoveryEvent::Loaded(Ok((items, skipped))) = wait_for_recovery_event(&worker) else {
        panic!("missing recovery data returns an empty loaded result");
    };

    assert!(items.is_empty());
    assert_eq!(skipped, 0);
    assert!(!root.exists());
}

// Verify that an unfinished cell buffer remains separate from committed document data.
#[test]
fn recovery_restores_uncommitted_cell_input_without_committing_it() {
    let root = test_root("cell-edit");
    let base = markdown_base("old");
    let snapshot = RecoverySnapshot {
        key: "ce11ed17".to_owned(),
        document_id: 1,
        name: "Book".to_owned(),
        original_path: None,
        original_identity: None,
        requires_initial_save: false,
        revision: 0,
        saved_revision: 0,
        generation: 1,
        structure_revision: 0,
        base,
        cells: Vec::new(),
        tables: markdown_base_tables(),
        text_overlay_revision: 0,
        text_overlay_cells: Arc::new(Vec::new()),
        cell_edit: Some(RecoveryCellEdit {
            table_id: 1,
            address: CellAddress::default(),
            original: crate::document::CellInput::Text("old".to_owned()),
            buffer: "unfinished".to_owned(),
            cursor_byte: 4,
            anchor_byte: 2,
        }),
    };
    save_snapshot(&root, snapshot).unwrap();
    let item = load_items(&root).unwrap().0.pop().unwrap();
    let recovered = restore_item(item, 9, crate::i18n::Language::English).unwrap();
    assert_eq!(
        recovered.document.tables[0]
            .cell(CellAddress::default())
            .unwrap()
            .input,
        crate::document::CellInput::Text("old".to_owned())
    );
    let edit = recovered.cell_edit.unwrap();
    assert_eq!(edit.buffer, "unfinished");
    assert_eq!((edit.anchor_byte, edit.cursor_byte), (2, 4));
    let _ = fs::remove_dir_all(root);
}

#[test]
// Verify that a recovered Markdown table keeps its stable ID and text overlay.
fn recovery_keeps_markdown_table_identity_and_overlay() {
    let root = test_root("markdown-table-id");
    let source_text = "| Name |\n| --- |\n| A |\n";
    let source = Arc::new(MarkdownDocument::parse(source_text.as_bytes().to_vec()).unwrap());
    let source_document = SourceDocument::Markdown(Arc::clone(&source));
    let mut document = crate::structure::source_to_document(&source_document, 1, &[9]).unwrap();
    document.set_source(Some(Arc::new(source_document)));
    let overlay = Cell::text("overlay text");
    document.install_text_overlay_snapshot(crate::document::TextOverlaySnapshot {
        revision: 1,
        cells: Arc::new(vec![(9, CellAddress::default(), overlay)]),
    });
    let snapshot =
        RecoverySnapshot::from_document(&document, "9a9a".to_owned(), None, None, None, 1)
            .expect("snapshot is available");
    save_snapshot(&root, snapshot).unwrap();
    let item = load_items(&root).unwrap().0.pop().unwrap();
    let recovered = restore_item(item, 8, crate::i18n::Language::Japanese).unwrap();

    assert_eq!(recovered.document.tables[0].id, 9);
    assert_eq!(
        recovered.document.tables[0]
            .cell(CellAddress::default())
            .unwrap()
            .input,
        crate::document::CellInput::Text("overlay text".to_owned())
    );
    let _ = fs::remove_dir_all(root);
}

// Verify that a save acknowledgement cannot erase a newer buffer at the same revision.
#[test]
fn recovery_old_save_ack_keeps_new_same_revision_cell_buffer() {
    let root = test_root("save-generation");
    let worker = super::storage::RecoveryWorker::new(root.clone());
    let _ = wait_for_recovery_event(&worker);
    let old = cell_snapshot("5a9e", 4, 2, "old buffer");
    assert!(worker.save(old).is_ok());
    assert!(matches!(
        wait_for_recovery_event(&worker),
        RecoveryEvent::Saved { .. }
    ));

    let newer = cell_snapshot("5a9e", 4, 3, "new buffer");
    assert!(worker.save_and_cleanup(newer, 4, 2).is_ok());
    assert!(matches!(
        wait_for_recovery_event(&worker),
        RecoveryEvent::Saved { generation: 3, .. }
    ));
    let item = load_items(&root).unwrap().0.pop().unwrap();
    assert_eq!(item.record.revision, 4);
    assert_eq!(item.record.generation, 3);
    assert_eq!(item.record.cell_edit.unwrap().buffer, "new buffer");

    let after_ack = cell_snapshot("5a9e", 4, 4, "after acknowledgement");
    assert!(worker.save(after_ack).is_ok());
    assert!(matches!(
        wait_for_recovery_event(&worker),
        RecoveryEvent::Saved { generation: 4, .. }
    ));
    let item = load_items(&root).unwrap().0.pop().unwrap();
    assert_eq!(item.record.generation, 4);
    assert_eq!(
        item.record.cell_edit.unwrap().buffer,
        "after acknowledgement"
    );
    assert!(worker.cleanup("5a9e".to_owned(), 4, 4));
    assert!(matches!(
        wait_for_recovery_event(&worker),
        RecoveryEvent::Removed { .. }
    ));
    assert!(
        worker
            .save_and_cleanup(cell_snapshot("5a9e", 4, 3, "delayed old flush"), 2, 2)
            .is_ok()
    );
    assert!(matches!(
        wait_for_recovery_event(&worker),
        RecoveryEvent::Saved { generation: 3, .. }
    ));
    assert!(load_items(&root).unwrap().0.is_empty());
    drop(worker);
    let _ = fs::remove_dir_all(root);
}

#[test]
// Verify that a queued cleanup cannot remove a newer recovery generation.
fn recovery_delayed_cleanup_keeps_a_newer_snapshot() {
    let root = test_root("delayed-cleanup");
    let worker = super::storage::RecoveryWorker::new(root.clone());
    let _ = wait_for_recovery_event(&worker);
    assert!(
        worker
            .save(cell_snapshot("c1ea0001", 8, 1, "older"))
            .is_ok()
    );
    assert!(matches!(
        wait_for_recovery_event(&worker),
        RecoveryEvent::Saved { generation: 1, .. }
    ));

    assert!(
        worker
            .save(cell_snapshot("c1ea0001", 8, 2, "newer"))
            .is_ok()
    );
    assert!(worker.cleanup("c1ea0001".to_owned(), 8, 1));
    assert!(matches!(
        wait_for_recovery_event(&worker),
        RecoveryEvent::Saved { generation: 2, .. }
    ));
    assert!(matches!(
        wait_for_recovery_event(&worker),
        RecoveryEvent::Removed { .. }
    ));

    let (items, skipped) = load_items(&root).expect("newer snapshot remains readable");
    assert_eq!(skipped, 0);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].record.generation, 2);
    assert_eq!(items[0].record.cell_edit.as_ref().unwrap().buffer, "newer");
    drop(worker);
    let _ = fs::remove_dir_all(root);
}

// Verify that one corrupt base does not hide other recovery items.
#[test]
fn recovery_skips_one_corrupt_item_and_keeps_the_rest() {
    let root = test_root("corrupt-item");
    save_snapshot(&root, cell_snapshot("a00001", 2, 1, "bad"))
        .expect("first recovery item is written");
    save_snapshot(&root, cell_snapshot("b00001", 3, 1, "good"))
        .expect("second recovery item is written");
    let bad = load_items(&root)
        .unwrap()
        .0
        .into_iter()
        .find(|item| item.record.key == "a00001")
        .unwrap();
    fs::write(root.join(bad.record.base_file), b"damaged").unwrap();
    let (items, skipped) = load_items(&root).expect("recovery directory can be scanned");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].record.key, "b00001");
    assert_eq!(skipped, 1);
    let _ = fs::remove_dir_all(root);
}

// Build a recovery base whose one-column Markdown table has `text` in cell A1.
fn markdown_base(text: &str) -> RecoveryBase {
    let source = format!("| {text} |\n| --- |\n");
    RecoveryBase::Markdown(Arc::new(
        MarkdownDocument::parse(source.into_bytes()).expect("test Markdown parses"),
    ))
}

// Describe the one table of `markdown_base` as the recovery record stores it.
fn markdown_base_tables() -> Vec<RecoveryTableInfo> {
    vec![RecoveryTableInfo {
        id: 1,
        name: "Table1".to_owned(),
        rows: 1,
        columns: 1,
        string_cells: Vec::new(),
        header_row: Some(0),
        selection: crate::document::CellRange::default(),
        scroll: crate::document::ScrollPosition::default(),
    }]
}

// Build one unfinished cell edit for a chosen sparse snapshot generation.
fn cell_snapshot(key: &str, revision: u64, generation: u64, buffer: &str) -> RecoverySnapshot {
    let base = markdown_base("base");
    RecoverySnapshot {
        key: key.to_owned(),
        document_id: 1,
        name: "Book".to_owned(),
        original_path: None,
        original_identity: None,
        requires_initial_save: false,
        revision,
        saved_revision: revision,
        generation,
        structure_revision: 0,
        base,
        cells: Vec::new(),
        tables: markdown_base_tables(),
        text_overlay_revision: 0,
        text_overlay_cells: Arc::new(Vec::new()),
        cell_edit: Some(RecoveryCellEdit {
            table_id: 1,
            address: CellAddress::default(),
            original: crate::document::CellInput::Text("base".to_owned()),
            buffer: buffer.to_owned(),
            cursor_byte: buffer.len(),
            anchor_byte: buffer.len(),
        }),
    }
}

// Wait for one worker result so tests observe durable request order.
fn wait_for_recovery_event(worker: &super::storage::RecoveryWorker) -> RecoveryEvent {
    for _ in 0..400 {
        if let Ok(event) = worker.try_recv() {
            return event;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("recovery worker did not return an event");
}

// Make a private directory for one recovery worker test.
fn test_root(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "texcel-recovery-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
