use super::*;
use egui_kittest::{
    Harness,
    kittest::{By, NodeT, Queryable},
};
use std::path::PathBuf;

/// Builds the platform command modifiers used by keyboard integration checks.
fn command_modifiers(is_mac: bool, shift: bool) -> Modifiers {
    let mut modifiers = Modifiers::COMMAND;
    if is_mac {
        modifiers.mac_cmd = true;
    } else {
        modifiers.ctrl = true;
    }
    modifiers.shift = shift;
    modifiers
}

/// Builds one key press with an exact modifier snapshot.
fn key_event(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// Sends one shortcut while holding its modifiers through the frame.
fn send_shortcut(harness: &Harness<'_, TexcelApp>, key: Key, shift: bool) {
    let modifiers = command_modifiers(harness.ctx.os().is_mac(), shift);
    harness.event_modifiers(key_event(key, modifiers), modifiers);
}

#[test]
// Verify text inputs and modal dialogs keep priority over app shortcuts.
fn text_input_and_modal_keep_priority_over_app_shortcuts() {
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    harness.step();
    let input = harness
        .query_all(By::new().role(egui::Role::TextInput))
        .next()
        .expect("the header exposes a text input");
    input.focus();
    harness.step();
    let document_count = harness.state().workspace.order.len();

    send_shortcut(&harness, Key::N, false);
    harness.step();
    assert_eq!(harness.state().workspace.order.len(), document_count);
}

#[test]
// Verify Cmd/Ctrl+F opens the existing search dialog.
fn search_shortcut_uses_the_existing_search_action() {
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    harness.step();
    harness
        .query_all(By::new().role(egui::Role::Button))
        .next()
        .expect("a header button can hold focus")
        .focus();
    harness.step();

    send_shortcut(&harness, Key::F, false);
    harness.step();
    assert!(harness.state().search_dialog.is_some());
    harness.event(Event::Text("needle".to_owned()));
    harness.step();
    let active_id = harness.state().workspace.active_document().unwrap().id;
    assert_eq!(
        harness.state().search_dialog.as_ref().unwrap().query,
        "needle"
    );
    assert_eq!(
        harness.ctx.memory(|memory| memory.focused()),
        Some(Id::unique(egui::IdSalt::new(("search_query", active_id))))
    );
}

#[test]
// Keeps a modal search field from leaking a click through to the grid.
fn search_modal_blocks_background_menu_clicks_without_losing_text() {
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    crate::ui::top::force_shared_menu_bar_for_test(&harness.ctx);
    let (document_id, table_id, expected_cell, active_cell) = {
        let document = harness.state().workspace.active_document().unwrap();
        let table_id = document.active_table().unwrap().id;
        let expected_cell = document.tables[0]
            .cell(CellAddress::default())
            .map(|cell| cell.input.clone());
        let active_cell = document.view(table_id).unwrap().active_cell();
        (document.id, table_id, expected_cell, active_cell)
    };
    harness.step();
    harness
        .query_all(By::new().role(egui::Role::Button))
        .next()
        .expect("a header button can hold shortcut focus")
        .focus();
    harness.step();
    send_shortcut(&harness, Key::F, false);
    harness.step();
    assert!(harness.state().search_dialog.is_some());
    harness.event(Event::Text("scratch".to_owned()));
    harness.step();
    assert_eq!(
        harness.state().search_dialog.as_ref().unwrap().query,
        "scratch"
    );
    let search_id = Id::unique(egui::IdSalt::new(("search_query", document_id)));
    assert_eq!(
        harness.ctx.memory(|memory| memory.focused()),
        Some(search_id)
    );
    let document_revision = harness.state().workspace.documents[&document_id].revision;
    assert!(!harness.state().workspace.documents[&document_id].can_undo());

    let edit_label = crate::i18n::context_text(&harness.ctx, "menu-edit");
    let edit_rect = harness
        .query_all(
            By::new()
                .role(egui::Role::Button)
                .label_contains(&edit_label),
        )
        .next()
        .expect("the Edit menu is visible")
        .rect();
    super::scenario_tests::click_point(&mut harness, edit_rect.center());
    assert_eq!(
        harness.state().search_dialog.as_ref().unwrap().query,
        "scratch",
        "the modal keeps its text when the background menu is blocked"
    );
    assert_eq!(
        harness.state().workspace.documents[&document_id].revision,
        document_revision
    );
    assert!(!harness.state().workspace.documents[&document_id].can_undo());
    assert_eq!(
        harness.state().workspace.documents[&document_id].tables[0]
            .cell(CellAddress::default())
            .map(|cell| cell.input.clone()),
        expected_cell
    );
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .active_table()
            .unwrap()
            .id,
        table_id
    );
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .view(table_id)
            .unwrap()
            .active_cell(),
        active_cell
    );
}

