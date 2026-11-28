use super::*;
use egui_kittest::{
    Harness,
    kittest::{By, NodeT, Queryable},
};
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const RENDER_SIZE: [f32; 2] = [1120.0, 760.0];
const OVERFLOW_MULTILINE_TEXT: &str = "12345678901234567890123456789012345678901234567890\nx";

// Finds the painted text created for the adopted Markdown sample cell.
fn painted_overflow_galley<'a>(
    harness: &'a Harness<'_, TexcelApp>,
) -> Option<&'a egui::text::Galley> {
    harness
        .output()
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) if text.galley.job.text == OVERFLOW_MULTILINE_TEXT => {
                Some(text.galley.as_ref())
            }
            _ => None,
        })
}

// Waits for the real painter to show the explicit second Markdown line.
fn wait_for_overflow_second_line(harness: &mut Harness<'_, TexcelApp>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if painted_overflow_galley(harness).is_some_and(|galley| {
            galley.rows.get(1).is_some_and(|row| {
                row.row
                    .glyphs
                    .iter()
                    .map(|glyph| glyph.chr)
                    .collect::<String>()
                    == "x"
            })
        }) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the adopted Markdown cell exposes its second line in the painted galley"
        );
        super::scenario_tests::send_batch(harness, []);
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Saves the main window and Settings in both supported languages and themes.
#[test]
#[ignore = "needs an available GPU adapter for raster output"]
fn render_main_and_settings_matrix() {
    let render_directory = render_directory();
    fs::create_dir_all(&render_directory).expect("create raster output directory");
    render_main_screen(&render_directory);
    render_settings(&render_directory);
    render_narrow_settings(&render_directory);
    render_narrow_settings_bottom(&render_directory);
}

#[test]
#[ignore = "needs an available GPU adapter for raster output"]
// Shows left, centered, and right-aligned long text in a production-loaded dark grid.
fn render_cell_value_display() {
    let directory = render_directory();
    fs::create_dir_all(&directory).expect("create raster output directory");
    let settings_directory = TestDirectory::new();
    let settings_path = settings_directory.0.join("settings.json");
    let sample_path = settings_directory.0.join("cell-display.md");
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("samples")
            .join("cell-display.md"),
        &sample_path,
    )
    .expect("copy the display fixture");
    let mut harness = Harness::builder()
        .with_size(RENDER_SIZE)
        .build_eframe(move |creation| {
            TexcelApp::new_with_settings_path(creation, Some(settings_path))
        });
    harness.step();
    set_language_and_theme(
        &mut harness,
        crate::i18n::Language::English,
        ThemeChoice::Dark,
    );
    super::scenario_tests::queue_open_and_wait(&mut harness, sample_path);
    clear_hover(&mut harness);
    save_render(&mut harness, directory.join("cell-display-en-dark.png"));
}

#[test]
#[ignore = "needs an available GPU adapter for raster output"]
// Shows overflow marks on long unwrapped rows after production adoption.
fn render_nowrap_multiline_overflow() {
    let directory = render_directory();
    fs::create_dir_all(&directory).expect("create raster output directory");
    let settings_directory = TestDirectory::new();
    let settings_path = settings_directory.0.join("settings.json");
    let sample_path = settings_directory.0.join("cell-display-overflow.md");
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("samples")
            .join("cell-display-overflow.md"),
        &sample_path,
    )
    .expect("copy the multiline overflow fixture");
    let mut harness = Harness::builder()
        .with_size(RENDER_SIZE)
        .build_eframe(move |creation| {
            TexcelApp::new_with_settings_path(creation, Some(settings_path))
        });
    harness.step();
    set_language_and_theme(
        &mut harness,
        crate::i18n::Language::English,
        ThemeChoice::Dark,
    );
    super::scenario_tests::queue_open_and_wait(&mut harness, sample_path);
    wait_for_overflow_second_line(&mut harness);
    let document_id = harness.state().workspace.active_document.unwrap();
    let document = &harness.state().workspace.documents[&document_id];
    let table_id = document.active_table;
    let painted =
        painted_overflow_galley(&harness).expect("the adopted cell has a painted text galley");
    assert_eq!(painted.rows.len(), 2);
    assert_eq!(
        painted.rows[1]
            .row
            .glyphs
            .iter()
            .map(|glyph| glyph.chr)
            .collect::<String>(),
        "x"
    );
    let line_height = crate::ui::cell_paint::cell_text_line_height(&harness.ctx, &painted.job);
    let two_line_height = painted
        .size()
        .y
        .max(painted.rows.len() as f32 * line_height)
        + crate::ui::grid_geometry::GRID_CELL_TEXT_VERTICAL_PADDING;
    let measured_height = harness.state().grid_row_layouts[&(document_id, table_id)].row_height(1);
    assert!(
        measured_height >= two_line_height,
        "the painted multiline cell has a tall enough measured row: {measured_height} vs {two_line_height}"
    );
    clear_hover(&mut harness);
    save_render(
        &mut harness,
        directory.join("cell-display-overflow-en-dark.png"),
    );
}

