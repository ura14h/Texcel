use super::*;
use egui::{Event, Modifiers, PointerButton, RawInput};
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

impl GridRowLayout {
    // Return the vertical position at the start of a test row.
    fn row_top(&self, row: usize) -> f32 {
        HEADER_HEIGHT + self.prefix_height(row)
    }
}

// Create a table with wrapped cell text for layout tests.
fn wrapped_row_cells(
    document: &Document,
    table: &crate::document::Table,
    layout: &TableLayout,
    row: usize,
    visible_columns: std::ops::Range<usize>,
) -> (u64, Vec<(CellAddress, String, f32)>) {
    wrapped_row_cells_for_ranges(document, table, layout, row, &[visible_columns], true)
}

static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(1);

impl TexcelApp {
    // Starts a file load through the production path for UI tests.
    pub(super) fn open_path(&mut self, path: PathBuf) {
        self.start_path_load(path)
            .expect("the test file has a supported format");
    }
}

// Installs a CSV fixture whose stored cells are present in the retained source.
pub(crate) fn install_test_csv_document(
    app: &mut TexcelApp,
    source_bytes: Vec<u8>,
    name: &str,
) -> (DocumentId, TableId) {
    let document_id = app.workspace.active_document.expect("active document");
    let source = crate::csv_adapter::CsvDocument::parse(
        source_bytes,
        crate::csv_adapter::EncodingChoice::Auto,
        ',',
    )
    .expect("the fixture CSV parses");
    let mut document = source.to_document(document_id, name);
    document.set_source(Some(Arc::new(SourceDocument::Csv(Arc::new(source)))));
    let table_id = document.active_table;
    app.clear_calculation_jobs_for_reset(document_id);
    let calculation = CalculationSnapshot::from_document(&document, false);
    app.workspace.documents.insert(document_id, document);
    let revision = app.workspace.documents[&document_id].calculation_revision;
    let token = app.next_job_token(document_id, revision);
    app.initialize_calculation(token, calculation);
    (document_id, table_id)
}

// Replaces the startup Markdown placeholder with a loaded grid whose cells match its source.
pub(crate) fn install_test_markdown_grid(
    app: &mut TexcelApp,
    rows: usize,
    columns: usize,
    name: &str,
) -> (DocumentId, TableId) {
    let document_id = app.workspace.active_document.expect("active document");
    let rows = rows.max(1);
    let columns = columns.max(1);
    let header = (0..columns)
        .map(|column| format!("1:{}", column + 1))
        .collect::<Vec<_>>();
    let separator = (0..columns).map(|_| "---".to_owned()).collect::<Vec<_>>();
    let body = (1..rows).map(|row| {
        (0..columns)
            .map(|column| format!("{}:{}", row + 1, column + 1))
            .collect::<Vec<_>>()
    });
    let source_text = std::iter::once(header)
        .chain(std::iter::once(separator))
        .chain(body)
        .map(|cells| format!("| {} |", cells.join(" | ")))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let source = crate::markdown_adapter::MarkdownDocument::parse(source_text.into_bytes())
        .expect("the fixture Markdown grid parses");
    let mut document = source.to_document_with_language(document_id, name, app.language);
    document.set_source(Some(Arc::new(SourceDocument::Markdown(Arc::new(source)))));
    for table in &mut document.tables {
        table.set_dimensions(rows, columns);
    }
    for view in document.views.values_mut() {
        view.header_row = Some(0);
    }
    let table_id = document.active_table;
    app.clear_calculation_jobs_for_reset(document_id);
    let calculation = CalculationSnapshot::from_document(&document, false);
    app.workspace.documents.insert(document_id, document);
    let revision = app.workspace.documents[&document_id].calculation_revision;
    let token = app.next_job_token(document_id, revision);
    app.initialize_calculation(token, calculation);
    (document_id, table_id)
}

#[test]
// Verify that the name box parses cells, ranges, and whole row or column selections.
fn name_box_parses_cells_rectangles_and_whole_axes() {
    assert_eq!(
        parse_name_box_range("C12", MAX_ROWS, MAX_COLUMNS),
        Some(CellRange::single(CellAddress { row: 11, column: 2 }))
    );
    assert_eq!(
        parse_name_box_range("A1:D20", MAX_ROWS, MAX_COLUMNS),
        Some(CellRange {
            anchor: CellAddress { row: 0, column: 0 },
            head: CellAddress { row: 19, column: 3 },
        })
    );
    assert_eq!(
        parse_name_box_range("$C:$E", MAX_ROWS, MAX_COLUMNS),
        Some(CellRange {
            anchor: CellAddress { row: 0, column: 2 },
            head: CellAddress {
                row: MAX_ROWS - 1,
                column: 4,
            },
        })
    );
    assert_eq!(
        parse_name_box_range("3:3", MAX_ROWS, MAX_COLUMNS),
        Some(CellRange {
            anchor: CellAddress { row: 2, column: 0 },
            head: CellAddress {
                row: 2,
                column: MAX_COLUMNS - 1,
            },
        })
    );
    assert!(parse_name_box_range("A0", MAX_ROWS, MAX_COLUMNS).is_none());
    assert!(parse_name_box_range("XFE1", MAX_ROWS, MAX_COLUMNS).is_none());
}

#[test]
// Accepts absolute and mixed references with one `$` before the column or the row only.
fn name_box_accepts_absolute_and_mixed_references() {
    let c3 = Some(CellRange::single(CellAddress { row: 2, column: 2 }));
    for text in ["C3", "$C3", "C$3", "$C$3", "$c$3"] {
        assert_eq!(
            parse_name_box_range(text, MAX_ROWS, MAX_COLUMNS),
            c3,
            "{text}"
        );
    }
    assert_eq!(
        parse_name_box_range("$A$1:B$2", MAX_ROWS, MAX_COLUMNS),
        Some(CellRange {
            anchor: CellAddress { row: 0, column: 0 },
            head: CellAddress { row: 1, column: 1 },
        })
    );
    assert_eq!(
        parse_name_box_range("$2:$4", MAX_ROWS, MAX_COLUMNS),
        Some(CellRange {
            anchor: CellAddress { row: 1, column: 0 },
            head: CellAddress {
                row: 3,
                column: MAX_COLUMNS - 1,
            },
        })
    );
    for text in ["C3$", "$$C3", "C$$3", "$", "C$", "$3", "A+1", "A$+1"] {
        assert!(
            parse_name_box_range(text, MAX_ROWS, MAX_COLUMNS).is_none(),
            "{text} is not a cell reference"
        );
    }
}

#[test]
// Verify that a long-cell editor can replace its full selection using a bounded widget.
fn long_cell_editor_replaces_the_full_selection_with_a_bounded_text_widget() {
    let ctx = egui::Context::default();
    let mut edit = EditSession::default();
    edit.begin(
        1,
        CellAddress::default(),
        CellInput::Text("cell text ".repeat(5_000)),
        crate::document::EditTarget::Cell,
    );
    let editor = editor_id(91, edit.session_id);
    ctx.memory_mut(|memory| memory.request_focus(editor));
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 320.0))),
        events: vec![
            key(Key::A, Modifiers::COMMAND),
            Event::Text("replaced".to_owned()),
        ],
        ..RawInput::default()
    };
    let frame_ctx = ctx.clone();
    let mut output = ctx.run_ui(raw, |ui| {
        show_bounded_edit_text(
            ui,
            &frame_ctx,
            editor,
            &mut edit,
            CellEditWidget::Singleline {
                width: 320.0,
                frame_none: false,
            },
        );
    });
    output.textures_delta.clear();

    assert_eq!(edit.buffer, "replaced");
    assert_eq!(edit.anchor_byte, edit.buffer.len());
    assert_eq!(edit.cursor_byte, edit.buffer.len());
    let window = edit_window_range(&format!("{}日本語", "x".repeat(40_000)), 40_000);
    assert!(window.len() <= MAX_CELL_EDIT_WINDOW_BYTES);
}

#[test]
// Verify that left-arrow input updates absolute selection before inserted text.
fn long_cell_editor_routes_left_before_text_through_the_absolute_selection() {
    let ctx = egui::Context::default();
    let source = "cell text ".repeat(5_000);
    let mut edit = EditSession::default();
    edit.begin(
        1,
        CellAddress::default(),
        CellInput::Text(source.clone()),
        crate::document::EditTarget::Cell,
    );
    let editor = editor_id(93, edit.session_id);
    ctx.memory_mut(|memory| memory.request_focus(editor));
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 320.0))),
        events: vec![
            key(Key::A, Modifiers::COMMAND),
            key(Key::ArrowLeft, Modifiers::NONE),
            Event::Text("changed".to_owned()),
        ],
        ..RawInput::default()
    };
    let frame_ctx = ctx.clone();
    let mut output = ctx.run_ui(raw, |ui| {
        show_bounded_edit_text(
            ui,
            &frame_ctx,
            editor,
            &mut edit,
            CellEditWidget::Singleline {
                width: 320.0,
                frame_none: false,
            },
        );
    });
    output.textures_delta.clear();

    assert_eq!(edit.buffer, format!("changed{source}"));
    assert_eq!(edit.cursor_byte, "changed".len());
}

#[test]
// Verify that a bounded cell editor keeps a valid UTF-8 window during IME composition.
fn bounded_cell_editor_moves_and_holds_its_utf8_window_during_ime() {
    let ctx = egui::Context::default();
    let mut edit = EditSession::default();
    edit.begin(
        1,
        CellAddress::default(),
        CellInput::Text(format!("{}{}", "x".repeat(20_000), "日本語".repeat(6_000))),
        crate::document::EditTarget::Cell,
    );
    let editor = editor_id(92, edit.session_id);
    let initial_range = edit_window_range(&edit.buffer, edit.cursor_byte);
    edit.edit_window_start = Some(initial_range.start);
    edit.anchor_byte = initial_range.start;
    edit.cursor_byte = initial_range.start;
    let previous = previous_char_boundary(&edit.buffer, edit.cursor_byte);
    ctx.memory_mut(|memory| memory.request_focus(editor));
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 320.0))),
        events: vec![key(Key::ArrowLeft, Modifiers::NONE)],
        ..RawInput::default()
    };
    let frame_ctx = ctx.clone();
    let mut output = ctx.run_ui(raw, |ui| {
        show_bounded_edit_text(
            ui,
            &frame_ctx,
            editor,
            &mut edit,
            CellEditWidget::Singleline {
                width: 320.0,
                frame_none: false,
            },
        );
    });
    output.textures_delta.clear();
    assert_eq!(edit.cursor_byte, previous);
    assert!(edit_window_range(&edit.buffer, edit.cursor_byte).start < initial_range.start);

    edit.composing = true;
    edit.edit_window_start = Some(initial_range.start);
    assert_eq!(edit_session_window_range(&edit).start, initial_range.start);
    edit.composing = false;
    assert_ne!(edit_session_window_range(&edit).start, initial_range.start);
}

#[test]
// Verify that both-mode formatting and Markdown breaks increase automatic row height.
fn both_mode_and_markdown_breaks_expand_automatic_row_height() {
    let ctx = egui::Context::default();
    install_texcel_grid_fonts(&ctx, FontPreset::Default);
    let mut output = ctx.run_ui(egui::RawInput::default(), |_| ());
    output.textures_delta.clear();

    let mut formula_document = crate::document::Document::new(93, "formula", &["Sheet1"]);
    let formula_table_id = formula_document.tables[0].id;
    formula_document
        .set_cell(
            formula_table_id,
            CellAddress { row: 0, column: 0 },
            Some(Cell::formula(
                "=1",
                Some(crate::document::CalcValue::Text("result\nline".to_owned())),
            )),
        )
        .expect("formula cell is valid");
    formula_document.apply_calculation_results(
        formula_document.calculation_revision,
        [(
            formula_table_id,
            CellAddress { row: 0, column: 0 },
            crate::document::CalcStatus::Fresh,
            Some(crate::document::CalcValue::Text("result\nline".to_owned())),
        )],
    );
    let visible_columns = std::iter::once(0..1).collect::<Vec<_>>();
    let (formula_fingerprint, formula_height) = measure_display_row_height(
        &ctx,
        GridTable {
            document: &formula_document,
            table: &formula_document.tables[0],
            layout: &formula_document.tables[0].layout,
            is_markdown: false,
            markdown_source: None,
            markdown_table_index: None,
        },
        GridDisplay {
            mode: CellDisplayMode::View,
            show_invisibles: false,
            wrap_text: false,
            font_layout_generation: 0,
        },
        0,
        &visible_columns,
        &BTreeMap::new(),
    );
    assert_ne!(formula_fingerprint, 0);
    assert!(formula_height > ROW_HEIGHT);

    let markdown = crate::markdown_adapter::MarkdownDocument::parse(
        b"| value |\n| --- |\n| first<br>second |\n".to_vec(),
    )
    .expect("Markdown parses");
    let address = CellAddress { row: 1, column: 0 };
    let mut markdown_document = crate::document::Document::new(94, "markdown", &["Table"]);
    let markdown_table_id = markdown_document.tables[0].id;
    markdown_document
        .set_cell(
            markdown_table_id,
            address,
            Some(Cell::text("first<br>second")),
        )
        .expect("Markdown cell is valid");
    let display = markdown
        .cell_display(0, address.row, address.column)
        .expect("Markdown cell display is available")
        .clone();
    assert_eq!(display.rendered_lines, 2);
    let input_revision = markdown_document.cell_input_revision(markdown_table_id, address);
    let mut displays = BTreeMap::new();
    displays.insert(
        (markdown_document.id, markdown_table_id, address),
        CachedMarkdownCellDisplay {
            document_revision: markdown_document.revision,
            calculation_revision: markdown_document.calculation_revision,
            cell_input_revision: Some(input_revision),
            raw_text: None,
            edit_session_id: None,
            display,
        },
    );
    let (_, markdown_height) = measure_display_row_height(
        &ctx,
        GridTable {
            document: &markdown_document,
            table: &markdown_document.tables[0],
            layout: &markdown_document.tables[0].layout,
            is_markdown: true,
            markdown_source: Some(&markdown),
            markdown_table_index: Some(0),
        },
        GridDisplay {
            mode: CellDisplayMode::View,
            show_invisibles: false,
            wrap_text: false,
            font_layout_generation: 0,
        },
        address.row,
        &visible_columns,
        &displays,
    );
    assert!(markdown_height > ROW_HEIGHT);
}

