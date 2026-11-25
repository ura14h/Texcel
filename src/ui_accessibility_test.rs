use super::*;
// Verify that zoom follows the live focused widget and still works from the grid.
#[test]
fn zoom_shortcuts_follow_grid_focus_and_leave_text_inputs_untouched() {
    use egui_kittest::{
        Harness,
        kittest::{By, Queryable},
    };

    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    harness.step();

    harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("A1"))
        .next()
        .expect("A1 is exposed as a grid cell")
        .click();
    harness.step();
    let grid_owner = harness.ctx.memory(|memory| memory.focused());
    assert!(harness.state().grid_focused);
    assert_eq!(harness.state().grid_keyboard_owner, grid_owner);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Plus);
    harness.step();
    assert!((harness.state().sheet_zoom.0 - 1.1).abs() < 0.001);

    let document_id = harness.state().workspace.active_document.unwrap();
    let name_box_id = Id::unique(egui::IdSalt::new(("name_box", document_id)));
    harness
        .ctx
        .memory_mut(|memory| memory.request_focus(name_box_id));
    harness.step();
    let zoom_before_text_shortcut = harness.state().sheet_zoom;
    harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Plus);
    harness.step();
    assert_eq!(harness.state().sheet_zoom, zoom_before_text_shortcut);
}

// Verify that ScrollIntoView preserves selection and Focus waits through IME composition.
#[test]
fn accesskit_scroll_preserves_selection_and_focus_waits_for_ime() {
    use egui_kittest::{
        Harness,
        kittest::{By, NodeT, Queryable},
    };

    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let (document_id, table_id) = {
        let app = harness.state_mut();
        let document_id = app.workspace.active_document.unwrap();
        app.clear_calculation_jobs_for_reset(document_id);
        let mut source_text = String::from("| key | value |\n| --- | --- |\n");
        for row in 1..=200 {
            source_text.push_str(&format!("| row {row} | B{row} |\n"));
        }
        let source = Arc::new(
            crate::markdown_adapter::MarkdownDocument::parse(source_text.into_bytes())
                .expect("large Markdown fixture parses"),
        );
        let mut document = source.to_document(document_id, "large.md");
        let table_id = document.tables[0].id;
        document.set_source(Some(Arc::new(crate::source::SourceDocument::Markdown(
            source,
        ))));
        app.workspace.documents.insert(document_id, document);
        (document_id, table_id)
    };
    harness
        .state_mut()
        .workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .view_mut(table_id)
        .unwrap()
        .scroll
        .y = 199.0 * ROW_HEIGHT;
    harness.step();
    {
        let target = harness
            .query_all(By::new().role(egui::Role::GridCell).label_contains("B200"))
            .next()
            .expect("B200 is initially visible to accessibility clients");
        let label = target.accesskit_node().label().expect("cell has a label");
        assert!(!label.contains('\u{2068}') && !label.contains('\u{2069}'));
        target.scroll_to_me();
    }
    let target_address = CellAddress {
        row: 199,
        column: 1,
    };
    {
        let view = harness
            .state_mut()
            .workspace
            .documents
            .get_mut(&document_id)
            .unwrap()
            .view_mut(table_id)
            .unwrap();
        view.set_selection(CellRange::single(CellAddress::default()));
        view.scroll.y = 0.0;
    }
    for _ in 0..6 {
        harness.step();
    }
    {
        let view = harness.state().workspace.documents[&document_id]
            .view(table_id)
            .unwrap();
        assert_eq!(view.selection, CellRange::single(CellAddress::default()));
        assert!(view.scroll.y > 150.0 * ROW_HEIGHT);
    }

    let editor_address = CellAddress::default();
    {
        let app = harness.state_mut();
        let view = app
            .workspace
            .documents
            .get_mut(&document_id)
            .unwrap()
            .view_mut(table_id)
            .unwrap();
        view.set_selection(CellRange::single(editor_address));
        view.scroll.y = 0.0;
        app.begin_cell_edit(table_id, editor_address);
    }
    harness.step();
    let editor_id = editor_id(document_id, harness.state().edit.session_id);
    assert!(harness.state().edit.is_editing());
    harness.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "にほん".to_owned(),
        active_range_chars: None,
    }));
    harness.step();
    assert!(harness.state().edit.composing);
    assert!(harness.ctx.memory(|memory| memory.has_focus(editor_id)));

    let table_token = harness.state().accesskit_table_tokens[&(document_id, table_id)];
    let target_node = grid_accesskit_cell_id(table_token, target_address, 0)
        .expect("B200 has a stable accessibility node")
        .accesskit_id();
    harness.event(egui::Event::AccessKitActionRequest(
        egui::accesskit::ActionRequest {
            action: egui::accesskit::Action::ScrollIntoView,
            target_tree: egui::accesskit::TreeId::ROOT,
            target_node,
            data: None,
        },
    ));
    harness.step();
    assert!(harness.state().request_scroll_to_address.is_none());
    assert!(harness.state().edit.is_editing());
    assert!(harness.ctx.memory(|memory| memory.has_focus(editor_id)));
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .view(table_id)
            .unwrap()
            .selection,
        CellRange::single(editor_address)
    );
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .view(table_id)
            .unwrap()
            .scroll
            .y,
        0.0
    );

    harness.event(egui::Event::AccessKitActionRequest(
        egui::accesskit::ActionRequest {
            action: egui::accesskit::Action::Focus,
            target_tree: egui::accesskit::TreeId::ROOT,
            target_node,
            data: None,
        },
    ));
    harness.step();
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .view(table_id)
            .unwrap()
            .selection,
        CellRange::single(editor_address)
    );
    assert_eq!(
        harness.state().pending_accesskit_focus,
        Some((document_id, table_id, target_address))
    );
    assert!(harness.state().edit.is_editing());
    assert!(harness.ctx.memory(|memory| memory.has_focus(editor_id)));

    harness.event(egui::Event::Ime(egui::ImeEvent::Commit("日本".to_owned())));
    harness.step();
    assert!(!harness.state().edit.composing);
    assert!(harness.state().edit.is_editing());
    assert!(harness.state().pending_accesskit_focus.is_some());
    assert!(harness.ctx.memory(|memory| memory.has_focus(editor_id)));
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .view(table_id)
            .unwrap()
            .selection,
        CellRange::single(editor_address)
    );

    harness.step();
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .view(table_id)
            .unwrap()
            .selection,
        CellRange::single(target_address)
    );
    assert!(!harness.state().edit.is_editing());
    assert!(
        harness.state().workspace.documents[&document_id].tables[0]
            .cell(editor_address)
            .unwrap()
            .input
            .source_text()
            .contains("日本")
    );
    harness.run_steps(6);

    {
        let target = harness
            .query_all(By::new().role(egui::Role::GridCell).label_contains("B200"))
            .next()
            .expect("the selected cell is exposed after deferred focus");
        target.focus();
    }
    {
        let app = harness.state_mut();
        app.workspace
            .documents
            .get_mut(&document_id)
            .unwrap()
            .view_mut(table_id)
            .unwrap()
            .set_selection(CellRange::single(CellAddress::default()));
        app.active_import_report = Some((
            document_id,
            Arc::new(crate::conversion_report::ImportReport::new(
                "file.csv",
                chrono::Utc::now(),
                "CSV",
                Vec::new(),
            )),
        ));
    }
    harness.step();
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .view(table_id)
            .unwrap()
            .selection,
        CellRange::single(CellAddress::default())
    );
    assert!(harness.state().pending_accesskit_focus.is_none());
}