#[test]
#[ignore = "needs an available GPU adapter for raster output"]
// Shows the sheet-band selection and fixed controls in both languages and themes.
fn render_sheet_tab_band_matrix() {
    let directory = render_directory();
    fs::create_dir_all(&directory).expect("create raster output directory");
    let three_table_directory = TestDirectory::new();
    let three_table_settings = three_table_directory.0.join("settings.json");
    let markdown_path = three_table_directory.0.join("interaction-workflow.md");
    let samples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("samples");
    fs::copy(samples.join("interaction-workflow.md"), &markdown_path)
        .expect("copy the three-table Markdown fixture");
    let mut harness = matrix_app_harness(RENDER_SIZE, three_table_settings);
    harness.step();
    set_language_and_theme(
        &mut harness,
        crate::i18n::Language::English,
        ThemeChoice::Light,
    );
    super::scenario_tests::queue_open_and_wait(&mut harness, markdown_path.clone());
    assert_eq!(
        harness
            .state()
            .workspace
            .active_document()
            .unwrap()
            .tables
            .len(),
        3
    );
    clear_hover(&mut harness);
    save_render(
        &mut harness,
        directory.join("sheet-band-three-en-light.png"),
    );

    for (language, theme, suffix) in [
        (crate::i18n::Language::English, ThemeChoice::Dark, "en-dark"),
        (
            crate::i18n::Language::Japanese,
            ThemeChoice::Light,
            "ja-light",
        ),
        (
            crate::i18n::Language::Japanese,
            ThemeChoice::Dark,
            "ja-dark",
        ),
    ] {
        set_language_and_theme(&mut harness, language, theme);
        harness.step();
        clear_hover(&mut harness);
        save_render(
            &mut harness,
            directory.join(format!("sheet-band-three-{suffix}.png")),
        );
    }

    let one_table_directory = TestDirectory::new();
    let one_table_settings = one_table_directory.0.join("settings.json");
    let csv_path = one_table_directory.0.join("strings.csv");
    fs::copy(samples.join("strings.csv"), &csv_path).expect("copy the one-table CSV fixture");
    let mut one_table_harness = matrix_app_harness(RENDER_SIZE, one_table_settings);
    one_table_harness.step();
    set_language_and_theme(
        &mut one_table_harness,
        crate::i18n::Language::Japanese,
        ThemeChoice::Dark,
    );
    super::scenario_tests::queue_open_and_wait(&mut one_table_harness, csv_path);
    assert_eq!(
        one_table_harness
            .state()
            .workspace
            .active_document()
            .unwrap()
            .tables
            .len(),
        1
    );
    clear_hover(&mut one_table_harness);
    save_render(
        &mut one_table_harness,
        directory.join("sheet-band-one-ja-dark.png"),
    );

    let many_table_directory = TestDirectory::new();
    let many_table_settings = many_table_directory.0.join("settings.json");
    let many_tables_path = many_table_directory.0.join("many-tables.md");
    fs::write(&many_tables_path, many_table_markdown())
        .expect("write a production-loadable many-table fixture");
    let mut many_table_harness = matrix_app_harness([640.0, 620.0], many_table_settings);
    many_table_harness.step();
    set_language_and_theme(
        &mut many_table_harness,
        crate::i18n::Language::Japanese,
        ThemeChoice::Dark,
    );
    super::scenario_tests::queue_open_and_wait(&mut many_table_harness, many_tables_path);
    assert!(
        many_table_harness
            .state()
            .workspace
            .active_document()
            .unwrap()
            .tables
            .len()
            >= 8
    );
    clear_hover(&mut many_table_harness);
    save_render(
        &mut many_table_harness,
        directory.join("sheet-band-many-ja-dark-narrow.png"),
    );
}