#[test]
// Sends Edit-menu Undo to a real cell editor without changing the stored cell.
fn edit_menu_undo_targets_the_focused_cell_editor() {
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    crate::ui::top::force_shared_menu_bar_for_test(&harness.ctx);
    harness.step();
    super::scenario_tests::click_cell(&mut harness, "A1");
    let document_id = harness.state().workspace.active_document().unwrap().id;
    let table_id = harness.state().workspace.documents[&document_id]
        .active_table()
        .unwrap()
        .id;
    super::scenario_tests::send_event(&mut harness, Event::Text("scratch".to_owned()));
    assert!(harness.state().edit.is_editing());
    assert_eq!(harness.state().edit.buffer, "scratch");
    super::scenario_tests::send_event(&mut harness, key_event(Key::Enter, Modifiers::NONE));
    super::scenario_tests::wait_until(&mut harness, |app| app.pending_structures.is_empty());
    let document_revision = harness.state().workspace.documents[&document_id].revision;
    let original_cell =
        super::scenario_tests::cell_text(harness.state(), document_id, CellAddress::default());
    assert_eq!(original_cell.as_deref(), Some("scratch"));
    super::scenario_tests::click_cell(&mut harness, "A1");
    super::scenario_tests::send_event(&mut harness, key_event(Key::F2, Modifiers::NONE));
    super::scenario_tests::send_event(&mut harness, Event::Text("!".to_owned()));
    assert_eq!(harness.state().edit.buffer, "scratch!");
    harness.step();
    let editor_id = editor_id(document_id, harness.state().edit.session_id);
    assert_eq!(
        harness.state().command_input_owner(&harness.ctx, false),
        (crate::ui::commands::InputOwner::CellEditor, Some(editor_id))
    );

    let edit_label = crate::i18n::context_text(&harness.ctx, "menu-edit");
    let edit_rect = harness
        .query_all(
            By::new()
                .role(egui::Role::Button)
                .label_contains(&edit_label),
        )
        .next()
        .expect("the Edit menu is visible")
        .rect();
    let edit_point = edit_rect.center();
    super::scenario_tests::send_batch(&mut harness, [Event::PointerMoved(edit_point)]);
    super::scenario_tests::send_batch(
        &mut harness,
        [Event::PointerButton {
            pos: edit_point,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }],
    );
    assert_eq!(
        harness.ctx.memory(|memory| memory.focused()),
        Some(editor_id),
        "Edit pointer-down keeps editor focus"
    );
    super::scenario_tests::send_batch(
        &mut harness,
        [Event::PointerButton {
            pos: edit_point,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
    );
    assert!(
        egui::Popup::is_any_open(&harness.ctx),
        "the Edit click leaves its popup open"
    );
    assert_eq!(
        harness.state().command_input_owner(&harness.ctx, false),
        (crate::ui::commands::InputOwner::CellEditor, Some(editor_id)),
        "opening Edit keeps the real cell editor as the text owner"
    );
    super::scenario_tests::send_batch(&mut harness, []);
    assert!(
        egui::Popup::is_any_open(&harness.ctx),
        "the Edit popup survives its initial sizing frame"
    );

    let undo_label = crate::i18n::context_text(&harness.ctx, "top-undo");
    let undo_rect = harness
        .query_all(By::new().label_contains(&undo_label))
        .find(|node| !node.accesskit_node().is_disabled() && has_menu_popup_ancestor(node))
        .unwrap_or_else(|| {
            let candidates = harness
                .query_all(By::new().label_contains(&undo_label))
                .map(|node| {
                    (
                        node.value(),
                        node.accesskit_node().role(),
                        menu_ancestor_roles(&node),
                        node.rect(),
                    )
                })
                .collect::<Vec<_>>();
            panic!("the Edit popup exposes Undo; candidates: {candidates:?}");
        })
        .rect();
    super::scenario_tests::click_point(&mut harness, undo_rect.center());
    super::scenario_tests::send_batch(&mut harness, []);

    assert_eq!(harness.state().edit.buffer, "scratch");
    assert_eq!(
        harness.ctx.memory(|memory| memory.focused()),
        Some(editor_id)
    );
    assert_eq!(
        harness.state().workspace.documents[&document_id].revision,
        document_revision
    );
    assert!(harness.state().workspace.documents[&document_id].can_undo());
    assert_eq!(
        super::scenario_tests::cell_text(harness.state(), document_id, CellAddress::default()),
        original_cell
    );
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .active_table()
            .unwrap()
            .id,
        table_id
    );

    send_shortcut(&harness, Key::A, false);
    harness.step();
    assert_eq!(harness.state().edit.anchor_byte, 0);
    assert_eq!(harness.state().edit.cursor_byte, "scratch".len());
    let edit_label = crate::i18n::context_text(&harness.ctx, "menu-edit");
    let edit_rect = harness
        .query_all(By::new().role(egui::Role::Button).label(&edit_label))
        .next()
        .expect("the Edit menu is visible")
        .rect();
    super::scenario_tests::click_point(&mut harness, edit_rect.center());
    super::scenario_tests::send_batch(&mut harness, []);
    let copy_label = crate::i18n::context_text(&harness.ctx, "action-copy");
    let copy_rect = harness
        .query_all(By::new().label_contains(&copy_label))
        .find(has_menu_popup_ancestor)
        .expect("the shared Edit popup exposes its Copy leaf")
        .rect();
    super::scenario_tests::click_point(&mut harness, copy_rect.center());
    super::scenario_tests::send_batch(&mut harness, []);
    assert!(
        harness
            .output()
            .platform_output
            .commands
            .iter()
            .any(|command| command == &egui::OutputCommand::CopyText("scratch".to_owned())),
        "the editor's selected draft reaches the shared Edit Copy leaf"
    );
    assert_eq!(harness.state().edit.buffer, "scratch");
    assert_eq!(
        harness.state().workspace.documents[&document_id].revision,
        document_revision
    );
}

#[test]
// Keeps Edit-menu Undo in the real name box instead of document history.
fn edit_menu_undo_targets_the_focused_name_box() {
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    crate::ui::top::force_shared_menu_bar_for_test(&harness.ctx);
    harness.step();
    super::scenario_tests::click_cell(&mut harness, "A1");
    super::scenario_tests::send_event(&mut harness, Event::Text("kept".to_owned()));
    super::scenario_tests::send_event(&mut harness, key_event(Key::Enter, Modifiers::NONE));
    super::scenario_tests::wait_until(&mut harness, |app| app.pending_structures.is_empty());

    let document_id = harness.state().workspace.active_document().unwrap().id;
    let table_id = harness.state().workspace.documents[&document_id]
        .active_table()
        .unwrap()
        .id;
    let revision = harness.state().workspace.documents[&document_id].revision;
    let name_box_id = crate::ui::name_box_id(document_id);
    let name_box = harness
        .query_all(By::new().role(egui::Role::TextInput))
        .find(|node| node.value().as_deref() == Some("A2"))
        .expect("the selected cell address appears in the name box");
    let rect = name_box.rect();
    super::scenario_tests::click_point(
        &mut harness,
        Pos2::new(rect.right() - 3.0, rect.center().y),
    );
    super::scenario_tests::send_event(&mut harness, key_event(Key::End, Modifiers::NONE));
    super::scenario_tests::send_event(&mut harness, Event::Text("B2".to_owned()));
    assert_eq!(harness.state().name_box, "A2B2");
    assert_eq!(
        harness.ctx.memory(|memory| memory.focused()),
        Some(name_box_id)
    );
    assert_eq!(
        harness.state().command_input_owner(&harness.ctx, false),
        (
            crate::ui::commands::InputOwner::OtherText,
            Some(name_box_id)
        )
    );

    let edit_label = crate::i18n::context_text(&harness.ctx, "menu-edit");
    let edit_rect = harness
        .query_all(By::new().role(egui::Role::Button).label(&edit_label))
        .next()
        .expect("the Edit menu is visible")
        .rect();
    super::scenario_tests::click_point(&mut harness, edit_rect.center());
    assert!(
        egui::Popup::is_any_open(&harness.ctx),
        "the Edit click leaves its popup open"
    );
    assert_eq!(
        harness.state().command_input_owner(&harness.ctx, false),
        (
            crate::ui::commands::InputOwner::OtherText,
            Some(name_box_id)
        ),
        "the open Edit menu keeps the real name box as its text owner"
    );
    super::scenario_tests::send_batch(&mut harness, []);
    assert!(
        egui::Popup::is_any_open(&harness.ctx),
        "the Edit popup survives its initial sizing frame"
    );
    assert_eq!(
        harness.state().name_box,
        "A2B2",
        "the open menu keeps the actual name box draft"
    );
    let undo_label = crate::i18n::context_text(&harness.ctx, "top-undo");
    let undo_rect = harness
        .query_all(By::new().label_contains(&undo_label))
        .find(|node| !node.accesskit_node().is_disabled() && has_menu_popup_ancestor(node))
        .unwrap_or_else(|| {
            let candidates = harness
                .query_all(By::new().label(&undo_label))
                .map(|node| {
                    (
                        node.value(),
                        node.accesskit_node().role(),
                        menu_ancestor_roles(&node),
                        node.rect(),
                    )
                })
                .collect::<Vec<_>>();
            panic!("the Edit popup exposes Undo; candidates: {candidates:?}");
        })
        .rect();
    super::scenario_tests::click_point(&mut harness, undo_rect.center());
    super::scenario_tests::send_batch(&mut harness, []);

    assert_eq!(harness.state().name_box, "A2");
    assert_eq!(
        harness.ctx.memory(|memory| memory.focused()),
        Some(name_box_id)
    );
    let document = &harness.state().workspace.documents[&document_id];
    assert_eq!(document.revision, revision);
    assert!(document.can_undo());
    assert_eq!(document.active_table().unwrap().id, table_id);
    assert_eq!(
        super::scenario_tests::cell_text(harness.state(), document_id, CellAddress::default())
            .as_deref(),
        Some("kept")
    );

    super::scenario_tests::send_event(&mut harness, key_event(Key::End, Modifiers::NONE));
    super::scenario_tests::send_event(&mut harness, Event::Text("B2".to_owned()));
    assert_eq!(harness.state().name_box, "A2B2");
    send_shortcut(&harness, Key::A, false);
    harness.step();
    assert!(
        egui::TextEdit::load_state(&harness.ctx, name_box_id)
            .and_then(|state| state.cursor.char_range())
            .is_some_and(|range| !range.is_empty())
    );
    let edit_label = crate::i18n::context_text(&harness.ctx, "menu-edit");
    let edit_rect = harness
        .query_all(By::new().role(egui::Role::Button).label(&edit_label))
        .next()
        .expect("the Edit menu is visible")
        .rect();
    super::scenario_tests::click_point(&mut harness, edit_rect.center());
    super::scenario_tests::send_batch(&mut harness, []);
    assert_eq!(harness.state().name_box, "A2B2");
    let copy_label = crate::i18n::context_text(&harness.ctx, "action-copy");
    let copy_rect = harness
        .query_all(By::new().label_contains(&copy_label))
        .find(has_menu_popup_ancestor)
        .expect("the shared Edit popup exposes its Copy leaf")
        .rect();
    super::scenario_tests::click_point(&mut harness, copy_rect.center());
    super::scenario_tests::send_batch(&mut harness, []);
    assert!(
        harness
            .output()
            .platform_output
            .commands
            .iter()
            .any(|command| command == &egui::OutputCommand::CopyText("A2B2".to_owned())),
        "the Copy leaf sends the selected name box text to the clipboard; commands: {:?}",
        harness.output().platform_output.commands,
    );
    assert_eq!(
        harness.state().workspace.documents[&document_id].revision,
        revision
    );

    let edit_label = crate::i18n::context_text(&harness.ctx, "menu-edit");
    let edit_rect = harness
        .query_all(By::new().role(egui::Role::Button).label(&edit_label))
        .next()
        .expect("the Edit menu is visible")
        .rect();
    super::scenario_tests::click_point(&mut harness, edit_rect.center());
    super::scenario_tests::send_batch(&mut harness, []);
    super::scenario_tests::send_event(&mut harness, key_event(Key::Escape, Modifiers::NONE));
    super::scenario_tests::send_batch(&mut harness, []);
    assert_eq!(harness.state().name_box, "A2B2");
    assert_eq!(
        harness.ctx.memory(|memory| memory.focused()),
        Some(name_box_id),
        "closing Edit with Escape returns focus to the name box"
    );
}

#[test]
// Keeps a normal focused grid from closing the File menu during its first layout pass.
fn shared_menu_stays_open_from_a_focused_grid() {
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    crate::ui::top::force_shared_menu_bar_for_test(&harness.ctx);
    harness.step();
    super::scenario_tests::click_cell(&mut harness, "A1");
    assert!(harness.state().grid_focused);
    assert!(!harness.ctx.text_edit_focused());

    let file_label = crate::i18n::context_text(&harness.ctx, "menu-file");
    let file_rect = harness
        .query_all(By::new().role(egui::Role::Button).label(&file_label))
        .next()
        .expect("the File menu is visible")
        .rect();
    super::scenario_tests::click_point(&mut harness, file_rect.center());
    assert!(egui::Popup::is_any_open(&harness.ctx));
    super::scenario_tests::send_batch(&mut harness, []);
    assert!(egui::Popup::is_any_open(&harness.ctx));

    let quit_label = crate::i18n::context_text(&harness.ctx, "menu-file-quit");
    let quit_is_visible = harness
        .query_all(By::new().label_contains(&quit_label))
        .any(|node| has_menu_popup_ancestor(&node));
    assert!(
        quit_is_visible,
        "File popup open={}, nodes={:?}",
        egui::Popup::is_any_open(&harness.ctx),
        popup_node_descriptions(&harness)
    );
}

/// Confirms a menu action belongs to the rendered popup tree, not another header control.
fn has_menu_popup_ancestor<'tree>(node: &impl NodeT<'tree>) -> bool {
    menu_ancestor_roles(node).contains(&egui::Role::Menu)
}

