use super::*;
use egui_kittest::{
    Harness,
    kittest::{By, Queryable},
};
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

/// Shows the selected cell address in the name box on the first frame after a selection change.
#[test]
fn name_box_refreshes_before_the_text_edit_is_painted() {
    let mut harness = Harness::builder()
        .with_size([760.0, 500.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let (document_id, table_id) = {
        let app = harness.state_mut();
        let document = app.workspace.active_document().unwrap();
        (document.id, document.active_table().unwrap().id)
    };
    harness
        .state_mut()
        .workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .view_mut(table_id)
        .unwrap()
        .set_selection(CellRange::single(CellAddress { row: 1, column: 1 }));

    harness.step();

    let values = harness
        .query_all(By::new().role(egui::Role::TextInput))
        .filter_map(|node| node.value())
        .collect::<Vec<_>>();
    assert!(values.iter().any(|value| value == "B2"));
}

/// Keeps localized status messages visible at the left side of the footer row.
#[test]
fn status_messages_keep_their_meaning_and_priority() {
    let mut harness = Harness::builder()
        .with_size([760.0, 500.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    harness.state_mut().language = crate::i18n::Language::Japanese;
    harness.state_mut().status = Some(crate::i18n::Message::new("status-save-conflict"));
    harness.step();

    let warning = harness
        .query_all(By::new().label_contains("ファイルが外部で変更された"))
        .next()
        .expect("the warning is shown in the status row");
    assert!(
        warning.rect().min.x <= harness.ctx.content_rect().min.x + 24.0,
        "the status begins at the left edge of its reserved region"
    );

    let detail = "保存に失敗しました。もう一度お試しください。".repeat(12);
    harness.state_mut().status = Some(
        crate::i18n::Message::new("status-operation-failed-detail").with_text("detail", detail),
    );
    harness.step();
    let failure = harness
        .query_all(By::new().label_contains("保存に失敗しました"))
        .next()
        .expect("failure status remains readable at the start of the row");
    assert!(failure.rect().min.x <= harness.ctx.content_rect().min.x + 24.0);
    assert!(failure.rect().max.x <= harness.ctx.content_rect().max.x);
}

/// Keeps the window title's unsaved mark as the only visible save-state indicator.
#[test]
fn footer_does_not_repeat_pending_or_completed_save_state() {
    let mut harness = Harness::builder()
        .with_size([760.0, 500.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let document_id = harness
        .state()
        .workspace
        .active_document()
        .expect("a document is active")
        .id;
    harness.state_mut().language = crate::i18n::Language::English;
    harness.state_mut().pending_saves.insert(
        document_id,
        crate::file_worker::JobToken {
            document_id,
            revision: 0,
            job_id: 1,
        },
    );
    harness.step();
    assert!(
        harness
            .query_all(By::new().label_contains("Saving"))
            .next()
            .is_none(),
        "pending saves do not add a second status indicator"
    );
    assert!(
        harness
            .query_all(By::new().label_contains("Saved."))
            .next()
            .is_none(),
        "save completion is represented by the document window title"
    );
}

/// Keeps the sheet picker and mode switch within reach beside a long sheet name.
#[test]
fn sheet_controls_stay_inside_a_narrow_window() {
    let mut harness = Harness::builder()
        .with_size([760.0, 500.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let app = harness.state_mut();
    app.language = crate::i18n::Language::Japanese;
    let document = app.workspace.active_document_mut().unwrap();
    document.tables[0].name = "Long sheet name ".repeat(12);
    app.multi_window_mode = true;
    harness.step();
    harness.step();

    let viewport = harness.ctx.content_rect();
    for key in ["dialog-table-list", "view-source"] {
        let label = crate::i18n::context_text(&harness.ctx, key);
        let node = harness
            .query_all(By::new().role(egui::Role::Button).label_contains(&label))
            .next()
            .unwrap_or_else(|| panic!("the {label} control remains reachable"));
        assert!(
            viewport.contains_rect(node.rect()),
            "{label}: {:?} within {viewport:?}",
            node.rect()
        );
    }
}

/// Reserves the detail button width while the formula editor is open.
#[test]
fn detail_button_stays_visible_while_editing_long_source() {
    let mut harness = Harness::builder()
        .with_size([760.0, 500.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let (document_id, table_id) = {
        let app = harness.state_mut();
        let document = app.workspace.active_document().unwrap();
        (document.id, document.active_table().unwrap().id)
    };
    let ctx = harness.ctx.clone();
    {
        let app = harness.state_mut();
        let document = app.workspace.documents.get_mut(&document_id).unwrap();
        let address = CellAddress { row: 0, column: 0 };
        document
            .set_cell(
                table_id,
                address,
                Some(Cell::text("long-text-".repeat(300))),
            )
            .unwrap();
        document
            .view_mut(table_id)
            .unwrap()
            .set_selection(CellRange::single(address));
        app.ui_font_size = 21.0;
        app.apply_appearance(&ctx);
    }
    harness.step();
    let preview = harness
        .query_all(
            By::new()
                .role(egui::Role::Button)
                .label_contains("long-text"),
        )
        .next()
        .expect("the source preview can start editing");
    preview.click();
    harness.step();
    assert_eq!(
        harness.state().edit.target,
        Some(crate::document::EditTarget::FormulaBar)
    );

    let detail = harness
        .query_all(By::new().role(egui::Role::Button).label_contains("長文"))
        .next()
        .expect("the detail control remains visible while editing");
    assert!(detail.rect().max.x <= harness.ctx.content_rect().max.x);
}

/// Prevents a blocked formula preview from starting an edit session.
#[test]
fn blocked_formula_preview_cannot_start_editing() {
    let mut harness = Harness::builder()
        .with_size([760.0, 500.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let (document_id, table_id) = {
        let app = harness.state_mut();
        let document = app.workspace.active_document().unwrap();
        (document.id, document.active_table().unwrap().id)
    };
    {
        let app = harness.state_mut();
        app.workspace
            .documents
            .get_mut(&document_id)
            .unwrap()
            .set_cell(
                table_id,
                CellAddress { row: 0, column: 0 },
                Some(Cell::text("guarded-preview")),
            )
            .unwrap();
        app.close_after_save.insert(document_id);
    }
    harness.step();

    let preview = harness
        .query_all(
            By::new()
                .role(egui::Role::Button)
                .label_contains("guarded-preview"),
        )
        .next()
        .expect("the blocked preview remains represented in the accessibility tree");
    preview.click();
    harness.step();

    assert!(!harness.state().edit.is_editing());
}

/// Refreshes only the main-window images affected by the tab layout adjustment.
#[test]
fn refreshes_main_window_light_and_dark_renders() {
    let settings_directory = TestDirectory::new();
    let settings_path = settings_directory.0.join("settings.json");
    let mut harness = Harness::builder()
        .with_size([1000.0, 760.0])
        .with_max_steps(10)
        .build_eframe(move |creation| {
            TexcelApp::new_with_settings_path(creation, Some(settings_path))
        });
    let render_directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("kittest-renders");
    fs::create_dir_all(&render_directory).expect("create render directory");

    for (theme, name) in [
        (ThemeChoice::Light, "main-light.png"),
        (ThemeChoice::Dark, "main-dark.png"),
    ] {
        let ctx = harness.ctx.clone();
        let app = harness.state_mut();
        app.theme = theme;
        app.apply_appearance(&ctx);
        harness.step();
        let rendered = catch_unwind(AssertUnwindSafe(|| harness.render()))
            .expect("kittest renderer is available")
            .expect("kittest frame renders");
        rendered
            .save(render_directory.join(name))
            .expect("save main window render");
    }
}