#[test]
// Verify that the fill handle maps to each of the four copy directions.
fn fill_handle_maps_to_all_four_copy_directions() {
    let source = CellRange {
        anchor: CellAddress { row: 2, column: 2 },
        head: CellAddress { row: 3, column: 3 },
    };
    let cases = [
        (
            CellAddress { row: 5, column: 3 },
            CellAddress { row: 2, column: 2 },
            CellAddress { row: 5, column: 3 },
        ),
        (
            CellAddress { row: 0, column: 3 },
            CellAddress { row: 0, column: 2 },
            CellAddress { row: 3, column: 3 },
        ),
        (
            CellAddress { row: 3, column: 6 },
            CellAddress { row: 2, column: 2 },
            CellAddress { row: 3, column: 6 },
        ),
        (
            CellAddress { row: 3, column: 0 },
            CellAddress { row: 2, column: 0 },
            CellAddress { row: 3, column: 3 },
        ),
    ];
    for (endpoint, expected_start, expected_end) in cases {
        let target = fill_handle_target(source, endpoint).expect("one-axis fill target");
        assert_eq!(target.normalized(), (expected_start, expected_end));
    }
    assert!(fill_handle_target(source, CellAddress { row: 2, column: 2 }).is_none());
    assert!(fill_handle_target(source, CellAddress { row: 5, column: 6 }).is_none());
}

#[test]
// Verify that appearance changes preserve the active editor and selection.
fn appearance_changes_keep_the_active_edit_session_and_selection_state() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let (document_id, table_id, revision, selection, scroll) = {
        let document = app
            .workspace
            .active_document_mut()
            .expect("demo document exists");
        let table_id = document.active_table().expect("demo table exists").id;
        let view = document.view_mut(table_id).expect("demo view exists");
        view.selection = CellRange::single(CellAddress { row: 2, column: 1 });
        view.scroll = crate::document::ScrollPosition { x: 48.0, y: 96.0 };
        (
            document.id,
            table_id,
            document.revision,
            document.view(table_id).unwrap().selection,
            document.view(table_id).unwrap().scroll,
        )
    };
    app.edit.begin(
        table_id,
        CellAddress { row: 2, column: 1 },
        CellInput::Text("原文".to_owned()),
        crate::document::EditTarget::Cell,
    );
    app.edit.buffer = "編集中の下書き".to_owned();
    let session_id = app.edit.session_id;
    app.font_preset = FontPreset::NotoSansJp;
    app.zoom_factor = 1.25;
    app.theme = ThemeChoice::Dark;
    app.apply_appearance(&ctx);
    let mut output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(480.0, 160.0))),
            ..RawInput::default()
        },
        |ui| {
            for family in [
                grid_font_family(false),
                grid_font_family(true),
                grid_mono_font_family(false),
                grid_mono_font_family(true),
            ] {
                ui.label(
                    egui::RichText::new("ABC xyz 012 日本語 あいう 漢字 ()[] +-=\t\n")
                        .font(FontId::new(16.0, family)),
                );
            }
        },
    );
    output.textures_delta.clear();

    let document = &app.workspace.documents[&document_id];
    assert_eq!(document.revision, revision);
    assert!(!document.is_dirty());
    assert_eq!(document.view(table_id).unwrap().selection, selection);
    assert_eq!(document.view(table_id).unwrap().scroll, scroll);
    assert_eq!(app.edit.session_id, session_id);
    assert_eq!(app.edit.buffer, "編集中の下書き");
    assert_eq!(app.installed_font_preset, FontPreset::NotoSansJp);
}

#[test]
// Verify that zoom shortcuts save their value without changing cell edits.
fn zoom_shortcuts_share_the_saved_zoom_and_leave_cell_edits_untouched() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    app.settings_loaded = true;
    let (document_id, table_id) = {
        let document = app.workspace.active_document().unwrap();
        (document.id, document.active_table().unwrap().id)
    };
    let mut row_layout = GridRowLayout::default();
    row_layout.ensure_row_count(2);
    row_layout.update_row(0, 0, 1, 80.0);
    app.grid_row_layouts
        .insert((document_id, table_id), row_layout);
    let initial_generation = app.font_layout_generation;
    let mut frame = eframe::Frame::_new_kittest();
    let mut time = 0.0;
    run_frame(
        &mut app,
        &ctx,
        &mut frame,
        &mut time,
        vec![key(Key::Plus, Modifiers::COMMAND)],
    );
    run_frame(&mut app, &ctx, &mut frame, &mut time, Vec::new());

    assert!((app.zoom_factor - 1.05).abs() < 0.01);
    assert!((ctx.zoom_factor() - 1.05).abs() < 0.01);
    assert!((app.snapshot_settings().app.zoom_factor - 1.05).abs() < 0.01);
    assert!(app.font_layout_generation > initial_generation);
    assert_eq!(
        app.grid_row_layouts[&(document_id, table_id)].display_generation,
        app.font_layout_generation
    );

    app.edit.begin(
        table_id,
        CellAddress::default(),
        CellInput::Text("source".to_owned()),
        crate::document::EditTarget::Cell,
    );
    app.edit.buffer = "draft".to_owned();
    let session_id = app.edit.session_id;
    run_frame(
        &mut app,
        &ctx,
        &mut frame,
        &mut time,
        vec![key(Key::Plus, Modifiers::COMMAND)],
    );
    assert!((app.zoom_factor - 1.05).abs() < 0.01);
    assert_eq!(app.edit.session_id, session_id);
    assert_eq!(app.edit.buffer, "draft");
}

#[test]
// Verify that document preferences are not reused for changed file contents.
fn preferences_are_not_restored_when_file_content_identity_changes() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let document = app
        .workspace
        .active_document_mut()
        .expect("demo document exists");
    let document_id = document.id;
    let table_id = document.active_table().expect("demo table exists").id;
    let table_name = document.active_table().unwrap().name.clone();
    let address = CellAddress { row: 1, column: 2 };
    let path = std::env::temp_dir().join("texcel-settings-identity.csv");
    let identity = FileIdentity {
        size: 3,
        modified_unix_nanos: None,
        sha256: [1; 32],
    };
    let mut app_preferences = SettingsFile::default();
    app_preferences.upsert_document(DocumentPreferences {
        key: DocumentKey {
            path: normalize_settings_path(&path),
            size: 3,
            sha256: [2; 32],
        },
        display_mode: crate::document::DocumentDisplayMode::Source,
        tables: vec![TablePreferences {
            table_index: 0,
            table_name,
            wrap_mode: WrapMode::On,
            view_layout: Arc::new(TableLayout::default()),
        }],
    });
    app.settings_file = app_preferences;
    app.settings_loaded = true;
    app.file_backings.insert(
        document_id,
        FileBacking {
            path,
            identity,
            source: None,
            overwrite_confirmed: false,
        },
    );

    app.restore_document_preferences(document_id);
    let document = &app.workspace.documents[&document_id];
    assert!(!document.tables[0].is_string_cell(address));
    let view = document.view(table_id).unwrap();
    assert_eq!(
        document.display_mode,
        crate::document::DocumentDisplayMode::Live
    );
    assert!(!view.wrap_text);
}

#[test]
// Verify that save-and-close preserves current CSV view and calculation preferences.
fn save_and_close_preserves_csv_view_and_calculation_settings_for_reopen() {
    let directory = TestDirectory::new();
    let path = directory.0.join("save-close.csv");
    let ctx = egui::Context::default();
    let (mut app, document_id, table_id) = load_csv_test_app_after_confirming_default_settings(
        &ctx,
        path.clone(),
        b"name,value\nfirst,1\n",
    );
    {
        let document = app.workspace.documents.get_mut(&document_id).unwrap();
        document.display_mode = crate::document::DocumentDisplayMode::Source;
        let view = document.view_mut(table_id).unwrap();
        view.wrap_text = true;
        let mut view_layout = TableLayout::default();
        view_layout.apply_value(
            LayoutTarget::Column(0),
            LayoutValue::Column(Some(ColumnLayout { width: Some(31.0) })),
        );
        view.layout = Arc::new(view_layout);
        document
            .set_cell(
                table_id,
                CellAddress { row: 1, column: 0 },
                Some(Cell::text("edited")),
            )
            .expect("edit source content before closing");
    }

    app.save_and_close_document(document_id);
    assert!(app.close_after_save.contains(&document_id));
    wait_for_condition(&mut app, |app| {
        !app.workspace.documents.contains_key(&document_id)
            && !app.pending_saves.contains_key(&document_id)
    });

    let saved_identity = FileIdentity::read(&path).expect("saved CSV identity exists");
    let saved_key = document_key(&path, &saved_identity);
    let saved_table = &app
        .settings_file
        .document(&saved_key)
        .expect("save-and-close captured view preferences")
        .tables[0];
    assert_eq!(
        app.settings_file.document(&saved_key).unwrap().display_mode,
        crate::document::DocumentDisplayMode::Source
    );
    assert_eq!(saved_table.wrap_mode, WrapMode::On);
    assert_eq!(saved_table.view_layout.column(0).unwrap().width, Some(31.0));

    app.open_path(path.clone());
    let reopened_id = confirm_pending_csv_settings_with_test_defaults(&mut app);
    let reopened = &app.workspace.documents[&reopened_id];
    let view = reopened.view(table_id).expect("CSV view was restored");
    assert_eq!(
        reopened.display_mode,
        crate::document::DocumentDisplayMode::Source
    );
    assert!(view.wrap_text);
    assert_eq!(view.layout.column(0).unwrap().width, Some(31.0));
    assert!(!reopened.is_dirty());
}

#[test]
// Verify that app close waits for the newest settings save result.
fn app_close_waits_for_the_latest_settings_save_acknowledgement() {
    let directory = TestDirectory::new();
    let settings_path = directory.0.join("settings.json");
    let ctx = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(ctx.clone());
    let mut app = TexcelApp::new_with_settings_path(&creation, Some(settings_path.clone()));
    let deadline = Instant::now() + Duration::from_secs(3);
    while !app.settings_loaded {
        let _ = run_settings_frame(&mut app, &ctx, Vec::new());
        assert!(Instant::now() < deadline, "settings load did not finish");
        std::thread::sleep(Duration::from_millis(2));
    }
    app.workspace.documents.clear();
    app.workspace.order.clear();
    app.workspace.active_document = None;
    app.theme = ThemeChoice::Dark;
    app.mark_app_preference_change(AppPreferenceField::Theme);

    let close_output = run_settings_frame(&mut app, &ctx, vec![egui::ViewportEvent::Close]);
    assert!(
        close_output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::CancelClose))
    );
    assert!(app.close_after_settings_save);

    let mut acknowledged_close = false;
    while app.close_after_settings_save {
        let output = run_settings_frame(&mut app, &ctx, Vec::new());
        acknowledged_close |= output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::Close));
        assert!(
            Instant::now() < deadline,
            "settings save was not acknowledged"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(acknowledged_close);
    let saved: SettingsFile =
        serde_json::from_slice(&fs::read(settings_path).expect("settings file was written"))
            .expect("settings JSON remains valid");
    assert_eq!(saved.app.theme, ThemeChoice::Dark);
}

#[test]
// Verify that app close asks before discarding a changed document.
fn app_close_cancels_for_unsaved_documents_and_keeps_their_edits() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    app.settings_loaded = true;
    let document = app.workspace.active_document_mut().unwrap();
    let document_id = document.id;
    let table_id = document.active_table;
    document
        .set_cell(
            table_id,
            CellAddress::default(),
            Some(Cell::text("kept after cancel")),
        )
        .unwrap();

    let output = run_settings_frame(&mut app, &ctx, vec![egui::ViewportEvent::Close]);
    assert!(
        output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::CancelClose))
    );
    assert!(app.app_close_dialog_open);
    assert!(app.workspace.documents[&document_id].is_dirty());
    app.app_close_dialog_open = false;
    assert!(app.workspace.documents.contains_key(&document_id));
    assert_eq!(
        app.workspace.documents[&document_id].tables[0]
            .cell(CellAddress::default())
            .unwrap()
            .input,
        CellInput::Text("kept after cancel".to_owned())
    );
}

#[test]
// Verify that a cancelled exit does not let a late save acknowledgement close the app.
fn app_close_cancel_ignores_a_late_save_acknowledgement() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let document_id = app.workspace.active_document.unwrap();
    let token = JobToken {
        document_id,
        revision: 3,
        job_id: 99,
    };
    app.app_close_dialog_open = true;
    app.app_close_save_tokens.insert(document_id, token);

    app.cancel_app_close();
    app.finish_app_close_save(document_id, token, true);

    assert!(!app.app_close_after_documents);
    assert!(!app.app_close_dialog_open);
    assert!(app.workspace.documents.contains_key(&document_id));
}

#[test]
// Verify that hidden-window close prompts return to a visible UI and catch later edits.
fn hidden_window_close_rechecks_edits_before_settings_exit() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let document = app.workspace.active_document_mut().unwrap();
    let document_id = document.id;
    let table_id = document.active_table;
    document
        .set_cell(
            table_id,
            CellAddress::default(),
            Some(Cell::text("new edit while settings save waited")),
        )
        .unwrap();
    app.close_after_settings_save = true;
    let mut frame = eframe::Frame::_new_kittest();
    let mut raw = RawInput::default();
    let viewport = raw.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
    viewport.events.push(egui::ViewportEvent::Close);
    viewport.minimized = Some(true);
    let mut output = ctx.run_ui(raw, |ui| {
        <TexcelApp as eframe::App>::logic(&mut app, ui.ctx(), &mut frame);
    });

    let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
    assert!(
        commands
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::CancelClose))
    );
    assert!(
        commands
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::Visible(true)))
    );
    assert!(
        commands
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::Minimized(false)))
    );
    assert!(app.app_close_dialog_open);
    assert!(app.workspace.documents.contains_key(&document_id));
    output.textures_delta.clear();
}

#[test]
// Verify that settings failure never discards documents during app exit.
fn app_close_keeps_documents_when_settings_save_failed() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let document_id = app.workspace.active_document.unwrap();
    app.settings_save_generation = 4;
    app.settings_failed_generation = Some(4);
    app.discard_all_before_exit();
    app.process_settings(&ctx);
    app.advance_app_close(&ctx);

    assert!(app.app_close_dialog_open);
    assert!(!app.app_close_discard_pending);
    assert!(app.workspace.documents.contains_key(&document_id));
}