/// Shows which accessible container owns a popup action during test failures.
fn menu_ancestor_roles<'tree>(node: &impl NodeT<'tree>) -> Vec<egui::Role> {
    let mut roles = Vec::new();
    let mut ancestor = node.parent();
    while let Some(parent) = ancestor {
        roles.push(parent.accesskit_node().role());
        ancestor = parent.parent();
    }
    roles
}

/// Lists rendered popup descendants when a menu leaf is unexpectedly missing.
fn popup_node_descriptions(harness: &Harness<'_, TexcelApp>) -> Vec<String> {
    harness
        .query_all(By::new().role(egui::Role::Menu))
        .flat_map(|menu| menu.children_recursive())
        .map(|node| {
            let access = node.accesskit_node();
            format!(
                "{:?} label={:?} value={:?} rect={:?}",
                access.role(),
                access.label(),
                access.value(),
                node.rect()
            )
        })
        .collect()
}

#[test]
// Verify a same-frame cell edit finishes before its format shortcut runs.
fn format_shortcut_keeps_text_typed_in_the_same_frame() {
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let (document_id, table_id, session_id) = {
        let app = harness.state_mut();
        let table_id = app
            .workspace
            .active_document()
            .and_then(|document| document.active_table())
            .unwrap()
            .id;
        app.begin_cell_edit(table_id, CellAddress::default());
        app.edit.buffer = "draft".to_owned();
        app.edit.cursor_byte = app.edit.buffer.len();
        app.edit.anchor_byte = app.edit.buffer.len();
        (app.edit_document_id.unwrap(), table_id, app.edit.session_id)
    };
    harness.step();
    let editor = editor_id(document_id, session_id);
    harness
        .ctx
        .memory_mut(|memory| memory.request_focus(editor));
    harness.step();
    harness.event(Event::Text(" tail".to_owned()));
    send_shortcut(&harness, Key::B, false);
    harness.step();

    assert_eq!(harness.state().edit.buffer, "**draft tail**");
    assert_eq!(harness.state().edit.table_id, Some(table_id));
}