/// Uses the same no-document-tabs mode as the desktop window host.
fn matrix_app_harness(size: [f32; 2], settings_path: PathBuf) -> Harness<'static, TexcelApp> {
    Harness::builder()
        .with_size(size)
        .build_eframe(move |creation| {
            let mut app = TexcelApp::new_with_settings_path(creation, Some(settings_path));
            app.multi_window_mode = true;
            app
        })
}

/// Creates many short Markdown tables so the real loader exercises tab scrolling.
fn many_table_markdown() -> String {
    let mut source = String::from("# Many table tabs\n\n");
    for index in 1..=40 {
        source.push_str(&format!(
            "## Sheet {index}\n\n| Key | Value |\n| --- | --- |\n| k{index} | v{index} |\n\n"
        ));
    }
    source
}

#[test]
// Keeps the second Markdown line visible after a real file is adopted.
fn adopted_nowrap_markdown_text_keeps_its_second_line_visible() {
    let settings_directory = TestDirectory::new();
    let settings_path = settings_directory.0.join("settings.json");
    let sample_path = settings_directory.0.join("cell-display-overflow.md");
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("samples")
            .join("cell-display-overflow.md"),
        &sample_path,
    )
    .expect("copy the Markdown overflow fixture");
    let mut harness = Harness::builder()
        .with_size(RENDER_SIZE)
        .build_eframe(move |creation| {
            TexcelApp::new_with_settings_path(creation, Some(settings_path))
        });
    harness.step();
    set_language_and_theme(
        &mut harness,
        crate::i18n::Language::English,
        ThemeChoice::Dark,
    );
    super::scenario_tests::queue_open_and_wait(&mut harness, sample_path);
    wait_for_overflow_second_line(&mut harness);
    let document_id = harness.state().workspace.active_document.unwrap();
    let document = &harness.state().workspace.documents[&document_id];
    let table_id = document.active_table;
    let address = CellAddress { row: 1, column: 0 };
    let painted =
        painted_overflow_galley(&harness).expect("the adopted cell has a painted text galley");
    let measured_height =
        harness.state().grid_row_layouts[&(document_id, table_id)].row_height(address.row);
    assert_eq!(
        painted.rows.len(),
        2,
        "the actual cell painter uses its measured row height {measured_height}"
    );
    assert_eq!(
        painted.rows[1]
            .row
            .glyphs
            .iter()
            .map(|glyph| glyph.chr)
            .collect::<String>(),
        "x"
    );
    assert!(measured_height > ROW_HEIGHT);
}