#[test]
// Verify that a failed settings write requires a second explicit close request.
fn app_close_allows_second_close_after_settings_failure() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    app.workspace.documents.clear();
    app.workspace.order.clear();
    app.workspace.active_document = None;
    app.settings_loaded = true;
    app.settings_save_generation = 7;
    app.settings_failed_generation = Some(7);

    let first = run_settings_frame(&mut app, &ctx, vec![egui::ViewportEvent::Close]);
    assert!(
        first.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::CancelClose))
    );
    assert_eq!(app.settings_close_attempt_generation, Some(7));
    assert_eq!(app.settings_exit_authorized_generation, None);

    let second = run_settings_frame(&mut app, &ctx, vec![egui::ViewportEvent::Close]);
    assert!(
        !second.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::CancelClose))
    );
    assert_eq!(app.settings_exit_authorized_generation, Some(7));

    let unsaved_id = app.workspace.add_document("Unsaved", &["Data"]);
    let unsaved_table = app.workspace.documents[&unsaved_id].tables[0].id;
    app.workspace
        .documents
        .get_mut(&unsaved_id)
        .expect("the new document is open")
        .set_cell(
            unsaved_table,
            CellAddress::default(),
            Some(Cell::text("requires a save")),
        )
        .expect("make the new document dirty");
    let with_unsaved = run_settings_frame(&mut app, &ctx, vec![egui::ViewportEvent::Close]);
    assert!(
        with_unsaved.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::CancelClose))
    );
    assert!(app.app_close_dialog_open);
    app.mark_settings_write();
    assert_eq!(app.settings_exit_authorized_generation, None);
}

#[test]
// Verify that settings failure keeps dirty and clean backed documents and live input.
fn app_close_settings_failure_keeps_dirty_and_clean_backings() {
    let directory = TestDirectory::new();
    let dirty_path = directory.0.join("dirty.csv");
    let clean_path = directory.0.join("clean.csv");
    let ctx = egui::Context::default();
    let (mut app, dirty_id, table_id) =
        load_csv_test_app_after_confirming_default_settings(&ctx, dirty_path, b"a,b\nold,value\n");
    fs::write(&clean_path, b"x,y\n1,2\n").unwrap();
    app.open_path(clean_path);
    confirm_pending_csv_settings_with_test_defaults(&mut app);
    let clean_id = app.workspace.active_document.unwrap();
    assert!(!app.workspace.documents[&clean_id].is_dirty());
    app.workspace.select_document(dirty_id);
    app.workspace
        .documents
        .get_mut(&dirty_id)
        .unwrap()
        .set_cell(
            table_id,
            CellAddress::default(),
            Some(Cell::text("changed")),
        )
        .unwrap();
    app.edit.begin(
        table_id,
        CellAddress { row: 0, column: 1 },
        CellInput::Text("value".to_owned()),
        crate::document::EditTarget::Cell,
    );
    app.edit_document_id = Some(dirty_id);
    app.edit.buffer = "unfinished".to_owned();
    app.discard_all_before_exit();
    app.process_settings(&ctx);
    app.advance_app_close(&ctx);

    assert!(app.app_close_dialog_open);
    assert!(app.workspace.documents.contains_key(&dirty_id));
    assert!(app.workspace.documents.contains_key(&clean_id));
    assert!(app.file_backings.contains_key(&dirty_id));
    assert!(app.file_backings.contains_key(&clean_id));
    assert!(app.workspace.documents[&dirty_id].is_dirty());
    assert!(!app.workspace.documents[&clean_id].is_dirty());
    assert_eq!(app.edit.buffer, "unfinished");
}

#[test]
// Verify that discard exit waits for recovery-file removal acknowledgement.
fn app_close_discard_waits_for_recovery_remove_ack() {
    let root = TestDirectory::new();
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let document_id = app.workspace.active_document.unwrap();
    let key = "a0000001".to_owned();
    app.recovery_worker = Some(crate::recovery::RecoveryWorker::new(root.0.clone()));
    app.recovery_keys.insert(document_id, key.clone());
    let table_id = app.workspace.documents[&document_id].active_table;
    app.workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .set_cell(
            table_id,
            CellAddress::default(),
            Some(Cell::text("discarded at exit")),
        )
        .unwrap();

    app.discard_all_before_exit();
    app.settings_write_requested = false;
    app.settings_saved_generation = app.settings_save_generation;
    app.advance_app_close(&ctx);
    assert!(app.workspace.documents.contains_key(&document_id));
    assert!(app.in_flight_recovery_removals.contains(&key));

    let deadline = Instant::now() + Duration::from_secs(3);
    while !app.app_close_discard_exit_ready {
        app.process_recovery(&ctx);
        app.advance_app_close(&ctx);
        assert!(
            Instant::now() < deadline,
            "recovery remove acknowledgement timed out"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!app.app_close_waiting_recovery_removal);
    assert!(app.app_close_discard_pending);
    assert!(app.workspace.documents.contains_key(&document_id));
    assert!(app.workspace.documents[&document_id].is_dirty());
    app.schedule_recovery();
    assert!(app.pending_recovery_flushes.is_empty());
    assert!(fs::read_dir(&root.0).unwrap().next().is_none());
}

#[test]
// Verify that a missing recovery worker cancels discard exit instead of waiting forever.
fn app_close_discard_reopens_when_recovery_worker_is_missing() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let document_id = app.workspace.active_document.unwrap();
    app.recovery_keys.insert(document_id, "a0000002".to_owned());

    app.discard_all_before_exit();
    app.settings_write_requested = false;
    app.settings_saved_generation = app.settings_save_generation;
    app.advance_app_close(&ctx);

    assert!(app.workspace.documents.contains_key(&document_id));
    assert!(app.app_close_dialog_open);
    assert_eq!(
        app.status,
        Some(Message::new("status-app-close-recovery-remove-failed"))
    );
}

#[test]
// Verify that an unsent discard remove is cancelled before recovery resumes.
fn app_close_abort_cancels_queued_remove_before_resaving() {
    let root = TestDirectory::new();
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let document_id = app.workspace.active_document.unwrap();
    let key = "a0000004".to_owned();
    let unrelated_key = "a0000005".to_owned();
    app.recovery_worker = Some(crate::recovery::RecoveryWorker::new(root.0.clone()));
    app.recovery_keys.insert(document_id, key.clone());
    let table_id = app.workspace.documents[&document_id].active_table;
    app.workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .set_cell(
            table_id,
            CellAddress::default(),
            Some(Cell::text("first edit")),
        )
        .unwrap();
    app.discard_all_before_exit();
    app.settings_write_requested = false;
    app.settings_saved_generation = app.settings_save_generation;

    // Keep this queued as if the bounded worker had no free request slot.
    app.queue_recovery_remove(key.clone());
    app.app_close_discard_state
        .as_mut()
        .unwrap()
        .queued_removals
        .insert(key.clone());
    app.pending_recovery_removals.insert(unrelated_key.clone());
    app.workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .set_cell(
            table_id,
            CellAddress::default(),
            Some(Cell::text("edit after discard approval")),
        )
        .unwrap();

    app.advance_app_close(&ctx);

    assert!(app.app_close_dialog_open);
    assert!(app.workspace.documents[&document_id].is_dirty());
    assert!(!app.pending_recovery_removals.contains(&key));
    assert!(app.pending_recovery_removals.contains(&unrelated_key));

    app.schedule_recovery();
    app.recovery_schedules
        .get_mut(&document_id)
        .unwrap()
        .changed_at = Instant::now() - Duration::from_secs(3);
    app.schedule_recovery();
    assert!(app.recovery_schedules[&document_id].in_flight.is_some());

    let manifest = root.0.join(format!("{key}.json"));
    let deadline = Instant::now() + Duration::from_secs(3);
    while !manifest.exists() {
        assert!(Instant::now() < deadline, "recovery snapshot was not saved");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
// Verify that Save All commits an inactive recovered cell buffer before saving its document.
fn app_close_save_all_commits_inactive_recovered_cell_buffers() {
    let directory = TestDirectory::new();
    let path = directory.0.join("recovered-cell.csv");
    let ctx = egui::Context::default();
    let (mut app, document_id, table_id) =
        load_csv_test_app_after_confirming_default_settings(&ctx, path, b"a,b\nold,y\n");
    let other_ids = app
        .workspace
        .order
        .iter()
        .copied()
        .filter(|id| *id != document_id)
        .collect::<Vec<_>>();
    for other_id in other_ids {
        app.workspace.remove_document(other_id);
    }
    app.pending_recovered_cell_edits.insert(
        document_id,
        crate::recovery::RecoveryCellEdit {
            table_id,
            address: CellAddress::default(),
            original: CellInput::Text("a".to_owned()),
            buffer: "restored input".to_owned(),
            cursor_byte: 14,
            anchor_byte: 14,
        },
    );

    assert!(app.app_close_needs_save());
    app.app_close_dialog_open = true;
    app.save_all_before_exit();

    assert_eq!(
        app.workspace.documents[&document_id].tables[0]
            .cell(CellAddress::default())
            .unwrap()
            .input,
        CellInput::Text("restored input".to_owned())
    );
    assert!(app.pending_saves.contains_key(&document_id));
    assert!(app.app_close_save_tokens.contains_key(&document_id));
}

#[test]
// Verify that cancelling the second save destination leaves every input untouched.
fn app_close_save_all_cancellation_keeps_buffers_and_undo() {
    let directory = TestDirectory::new();
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let first_id = app.workspace.order[0];
    let first_table = app.workspace.documents[&first_id].active_table;
    app.workspace
        .documents
        .get_mut(&first_id)
        .unwrap()
        .set_cell(
            first_table,
            CellAddress::default(),
            Some(Cell::text("changed cell")),
        )
        .unwrap();
    let second_id = app.workspace.add_document("Second", &["Data"]);
    let second_table = app.workspace.documents[&second_id].active_table;
    app.edit.begin(
        second_table,
        CellAddress::default(),
        CellInput::Text("before".to_owned()),
        crate::document::EditTarget::Cell,
    );
    app.edit_document_id = Some(second_id);
    app.edit.buffer = "typed but not committed".to_owned();
    app.edit.cursor_byte = app.edit.buffer.len();
    app.pending_recovered_cell_edits.insert(
        first_id,
        crate::recovery::RecoveryCellEdit {
            table_id: first_table,
            address: CellAddress { row: 0, column: 1 },
            original: CellInput::Text("old buffer".to_owned()),
            buffer: "recovered buffer".to_owned(),
            cursor_byte: 16,
            anchor_byte: 16,
        },
    );
    let first_revision = app.workspace.documents[&first_id].revision;
    let second_revision = app.workspace.documents[&second_id].revision;
    let session_id = app.edit.session_id;
    let mut picks = 0;

    app.save_all_before_exit_with_path_picker(|_, document_id| {
        picks += 1;
        (picks == 1).then(|| directory.0.join(format!("{document_id}.md")))
    });

    assert_eq!(picks, 2);
    assert_eq!(app.workspace.active_document, Some(second_id));
    assert_eq!(app.edit.session_id, session_id);
    assert_eq!(app.edit.buffer, "typed but not committed");
    assert_eq!(app.edit.cursor_byte, app.edit.buffer.len());
    assert_eq!(app.workspace.documents[&first_id].revision, first_revision);
    assert_eq!(
        app.workspace.documents[&second_id].revision,
        second_revision
    );
    assert!(app.workspace.documents[&first_id].can_undo());
    assert_eq!(
        app.pending_recovered_cell_edits[&first_id].buffer,
        "recovered buffer"
    );
    assert!(app.pending_saves.is_empty());
    assert!(app.app_close_save_tokens.is_empty());
}

#[test]
// Verify that a save conflict opens the overwrite choice and keeps the open document.
fn save_conflict_opens_overwrite_choice_without_losing_edits() {
    let directory = TestDirectory::new();
    let path = directory.0.join("save-conflict.csv");
    let ctx = egui::Context::default();
    let (mut app, document_id, _) =
        load_csv_test_app_after_confirming_default_settings(&ctx, path, b"a,b\nold,y\n");
    let revision = app.workspace.documents[&document_id].revision;
    let token = app.next_job_token(document_id, revision);
    app.pending_saves.insert(document_id, token);
    app.handle_save_conflict(document_id, token);

    assert_eq!(
        app.save_conflict_dialog.map(|prompt| prompt.document_id),
        Some(document_id)
    );
    assert!(!app.file_backings[&document_id].overwrite_confirmed);
    assert!(!app.pending_saves.contains_key(&document_id));
    assert!(app.workspace.documents.contains_key(&document_id));
}

#[test]
// Verify that restoring another document keeps each unfinished cell buffer by document.
fn recovered_cell_buffers_wait_for_their_own_document() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    for buffer in ["first pending", "second pending"] {
        let document_id = app.workspace.reserve_document_id();
        let document = Document::new(document_id, "Recovered", &["Data"]);
        let table_id = document.tables[0].id;
        let calculation = CalculationSnapshot::from_document(&document, false);
        app.install_recovered_document(crate::recovery::RecoveredDocument {
            key: format!("key-{}", document_id),
            generation: 1,
            document,
            cell_edit: Some(crate::recovery::RecoveryCellEdit {
                table_id,
                address: CellAddress::default(),
                original: CellInput::Text("base".to_owned()),
                buffer: buffer.to_owned(),
                cursor_byte: buffer.len(),
                anchor_byte: buffer.len(),
            }),
            original_changed: false,
            requires_initial_save: false,
            original_path: None,
            original_identity: None,
            calculation,
            import_report: None,
        });
    }

    let first_id = app.workspace.order[1];
    let second_id = app.workspace.order[2];
    assert_eq!(app.edit.buffer, "second pending");
    assert_eq!(
        app.pending_recovered_cell_edits[&first_id].buffer,
        "first pending"
    );
    app.commit_edit(None);
    app.workspace.select_document(first_id);
    app.restore_recovered_cell_edit(first_id);
    assert_eq!(app.edit.buffer, "first pending");
    assert_eq!(app.edit_document_id, Some(first_id));
    assert_eq!(app.workspace.active_document, Some(first_id));
    assert!(app.workspace.documents.contains_key(&second_id));
}

#[test]
// Verify that a short recovered editor keeps its UTF-8 cursor and anchor on its first frame.
fn recovered_short_cell_editor_seeds_utf8_cursor_on_first_frame() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let document_id = app.workspace.active_document.unwrap();
    let table_id = app.workspace.documents[&document_id].tables[0].id;
    app.edit.begin(
        table_id,
        CellAddress::default(),
        CellInput::Text("base".to_owned()),
        crate::document::EditTarget::Cell,
    );
    app.edit.buffer = "日a本".to_owned();
    app.edit.cursor_byte = 4;
    app.edit.anchor_byte = 3;
    app.edit_document_id = Some(document_id);
    app.request_editor_focus = true;
    let id = editor_id(document_id, app.edit.session_id);
    let mut frame = eframe::Frame::_new_kittest();
    let mut time = 0.0;
    run_frame(&mut app, &ctx, &mut frame, &mut time, Vec::new());

    let state = egui::TextEdit::load_state(&ctx, id).expect("cell editor state is created");
    let range = state.cursor.char_range().expect("cursor range is seeded");
    assert_eq!(range.primary.index.0, 2);
    assert_eq!(app.edit.anchor_byte, 3);
    assert_eq!(app.edit.cursor_byte, 4);
}

