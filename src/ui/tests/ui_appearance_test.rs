use super::*;
use egui_kittest::{
    Harness,
    kittest::{By, Queryable},
};
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

/// Checks that app settings stay centered and fit within the current viewport.
#[test]
fn settings_page_renders_in_light_and_dark() {
    let settings_directory = TestDirectory::new();
    let settings_path = settings_directory.0.join("settings.json");
    let mut harness = Harness::builder()
        .with_size([1000.0, 760.0])
        .with_max_steps(10)
        .build_eframe(move |creation| {
            TexcelApp::new_with_settings_path(creation, Some(settings_path))
        });
    {
        let ctx = harness.ctx.clone();
        let app = harness.state_mut();
        app.ui_font_size = visual::BODY_SIZE;
        app.theme = ThemeChoice::Light;
        app.settings_window_open = true;
        app.apply_appearance(&ctx);
    }
    harness.step();
    harness.step();
    assert!(harness.state().settings_window_open);
    assert_settings_window_is_inside_viewport(&harness);
    set_theme(&mut harness, ThemeChoice::Dark);
    assert_eq!(harness.state().theme, ThemeChoice::Dark);
}

/// Saves the Settings page in both languages and themes for visual review.
#[test]
#[ignore = "needs an available GPU adapter for raster output"]
fn render_settings_language_appearance_matrix() {
    let render_directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("kittest-renders");
    fs::create_dir_all(&render_directory).expect("create render directory");
    for (language, choice, language_name) in [
        (
            crate::i18n::Language::English,
            crate::settings::LanguageChoice::English,
            "en",
        ),
        (
            crate::i18n::Language::Japanese,
            crate::settings::LanguageChoice::Japanese,
            "ja",
        ),
    ] {
        for (theme, theme_name) in [(ThemeChoice::Light, "light"), (ThemeChoice::Dark, "dark")] {
            let settings_directory = TestDirectory::new();
            let settings_path = settings_directory.0.join("settings.json");
            let mut harness = Harness::builder()
                .with_size([1000.0, 760.0])
                .with_max_steps(10)
                .build_eframe(move |creation| {
                    TexcelApp::new_with_settings_path(creation, Some(settings_path))
                });
            {
                let ctx = harness.ctx.clone();
                let app = harness.state_mut();
                app.language_choice = choice;
                app.language = language;
                app.theme = theme;
                app.settings_window_open = true;
                crate::i18n::set_context_language(&ctx, language);
                app.apply_appearance(&ctx);
            }
            harness.step();
            harness.step();
            clear_pointer_hover(&mut harness);
            save_render(
                &mut harness,
                render_directory.join(format!(
                    "settings-language-appearance-{language_name}-{theme_name}.png"
                )),
            );
        }
    }
}

/// Shows the first compact confirmation and input dialogs in both languages.
#[test]
#[ignore = "needs an available GPU adapter for raster output"]
fn render_modal_confirmation_and_input_representatives() {
    let render_directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("kittest-renders");
    fs::create_dir_all(&render_directory).expect("create render directory");
    for (language, language_name) in [
        (crate::i18n::Language::English, "en"),
        (crate::i18n::Language::Japanese, "ja"),
    ] {
        for (kind, kind_name) in [("confirmation", "app-close"), ("input", "layout")] {
            let settings_directory = TestDirectory::new();
            let settings_path = settings_directory.0.join("settings.json");
            let mut harness = Harness::builder()
                .with_size([900.0, 620.0])
                .with_max_steps(10)
                .build_eframe(move |creation| {
                    TexcelApp::new_with_settings_path(creation, Some(settings_path))
                });
            {
                let ctx = harness.ctx.clone();
                let app = harness.state_mut();
                app.language = language;
                app.language_choice = match language {
                    crate::i18n::Language::English => crate::settings::LanguageChoice::English,
                    crate::i18n::Language::Japanese => crate::settings::LanguageChoice::Japanese,
                };
                app.theme = ThemeChoice::Light;
                crate::i18n::set_context_language(&ctx, language);
                match kind {
                    "confirmation" => app.app_close_dialog_open = true,
                    "input" => {
                        let document = app
                            .workspace
                            .active_document()
                            .expect("the default document is active");
                        let table_id = document.active_table().expect("a table is active").id;
                        app.layout_dialog = Some(LayoutDialogState {
                            document_id: document.id,
                            table_id,
                            address: CellAddress { row: 1, column: 2 },
                            value: "28".to_owned(),
                            focus_input: false,
                        });
                    }
                    _ => unreachable!("only representative dialog kinds are listed"),
                }
                app.apply_appearance(&ctx);
            }
            harness.step();
            harness.step();
            clear_pointer_hover(&mut harness);
            save_render(
                &mut harness,
                render_directory.join(format!("dialog-{kind_name}-{language_name}-light.png")),
            );
        }
    }
}

