use super::*;

/// Runs a small text widget through egui with the app's deferred text commands.
fn run_text_field(
    ctx: &egui::Context,
    app: &mut TexcelApp,
    field: &mut String,
    id: Id,
    events: Vec<egui::Event>,
) {
    let input = egui::RawInput {
        events,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        app.apply_deferred_text_commands(ui.ctx());
        ui.add(egui::TextEdit::singleline(field).id(id))
            .request_focus();
    });
    output.textures_delta.clear();
    app.last_text_input_focus = Some(id);
}

/// Keeps cell context actions disabled whenever their shared command cannot run.
#[test]
fn grid_context_commands_follow_save_bulk_and_editor_locks() {
    let ctx = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(ctx);
    let mut app = TexcelApp::new_with_settings_path(&creation, None);
    let document_id = app.workspace.active_document().unwrap().id;
    assert_eq!(
        app.grid_context_command_availability(false),
        [true, true, true, true, true, true]
    );

    app.pending_saves.insert(
        document_id,
        crate::file_worker::JobToken {
            document_id,
            revision: 0,
            job_id: 1,
        },
    );
    assert_eq!(
        app.grid_context_command_availability(false),
        [true, true, false, true, true, false]
    );
    app.pending_saves.clear();

    app.pending_structures.insert(
        document_id,
        crate::file_worker::JobToken {
            document_id,
            revision: 0,
            job_id: 2,
        },
    );
    assert_eq!(
        app.grid_context_command_availability(false),
        [false, true, false, false, false, false]
    );
    app.pending_structures.clear();

    app.begin_selected_edit();
    assert_eq!(
        app.grid_context_command_availability(false),
        [false, false, false, false, false, false]
    );
}

/// Confirms each paste request carries its own options and regular paste stays default.
#[test]
fn paste_options_are_scoped_to_one_request() {
    let special = AppCommand::PasteText {
        text: "a\tb".to_owned(),
        options: PasteOptions {
            transpose: true,
            values_only: true,
            skip_blank: true,
            ..PasteOptions::default()
        },
    };
    let regular = AppCommand::PasteText {
        text: "c\td".to_owned(),
        options: PasteOptions::default(),
    };
    let AppCommand::PasteText { options, .. } = special else {
        unreachable!();
    };
    assert_eq!(
        options,
        PasteOptions {
            transpose: true,
            values_only: true,
            skip_blank: true,
            ..PasteOptions::default()
        }
    );
    let AppCommand::PasteText { options, .. } = regular else {
        unreachable!();
    };
    assert_eq!(options, PasteOptions::default());
}

/// Verifies an Edit-menu Undo reaches its text field and leaves grid history alone.
#[test]
fn text_field_undo_uses_text_edit_history() {
    let ctx = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(ctx.clone());
    let mut app = TexcelApp::new_with_settings_path(&creation, None);
    let (document_id, table_id) = {
        let document = app.workspace.active_document_mut().unwrap();
        let table_id = document.active_table().unwrap().id;
        document
            .set_cell(table_id, CellAddress::default(), Some(Cell::text("grid")))
            .unwrap();
        (document.id, table_id)
    };
    let id = Id::unique("other_text_owner");
    let mut field = "base".to_owned();
    run_text_field(&ctx, &mut app, &mut field, id, Vec::new());
    run_text_field(
        &ctx,
        &mut app,
        &mut field,
        id,
        vec![egui::Event::Text("x".to_owned())],
    );
    assert_ne!(field, "base");

    app.dispatch_command(
        AppCommand::Undo,
        InputOwner::OtherText,
        false,
        Some(id),
        &ctx,
    );
    run_text_field(&ctx, &mut app, &mut field, id, Vec::new());

    assert_eq!(field, "base");
    assert_eq!(
        app.workspace.documents[&document_id].tables[0]
            .cell(CellAddress::default())
            .unwrap()
            .input,
        CellInput::Text("grid".to_owned())
    );
    assert_eq!(
        app.workspace.documents[&document_id]
            .active_table()
            .unwrap()
            .id,
        table_id
    );
}

#[test]
// Let current IME composition win over a delayed native paste command.
fn delayed_text_command_does_not_replace_current_ime_input() {
    let ctx = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(ctx.clone());
    let mut app = TexcelApp::new_with_settings_path(&creation, None);
    let field_id = Id::unique("ime_target");
    let mut field = "kept".to_owned();
    run_text_field(&ctx, &mut app, &mut field, field_id, Vec::new());
    app.dispatch_command(
        AppCommand::PasteText {
            text: "native paste".to_owned(),
            options: PasteOptions::default(),
        },
        InputOwner::OtherText,
        false,
        Some(field_id),
        &ctx,
    );
    assert_eq!(app.deferred_text_commands.len(), 1);

    run_text_field(
        &ctx,
        &mut app,
        &mut field,
        field_id,
        vec![egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "にほん".to_owned(),
            active_range_chars: None,
        })],
    );

    assert_eq!(field, "keptにほん");
    assert!(!field.contains("native paste"));
    assert!(app.deferred_text_commands.is_empty());
}

#[test]
// Discard delayed text commands after their active document changes.
fn delayed_text_command_cannot_follow_a_tab_switch() {
    let ctx = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(ctx.clone());
    let mut app = TexcelApp::new_with_settings_path(&creation, None);
    let field_id = Id::unique("stale_target");
    let mut field = "kept".to_owned();
    run_text_field(&ctx, &mut app, &mut field, field_id, Vec::new());
    app.dispatch_command(
        AppCommand::PasteText {
            text: "stale paste".to_owned(),
            options: PasteOptions::default(),
        },
        InputOwner::OtherText,
        false,
        Some(field_id),
        &ctx,
    );
    app.create_new_document("New document".to_owned())
        .expect("create another document");

    run_text_field(&ctx, &mut app, &mut field, field_id, Vec::new());

    assert_eq!(field, "kept");
    assert!(app.deferred_text_commands.is_empty());
}

/// Maps the held modifiers of a V chord to Paste, Paste as Text, or Paste Special.
#[test]
fn paste_chords_choose_text_or_special_paste() {
    use super::keyboard::{PasteShortcut, paste_shortcut_kind};
    let mut modifiers = egui::Modifiers::COMMAND;
    assert_eq!(paste_shortcut_kind(modifiers), PasteShortcut::Normal);
    modifiers.shift = true;
    assert_eq!(paste_shortcut_kind(modifiers), PasteShortcut::Text);
    let special = if cfg!(target_os = "macos") {
        egui::Modifiers::MAC_CMD | egui::Modifiers::CTRL
    } else {
        egui::Modifiers::CTRL | egui::Modifiers::ALT
    };
    assert_eq!(paste_shortcut_kind(special), PasteShortcut::Special);
}