#[test]
// Verify that app exit cannot save or discard text during IME composition.
fn app_close_waits_for_cell_ime_composition() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let document = app.workspace.active_document().unwrap();
    let document_id = document.id;
    let table_id = document.active_table;
    app.edit.begin(
        table_id,
        CellAddress::default(),
        CellInput::Text("source".to_owned()),
        crate::document::EditTarget::Cell,
    );
    app.edit.buffer = "unconfirmed".to_owned();
    app.edit.set_composing(true);
    app.app_close_dialog_open = true;

    app.save_all_before_exit();
    app.discard_all_before_exit();

    assert!(app.app_close_dialog_open);
    assert!(app.app_close_has_active_composition());
    assert!(app.pending_saves.is_empty());
    assert!(app.edit.is_editing());
    assert_eq!(app.edit.buffer, "unconfirmed");
    assert!(app.workspace.documents.contains_key(&document_id));
}

#[test]
// Verify that one failed save holds every document open at exit.
fn app_close_save_failure_keeps_the_document_and_external_file() {
    let directory = TestDirectory::new();
    let path = directory.0.join("changed.csv");
    fs::write(&path, b"old\n").unwrap();
    let identity = FileIdentity::read(&path).unwrap();
    let source = Arc::new(
        crate::csv_adapter::CsvDocument::parse(b"old\n".to_vec(), EncodingChoice::Auto, ',')
            .unwrap(),
    );
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    app.settings_loaded = true;
    let document = app.workspace.active_document_mut().unwrap();
    let document_id = document.id;
    let table_id = document.active_table;
    document
        .set_cell(
            table_id,
            CellAddress::default(),
            Some(Cell::text("local edit")),
        )
        .unwrap();
    document.set_source(Some(Arc::new(SourceDocument::Csv(Arc::clone(&source)))));
    app.file_backings.insert(
        document_id,
        FileBacking {
            path: path.clone(),
            identity,
            source: Some(SourceDocument::Csv(source)),
            overwrite_confirmed: false,
        },
    );
    fs::write(&path, b"external edit\n").unwrap();
    app.app_close_dialog_open = true;
    app.save_all_before_exit();

    let deadline = Instant::now() + Duration::from_secs(3);
    while app.pending_saves.contains_key(&document_id) {
        app.process_file_results();
        assert!(Instant::now() < deadline, "save result did not arrive");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(app.app_close_dialog_open);
    assert!(app.app_close_save_failed);
    assert!(app.workspace.documents.contains_key(&document_id));
    assert!(app.workspace.documents[&document_id].is_dirty());
    assert_eq!(fs::read(path).unwrap(), b"external edit\n");
}

#[test]
// Verify that settings write errors appear in the app status.
fn settings_write_failures_are_returned_to_the_application_status() {
    let directory = TestDirectory::new();
    let ctx = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(ctx.clone());
    let mut app = TexcelApp::new_with_settings_path(&creation, Some(directory.0.clone()));
    let deadline = Instant::now() + Duration::from_secs(3);
    while !app.settings_loaded {
        let _ = run_settings_frame(&mut app, &ctx, Vec::new());
        assert!(Instant::now() < deadline, "settings load did not finish");
        std::thread::sleep(Duration::from_millis(2));
    }
    app.theme = ThemeChoice::Dark;
    app.mark_app_preference_change(AppPreferenceField::Theme);

    while app.settings_failed_generation != Some(app.settings_save_generation) {
        let _ = run_settings_frame(&mut app, &ctx, Vec::new());
        assert!(
            Instant::now() < deadline,
            "settings write failure was not returned"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(app.status.as_ref().is_some_and(|status| {
        status
            .resolve(crate::i18n::Language::Japanese)
            .contains("設定を保存できません")
    }));
}

#[test]
// Verify that visible cells expose accessibility metadata and scrolling actions.
fn visible_grid_cells_publish_accesskit_metadata_and_scroll_action() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    crate::i18n::set_context_language(&ctx, crate::i18n::Language::Japanese);
    let address = CellAddress { row: 2, column: 1 };
    let label = grid_cell_accessibility_label(&ctx, "売上", address, "商品名", "東京", true, true);
    let id = Id::unique("grid-cell-accessibility-test");
    let edit_id = Id::unique("grid-cell-edit-accessibility-test");
    let mut output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(320.0, 240.0))),
            ..RawInput::default()
        },
        |ui| {
            let response = ui.interact(
                Rect::from_min_size(Pos2::ZERO, Vec2::new(120.0, 28.0)),
                id,
                Sense::click(),
            );
            register_grid_cell_accessibility(
                ui.ctx(),
                &response,
                &label,
                "日本語の値",
                address,
                true,
            );
            let mut editing_text = "編集中の値".to_owned();
            let edit_response = ui.add(egui::TextEdit::singleline(&mut editing_text).id(edit_id));
            register_editing_cell_accessibility(ui.ctx(), &edit_response, &label, address, true);
        },
    );
    output.textures_delta.clear();
    let update = output
        .platform_output
        .accesskit_update
        .expect("egui emits the accessibility tree update");
    let (_, node) = update
        .nodes
        .iter()
        .find(|(_, node)| node.role() == egui::Role::GridCell)
        .expect("visible grid cell is in the tree");
    assert!(node.label().unwrap().contains("B3"));
    assert!(node.label().unwrap().contains("見出し"));
    assert!(node.label().unwrap().contains("商品名"));
    assert!(node.label().unwrap().contains("行見出し"));
    assert!(node.label().unwrap().contains("東京"));
    assert!(node.label().unwrap().contains("編集中"));
    assert_eq!(node.value(), Some("日本語の値"));
    assert_eq!(node.row_index(), Some(2));
    assert_eq!(node.column_index(), Some(1));
    assert_eq!(node.is_selected(), Some(true));
    assert!(node.supports_action(egui::accesskit::Action::Focus));
    assert!(node.supports_action(egui::accesskit::Action::Click));
    assert!(node.supports_action(egui::accesskit::Action::ScrollIntoView));
    let (_, edit_node) = update
        .nodes
        .iter()
        .find(|(_, node)| node.role() == egui::Role::TextInput)
        .expect("editing cell remains a text input");
    assert!(edit_node.label().unwrap().contains("B3"));
    assert_eq!(edit_node.value(), Some("編集中の値"));
    assert_eq!(edit_node.row_index(), Some(2));
    assert_eq!(edit_node.column_index(), Some(1));
    assert_eq!(edit_node.is_selected(), Some(true));
    assert!(edit_node.supports_action(egui::accesskit::Action::ScrollIntoView));
}

#[test]
// Verify that accessibility focus selects and reveals both visible and virtual cells.
fn accesskit_focus_requests_select_and_reveal_visible_and_virtual_cells() {
    use egui_kittest::{
        Harness,
        kittest::{By, Queryable},
    };

    let settings_directory = TestDirectory::new();
    let settings_path = settings_directory.0.join("settings.json");
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(move |creation| {
            TexcelApp::new_with_settings_path(creation, Some(settings_path))
        });
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
        document.set_source(Some(Arc::new(SourceDocument::Markdown(source))));
        app.workspace.documents.insert(document_id, document);
        (document_id, table_id)
    };

    // Lay out the loaded sheet before moving away from its first row.
    harness.step();
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

    let target_address = CellAddress {
        row: 199,
        column: 1,
    };
    harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("B200"))
        .next()
        .expect("visible B200 grid cell is exposed")
        .focus();
    harness.step();
    {
        let app = harness.state();
        let view = app.workspace.documents[&document_id]
            .view(table_id)
            .unwrap();
        assert_eq!(view.selection, CellRange::single(target_address));
        assert!(app.grid_focused);
    }

    harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("B200"))
        .next()
        .expect("visible B200 grid cell is exposed")
        .focus();
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

    let app = harness.state();
    let view = app.workspace.documents[&document_id]
        .view(table_id)
        .unwrap();
    assert_eq!(view.selection, CellRange::single(target_address));
    assert!(view.scroll.y > ROW_HEIGHT * 150.0);
    assert!(app.grid_focused);
    let revealed_cell = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("B200"))
        .next()
        .expect("focused B200 grid cell is exposed");
    assert!(revealed_cell.is_focused(), "{revealed_cell:?}");
}

#[test]
// Verify that accessibility headers and Markdown values match rendered cells.
fn accesskit_grid_headers_and_markdown_values_match_the_visible_cells() {
    use egui_kittest::{
        Harness,
        kittest::{By, NodeT, Queryable},
    };

    let settings_directory = TestDirectory::new();
    let settings_path = settings_directory.0.join("settings.json");
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(move |creation| {
            TexcelApp::new_with_settings_path(creation, Some(settings_path))
        });
    let csv_table_id = {
        let app = harness.state_mut();
        let csv_document_id = app.workspace.add_document("No header", &["CSV"]);
        let csv_document = app.workspace.documents.get_mut(&csv_document_id).unwrap();
        let csv_table = &mut csv_document.tables[0];
        csv_table.set_loaded_cell(CellAddress { row: 0, column: 0 }, Cell::text("name"));
        csv_table.set_loaded_cell(CellAddress { row: 0, column: 1 }, Cell::text("city"));
        csv_table.set_loaded_cell(CellAddress { row: 1, column: 0 }, Cell::text("Alice"));
        csv_table.set_loaded_cell(CellAddress { row: 1, column: 1 }, Cell::text("Tokyo"));
        csv_table.id
    };
    harness.step();
    let csv_cell = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("B2"))
        .next()
        .expect("CSV B2 remains in the visible accessibility grid");
    assert!(
        !csv_cell
            .accesskit_node()
            .label()
            .unwrap()
            .contains("見出し")
    );
    assert!(
        !csv_cell
            .accesskit_node()
            .label()
            .unwrap()
            .contains("行見出し")
    );

    {
        let app = harness.state_mut();
        let document_id = app.workspace.active_document.unwrap();
        app.workspace
            .documents
            .get_mut(&document_id)
            .unwrap()
            .view_mut(csv_table_id)
            .unwrap()
            .header_row = Some(0);
    }
    harness.step();
    let csv_cell = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("B2"))
        .next()
        .expect("headered CSV B2 remains visible");
    let csv_label = csv_cell.accesskit_node().label().unwrap();
    assert!(csv_label.contains("見出し"));
    assert!(csv_label.contains("city"));
    assert!(csv_label.contains("行見出し"));
    assert!(csv_label.contains("Alice"));

    let markdown = crate::markdown_adapter::MarkdownDocument::parse(
        "| item | city |\n| --- | --- |\n| 1 | **東京** |\n"
            .as_bytes()
            .to_vec(),
    )
    .expect("parse a Markdown table with visible emphasis");
    let markdown_document = markdown.to_document(300, "Markdown");
    let markdown_table_id = markdown_document.tables[0].id;
    let mut markdown_document = markdown_document;
    markdown_document.set_source(Some(Arc::new(SourceDocument::Markdown(Arc::new(
        markdown.clone(),
    )))));
    markdown_document
        .view_mut(markdown_table_id)
        .unwrap()
        .header_row = Some(0);
    harness
        .state_mut()
        .workspace
        .insert_loaded_document(markdown_document);
    harness.step();
    let markdown_cell = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("B2"))
        .next()
        .expect("Markdown B2 is exposed to accessibility clients");
    assert_eq!(
        markdown_cell.accesskit_node().value(),
        Some("東京".to_owned())
    );
    assert!(
        harness.state().workspace.documents[&300].tables[0]
            .cell(CellAddress { row: 1, column: 1 })
            .is_some_and(|cell| cell.input.source_text() == "**東京**")
    );
}