#[test]
// Verify Ctrl/Cmd+N creates a clean Markdown document immediately.
fn new_document_shortcut_uses_the_existing_menu_action() {
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let initial_count = harness.state().workspace.order.len();
    harness.step();
    harness
        .query_all(By::new().role(egui::Role::Button))
        .next()
        .expect("a header button can hold focus")
        .focus();
    harness.step();

    send_shortcut(&harness, Key::N, false);
    harness.step();
    let app = harness.state();
    assert_eq!(app.workspace.order.len(), initial_count + 1);
    let document = app
        .workspace
        .active_document()
        .expect("new document is active");
    assert!(matches!(
        document.source.as_deref(),
        Some(SourceDocument::Markdown(_))
    ));
    assert!(!document.is_dirty());
}

#[test]
// Verify the shortcut frame commits cell text before creating the next document.
fn new_shortcut_keeps_same_frame_editor_text_out_of_the_form() {
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let (document_id, session_id) = {
        let app = harness.state_mut();
        let document = app.workspace.active_document().unwrap();
        let (document_id, table_id) = (document.id, document.active_table().unwrap().id);
        app.begin_cell_edit(table_id, CellAddress::default());
        app.edit.buffer = "draft".to_owned();
        app.edit.cursor_byte = app.edit.buffer.len();
        app.edit.anchor_byte = app.edit.buffer.len();
        (document_id, app.edit.session_id)
    };
    harness.step();
    let edit_id = editor_id(document_id, session_id);
    harness
        .ctx
        .memory_mut(|memory| memory.request_focus(edit_id));
    harness.step();

    let modifiers = command_modifiers(harness.ctx.os().is_mac(), false);
    harness.event(Event::Text(" tail".to_owned()));
    harness.event(Event::ModifiersChanged(modifiers));
    harness.event(key_event(Key::N, modifiers));
    harness.step();

    let app = harness.state();
    assert!(!app.edit.is_editing());
    assert_eq!(
        app.workspace.documents[&document_id].tables[0]
            .cell(CellAddress::default())
            .map(|cell| &cell.input),
        Some(&CellInput::Text("draft tail".to_owned()))
    );
    assert!(matches!(
        app.workspace.active_document().unwrap().source.as_deref(),
        Some(SourceDocument::Markdown(_))
    ));
}