// Verify that a focused remote cell commits an active draft to its original cell once.
#[test]
fn accesskit_focus_commits_an_active_edit_before_selecting_another_cell() {
    use egui_kittest::{
        Harness,
        kittest::{By, Queryable},
    };

    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    harness.step();
    let (document_id, table_id, target) = {
        let app = harness.state_mut();
        let document = app.workspace.active_document_mut().unwrap();
        let table_id = document.active_table;
        let source_address = CellAddress::default();
        document.tables[0].set_loaded_cell(source_address, Cell::text("source"));
        document
            .view_mut(table_id)
            .unwrap()
            .set_selection(CellRange::single(source_address));
        app.edit.begin(
            table_id,
            source_address,
            CellInput::Text("source".to_owned()),
            crate::document::EditTarget::Cell,
        );
        app.edit.buffer = "uncommitted draft".to_owned();
        app.edit_document_id = Some(document.id);
        (document.id, table_id, CellAddress { row: 1, column: 1 })
    };
    {
        let target_node = harness
            .query_all(By::new().role(egui::Role::GridCell).label_contains("B2"))
            .next()
            .expect("B2 is exposed to accessibility clients");
        target_node.focus();
    }
    harness.step();

    let document = &harness.state().workspace.documents[&document_id];
    assert!(!harness.state().edit.is_editing());
    assert_eq!(
        document.tables[0]
            .cell(CellAddress::default())
            .map(|cell| &cell.input),
        Some(&CellInput::Text("uncommitted draft".to_owned()))
    );
    assert_eq!(
        document.view(table_id).unwrap().selection,
        CellRange::single(target)
    );
}