#[test]
// Verify appearance rendering and save screenshots when a WGPU adapter is available.
fn kittest_checks_appearance_and_saves_pngs_when_wgpu_is_available() {
    use egui_kittest::Harness;
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    use egui_kittest::kittest::{By, Queryable};
    use std::panic::{AssertUnwindSafe, catch_unwind};

    let settings_directory = TestDirectory::new();
    let settings_path = settings_directory.0.join("settings.json");
    let mut harness = Harness::builder()
        .with_size([1000.0, 760.0])
        .with_max_steps(10)
        .build_eframe(move |creation| {
            TexcelApp::new_with_settings_path(creation, Some(settings_path))
        });
    let (document_id, table_id, revision, selection, scroll, session_id) = {
        let app = harness.state_mut();
        let document = app.workspace.active_document().unwrap();
        let table_id = document.active_table().unwrap().id;
        let document_id = document.id;
        let view = document.view(table_id).unwrap();
        app.edit.begin(
            table_id,
            view.selection.head,
            CellInput::Text("編集中".to_owned()),
            crate::document::EditTarget::Cell,
        );
        app.edit.buffer = "未確定の日本語入力".to_owned();
        let session_id = app.edit.session_id;
        app.font_preset = FontPreset::NotoSansJp;
        app.theme = ThemeChoice::Dark;
        app.zoom_factor = 1.15;
        let document = &app.workspace.documents[&document_id];
        let view = document.view(table_id).unwrap();
        (
            document_id,
            table_id,
            document.revision,
            view.selection,
            view.scroll,
            session_id,
        )
    };
    let ctx = harness.ctx.clone();
    harness.state_mut().apply_appearance(&ctx);
    harness.step();
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let states = harness.state().native_menu_states_for_frame(
            crate::ui::top::TopActionState::default(),
            false,
            &harness.ctx,
        );
        assert!(states["app.settings"].enabled);
        let target = harness.state().native_action_target(&harness.ctx, false);
        harness.state_mut().native_menu_events_for_test =
            vec![crate::native_menu::NativeMenuEvent {
                id: "app.settings".to_owned(),
                recent_path: None,
                target,
            }];
        harness.step();
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let file_label = crate::i18n::context_text(&harness.ctx, "menu-file");
        harness
            .query_all(By::new().role(egui::Role::Button).label(&file_label))
            .next()
            .expect("the File menu is visible")
            .click_accesskit();
        harness.run_steps(2);
        let settings_label = crate::i18n::context_text(&harness.ctx, "action-settings");
        harness
            .query_all(By::new().label(&settings_label))
            .next()
            .expect("the File menu offers Settings")
            .click();
        harness.run_steps(2);
    }

    assert!(harness.state().settings_window_open);
    let appearance_rect = harness
        .ctx
        .memory(|memory| memory.area_rect(Id::unique("texcel_appearance_settings")));
    assert!(appearance_rect.is_some_and(|rect| rect.width() > 300.0 && rect.height() > 400.0));
    assert!(harness.ctx.style_of(harness.ctx.theme()).visuals.dark_mode);
    assert!((harness.ctx.zoom_factor() - 1.15).abs() < 0.01);
    {
        let app = harness.state();
        let document = &app.workspace.documents[&document_id];
        assert_eq!(document.revision, revision);
        assert!(!document.is_dirty());
        assert_eq!(document.view(table_id).unwrap().selection, selection);
        assert_eq!(document.view(table_id).unwrap().scroll, scroll);
        assert_eq!(app.edit.session_id, session_id);
        assert_eq!(app.edit.buffer, "未確定の日本語入力");
        assert_eq!(app.installed_font_preset, FontPreset::NotoSansJp);
    }

    let render_directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("kittest-renders");
    fs::create_dir_all(&render_directory).expect("create temporary render directory");
    let rendered_dark = match catch_unwind(AssertUnwindSafe(|| harness.render())) {
        Ok(Ok(image)) => {
            image
                .save(render_directory.join("appearance-dark.png"))
                .expect("save dark appearance image");
            true
        }
        Ok(Err(error)) => {
            eprintln!("wgpu appearance render unavailable: {error}");
            false
        }
        Err(_) => {
            eprintln!("wgpu appearance render unavailable: no usable adapter");
            false
        }
    };

    let ctx = harness.ctx.clone();
    {
        let app = harness.state_mut();
        app.theme = ThemeChoice::Light;
        app.apply_appearance(&ctx);
    }
    harness.step();
    assert!(!harness.ctx.style_of(harness.ctx.theme()).visuals.dark_mode);
    if rendered_dark {
        match catch_unwind(AssertUnwindSafe(|| harness.render())) {
            Ok(Ok(image)) => image
                .save(render_directory.join("appearance-light.png"))
                .expect("save light appearance image"),
            Ok(Err(error)) => eprintln!("wgpu light render unavailable: {error}"),
            Err(_) => eprintln!("wgpu light render unavailable: renderer failed"),
        }
    }
}

#[test]
// Verify that invisible markers and font fallback order affect display only.
fn invisible_markers_and_font_fallback_order_are_display_only() {
    let source = "A B\t日本\r\nあ\u{00a0}\u{200b}";
    let visible = visualize_invisibles(source, true);
    assert_eq!(visible, "A·B⇥日本↵\nあ⍽⟂");
    assert_eq!(visualize_invisibles(source, false), source);
    assert_eq!(source, "A B\t日本\r\nあ\u{00a0}\u{200b}");

    let fallback = vec!["egui-latin".to_owned(), "egui-mono".to_owned()];
    assert_eq!(
        font_fallback_order(FontPreset::Default, &fallback, "noto", false),
        ["egui-latin", "egui-mono", "noto"]
    );
    assert_eq!(
        font_fallback_order(FontPreset::Default, &fallback, "noto-bold", true),
        ["noto-bold", "egui-latin", "egui-mono"]
    );
    assert_eq!(
        font_fallback_order(FontPreset::NotoSansJp, &fallback, "noto", false),
        ["noto", "egui-latin", "egui-mono"]
    );
}

#[test]
// Verify that automatic fonts follow the active language and explicit choices stay fixed.
fn automatic_font_preset_tracks_language_changes() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    app.apply_app_preferences(
        &ctx,
        &AppPreferences {
            language: crate::settings::LanguageChoice::Japanese,
            font_preset: FontPreset::Auto,
            ..AppPreferences::default()
        },
    );
    assert_eq!(app.font_preset, FontPreset::Auto);
    assert_eq!(app.installed_font_preset, FontPreset::NotoSansJp);

    app.apply_app_preferences(
        &ctx,
        &AppPreferences {
            language: crate::settings::LanguageChoice::English,
            font_preset: FontPreset::Auto,
            ..AppPreferences::default()
        },
    );
    assert_eq!(app.installed_font_preset, FontPreset::Default);

    app.apply_app_preferences(
        &ctx,
        &AppPreferences {
            language: crate::settings::LanguageChoice::Japanese,
            font_preset: FontPreset::Default,
            ..AppPreferences::default()
        },
    );
    assert_eq!(app.installed_font_preset, FontPreset::Default);
}

#[test]
// Verify that app-wide appearance changes invalidate every background window's grid cache.
fn shared_appearance_change_invalidates_background_window_layouts() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let active = egui::ViewportId::from_hash_of("active-window");
    let background = egui::ViewportId::from_hash_of("background-window");
    app.multi_window_mode = true;
    app.current_viewport = Some(active);
    app.window_states.insert(
        active,
        crate::ui::window_state::WindowState {
            font_layout_generation: 3,
            ..crate::ui::window_state::WindowState::default()
        },
    );
    app.window_states.insert(
        background,
        crate::ui::window_state::WindowState {
            font_layout_generation: 7,
            ..crate::ui::window_state::WindowState::default()
        },
    );

    app.invalidate_shared_window_appearance(&ctx);

    assert_eq!(app.window_states[&active].font_layout_generation, 3);
    assert_eq!(app.window_states[&background].font_layout_generation, 8);
}

#[test]
// Verify that unchanged formula-like text remains text after F2 and Enter.
fn unchanged_formula_looking_text_remains_text_after_f2_enter() {
    let source = CellInput::Text("=SUM(A1:A5)".to_owned());
    let existing = Cell::text("=SUM(A1:A5)");
    assert_eq!(changed_editor_input(source.clone(), Some(&existing)), None);
    assert_eq!(
        changed_editor_input(source.clone(), Some(&Cell::text("old"))),
        Some(source)
    );
}

#[test]
// Verify that an old worker error reschedules current work but a current error stops retrying.
fn old_failure_ack_schedules_new_revision_but_same_revision_error_stops() {
    assert!(should_schedule_after_calculation_ack(2, 1, true, true));
    assert!(!should_schedule_after_calculation_ack(1, 1, true, true));
    assert!(!should_schedule_after_calculation_ack(2, 1, false, true));
}

#[test]
// Verify that export and document save cannot run at the same time.
fn export_and_save_operations_are_mutually_excluded() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let active_id = app.workspace.active_document().expect("demo document").id;
    let other_id = active_id.saturating_add(10_000);
    let other_token = JobToken {
        document_id: other_id,
        revision: 0,
        job_id: 1,
    };
    app.pending_exports.insert(other_id, other_token);
    app.start_export_preflight(
        active_id,
        ExportOptions {
            format: ExportFormat::Csv,
            scope: ExportScope::AllTables,
            formulas: FormulaOutput::Formula,
            modifiers: ModifierOutput::Convert,
            markdown_header: MarkdownHeader::UseFirstRow,
        },
        PathBuf::from("out.csv"),
    );
    assert_eq!(app.pending_exports.len(), 1);
    assert!(!app.pending_exports.contains_key(&active_id));

    app.pending_exports.clear();
    app.pending_exports.insert(
        active_id,
        JobToken {
            document_id: active_id,
            revision: 0,
            job_id: 2,
        },
    );
    app.save_active(false);
    assert!(!app.pending_saves.contains_key(&active_id));
    assert!(app.status.as_ref().is_some_and(|status| {
        status
            .resolve(crate::i18n::Language::Japanese)
            .contains("エクスポート中")
    }));
}

#[test]
// Verify that a normal edit before Markdown save rebases a promoted CSV overlay.
fn csv_formatted_paste_markdown_save_rebases_overlay_after_normal_edit() {
    let directory = TestDirectory::new();
    let path = directory.0.join("overlay.csv");
    let markdown_path = directory.0.join("overlay.md");
    fs::write(&path, b"old,\n").expect("create CSV input");
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    app.open_path(path.clone());
    let document_id = confirm_pending_csv_settings_with_test_defaults(&mut app);
    let table_id = app.workspace.documents[&document_id]
        .active_table()
        .unwrap()
        .id;
    let first = CellAddress { row: 0, column: 0 };
    let second = CellAddress { row: 0, column: 1 };
    submit_test_paste(
        &mut app,
        document_id,
        table_id,
        CellRange::single(first),
        "old",
        Some("<table><tr><td style=\"font-weight:bold\">old</td></tr></table>".to_owned()),
    );
    let document = &app.workspace.documents[&document_id];
    let Some(SourceDocument::Markdown(markdown)) = document.source.as_deref() else {
        panic!("formatted paste promotes the CSV document to Markdown");
    };
    assert_eq!(markdown.tables[0].rows[0][0].value, "**old**");
    assert!(
        markdown
            .cell_display(0, 0, 0)
            .unwrap()
            .runs
            .iter()
            .any(|run| run.bold)
    );
    assert!(matches!(
        document.source.as_deref(),
        Some(SourceDocument::Markdown(_))
    ));

    app.begin_cell_edit(table_id, first);
    app.edit.buffer = "new".to_owned();
    app.edit.mark_buffer_changed();
    app.commit_edit(None);
    app.save_as_to_path(document_id, markdown_path.clone());
    wait_for_condition(&mut app, |app| {
        !app.pending_saves.contains_key(&document_id)
    });
    let document = &app.workspace.documents[&document_id];
    assert_eq!(document.text_overlay_revision, document.saved_revision);
    assert!(document.text_overlay_cells().is_empty());
    assert!(
        fs::read(&markdown_path)
            .expect("read saved Markdown")
            .windows(b"new".len())
            .any(|window| window == b"new")
    );
    assert_eq!(
        document.tables[0].cell(first).unwrap().input.source_text(),
        "new"
    );

    submit_test_paste(
        &mut app,
        document_id,
        table_id,
        CellRange::single(second),
        "next",
        None,
    );
    let first_cell = app.workspace.documents[&document_id].tables[0]
        .cell(first)
        .unwrap();
    assert_eq!(first_cell.input.source_text(), "new");
    let Some(SourceDocument::Markdown(markdown)) =
        app.workspace.documents[&document_id].source.as_deref()
    else {
        panic!("the saved document retains its Markdown source");
    };
    assert!(
        !markdown
            .cell_display(0, 0, 0)
            .unwrap()
            .runs
            .iter()
            .any(|run| run.bold)
    );
    assert_eq!(
        app.workspace.documents[&document_id].tables[0]
            .cell(second)
            .unwrap()
            .input
            .source_text(),
        "next"
    );
}

#[test]
// Verify that a style clear after Markdown save remains a dirty overlay change.
fn csv_formatted_paste_markdown_save_keeps_later_style_clear_dirty() {
    let directory = TestDirectory::new();
    let path = directory.0.join("overlay-race.csv");
    let markdown_path = directory.0.join("overlay-race.md");
    fs::write(&path, b"old,\n").expect("create CSV input");
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    app.open_path(path);
    let document_id = confirm_pending_csv_settings_with_test_defaults(&mut app);
    let table_id = app.workspace.documents[&document_id]
        .active_table()
        .unwrap()
        .id;
    let first = CellAddress { row: 0, column: 0 };
    let second = CellAddress { row: 0, column: 1 };
    submit_test_paste(
        &mut app,
        document_id,
        table_id,
        CellRange::single(first),
        "old",
        Some("<table><tr><td style=\"font-weight:bold\">old</td></tr></table>".to_owned()),
    );
    let Some(SourceDocument::Markdown(markdown)) =
        app.workspace.documents[&document_id].source.as_deref()
    else {
        panic!("formatted paste promotes the CSV document to Markdown");
    };
    assert_eq!(markdown.tables[0].rows[0][0].value, "**old**");
    assert!(
        markdown
            .cell_display(0, 0, 0)
            .unwrap()
            .runs
            .iter()
            .any(|run| run.bold)
    );
    assert!(matches!(
        app.workspace.documents[&document_id].source.as_deref(),
        Some(SourceDocument::Markdown(_))
    ));

    let save_revision = app.workspace.documents[&document_id].revision;
    app.save_as_to_path(document_id, markdown_path.clone());
    assert!(app.pending_saves.contains_key(&document_id));
    app.begin_cell_edit(table_id, first);
    app.apply_markdown_modifier(CellFormatAction::Bold, &ctx);
    assert_eq!(app.edit.buffer, "old");
    app.commit_edit(None);
    assert!(app.workspace.documents[&document_id].revision > save_revision);
    wait_for_condition(&mut app, |app| {
        !app.pending_saves.contains_key(&document_id)
    });
    let document = &app.workspace.documents[&document_id];
    assert_eq!(document.text_overlay_revision, save_revision);
    assert_eq!(document.saved_revision, save_revision);
    assert!(document.is_dirty(), "later style edit remains unsaved");
    assert!(document.cell_input_is_dirty(table_id, first));
    let saved_markdown = crate::markdown_adapter::MarkdownDocument::parse(
        fs::read(markdown_path).expect("read the save snapshot"),
    )
    .expect("saved Markdown parses");
    assert_eq!(saved_markdown.tables[0].rows[0][0].value, "**old**");
    assert!(
        saved_markdown
            .cell_display(0, 0, 0)
            .unwrap()
            .runs
            .iter()
            .any(|run| run.bold)
    );
    assert_eq!(
        document.tables[0].cell(first).unwrap().input.source_text(),
        "old"
    );
    let document = app.workspace.documents.get_mut(&document_id).unwrap();
    document
        .undo()
        .expect("Undo restores the saved Markdown emphasis");
    assert_eq!(
        document.tables[0].cell(first).unwrap().input.source_text(),
        "**old**"
    );
    document.redo().expect("Redo removes the Markdown emphasis");
    assert_eq!(
        document.tables[0].cell(first).unwrap().input.source_text(),
        "old"
    );

    submit_test_paste(
        &mut app,
        document_id,
        table_id,
        CellRange::single(second),
        "next",
        None,
    );
    let first_cell = app.workspace.documents[&document_id].tables[0]
        .cell(first)
        .unwrap();
    assert_eq!(first_cell.input.source_text(), "old");
    assert!(app.workspace.documents[&document_id].is_dirty());
}