#[test]
#[ignore = "needs an available GPU adapter for raster output"]
// Shows the imported formatting notice and the same report reopened from File.
fn render_formatting_import_notice_and_report() {
    let directory = render_directory();
    fs::create_dir_all(&directory).expect("create raster output directory");
    let settings_directory = TestDirectory::new();
    let settings_path = settings_directory.0.join("settings.json");
    let source_path = settings_directory.0.join("formatting-only.xlsx");
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("samples")
            .join("formatting-only.xlsx"),
        &source_path,
    )
    .expect("copy the formatting-only XLSX fixture");
    let mut harness = Harness::builder()
        .with_size(RENDER_SIZE)
        .build_eframe(move |creation| {
            TexcelApp::new_with_settings_path(creation, Some(settings_path))
        });
    crate::ui::top::force_shared_menu_bar_for_test(&harness.ctx);
    harness.step();
    set_language_and_theme(
        &mut harness,
        crate::i18n::Language::English,
        ThemeChoice::Light,
    );
    super::scenario_tests::queue_open_and_wait(&mut harness, source_path);
    assert!(harness.state().active_import_report.is_none());
    assert!(harness.state().import_notice.is_some());
    assert!(
        harness
            .query_all(By::new().label_contains("Some formatting was not imported"))
            .count()
            > 0
    );
    clear_hover(&mut harness);
    save_render(
        &mut harness,
        directory.join("import-formatting-notice-en-light.png"),
    );

    // The import notice in the status bar reopens the report with its Details button.
    let details_label = crate::i18n::context_text(&harness.ctx, "action-import-details");
    let details_rect = button_rect(&harness, &details_label);
    super::scenario_tests::click_point(&mut harness, details_rect.center());
    for _ in 0..4 {
        if harness.state().active_import_report.is_some() {
            break;
        }
        super::scenario_tests::send_batch(&mut harness, []);
    }
    assert!(
        harness.state().active_import_report.is_some(),
        "the notice's Details button reopens the report for the adopted document"
    );
    let report = &harness.state().active_import_report.as_ref().unwrap().1;
    assert_eq!(report.file_name, "formatting-only.xlsx");
    assert!(!report.formatted_imported_at().is_empty());
    let title = crate::i18n::Message::new("dialog-report-title-import")
        .with_text("format", "XLSX")
        .resolve(harness.state().language);
    for _ in 0..8 {
        if harness.query_all(By::new().label_contains(&title)).count() > 0 {
            break;
        }
        super::scenario_tests::send_batch(&mut harness, []);
    }
    assert!(
        harness.query_all(By::new().label_contains(&title)).count() > 0,
        "the reopened report is rendered"
    );
    // Each of the three report columns carries its own heading.
    for key in [
        "report-column-result",
        "report-column-location",
        "report-column-detail",
    ] {
        let heading = crate::i18n::context_text(&harness.ctx, key);
        assert!(
            harness.query_all(By::new().label(&heading)).count() > 0,
            "the report shows the {heading} column heading"
        );
    }
    clear_hover(&mut harness);
    save_render(
        &mut harness,
        directory.join("import-formatting-report-en-light.png"),
    );

    set_language_and_theme(
        &mut harness,
        crate::i18n::Language::Japanese,
        ThemeChoice::Light,
    );
    harness.step();
    harness.step();
    clear_hover(&mut harness);
    save_render(
        &mut harness,
        directory.join("import-formatting-report-ja-light.png"),
    );
}

/// Captures the current spreadsheet shell at the approved design size.
fn render_main_screen(directory: &std::path::Path) {
    let settings_directory = TestDirectory::new();
    let settings_path = settings_directory.0.join("settings.json");
    let mut harness = matrix_app_harness(RENDER_SIZE, settings_path);
    crate::ui::top::force_shared_menu_bar_for_test(&harness.ctx);
    harness.step();
    set_language_and_theme(
        &mut harness,
        crate::i18n::Language::English,
        ThemeChoice::Light,
    );
    let markdown_path = settings_directory.0.join("interaction-workflow.md");
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("samples")
            .join("interaction-workflow.md"),
        &markdown_path,
    )
    .expect("copy the three-table Markdown fixture");
    super::scenario_tests::queue_open_and_wait(&mut harness, markdown_path.clone());
    select_markdown_sample(&mut harness, &markdown_path);
    render_top_menu_samples(&mut harness, directory);

    render_locales_and_themes(
        &mut harness,
        |language, theme| {
            format!(
                "main-1120x760-{}-{}.png",
                language_name(language),
                theme_name(theme)
            )
        },
        directory,
    );
    set_language_and_theme(
        &mut harness,
        crate::i18n::Language::Japanese,
        ThemeChoice::Light,
    );
    harness.set_size(Vec2::new(840.0, 620.0));
    harness.step();
    clear_hover(&mut harness);
    save_render(
        &mut harness,
        directory.join("main-narrow-840x620-ja-light.png"),
    );
}