#[test]
// Verify Search waits for the active cell editor before opening its query field.
fn search_shortcut_keeps_same_frame_cell_text_in_the_editor() {
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let (document_id, session_id, table_id) = {
        let app = harness.state_mut();
        let document = app.workspace.active_document().unwrap();
        let (document_id, table_id) = (document.id, document.active_table().unwrap().id);
        app.begin_cell_edit(table_id, CellAddress::default());
        app.edit.buffer = "find draft".to_owned();
        app.edit.cursor_byte = app.edit.buffer.len();
        app.edit.anchor_byte = app.edit.buffer.len();
        (document_id, app.edit.session_id, table_id)
    };
    harness.step();
    let edit_id = editor_id(document_id, session_id);
    harness
        .ctx
        .memory_mut(|memory| memory.request_focus(edit_id));
    harness.step();

    let modifiers = command_modifiers(harness.ctx.os().is_mac(), false);
    harness.event(Event::Text(" tail".to_owned()));
    harness.event(Event::ModifiersChanged(modifiers));
    harness.event(key_event(Key::F, modifiers));
    harness.step();

    assert!(!harness.state().edit.is_editing());
    assert_eq!(
        harness.state().workspace.documents[&document_id].tables[0]
            .cell(CellAddress::default())
            .unwrap()
            .input,
        CellInput::Text("find draft tail".to_owned())
    );
    assert!(harness.state().search_dialog.is_some());
    assert_eq!(harness.state().search_dialog.as_ref().unwrap().query, "");
    harness.event(Event::Text("query".to_owned()));
    harness.step();
    assert_eq!(
        harness.state().search_dialog.as_ref().unwrap().query,
        "query"
    );
    assert_eq!(
        harness.state().workspace.documents[&document_id].tables[0]
            .cell(CellAddress::default())
            .unwrap()
            .input,
        CellInput::Text("find draft tail".to_owned())
    );
    assert_eq!(
        harness.state().workspace.documents[&document_id].tables[0].id,
        table_id
    );
}