/// Keeps app-wide settings independent of a long table name.
#[test]
fn long_table_name_does_not_enter_app_settings() {
    let settings_directory = TestDirectory::new();
    let settings_path = settings_directory.0.join("settings.json");
    let mut harness = Harness::builder()
        .with_size([760.0, 500.0])
        .with_max_steps(10)
        .build_eframe(move |creation| {
            TexcelApp::new_with_settings_path(creation, Some(settings_path))
        });
    {
        let app = harness.state_mut();
        let document = app
            .workspace
            .active_document_mut()
            .expect("the default document opens");
        document
            .active_table_mut()
            .expect("the default table opens")
            .name = "売上の長い表名".repeat(30);
    }
    harness.state_mut().settings_window_open = true;
    harness.step();
    // The first pass lets egui measure the modal before its settled rectangle is used.
    harness.step();
    assert_settings_window_is_inside_viewport(&harness);
    let window = harness
        .ctx
        .memory(|memory| memory.area_rect(Id::unique("texcel_appearance_settings")))
        .expect("settings window has a screen rectangle");
    assert!(
        harness
            .query_all(By::new().label_contains("対象の表"))
            .next()
            .is_none(),
        "app-wide settings do not present a current table section"
    );
    assert!(
        window.width() <= 680.0,
        "settings width is {}",
        window.width()
    );
}

/// Keeps the settings body and footer inside a small viewport at high zoom.
#[test]
fn settings_remain_reachable_at_max_zoom_in_a_small_window() {
    let settings_directory = TestDirectory::new();
    let settings_path = settings_directory.0.join("settings.json");
    let mut harness = Harness::builder()
        .with_size([760.0, 500.0])
        .with_max_steps(10)
        .build_eframe(move |creation| {
            TexcelApp::new_with_settings_path(creation, Some(settings_path))
        });
    {
        let ctx = harness.ctx.clone();
        let app = harness.state_mut();
        app.ui_font_size = 21.0;
        app.theme = ThemeChoice::Light;
        app.settings_window_open = true;
        app.apply_appearance(&ctx);
    }
    harness.step();
    // The first pass measures the Settings modal at its current zoom level.
    harness.step();
    let area = harness
        .ctx
        .memory(|memory| memory.area_rect(Id::unique("texcel_appearance_settings")))
        .expect("the settings modal has an area");
    assert_settings_window_is_inside_viewport(&harness);
    let close = harness
        .ctx
        .interactive_rects_last_pass()
        .into_iter()
        .find(|rect| {
            area.contains_rect(*rect)
                && (rect.width() - 112.0).abs() < 1.0
                && (rect.height() - visual::BUTTON_HEIGHT).abs() < 1.0
        })
        .expect("Settings keeps a visible Close button at maximum zoom");
    assert!(
        area.contains_rect(close),
        "Settings Close remains within its modal: area={area:?}, close={:?}",
        close
    );
}

/// Keeps language changes visible while Settings is open and after it closes.
#[test]
fn settings_language_is_applied_without_clipping_at_small_size() {
    let settings_directory = TestDirectory::new();
    let settings_path = settings_directory.0.join("settings.json");
    let mut harness = Harness::builder()
        .with_size([760.0, 500.0])
        .with_max_steps(10)
        .build_eframe(move |creation| {
            TexcelApp::new_with_settings_path(creation, Some(settings_path))
        });
    harness.step();
    {
        let ctx = harness.ctx.clone();
        let app = harness.state_mut();
        app.language_choice = crate::settings::LanguageChoice::Japanese;
        app.language = crate::i18n::Language::Japanese;
        crate::i18n::set_context_language(&ctx, app.language);
        app.ui_font_size = 21.0;
        app.settings_window_open = true;
        app.apply_appearance(&ctx);
    }
    harness.step();
    assert_eq!(harness.state().language, crate::i18n::Language::Japanese);
    assert_settings_window_is_inside_viewport(&harness);
    {
        let ctx = harness.ctx.clone();
        let app = harness.state_mut();
        app.language_choice = crate::settings::LanguageChoice::English;
        app.language = crate::i18n::Language::English;
        crate::i18n::set_context_language(&ctx, app.language);
        app.apply_appearance(&ctx);
    }
    harness.step();
    assert_eq!(harness.state().language, crate::i18n::Language::English);
    assert_settings_window_is_inside_viewport(&harness);
    harness.state_mut().settings_window_open = false;
    harness.step();
    assert!(!harness.state().settings_window_open);
}