/// Captures the shared menus and the copy options opened from the Edit menu.
fn render_top_menu_samples(harness: &mut Harness<'_, TexcelApp>, directory: &std::path::Path) {
    set_language_and_theme(harness, crate::i18n::Language::Japanese, ThemeChoice::Light);
    harness.step();

    for menu in crate::command_menu::MENUS {
        let label = crate::i18n::context_text(&harness.ctx, menu.label);
        let rect = button_rect(harness, &label);
        super::scenario_tests::click_point(harness, rect.center());
        let item_label = last_visible_menu_label(&harness.ctx, menu.entries);
        wait_for_popup_button(harness, &item_label);
        let suffix = menu.id.strip_prefix("menu.").unwrap_or(menu.id);
        save_render(
            harness,
            directory.join(format!("menu-{suffix}-ja-light.png")),
        );
        close_menu(harness, &item_label);
    }

    let edit_label = crate::i18n::context_text(&harness.ctx, "menu-edit");
    let edit_rect = button_rect(harness, &edit_label);
    super::scenario_tests::click_point(harness, edit_rect.center());
    let copy_label = crate::i18n::context_text(&harness.ctx, "menu-edit-copy-as");
    let copy_rect = wait_for_popup_button(harness, &copy_label);
    super::scenario_tests::click_point(harness, copy_rect.center());
    super::scenario_tests::wait_until(harness, |app| app.copy_options_dialog.is_some());
    let cancel = crate::i18n::context_text(&harness.ctx, "action-cancel");
    for _ in 0..8 {
        if harness
            .query_all(By::new().role(egui::Role::Button).label(&cancel))
            .next()
            .is_some()
        {
            break;
        }
        super::scenario_tests::send_batch(harness, []);
    }
    clear_hover(harness);
    save_render(harness, directory.join("edit-copy-options-ja-light.png"));

    let cancel_rect = button_rect(harness, &cancel);
    super::scenario_tests::click_point(harness, cancel_rect.center());
    assert!(harness.state().copy_options_dialog.is_none());
}

/// Finds a visible button by its localized accessible name.
fn button_rect(harness: &Harness<'_, TexcelApp>, label: &str) -> egui::Rect {
    harness
        .query_all(By::new().role(egui::Role::Button).label(label))
        .next()
        .unwrap_or_else(|| panic!("button {label:?} is visible"))
        .rect()
}

/// Finds the last visible item label for a menu-open check.
fn last_visible_menu_label(
    ctx: &egui::Context,
    entries: &[crate::command_menu::MenuEntry],
) -> String {
    entries
        .iter()
        .rev()
        .find_map(|entry| match entry {
            crate::command_menu::MenuEntry::Action(action) => {
                let label = crate::i18n::context_text(ctx, action.label);
                let shortcut = crate::command_menu::shortcut_text(ctx, action.shortcut);
                Some(if shortcut.is_empty() {
                    label
                } else {
                    format!("{label} {shortcut}")
                })
            }
            crate::command_menu::MenuEntry::Submenu(submenu) => {
                Some(crate::i18n::context_text(ctx, submenu.label))
            }
            crate::command_menu::MenuEntry::RecentFiles => {
                Some(crate::i18n::context_text(ctx, "menu-file-open-recent"))
            }
            crate::command_menu::MenuEntry::WindowList => {
                Some(crate::i18n::context_text(ctx, "menu-window-open"))
            }
            crate::command_menu::MenuEntry::Separator => None,
        })
        .expect("every top-level menu has a visible entry")
}

/// Waits until a popup item is exposed through the real UI frame.
fn wait_for_popup_button(harness: &mut Harness<'_, TexcelApp>, label: &str) -> egui::Rect {
    for _ in 0..8 {
        if let Some(rect) = visible_popup_button_rect(harness, label) {
            return rect;
        }
        super::scenario_tests::send_batch(harness, []);
    }
    let menu_nodes = popup_node_descriptions(harness);
    let app = harness.state();
    let popup_open = egui::Popup::is_any_open(&harness.ctx);
    let import_report_open = app.active_import_report.is_some();
    let queued_reports = app.queued_import_reports.len();
    let screenshot_path = render_directory().join("menu-popup-wait-failure.png");
    let screenshot_saved = catch_unwind(AssertUnwindSafe(|| harness.render()))
        .ok()
        .and_then(Result::ok)
        .is_some_and(|image| image.save(&screenshot_path).is_ok());
    panic!(
        "button {label:?} appears after popup; popup_open={popup_open}, import_report={import_report_open}, queued_reports={queued_reports}, menu_nodes={menu_nodes:?}, screenshot_saved={screenshot_saved} at {screenshot_path:?}"
    )
}

