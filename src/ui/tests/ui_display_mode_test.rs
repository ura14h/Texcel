use super::*;
use egui_kittest::{
    Harness,
    kittest::{By, NodeT, Queryable},
};

/// Checks the View menu applies one display mode to a whole document without editing content.
#[test]
fn view_menu_changes_document_mode_without_changing_history_or_dirty_state() {
    let mut harness = Harness::builder()
        .with_size([980.0, 680.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let document_id = harness
        .state_mut()
        .workspace
        .add_document("Mode sample", &["First", "Second"]);
    harness.state_mut().settings_window_open = false;
    harness.step();

    let before = {
        let document = &harness.state().workspace.documents[&document_id];
        assert_eq!(document.tables.len(), 2);
        (
            document.revision,
            document.saved_revision,
            document.can_undo(),
            document.is_dirty(),
        )
    };
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        assert!(
            !harness
                .state()
                .native_menu_states_for_frame(
                    crate::ui::top::TopActionState::default(),
                    false,
                    &harness.ctx,
                )
                .contains_key("view.current_table"),
            "the native View menu omits the old no-op row"
        );
        let target = harness.state().native_action_target(&harness.ctx, false);
        harness.state_mut().native_menu_events_for_test =
            vec![crate::native_menu::NativeMenuEvent {
                id: "view.toggle_mode".to_owned(),
                recent_path: None,
                target,
            }];
        harness.step();
        harness.step();
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let menu_label = crate::i18n::context_text(&harness.ctx, "menu-view");
        harness
            .query_all(By::new().role(egui::Role::Button).label(&menu_label))
            .next()
            .expect("the View menu is visible")
            .click_accesskit();
        harness.run_steps(2);
        let source_label = crate::i18n::context_text(&harness.ctx, "view-switch-to-source");
        harness
            .query_all(By::new().label(&source_label))
            .next()
            .expect("the View menu offers the switch to Source Editing")
            .click();
        harness.run_steps(2);
    }

    let document = &harness.state().workspace.documents[&document_id];
    assert_eq!(
        document.display_mode,
        crate::document::DocumentDisplayMode::Source
    );
    assert!(
        document
            .tables
            .iter()
            .all(|table| document.view(table.id).is_some())
    );
    assert_eq!(
        (
            document.revision,
            document.saved_revision,
            document.can_undo(),
            document.is_dirty(),
        ),
        before
    );
}

/// Checks the live/source bar and Ctrl/Cmd+/ share the document-wide display setting.
#[test]
fn mode_bar_and_shortcut_change_display_mode_without_editing_the_document() {
    let mut harness = Harness::builder()
        .with_size([980.0, 680.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let document_id = harness
        .state_mut()
        .workspace
        .add_document("Mode toggle sample", &["First", "Second"]);
    harness.step();
    let before = {
        let document = &harness.state().workspace.documents[&document_id];
        (
            document.revision,
            document.saved_revision,
            document.can_undo(),
            document.is_dirty(),
        )
    };

    let mode_switch = crate::i18n::context_text(&harness.ctx, "view-live");
    harness
        .query_all(By::new().role(egui::Role::Button).label(&mode_switch))
        .next()
        .expect("the switch beside the formula bar can switch to Source")
        .click();
    harness.step();
    assert_eq!(
        harness.state().workspace.documents[&document_id].display_mode,
        crate::document::DocumentDisplayMode::Source
    );

    let is_mac = harness.ctx.os().is_mac();
    let mut modifiers = egui::Modifiers::COMMAND;
    if is_mac {
        modifiers.mac_cmd = true;
    } else {
        modifiers.ctrl = true;
    }
    harness.event_modifiers(
        egui::Event::Key {
            key: egui::Key::Slash,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        },
        modifiers,
    );
    harness.step();

    let document = &harness.state().workspace.documents[&document_id];
    assert_eq!(
        document.display_mode,
        crate::document::DocumentDisplayMode::Live
    );
    assert_eq!(
        (
            document.revision,
            document.saved_revision,
            document.can_undo(),
            document.is_dirty(),
        ),
        before
    );
}

/// Checks Live and Source expose rendered values and the original Markdown in the real grid.
#[test]
fn live_and_source_accessibility_values_follow_markdown_content() {
    let mut harness = Harness::builder()
        .with_size([980.0, 680.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let document_id = harness
        .state_mut()
        .workspace
        .add_document("Markdown mode sample", &["Sheet1"]);
    let markdown = std::sync::Arc::new(
        crate::markdown_adapter::MarkdownDocument::parse(
            "| Text | Formula |\n| --- | --- |\n| **語** | =1+2 |\n"
                .as_bytes()
                .to_vec(),
        )
        .expect("the Markdown sample parses"),
    );
    let mut document = markdown.to_document(document_id, "Markdown mode sample");
    let table_id = document.tables[0].id;
    document.set_source(Some(std::sync::Arc::new(
        crate::source::SourceDocument::Markdown(std::sync::Arc::clone(&markdown)),
    )));
    let formula = CellAddress { row: 1, column: 1 };
    let calculation_revision = document.calculation_revision;
    assert!(document.apply_calculation_results(
        calculation_revision,
        [(
            table_id,
            formula,
            crate::document::CalcStatus::Fresh,
            Some(crate::document::CalcValue::Number(3.0)),
        )]
    ));
    harness
        .state_mut()
        .workspace
        .documents
        .insert(document_id, document);
    harness.step();

    let cell_value = |harness: &Harness<'_, TexcelApp>, coordinate: &str| {
        harness
            .query_all(
                By::new()
                    .role(egui::Role::GridCell)
                    .label_contains(coordinate),
            )
            .find_map(|node| node.accesskit_node().value())
            .unwrap_or_else(|| panic!("the {coordinate} cell is visible in AccessKit"))
    };
    assert_eq!(cell_value(&harness, "A2"), "語");
    assert_eq!(cell_value(&harness, "B2"), "3");

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let target = harness.state().native_action_target(&harness.ctx, false);
        harness.state_mut().native_menu_events_for_test =
            vec![crate::native_menu::NativeMenuEvent {
                id: "view.toggle_mode".to_owned(),
                recent_path: None,
                target,
            }];
        harness.step();
        harness.step();
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let menu_label = crate::i18n::context_text(&harness.ctx, "menu-view");
        harness
            .query_all(By::new().role(egui::Role::Button).label(&menu_label))
            .next()
            .expect("the View menu is visible")
            .click_accesskit();
        harness.run_steps(2);
        let source_label = crate::i18n::context_text(&harness.ctx, "view-switch-to-source");
        harness
            .query_all(By::new().label(&source_label))
            .next()
            .expect("the View menu offers the switch to Source Editing")
            .click();
        harness.run_steps(2);
    }

    assert_eq!(
        harness.state().workspace.documents[&document_id].display_mode,
        crate::document::DocumentDisplayMode::Source
    );
    // The menu item now names the switch back, which Cmd+/ performs next.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    assert_eq!(
        harness
            .state()
            .native_menu_states_for_frame(
                crate::ui::top::TopActionState::default(),
                false,
                &harness.ctx,
            )
            .get("view.toggle_mode")
            .and_then(|state| state.label.clone()),
        Some(crate::i18n::context_text(
            &harness.ctx,
            "view-switch-to-live"
        ))
    );
    assert_eq!(cell_value(&harness, "A2"), "**語**");
    assert_eq!(cell_value(&harness, "B2"), "=1+2");
}