#[test]
// Verify a visible CSV choice prompt blocks app-wide shortcuts behind it.
fn csv_choice_prompt_blocks_close_shortcut() {
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let active_id = harness.state().workspace.active_document().unwrap().id;
    harness.state_mut().pending_csv_opens.insert(
        active_id,
        PendingCsvOpen {
            path: PathBuf::from("pending.csv"),
            encoding: EncodingChoice::Auto,
            delimiter: None,
            prompt: Some(CsvLoadPrompt::Settings(CsvLoadSettingsPrompt {
                candidates: Vec::new(),
                delimiter_text: ",".to_owned(),
                previews: BTreeMap::new(),
                selected_encoding: EncodingChoice::Auto,
                needs_encoding: true,
                error: None,
            })),
            job_id: 1,
            retry_submission: None,
        },
    );
    harness.step();

    send_shortcut(&harness, Key::W, false);
    harness.step();

    assert!(harness.state().pending_close_document.is_none());
    assert!(
        harness.state().pending_csv_opens[&active_id]
            .prompt
            .is_some()
    );
}

#[test]
// Verify an active IME composition prevents an app shortcut in the same frame.
fn ime_preedit_keeps_app_shortcut_ownership() {
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    harness.step();
    harness
        .query_all(By::new().role(egui::Role::GridCell))
        .next()
        .expect("the grid exposes a selectable cell")
        .click();
    harness.step();
    assert!(harness.state().grid_focused);
    harness.event(key_event(Key::F2, Modifiers::NONE));
    harness.step();
    assert!(harness.state().edit.is_editing());
    harness.event(Event::Ime(egui::ImeEvent::Preedit {
        text: "にほん".to_owned(),
        active_range_chars: None,
    }));
    send_shortcut(&harness, Key::N, false);
    harness.step();

    assert!(harness.state().edit.composing);
}