/// Restores the same cell or formula editor after Settings closes with a real click.
#[test]
fn settings_close_returns_focus_to_the_same_editor_session() {
    for target in [
        crate::document::EditTarget::Cell,
        crate::document::EditTarget::FormulaBar,
    ] {
        let settings_directory = TestDirectory::new();
        let settings_path = settings_directory.0.join("settings.json");
        let mut harness =
            Harness::builder()
                .with_size([900.0, 760.0])
                .build_eframe(move |creation| {
                    TexcelApp::new_with_settings_path(creation, Some(settings_path))
                });
        harness.step();
        let (document_id, table_id) = {
            let document = harness.state().workspace.active_document().unwrap();
            (document.id, document.active_table().unwrap().id)
        };
        {
            let app = harness.state_mut();
            app.begin_cell_edit(table_id, CellAddress::default());
            app.edit.switch_target(target);
            app.edit.buffer = "formula draft".to_owned();
            app.edit.cursor_byte = app.edit.buffer.len();
            app.edit.anchor_byte = app.edit.cursor_byte;
            app.request_editor_focus = true;
        }
        harness.step();
        let editor = editor_id(document_id, harness.state().edit.session_id);
        assert_eq!(harness.ctx.memory(|memory| memory.focused()), Some(editor));

        harness.state_mut().settings_window_open = true;
        harness.step();
        harness.step();
        let settings_rect = harness
            .ctx
            .memory(|memory| memory.area_rect(Id::unique("texcel_appearance_settings")))
            .expect("Settings has a visible rectangle");
        let close_rect = harness
            .ctx
            .interactive_rects_last_pass()
            .into_iter()
            .find(|rect| {
                settings_rect.contains_rect(*rect)
                    && (rect.width() - 112.0).abs() < 1.0
                    && (rect.height() - visual::BUTTON_HEIGHT).abs() < 1.0
            })
            .expect("Settings Close button has a stable interactive rectangle");
        let close_position = close_rect.center();
        harness.input_mut().events.extend([
            egui::Event::PointerMoved(close_position),
            egui::Event::PointerButton {
                pos: close_position,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos: close_position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        harness.step();
        assert!(!harness.state().settings_window_open);
        harness.step();
        assert_eq!(
            harness.ctx.memory(|memory| memory.focused()),
            Some(editor),
            "closing Settings returns focus to the same edit session"
        );
        harness.event(egui::Event::Text("!".to_owned()));
        harness.step();
        assert_eq!(harness.state().edit.buffer, "formula draft!");
    }
}

/// Saves one screenshot while the current theme is applied.
fn save_render(harness: &mut Harness<'_, TexcelApp>, path: PathBuf) {
    let rendered = catch_unwind(AssertUnwindSafe(|| harness.render()))
        .expect("kittest renderer is available")
        .expect("kittest frame renders");
    rendered.save(path).expect("save rendered image");
}

/// Moves the test pointer away so screenshots show selected states without hover outlines.
fn clear_pointer_hover(harness: &mut Harness<'_, TexcelApp>) {
    harness.event(egui::Event::PointerMoved(egui::Pos2::new(-20.0, -20.0)));
    harness.step();
}

/// Applies a theme change through the same app path used by the settings form.
fn set_theme(harness: &mut Harness<'_, TexcelApp>, theme: ThemeChoice) {
    let ctx = harness.ctx.clone();
    harness.state_mut().theme = theme;
    harness.state_mut().apply_appearance(&ctx);
    harness.step();
}

/// Checks that the settings window does not extend beyond the native content area.
fn assert_settings_window_is_inside_viewport(harness: &Harness<'_, TexcelApp>) {
    let viewport = harness.ctx.content_rect();
    let window = harness
        .ctx
        .memory(|memory| memory.area_rect(Id::unique("texcel_appearance_settings")))
        .expect("settings window has a screen rectangle");
    assert!(window.min.x >= viewport.min.x - 1.0);
    assert!(window.min.y >= viewport.min.y - 1.0);
    assert!(
        window.max.x <= viewport.max.x + 1.0,
        "settings rect {window:?} exceeds viewport {viewport:?}"
    );
    assert!(
        window.max.y <= viewport.max.y + 1.0,
        "settings rect {window:?} exceeds viewport {viewport:?}"
    );
}