#[test]
// Verify that name-box Enter and row-header actions preserve grid keyboard focus.
fn name_box_enter_and_row_header_keep_grid_keyboard_focus() {
    use egui_kittest::kittest::{By, NodeT, Queryable};

    let directory = super::scenario_tests::ScenarioDirectory::new();
    let mut harness = super::scenario_tests::scenario_harness(directory.0.join("settings.json"));
    let (document_id, table_id) =
        install_test_markdown_grid(harness.state_mut(), 2, 2, "Name box focus");
    harness.step();

    let scenario = super::scenario_tests::click_point;
    let name_box_id = crate::ui::name_box_id(document_id);
    let name_box = harness
        .query_all(By::new().role(egui::Role::TextInput))
        .find(|node| node.value().as_deref() == Some("A1"))
        .expect("the name box shows the selected A1 cell");
    let name_box_rect = name_box.rect();
    scenario(
        &mut harness,
        Pos2::new(name_box_rect.right() - 3.0, name_box_rect.center().y),
    );
    super::scenario_tests::wait_until(&mut harness, |app| app.pending_edit_input.is_none());
    assert_eq!(
        harness.ctx.memory(|memory| memory.focused()),
        Some(name_box_id)
    );
    super::scenario_tests::send_event(
        &mut harness,
        key(
            Key::A,
            Modifiers {
                command: true,
                ..Modifiers::NONE
            },
        ),
    );
    super::scenario_tests::send_event(&mut harness, Event::Text("B1".to_owned()));
    assert_eq!(harness.state().name_box, "B1");
    super::scenario_tests::send_event(&mut harness, key(Key::Enter, Modifiers::NONE));
    let selected_cell = CellRange::single(CellAddress { row: 0, column: 1 });
    super::scenario_tests::wait_until(&mut harness, |app| {
        app.pending_edit_input.is_none()
            && app.workspace.documents[&document_id]
                .view(table_id)
                .is_some_and(|view| view.selection == selected_cell)
    });
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .view(table_id)
            .unwrap()
            .selection,
        selected_cell
    );
    let focused = harness
        .ctx
        .memory(|memory| memory.focused())
        .expect("Name box Enter restores keyboard focus to the grid");
    let focused_cell = harness
        .query_all(By::new().role(egui::Role::GridCell))
        .find(|node| node.accesskit_node().is_focused())
        .expect("the selected grid cell owns keyboard focus");
    assert_eq!(
        u128::from(focused_cell.accesskit_node().id()) >> 64,
        u128::from(focused.accesskit_id().0)
    );

    let a1 = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("A1"))
        .next()
        .expect("A1 provides the actual row position");
    let a1_rect = a1.rect();
    scenario(
        &mut harness,
        Pos2::new(a1_rect.left() - ROW_HEADER_WIDTH / 2.0, a1_rect.center().y),
    );
    let row_selection = CellRange {
        anchor: CellAddress { row: 0, column: 0 },
        head: CellAddress {
            row: 0,
            column: MAX_COLUMNS - 1,
        },
    };
    super::scenario_tests::wait_until(&mut harness, |app| {
        app.pending_edit_input.is_none()
            && app.workspace.documents[&document_id]
                .view(table_id)
                .is_some_and(|view| view.selection == row_selection)
    });
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .view(table_id)
            .unwrap()
            .selection,
        row_selection
    );
    assert!(
        harness.state().grid_focused,
        "row-header focus keeps grid keys active"
    );
    super::scenario_tests::send_event(&mut harness, key(Key::Delete, Modifiers::NONE));
    assert!(
        harness.state().grid_focused,
        "row-header keeps focus after Delete input"
    );
    for address in [
        CellAddress { row: 0, column: 0 },
        CellAddress { row: 0, column: 1 },
    ] {
        assert!(
            harness.state().workspace.documents[&document_id].tables[0]
                .cell(address)
                .is_none_or(|cell| cell.input == CellInput::Empty)
        );
    }
}

// Run one app frame with the supplied input events.
fn run_frame(
    app: &mut TexcelApp,
    ctx: &egui::Context,
    frame: &mut eframe::Frame,
    time: &mut f64,
    events: Vec<Event>,
) {
    run_frame_sized(app, ctx, frame, time, events, Vec2::new(900.0, 620.0));
}

// Run an app frame at a requested viewport size.
fn run_frame_sized(
    app: &mut TexcelApp,
    ctx: &egui::Context,
    frame: &mut eframe::Frame,
    time: &mut f64,
    events: Vec<Event>,
    screen_size: Vec2,
) {
    *time += 1.0 / 60.0;
    let mut raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, screen_size)),
        time: Some(*time),
        events,
        ..RawInput::default()
    };
    // Native eframe invokes this hook before InputState derives pointer and focus state.
    eframe::App::raw_input_hook(app, ctx, &mut raw);
    ctx.run_ui(raw, |ui| eframe::App::ui(app, ui, frame))
        .textures_delta
        .clear();
}

// Create a keyboard event for a test key press.
fn key(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

// Create a Texcel app with isolated test settings.
pub(super) fn new_app(ctx: &egui::Context) -> TexcelApp {
    let creation = eframe::CreationContext::_new_kittest(ctx.clone());
    TexcelApp::new_with_settings_path(&creation, None)
}

#[test]
// Verify that product document creation makes one named Markdown table.
fn new_document_starts_with_a_named_markdown_table() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    app.language_choice = crate::settings::LanguageChoice::English;
    app.language = crate::i18n::Language::English;
    crate::i18n::set_context_language(&ctx, app.language);
    app.create_new_document("  Draft  ".to_owned())
        .expect("create a Markdown document");
    let document = app
        .workspace
        .active_document()
        .expect("new document becomes active");
    assert_eq!(document.name, "Draft");
    assert_eq!(document.tables.len(), 1);
    assert!(document.tables[0].row_count() > 0);
    assert!(document.tables[0].column_count() > 0);
    let Some(SourceDocument::Markdown(source)) = document.source.as_deref() else {
        panic!("new document starts with a Markdown source");
    };
    assert_eq!(source.tables.len(), 1);
    assert!(!app.file_backings.contains_key(&document.id));
}

// Install one cell with known content for search dialog tests.
fn install_search_test_cell(app: &mut TexcelApp) -> (DocumentId, u64, CellRange) {
    let document = app
        .workspace
        .active_document_mut()
        .expect("test workspace has an active document");
    let table_id = document.active_table;
    let selection = CellRange::single(CellAddress::default());
    document
        .set_cell(table_id, CellAddress::default(), Some(Cell::text("needle")))
        .expect("search test cell is within the logical sheet");
    document
        .view_mut(table_id)
        .expect("test table view exists")
        .selection = selection;
    (document.id, table_id, selection)
}

// Create a search dialog state for the active test document.
fn search_test_dialog(document_id: DocumentId, table_id: u64, query: &str) -> SearchDialogState {
    SearchDialogState {
        document_id,
        table_id,
        query: query.to_owned(),
        replacement: "replaced".to_owned(),
        scope: SearchScope::Selection,
        representation: crate::structure::SearchRepresentation::Source,
        case_sensitive: true,
        regular_expression: false,
        whole_cell: true,
        replace_mode: true,
        message: None,
        preview: None,
        preview_key: None,
        focus_query: false,
    }
}

#[test]
// Verify that a pending search response is ignored after the query changes.
fn pending_search_response_is_discarded_after_query_changes() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let (document_id, table_id, _) = install_search_test_cell(&mut app);
    let state = search_test_dialog(document_id, table_id, "needle");
    app.search_dialog = Some(search_test_dialog(document_id, table_id, "needle"));
    app.submit_search(&state);
    assert!(app.pending_searches.contains_key(&document_id));

    app.search_dialog.as_mut().unwrap().query = "changed".to_owned();
    wait_for_condition(&mut app, |app| {
        !app.pending_searches.contains_key(&document_id)
    });
    let dialog = app
        .search_dialog
        .as_ref()
        .expect("search dialog remains open");
    assert!(dialog.preview.is_none());
    assert!(dialog.preview_key.is_none());
    assert!(dialog.message.as_ref().is_some_and(|message| {
        message
            .resolve(crate::i18n::Language::Japanese)
            .contains("条件が変わりました")
    }));
    assert_eq!(
        app.workspace.documents[&document_id].tables[0]
            .cell(CellAddress::default())
            .unwrap()
            .input,
        CellInput::Text("needle".to_owned())
    );
}

#[test]
// Verify that a search preview cannot apply after its selection changes.
fn search_preview_cannot_apply_after_selection_changes() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let (document_id, table_id, _) = install_search_test_cell(&mut app);
    let state = search_test_dialog(document_id, table_id, "needle");
    app.search_dialog = Some(search_test_dialog(document_id, table_id, "needle"));
    app.submit_search(&state);
    wait_for_condition(&mut app, |app| {
        !app.pending_searches.contains_key(&document_id)
    });

    let preview = app
        .search_dialog
        .as_mut()
        .and_then(|dialog| dialog.preview.take())
        .expect("matching search result is shown as a preview");
    let prepared = preview.prepared.expect("replacement has a prepared root");
    app.workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .view_mut(table_id)
        .unwrap()
        .selection = CellRange::single(CellAddress { row: 0, column: 1 });
    app.apply_search_prepared(
        document_id,
        prepared,
        preview.document_revision,
        preview.calculation_revision,
    );

    assert_eq!(
        app.workspace.documents[&document_id].tables[0]
            .cell(CellAddress::default())
            .unwrap()
            .input,
        CellInput::Text("needle".to_owned())
    );
    let dialog = app
        .search_dialog
        .as_ref()
        .expect("dialog reports the stale preview");
    assert!(dialog.preview.is_none());
    assert!(dialog.message.as_ref().is_some_and(|message| {
        message
            .resolve(crate::i18n::Language::Japanese)
            .contains("選択範囲")
    }));
}

#[test]
// Verify that a worker rejection reason stays visible in the data edit dialog.
fn data_edit_submit_keeps_worker_rejection_reason_in_its_dialog() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let document = app.workspace.active_document().unwrap();
    let document_id = document.id;
    let table_id = document.active_table;
    let range = CellRange::single(CellAddress::default());
    let mut dialog = DataEditDialogState {
        document_id,
        table_id,
        kind: DataEditKind::SplitColumn,
        range,
        range_text: "not-a-range".to_owned(),
        selected_ranges: vec![range],
        delimiter: "|".to_owned(),
        destination_column: "B".to_owned(),
        overwrite: false,
        sort_keys: vec![SortKeyEditor {
            column: "A".to_owned(),
            kind: crate::structure::SortValueKind::Text,
            descending: false,
        }],
        preview: None,
        preview_key: None,
        message: None,
    };
    let state = dialog.submission_snapshot();
    app.data_edit_dialog = Some(dialog.submission_snapshot());
    app.submit_data_edit(&state);
    dialog = app
        .data_edit_dialog
        .take()
        .expect("the submit error remains attached to the open dialog");
    assert!(dialog.message.as_ref().is_some_and(|message| {
        message
            .resolve(crate::i18n::Language::Japanese)
            .contains("A1:B2")
    }));
}

#[test]
// Verify that a pending data edit is discarded when the selection changes.
fn pending_data_edit_is_discarded_after_selection_changes() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let document = app.workspace.active_document_mut().unwrap();
    let document_id = document.id;
    let table_id = document.active_table;
    let range = CellRange::single(CellAddress::default());
    document
        .set_cell(
            table_id,
            CellAddress::default(),
            Some(Cell::text("  trim me  ")),
        )
        .expect("trim test cell is in range");
    document.view_mut(table_id).unwrap().selection = range;
    let dialog = DataEditDialogState {
        document_id,
        table_id,
        kind: DataEditKind::TrimWhitespace,
        range,
        range_text: "A1:A1".to_owned(),
        selected_ranges: vec![range],
        delimiter: "|".to_owned(),
        destination_column: "B".to_owned(),
        overwrite: false,
        sort_keys: Vec::new(),
        preview: None,
        preview_key: None,
        message: None,
    };
    let state = dialog.submission_snapshot();
    app.data_edit_dialog = Some(dialog);
    app.submit_data_edit(&state);
    assert!(app.pending_data_edits.contains_key(&document_id));

    app.workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .view_mut(table_id)
        .unwrap()
        .selection = CellRange::single(CellAddress { row: 0, column: 1 });
    wait_for_condition(&mut app, |app| {
        !app.pending_data_edits.contains_key(&document_id)
    });
    let dialog = app.data_edit_dialog.as_ref().expect("dialog remains open");
    assert!(dialog.preview.is_none());
    assert!(dialog.preview_key.is_none());
    assert!(dialog.message.as_ref().is_some_and(|message| {
        message
            .resolve(crate::i18n::Language::Japanese)
            .contains("選択範囲")
    }));
    assert_eq!(
        app.workspace.documents[&document_id].tables[0]
            .cell(CellAddress::default())
            .unwrap()
            .input,
        CellInput::Text("  trim me  ".to_owned())
    );
}