/// Closes a popup with a complete Escape key press and release.
fn close_menu(harness: &mut Harness<'_, TexcelApp>, item_label: &str) {
    let escape = |pressed| egui::Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    super::scenario_tests::send_batch(harness, [escape(true)]);
    super::scenario_tests::send_batch(harness, [escape(false)]);
    for _ in 0..8 {
        if visible_popup_button_rect(harness, item_label).is_none() {
            return;
        }
        super::scenario_tests::send_batch(harness, []);
    }
    panic!("Escape closes the popup containing {item_label:?}");
}

/// Finds a named button only when an egui Menu contains it.
fn visible_popup_button_rect(harness: &Harness<'_, TexcelApp>, label: &str) -> Option<egui::Rect> {
    harness
        .query_all(By::new())
        .find(|button| {
            has_menu_popup_ancestor(button)
                && button
                    .accesskit_node()
                    .label()
                    .is_some_and(|accessible_label| accessible_label.trim_end() == label.trim_end())
        })
        .map(|button| button.rect())
}

/// Checks whether an accessible button belongs to a visible menu popup.
fn has_menu_popup_ancestor<'tree>(node: &impl NodeT<'tree>) -> bool {
    let mut ancestor = node.parent();
    while let Some(parent) = ancestor {
        if parent.accesskit_node().role() == egui::Role::Menu {
            return true;
        }
        ancestor = parent.parent();
    }
    false
}

/// Lists accessible menu descendants when a rendered popup item is missing.
fn popup_node_descriptions(harness: &Harness<'_, TexcelApp>) -> Vec<String> {
    harness
        .query_all(By::new().role(egui::Role::Menu))
        .flat_map(|menu| menu.children_recursive())
        .map(|node| {
            let access = node.accesskit_node();
            format!(
                "{:?} label={:?} value={:?} disabled={} bounds={:?}",
                access.role(),
                access.label(),
                access.value(),
                access.is_disabled(),
                access.bounding_box()
            )
        })
        .collect()
}

/// Selects the third table inside the loaded Markdown document window.
fn select_markdown_sample(harness: &mut Harness<'_, TexcelApp>, path: &std::path::Path) {
    let markdown_id = harness.state().workspace.active_document.unwrap();
    assert_eq!(harness.state().user_file_paths[&markdown_id], path);
    assert_eq!(
        harness.state().workspace.documents[&markdown_id]
            .tables
            .len(),
        3
    );
    let notes_tab = harness
        .query_all(By::new().role(egui::Role::Button).label("Notes"))
        .next()
        .expect("the Notes table tab is visible")
        .rect();
    super::scenario_tests::click_point(harness, notes_tab.center());
    harness.step();
    let document = &harness.state().workspace.documents[&markdown_id];
    assert_eq!(document.active_table().unwrap().name, "Notes");
    assert!(
        harness
            .state()
            .last_viewport_title
            .as_deref()
            .is_some_and(|title| title.contains("interaction-workflow.md")
                && title.contains("Notes")
                && title.contains("Texcel"))
    );
}

/// Captures Settings in each language and theme.
fn render_settings(directory: &std::path::Path) {
    let settings_directory = TestDirectory::new();
    let settings_path = settings_directory.0.join("settings.json");
    let mut harness = Harness::builder()
        .with_size(RENDER_SIZE)
        .build_eframe(move |creation| {
            TexcelApp::new_with_settings_path(creation, Some(settings_path))
        });
    harness.state_mut().settings_window_open = true;
    harness.step();
    harness.step();
    render_locales_and_themes(
        &mut harness,
        |language, theme| {
            format!(
                "settings-appearance-{}-{}.png",
                language_name(language),
                theme_name(theme),
            )
        },
        directory,
    );
}