#[test]
// Verify Ctrl/Cmd+Shift+N stays assigned to adding a table.
fn shifted_new_shortcut_keeps_the_existing_add_table_action() {
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let document_id = {
        let app = harness.state_mut();
        app.create_new_document("Table shortcut".to_owned())
            .expect("create a Markdown document");
        let document_id = app.workspace.active_document().unwrap().id;
        app.grid_focused = true;
        document_id
    };
    harness.step();
    let modifiers = command_modifiers(harness.ctx.os().is_mac(), true);
    harness.event(Event::ModifiersChanged(modifiers));
    harness.event(key_event(Key::N, modifiers));
    harness.step();

    let app = harness.state();
    assert!(app.pending_structures.contains_key(&document_id));
}

#[test]
// Verify Ctrl/Cmd+Shift+R and +C insert a row above and a column left of the selection.
fn shifted_r_and_c_insert_rows_and_columns() {
    for (key, moved_to) in [
        (Key::R, CellAddress { row: 1, column: 0 }),
        (Key::C, CellAddress { row: 0, column: 1 }),
    ] {
        let mut harness = Harness::builder()
            .with_size([900.0, 620.0])
            .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
        let (document_id, table_id) = {
            let app = harness.state_mut();
            app.create_new_document("Insert shortcut".to_owned())
                .expect("create a Markdown document");
            let document = app.workspace.active_document_mut().unwrap();
            let table_id = document.active_table;
            document
                .set_cell(table_id, CellAddress::default(), Some(Cell::text("x")))
                .expect("A1 accepts text");
            app.grid_focused = true;
            (document.id, table_id)
        };
        harness.step();
        let modifiers = command_modifiers(harness.ctx.os().is_mac(), true);
        harness.event(Event::ModifiersChanged(modifiers));
        harness.event(key_event(key, modifiers));
        harness.step();
        for _ in 0..200 {
            if !harness
                .state()
                .pending_structures
                .contains_key(&document_id)
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
            harness.step();
        }

        let document = &harness.state().workspace.documents[&document_id];
        let table = document.tables.iter().find(|table| table.id == table_id);
        let text_at = |address| {
            table
                .and_then(|table| table.cell(address))
                .map(|cell| cell.input.source_text())
                .unwrap_or_default()
        };
        assert_eq!(text_at(moved_to), "x", "{key:?} inserts before A1");
        assert_eq!(text_at(CellAddress::default()), "");
    }
}