#[test]
// Verify that a failed save during close leaves the document open and dirty.
fn conflicting_save_before_close_keeps_the_document_open_and_dirty() {
    let directory = TestDirectory::new();
    let path = directory.0.join("document.md");
    let markdown = Arc::new(
        crate::markdown_adapter::MarkdownDocument::parse(
            b"| Value |\n| --- |\n| original |\n".to_vec(),
        )
        .expect("parse original Markdown source"),
    );
    fs::write(&path, markdown.source_bytes()).expect("create original save target");
    let identity = FileIdentity::read(&path).expect("capture original identity");
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let document_id = 1;
    let mut document = markdown.to_document(document_id, "document");
    document.set_source(Some(Arc::new(SourceDocument::Markdown(Arc::clone(
        &markdown,
    )))));
    document
        .set_cell(
            1,
            CellAddress { row: 1, column: 0 },
            Some(Cell::text("edited")),
        )
        .expect("make the document dirty");
    let mut workspace = Workspace::default();
    workspace.insert_loaded_document(document);
    app.workspace = workspace;
    app.file_backings.insert(
        document_id,
        FileBacking {
            path: path.clone(),
            identity,
            source: Some(SourceDocument::Markdown(markdown)),
            overwrite_confirmed: false,
        },
    );
    fs::write(&path, b"external\n").expect("simulate an external replacement");

    app.save_active(false);
    assert!(app.pending_saves.contains_key(&document_id));
    app.request_close_document(document_id);
    assert!(app.close_after_save.contains(&document_id));
    wait_for_condition(&mut app, |app| {
        !app.pending_saves.contains_key(&document_id)
    });

    // The conflict dialog replaces the close dialog and remembers the close request.
    assert!(app.workspace.documents.contains_key(&document_id));
    assert!(app.pending_close_document.is_none());
    assert_eq!(
        app.save_conflict_dialog,
        Some(super::save_conflict::SaveConflictPrompt {
            document_id,
            close_after_save: true,
        })
    );
    assert!(app.workspace.documents[&document_id].is_dirty());
    assert_eq!(
        fs::read(&path).expect("failed save leaves external target"),
        b"external\n"
    );
}

#[test]
// Verify that a delayed save keeps an uncommitted edit with its original document.
fn delayed_save_does_not_close_or_redirect_an_uncommitted_edit_to_another_document() {
    let directory = TestDirectory::new();
    let path = directory.0.join("first.csv");
    fs::write(&path, b"first\n").expect("create first CSV");
    let identity = FileIdentity::read(&path).expect("capture first CSV identity");
    let source = Arc::new(
        crate::csv_adapter::CsvDocument::parse(b"first\n".to_vec(), EncodingChoice::Auto, ',')
            .expect("parse first CSV"),
    );
    let mut first = source.to_document(1, "first.csv");
    first.set_source(Some(Arc::new(SourceDocument::Csv(Arc::clone(&source)))));
    let mut second = crate::document::Document::new(2, "second", &["Sheet1"]);
    second
        .set_cell(1, CellAddress::default(), Some(Cell::text("other")))
        .expect("set second document cell");
    let mut workspace = Workspace::default();
    workspace.insert_loaded_document(first);
    workspace.insert_loaded_document(second);
    workspace.select_document(1);

    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    app.workspace = workspace;
    app.file_backings.insert(
        1,
        FileBacking {
            path,
            identity,
            source: Some(SourceDocument::Csv(source)),
            overwrite_confirmed: false,
        },
    );
    app.save_active(false);
    assert!(app.pending_saves.contains_key(&1));
    app.begin_cell_edit(1, CellAddress::default());
    app.edit.buffer = "uncommitted".to_owned();
    app.request_close_document(1);
    assert!(app.close_after_save.contains(&1));

    wait_for_condition(&mut app, |app| !app.pending_saves.contains_key(&1));
    assert!(app.workspace.documents.contains_key(&1));
    assert_eq!(app.pending_close_document, Some(1));
    assert_eq!(app.edit_document_id, Some(1));
    assert_eq!(app.edit.buffer, "uncommitted");

    app.workspace.select_document(2);
    app.commit_edit(None);
    assert_eq!(
        app.workspace.documents[&1].tables[0]
            .cell(CellAddress::default())
            .unwrap()
            .input,
        CellInput::Text("uncommitted".to_owned())
    );
    assert_eq!(
        app.workspace.documents[&2].tables[0]
            .cell(CellAddress::default())
            .unwrap()
            .input,
        CellInput::Text("other".to_owned())
    );
}

#[test]
// Verify that failed save-and-close restores the edit buffer and document content.
fn failed_save_and_close_restores_the_editor_buffer_and_keeps_document_content() {
    let directory = TestDirectory::new();
    let path = directory.0.join("conflict.csv");
    fs::write(&path, b"old\n").expect("create original CSV");
    let identity = FileIdentity::read(&path).expect("capture CSV identity");
    let source = Arc::new(
        crate::csv_adapter::CsvDocument::parse(b"old\n".to_vec(), EncodingChoice::Auto, ',')
            .expect("parse original CSV"),
    );
    let mut document = source.to_document(1, "conflict.csv");
    document.set_source(Some(Arc::new(SourceDocument::Csv(Arc::clone(&source)))));
    let mut workspace = Workspace::default();
    workspace.insert_loaded_document(document);
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    app.workspace = workspace;
    app.file_backings.insert(
        1,
        FileBacking {
            path: path.clone(),
            identity,
            source: Some(SourceDocument::Csv(source)),
            overwrite_confirmed: false,
        },
    );
    app.begin_cell_edit(1, CellAddress::default());
    app.edit.buffer = "edited".to_owned();
    fs::write(&path, b"external\n").expect("simulate external modification");

    app.save_and_close_document(1);
    assert!(app.pending_saves.contains_key(&1));
    wait_for_condition(&mut app, |app| !app.pending_saves.contains_key(&1));

    assert!(app.workspace.documents.contains_key(&1));
    assert!(app.workspace.documents[&1].is_dirty());
    assert_eq!(app.edit_document_id, Some(1));
    assert_eq!(app.edit.buffer, "edited");
    assert_eq!(
        app.workspace.documents[&1].tables[0]
            .cell(CellAddress::default())
            .unwrap()
            .input,
        CellInput::Text("edited".to_owned())
    );
    assert_eq!(
        fs::read(path).expect("external file remains unchanged"),
        b"external\n"
    );
}

#[test]
// Verify that an imported freeze top-left cell stays reachable across frame sizes.
fn imported_freeze_top_left_remains_reachable_after_two_frames_at_varied_sizes() {
    for screen_size in [
        Vec2::new(480.0, 320.0),
        Vec2::new(900.0, 620.0),
        Vec2::new(1440.0, 900.0),
    ] {
        let ctx = egui::Context::default();
        let mut app = new_app(&ctx);
        let document = app
            .workspace
            .active_document_mut()
            .expect("demo workspace has an active document");
        let table_id = document.active_table().expect("demo has a table").id;
        document
            .active_table_mut()
            .expect("demo has an active table")
            .set_dimensions(40, 5);
        let view = document.view_mut(table_id).expect("table has view state");
        Arc::make_mut(&mut view.layout).apply_value(
            LayoutTarget::Freeze,
            LayoutValue::Freeze(FrozenPanes {
                rows: 1,
                columns: 0,
            }),
        );
        view.scroll.y = 988.0;

        let mut frame = eframe::Frame::_new_kittest();
        let mut time = 0.0;
        run_frame_sized(
            &mut app,
            &ctx,
            &mut frame,
            &mut time,
            Vec::new(),
            screen_size,
        );
        run_frame_sized(
            &mut app,
            &ctx,
            &mut frame,
            &mut time,
            Vec::new(),
            screen_size,
        );

        let scroll = app
            .workspace
            .active_document()
            .and_then(|document| document.view(table_id))
            .expect("table view remains available")
            .scroll
            .y;
        assert!(
            scroll >= 987.0,
            "scroll offset was clamped at {scroll} for viewport {screen_size:?}"
        );
    }
}

pub(super) struct TestDirectory(pub(super) PathBuf);

impl TestDirectory {
    // Create a file worker for the UI integration tests.
    pub(super) fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "texcel-ui-{}-{unique}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create isolated test directory");
        Self(path)
    }
}