/// Keeps the Japanese Settings layout visible at a narrow window size.
fn render_narrow_settings(directory: &std::path::Path) {
    let settings_directory = TestDirectory::new();
    let settings_path = settings_directory.0.join("settings.json");
    let mut harness = Harness::builder()
        .with_size([460.0, 360.0])
        .build_eframe(move |creation| {
            TexcelApp::new_with_settings_path(creation, Some(settings_path))
        });
    set_language_and_theme(
        &mut harness,
        crate::i18n::Language::Japanese,
        ThemeChoice::Light,
    );
    harness.state_mut().settings_window_open = true;
    harness.step();
    harness.step();
    clear_hover(&mut harness);
    save_render(
        &mut harness,
        directory.join("settings-narrow-ja-light-460x360.png"),
    );
}

/// Confirms the Close footer stays reachable in a narrow Japanese Settings window.
fn render_narrow_settings_bottom(directory: &std::path::Path) {
    let settings_directory = TestDirectory::new();
    let settings_path = settings_directory.0.join("settings.json");
    let mut harness = Harness::builder()
        .with_size([460.0, 360.0])
        .build_eframe(move |creation| {
            TexcelApp::new_with_settings_path(creation, Some(settings_path))
        });
    set_language_and_theme(
        &mut harness,
        crate::i18n::Language::Japanese,
        ThemeChoice::Light,
    );
    harness.state_mut().settings_window_open = true;
    harness.step();
    harness.step();
    scroll_settings_to_bottom(&mut harness, Pos2::new(230.0, 170.0));
    save_render(
        &mut harness,
        directory.join("settings-narrow-bottom-ja-light-460x360.png"),
    );
}

/// Scrolls the Settings body without moving its footer.
fn scroll_settings_to_bottom(harness: &mut Harness<'_, TexcelApp>, pointer: Pos2) {
    harness.event(egui::Event::PointerMoved(pointer));
    harness.step();
    for _ in 0..6 {
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: Vec2::new(0.0, -880.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: egui::Modifiers::NONE,
        });
        harness.step();
    }
}

/// Applies each locale and theme before saving the same fixture state.
fn render_locales_and_themes(
    harness: &mut Harness<'_, TexcelApp>,
    file_name: impl Fn(crate::i18n::Language, ThemeChoice) -> String,
    directory: &std::path::Path,
) {
    for language in [
        crate::i18n::Language::English,
        crate::i18n::Language::Japanese,
    ] {
        for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
            set_language_and_theme(harness, language, theme);
            harness.step();
            harness.step();
            clear_hover(harness);
            save_render(harness, directory.join(file_name(language, theme)));
        }
    }
}

/// Sets app and egui appearance together, matching the Settings control path.
fn set_language_and_theme(
    harness: &mut Harness<'_, TexcelApp>,
    language: crate::i18n::Language,
    theme: ThemeChoice,
) {
    let context = harness.ctx.clone();
    let app = harness.state_mut();
    app.language = language;
    app.language_choice = match language {
        crate::i18n::Language::English => crate::settings::LanguageChoice::English,
        crate::i18n::Language::Japanese => crate::settings::LanguageChoice::Japanese,
    };
    app.theme = theme;
    crate::i18n::set_context_language(&context, language);
    app.apply_appearance(&context);
}

/// Moves the pointer away so static images do not emphasize hover states.
fn clear_hover(harness: &mut Harness<'_, TexcelApp>) {
    harness.event(egui::Event::PointerMoved(egui::pos2(-20.0, -20.0)));
    harness.step();
}

/// Saves one frame after the current appearance state has settled.
fn save_render(harness: &mut Harness<'_, TexcelApp>, path: PathBuf) {
    let rendered = catch_unwind(AssertUnwindSafe(|| harness.render()))
        .expect("kittest renderer is available")
        .expect("kittest frame renders");
    rendered.save(path).expect("save raster output");
}

/// Returns the standard raster output location without placing images in samples.
fn render_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("kittest-renders")
}

/// Gives filenames stable language suffixes for scripted review runs.
fn language_name(language: crate::i18n::Language) -> &'static str {
    match language {
        crate::i18n::Language::English => "en",
        crate::i18n::Language::Japanese => "ja",
    }
}

/// Gives filenames stable theme suffixes for scripted review runs.
fn theme_name(theme: ThemeChoice) -> &'static str {
    match theme {
        ThemeChoice::Light => "light",
        ThemeChoice::Dark => "dark",
        ThemeChoice::System => "system",
    }
}