impl Drop for TestDirectory {
    // Stop the test worker and release pending jobs.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

// Wait until an asynchronous UI condition becomes true.
pub(super) fn wait_for_condition(app: &mut TexcelApp, condition: impl Fn(&TexcelApp) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !condition(app) {
        let _ = app.file_worker.poll_load_progress();
        app.process_file_results();
        assert!(
            Instant::now() < deadline,
            "file UI operation timed out; status={:?}, saves={:?}, structures={:?}",
            app.status,
            app.pending_saves,
            app.pending_structures
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    let _ = app.file_worker.poll_load_progress();
    app.process_file_results();
}

// Create an app with a loaded CSV document for UI tests.
pub(super) fn load_csv_test_app_after_confirming_default_settings(
    ctx: &egui::Context,
    path: PathBuf,
    source: &[u8],
) -> (TexcelApp, DocumentId, u64) {
    fs::write(&path, source).expect("write CSV fixture");
    let mut app = new_app(ctx);
    app.settings_loaded = true;
    app.open_path(path);
    let document_id = confirm_pending_csv_settings_with_test_defaults(&mut app);
    let table_id = app.workspace.documents[&document_id]
        .active_table()
        .expect("loaded CSV table exists")
        .id;
    (app, document_id, table_id)
}

// Waits for a normal CSV load or confirms a real options prompt.
pub(super) fn confirm_pending_csv_settings_with_test_defaults(app: &mut TexcelApp) -> DocumentId {
    let document_id = *app
        .pending_csv_opens
        .keys()
        .next()
        .expect("CSV load was submitted");
    wait_for_condition(app, |app| {
        app.workspace.documents.contains_key(&document_id)
            || app
                .pending_csv_opens
                .get(&document_id)
                .is_some_and(|pending| pending.prompt.is_some())
    });
    if app.workspace.documents.contains_key(&document_id) {
        return document_id;
    }
    let prompted_id = submit_pending_csv_settings_with_test_defaults(app);
    assert_eq!(prompted_id, document_id, "the prompt belongs to this load");
    wait_for_condition(app, |app| {
        !app.pending_csv_opens.contains_key(&document_id)
            && app.workspace.documents.contains_key(&document_id)
    });
    document_id
}

// Submits the test's visible CSV choices while leaving terminal load handling to the caller.
pub(super) fn submit_pending_csv_settings_with_test_defaults(app: &mut TexcelApp) -> DocumentId {
    wait_for_condition(app, |app| {
        app.pending_csv_opens
            .values()
            .any(|pending| pending.prompt.is_some())
    });
    let document_id = *app
        .pending_csv_opens
        .iter()
        .find_map(|(id, pending)| pending.prompt.is_some().then_some(id))
        .expect("CSV settings prompt is ready");
    let (delimiter, encoding) = match app.pending_csv_opens[&document_id]
        .prompt
        .as_ref()
        .expect("CSV settings are visible")
    {
        CsvLoadPrompt::Settings(settings) => {
            assert!(
                !settings.needs_encoding,
                "test input has a detectable encoding"
            );
            let delimiter = settings
                .delimiter_text
                .chars()
                .next()
                .expect("the displayed delimiter has one character");
            assert_eq!(settings.delimiter_text.chars().count(), 1);
            (delimiter, settings.selected_encoding)
        }
    };
    let pending = app.pending_csv_opens.get_mut(&document_id).unwrap();
    pending.delimiter = Some(delimiter);
    pending.encoding = encoding;
    let _ = app.submit_csv_load(document_id);
    document_id
}

// Run an app frame and wait for settings work to finish.
fn run_settings_frame(
    app: &mut TexcelApp,
    ctx: &egui::Context,
    viewport_events: Vec<egui::ViewportEvent>,
) -> egui::FullOutput {
    let mut raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
        ..RawInput::default()
    };
    raw.viewports
        .get_mut(&egui::ViewportId::ROOT)
        .expect("raw input includes the root viewport")
        .events = viewport_events;
    let mut output = ctx.run_ui(raw, |ui| {
        app.process_settings(ctx);
        app.handle_app_close_request(ctx);
        let _ = ui;
    });
    output.textures_delta.clear();
    output
}

// Submit a clipboard paste request from the active test cell.
fn submit_test_paste(
    app: &mut TexcelApp,
    document_id: DocumentId,
    table_id: u64,
    selection: CellRange,
    text: &str,
    html: Option<String>,
) {
    let (rows, columns, scroll, revision) = {
        let document = app
            .workspace
            .documents
            .get(&document_id)
            .expect("test document exists");
        let table = document
            .tables
            .iter()
            .find(|table| table.id == table_id)
            .expect("test table exists");
        (
            table.row_count(),
            table.column_count(),
            document.view(table_id).unwrap().scroll,
            document.revision,
        )
    };
    app.submit_clipboard_paste(
        document_id,
        table_id,
        ClipboardPasteRequest {
            text: text.to_owned(),
            html,
            options: PasteOptions::default(),
            selection,
            rows,
            columns,
        },
        scroll,
    );
    wait_for_condition(app, |app| {
        !app.pending_structures.contains_key(&document_id)
            && app.workspace.documents[&document_id].revision > revision
    });
}

// Enter text into a blank cell through the UI.
fn edit_blank_cell(app: &mut TexcelApp) -> (DocumentId, u64, CellAddress) {
    let document = app
        .workspace
        .active_document_mut()
        .expect("demo workspace has an active document");
    let table_id = document.active_table().expect("demo has a table").id;
    let address = CellAddress { row: 1, column: 0 };
    document
        .view_mut(table_id)
        .expect("every table has view state")
        .selection = CellRange::single(address);
    let document_id = document.id;
    // Tests may begin editing before the startup grid receives its first frame.
    app.request_grid_focus = false;
    app.begin_cell_edit(table_id, address);
    (document_id, table_id, address)
}

#[test]
// Verify that CSV open, edit, save, and external change checks use workers.
fn csv_open_edit_save_and_save_conflict_are_connected() {
    let directory = TestDirectory::new();
    let path = directory.0.join("input.csv");
    fs::write(&path, b"a,b\nx,y\n").expect("create CSV input");
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);

    app.open_path(path.clone());
    let document_id = confirm_pending_csv_settings_with_test_defaults(&mut app);
    let document = app
        .workspace
        .documents
        .get(&document_id)
        .expect("opened document exists");
    assert_eq!(
        document
            .active_table()
            .and_then(|table| table.cell(CellAddress { row: 1, column: 0 }))
            .map(|cell| cell.input.clone()),
        Some(CellInput::Text("x".to_owned()))
    );
    assert!(!document.is_dirty());

    let document = app
        .workspace
        .active_document_mut()
        .expect("opened document remains active");
    document
        .set_cell(
            1,
            CellAddress { row: 1, column: 0 },
            Some(Cell::text("local")),
        )
        .expect("edit one imported cell");
    app.save_active(false);
    wait_for_condition(&mut app, |app| {
        !app.pending_saves.contains_key(&document_id)
    });
    assert_eq!(fs::read(&path).expect("read saved CSV"), b"a,b\nlocal,y\n");
    assert!(
        !app.workspace
            .documents
            .get(&document_id)
            .expect("saved document remains open")
            .is_dirty()
    );

    // A file changed by another app is never overwritten without the user's choice.
    fs::write(&path, b"external change\n").expect("simulate external edit");
    app.workspace
        .documents
        .get_mut(&document_id)
        .expect("document remains open")
        .set_cell(
            1,
            CellAddress { row: 1, column: 0 },
            Some(Cell::text("another local edit")),
        )
        .expect("edit after external change");
    app.save_active(false);
    wait_for_condition(&mut app, |app| {
        !app.pending_saves.contains_key(&document_id)
    });
    assert_eq!(
        fs::read(&path).expect("external file remains untouched"),
        b"external change\n"
    );
    assert!(app.workspace.documents[&document_id].is_dirty());
    assert_eq!(
        app.save_conflict_dialog.map(|prompt| prompt.document_id),
        Some(document_id)
    );

    // Choosing Overwrite saves the local version over the changed file.
    app.save_conflict_dialog = None;
    app.file_backings
        .get_mut(&document_id)
        .expect("backing remains")
        .overwrite_confirmed = true;
    app.save_active(false);
    wait_for_condition(&mut app, |app| {
        !app.pending_saves.contains_key(&document_id)
    });
    assert_eq!(
        fs::read(&path).expect("read overwritten CSV"),
        b"a,b\nanother local edit,y\n"
    );
    assert!(!app.file_backings[&document_id].overwrite_confirmed);
}

#[test]
// Verify that IME input, Alt+Enter, and manual scrolling keep their event behavior.
fn ime_click_alt_enter_and_manual_scroll_keep_their_event_semantics() {
    let ctx = egui::Context::default();
    let mut frame = eframe::Frame::_new_kittest();
    let mut time = 0.0;
    let mut app = new_app(&ctx);
    let (document_id, table_id, address) = edit_blank_cell(&mut app);
    run_frame(&mut app, &ctx, &mut frame, &mut time, vec![]);

    run_frame(
        &mut app,
        &ctx,
        &mut frame,
        &mut time,
        vec![Event::Ime(egui::ImeEvent::Preedit {
            text: "にほん".into(),
            active_range_chars: None,
        })],
    );
    let editor = editor_id(document_id, app.edit.session_id);
    let pointer = ctx
        .read_response(editor)
        .expect("the focused editor has a pointer rectangle")
        .rect
        .center();
    run_frame(
        &mut app,
        &ctx,
        &mut frame,
        &mut time,
        vec![
            Event::PointerMoved(pointer),
            Event::PointerButton {
                pos: pointer,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
    );
    run_frame(
        &mut app,
        &ctx,
        &mut frame,
        &mut time,
        vec![Event::PointerButton {
            pos: pointer,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
    );
    assert_eq!(ctx.memory(|memory| memory.focused()), Some(editor));
    assert_eq!(
        app.active_selection().map(|(_, selected)| selected),
        Some(address)
    );
    assert!(!app.edit.composing);
    assert!(app.edit.buffer.is_empty());

    run_frame(
        &mut app,
        &ctx,
        &mut frame,
        &mut time,
        vec![Event::Ime(egui::ImeEvent::Preedit {
            text: "にほん".into(),
            active_range_chars: None,
        })],
    );

    run_frame(
        &mut app,
        &ctx,
        &mut frame,
        &mut time,
        vec![
            Event::Ime(egui::ImeEvent::Commit("日本".into())),
            key(Key::Enter, Modifiers::NONE),
        ],
    );
    assert!(app.edit.is_editing());
    assert_eq!(app.edit.buffer, "日本");
    let existing_input = app
        .workspace
        .active_document()
        .and_then(|document| document.active_table())
        .and_then(|table| table.cell(address))
        .map(|cell| cell.input.source_text());
    assert!(existing_input.is_none_or(|input| input.is_empty()));

    run_frame(
        &mut app,
        &ctx,
        &mut frame,
        &mut time,
        vec![key(Key::Enter, Modifiers::NONE)],
    );
    assert!(!app.edit.is_editing());
    // The new source cell must pass the read limits before the edit reaches history.
    wait_for_condition(&mut app, |app| app.pending_structures.is_empty());
    let document = app
        .workspace
        .active_document_mut()
        .expect("document remains open");
    assert_eq!(
        document
            .tables
            .iter()
            .find(|table| table.id == table_id)
            .and_then(|table| table.cell(address))
            .map(|cell| cell.input.clone()),
        Some(CellInput::Text("日本".into()))
    );
    assert_eq!(document.undo(), Ok(Some((table_id, address))));
    let restored_input = document
        .tables
        .iter()
        .find(|table| table.id == table_id)
        .and_then(|table| table.cell(address))
        .map(|cell| cell.input.source_text());
    assert!(restored_input.is_none_or(|input| input.is_empty()));
    assert_eq!(document.undo(), Ok(None));

    let mut alt_app = new_app(&ctx);
    let (_, _, alt_address) = edit_blank_cell(&mut alt_app);
    run_frame(&mut alt_app, &ctx, &mut frame, &mut time, vec![]);
    run_frame(
        &mut alt_app,
        &ctx,
        &mut frame,
        &mut time,
        vec![
            Event::ModifiersChanged(Modifiers::ALT),
            key(Key::Enter, Modifiers::ALT),
            Event::ModifiersChanged(Modifiers::NONE),
        ],
    );
    assert!(alt_app.edit.is_editing());
    assert_eq!(alt_app.edit.buffer, "\n");
    assert_eq!(
        alt_app.active_selection().map(|(_, cell)| cell),
        Some(alt_address)
    );

    let mut scroll_app = new_app(&ctx);
    let (scroll_document_id, scroll_table_id, scroll_columns) = {
        let document = scroll_app
            .workspace
            .active_document()
            .expect("the scrolling document is active");
        let table = document
            .active_table()
            .expect("the scrolling table is active");
        (document.id, table.id, table.column_count())
    };
    // Give the manual-scroll check enough rows to move through.
    scroll_app
        .workspace
        .documents
        .get_mut(&scroll_document_id)
        .unwrap()
        .tables
        .iter_mut()
        .find(|table| table.id == scroll_table_id)
        .unwrap()
        .set_dimensions(200, scroll_columns);
    run_frame(&mut scroll_app, &ctx, &mut frame, &mut time, vec![]);
    run_frame(
        &mut scroll_app,
        &ctx,
        &mut frame,
        &mut time,
        vec![
            Event::PointerMoved(Pos2::new(280.0, 245.0)),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0.0, -1100.0),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                modifiers: Modifiers::NONE,
            },
        ],
    );
    let read_scroll = |app: &TexcelApp| {
        app.workspace
            .active_document()
            .and_then(|document| {
                document
                    .active_table()
                    .and_then(|table| document.view(table.id))
            })
            .map_or(0.0, |view| view.scroll.y)
    };
    let mut peak = read_scroll(&scroll_app);
    for _ in 0..90 {
        run_frame(&mut scroll_app, &ctx, &mut frame, &mut time, vec![]);
        peak = peak.max(read_scroll(&scroll_app));
    }
    assert!(peak > 0.0);
    assert!(read_scroll(&scroll_app) > 0.0);
}

#[test]
// Verify that wrapped row height changes preserve the view anchor and selection.
fn wrapped_rows_keep_the_view_anchor_and_selected_row_position() {
    let ctx = egui::Context::default();
    install_texcel_grid_fonts(&ctx, FontPreset::Default);
    ctx.run_ui(egui::RawInput::default(), |_| ())
        .textures_delta
        .clear();
    let mut document = crate::document::Document::new(50, "wrap", &["Sheet1"]);
    let table_id = document.tables[0].id;
    document.tables[0].set_dimensions(5, 6);
    let cell =
        Cell::text("A long wrapped cell value with enough words to occupy several grid lines.");
    document
        .set_cell(table_id, CellAddress { row: 2, column: 0 }, Some(cell))
        .expect("wrapped cell is valid");
    let offscreen_cell = Cell::text("Wrapped text in a horizontally offscreen column.");
    document
        .set_cell(
            table_id,
            CellAddress { row: 2, column: 5 },
            Some(offscreen_cell),
        )
        .expect("offscreen wrapped cell is valid");

    let table = &document.tables[0];
    let (_, visible_cells) = wrapped_row_cells(&document, table, &table.layout, 2, 0..1);
    assert_eq!(visible_cells.len(), 1);
    assert_eq!(visible_cells[0].0.column, 0);
    let (_, horizontally_scrolled_cells) =
        wrapped_row_cells(&document, table, &table.layout, 2, 5..6);
    assert_eq!(horizontally_scrolled_cells.len(), 1);
    assert_eq!(horizontally_scrolled_cells[0].0.column, 5);
    let mut layout = GridRowLayout::default();
    layout.ensure_row_count(table.row_count());
    let (fingerprint, wrapped_cells) = wrapped_row_cells(&document, table, &table.layout, 2, 0..1);
    let measured_height = measure_wrapped_cells(&ctx, &wrapped_cells, true, false, false);
    assert!(measured_height > ROW_HEIGHT);

    let selected_row = 4;
    let old_scroll = HEADER_HEIGHT + layout.prefix_height(3) + 8.0;
    assert_eq!(layout.row_at_position(old_scroll - HEADER_HEIGHT), 3);
    let old_selected_y = layout.row_top(selected_row) - old_scroll;

    let delta = layout.update_row(2, 0, fingerprint, measured_height);
    let anchored_scroll = old_scroll + delta;
    assert_eq!(layout.row_at_position(anchored_scroll - HEADER_HEIGHT), 3);
    assert!((layout.row_top(selected_row) - anchored_scroll - old_selected_y).abs() < 0.01);
}

#[test]
// Verify that wrap height cache retains its horizontal maximum for one revision.
fn wrap_height_cache_keeps_horizontal_max_until_document_revision_changes() {
    let mut layout = GridRowLayout::default();
    layout.ensure_row_count(4);
    assert_eq!(layout.update_row(2, 7, 10, 84.0), 58.0);
    assert_eq!(layout.update_row(2, 7, 11, ROW_HEIGHT), 0.0);
    assert_eq!(layout.row_height(2), 84.0);
    assert_eq!(layout.update_row(2, 8, 12, ROW_HEIGHT), ROW_HEIGHT - 84.0);
    assert_eq!(layout.row_height(2), ROW_HEIGHT);
}

#[test]
// Verify that CSV layout actions change only the view state.
fn csv_layout_actions_change_only_the_view_state() {
    let ctx = egui::Context::default();
    let mut app = new_app(&ctx);
    let document = app.workspace.active_document().unwrap();
    let document_id = document.id;
    let table_id = document.active_table().unwrap().id;
    let revision = document.revision;
    let dirty = document.is_dirty();

    app.apply_layout_values(
        document_id,
        table_id,
        vec![(
            LayoutTarget::Column(3),
            LayoutValue::Column(Some(ColumnLayout { width: Some(12.0) })),
        )],
    )
    .expect("text formats can change local display geometry");

    let document = app.workspace.documents.get(&document_id).unwrap();
    assert_eq!(document.revision, revision);
    assert_eq!(document.is_dirty(), dirty);
    assert_eq!(document.tables[0].layout.column(3), None);
    assert_eq!(
        document
            .view(table_id)
            .unwrap()
            .layout
            .column(3)
            .unwrap()
            .width,
        Some(12.0)
    );
}

#[test]
// Verify that Alt+Enter uses key event modifiers after Alt has been released.
fn alt_enter_uses_the_key_event_modifiers_after_alt_is_released() {
    let events = [
        Event::ModifiersChanged(Modifiers::ALT),
        key(Key::Enter, Modifiers::ALT),
        Event::ModifiersChanged(Modifiers::NONE),
    ];
    assert_eq!(
        events.iter().find_map(editing_key_from_event),
        Some((InputKey::AltEnter, false))
    );
}
#[cfg(test)]
#[path = "ui/tests/ui_appearance_matrix_test.rs"]
mod appearance_matrix_tests;
#[cfg(test)]
#[path = "ui/tests/ui_appearance_test.rs"]
mod appearance_visual_tests;
#[cfg(test)]
#[path = "ui/tests/ui_chrome_layout_test.rs"]
mod chrome_layout_tests;
#[cfg(test)]
#[path = "ui/tests/ui_export_policy_test.rs"]
mod export_policy_tests;
#[cfg(test)]
#[path = "ui/tests/ui_ime_focus_test.rs"]
mod ime_focus_tests;
#[cfg(test)]
#[path = "ui/tests/ui_load_preview_test.rs"]
mod load_preview_tests;

#[cfg(test)]
#[path = "ui/tests/ui_calculation_cancel_test.rs"]
mod calculation_cancel_tests;
#[cfg(test)]
#[path = "ui/tests/ui_calculation_reconfigure_test.rs"]
mod calculation_reconfigure_tests;
#[cfg(test)]
#[path = "ui/tests/ui_display_mode_test.rs"]
mod display_mode_tests;
#[cfg(test)]
#[path = "ui/tests/ui_footer_csv_test.rs"]
mod footer_csv_tests;
#[cfg(test)]
#[path = "ui/tests/ui_formula_text_test.rs"]
mod formula_text_tests;
#[path = "ui/tests/ui_help_test.rs"]
mod help_tests;
#[cfg(test)]
#[path = "ui/tests/ui_import_markdown_test.rs"]
mod import_markdown_tests;
#[path = "ui/tests/ui_load_test.rs"]
mod load_tests;
#[cfg(test)]
#[path = "ui/tests/ui_save_as_test.rs"]
mod save_as_tests;
#[cfg(test)]
#[path = "ui/tests/ui_save_processing_test.rs"]
mod save_processing_tests;

#[cfg(test)]
#[path = "ui/tests/ui_shortcuts_test.rs"]
mod shortcuts_integration_tests;
