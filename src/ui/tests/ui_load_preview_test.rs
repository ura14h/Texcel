use super::*;
use egui_kittest::{
    Harness,
    kittest::{By, NodeT, Queryable},
};
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Builds one preview table with distinct text at every coordinate.
fn preview_state(document_id: DocumentId, rows: usize, columns: usize) -> LoadPreviewState {
    let preview_rows = (0..rows)
        .map(|row| LoadPreviewRow {
            index: row,
            cells: (0..columns)
                .map(|column| crate::file_worker::LoadPreviewCell {
                    column,
                    text: format!("r{row}c{column}"),
                })
                .collect::<Vec<_>>()
                .into(),
        })
        .collect::<Vec<_>>();
    let chunk = Arc::new(LoadPreviewChunk {
        table_index: 0,
        table_name: "CSV".to_owned(),
        first_row: 0,
        last_row: rows.saturating_sub(1),
        column_count: columns,
        rows: preview_rows.into(),
    });
    LoadPreviewState {
        token: JobToken {
            document_id,
            revision: 0,
            job_id: 90,
        },
        path: PathBuf::from("preview.csv"),
        stage: LoadStage::ParsingSource,
        selected_table: 0,
        tables: BTreeMap::from([(
            0,
            LoadPreviewTable {
                name: "CSV".to_owned(),
                row_count: rows,
                column_count: columns,
                chunks: BTreeMap::from([(0, chunk)]),
            },
        )]),
    }
}

/// Saves one rendered frame when this host exposes a WGPU adapter.
fn save_preview_render(harness: &mut Harness<'_, TexcelApp>, path: &Path) -> bool {
    let Ok(Ok(rendered)) = catch_unwind(AssertUnwindSafe(|| harness.render())) else {
        return false;
    };
    rendered.save(path).expect("save rendered preview");
    true
}

#[test]
// Keep preview data separate from documents and retain the live cell draft.
fn load_preview_is_read_only_and_renders_in_english_and_japanese() {
    let render_directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("kittest-renders");
    fs::create_dir_all(&render_directory).expect("create render directory");
    for (language, size, suffix) in [
        (crate::i18n::Language::English, [900.0, 620.0], "en"),
        (crate::i18n::Language::Japanese, [460.0, 330.0], "ja-narrow"),
    ] {
        let mut harness = Harness::builder()
            .with_size(size)
            .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
        let document_id = harness.state().workspace.active_document.unwrap();
        {
            let ctx = harness.ctx.clone();
            let app = harness.state_mut();
            app.language = language;
            crate::i18n::set_context_language(&ctx, language);
            let order = app.workspace.order.clone();
            let revision = app.workspace.documents[&document_id].revision;
            let (_, _, _) = edit_blank_cell(app);
            app.edit.buffer = "keep this draft".to_owned();
            app.pending_load_previews
                .insert(document_id, preview_state(document_id, 80, 8));
            assert_eq!(app.workspace.order, order);
            assert_eq!(app.workspace.documents[&document_id].revision, revision);
        }
        harness.step();
        harness.step();
        assert_eq!(harness.state().pending_load_previews.len(), 1);
        let _ = save_preview_render(
            &mut harness,
            &render_directory.join(format!("load-preview-{suffix}.png")),
        );
        let cancel = crate::i18n::text(language, "action-cancel");
        assert!(
            harness
                .query_all(By::new().role(egui::Role::Button).label_contains(&cancel))
                .next()
                .is_some(),
            "the preview cancel button is reachable"
        );
        assert_eq!(harness.state().edit.buffer, "keep this draft");
        harness.state_mut().pending_load_previews.clear();
        harness.state_mut().pending_csv_opens.insert(
            document_id,
            PendingCsvOpen {
                path: PathBuf::from("settings.csv"),
                encoding: EncodingChoice::Auto,
                delimiter: None,
                prompt: Some(CsvLoadPrompt::Settings(CsvLoadSettingsPrompt {
                    candidates: vec![',', ';', '\t'],
                    delimiter_text: ";".to_owned(),
                    previews: BTreeMap::new(),
                    selected_encoding: EncodingChoice::Auto,
                    needs_encoding: false,
                    error: None,
                })),
                job_id: 91,
                retry_submission: None,
            },
        );
        harness.step();
        harness.step();
        let encoding_label = crate::i18n::text(language, "dialog-csv-encoding-label");
        let delimiter_label = crate::i18n::text(language, "dialog-csv-delimiter-label");
        assert!(
            harness
                .query_all(By::new().label_contains(&encoding_label))
                .next()
                .is_some(),
            "the encoding choice is reachable"
        );
        assert!(
            harness
                .query_all(By::new().label_contains(&delimiter_label))
                .next()
                .is_some(),
            "the editable delimiter choice is reachable"
        );
        let _ = save_preview_render(
            &mut harness,
            &render_directory.join(format!("csv-settings-{suffix}.png")),
        );
    }
}

#[test]
// Keep cells from later rows and columns visible after both axes scroll.
fn load_preview_scrolls_beyond_the_first_rows_and_columns() {
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let document_id = harness.state().workspace.active_document.unwrap();
    harness
        .state_mut()
        .pending_load_previews
        .insert(document_id, preview_state(document_id, 120, 12));
    harness.step();
    assert!(
        harness
            .query_all(By::new().label_contains("r35c5"))
            .next()
            .is_none(),
        "the target cell starts outside the first viewport"
    );

    harness.event(egui::Event::PointerMoved(Pos2::new(450.0, 340.0)));
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: Vec2::new(-900.0, -900.0),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Unknown,
        modifiers: Modifiers::NONE,
    });
    harness.step();
    harness.step();

    assert!(
        harness
            .query_all(By::new().label_contains("r35c5"))
            .next()
            .is_some(),
        "a cell beyond the first screen is still rendered after two-axis scrolling"
    );
    assert!(
        harness
            .query_all(By::new().label_contains("r0c0"))
            .next()
            .is_none(),
        "off-screen cells are removed from the virtual accessibility tree"
    );
}

#[test]
// Blocks background accessibility actions while a CSV choice is active.
fn csv_choice_blocks_background_accesskit_actions_and_cancel_advances_fifo() {
    let directory = TestDirectory::new();
    let choice_path = directory.0.join("choice.csv");
    let next_path = directory.0.join("next.md");
    fs::write(&choice_path, b"\"a\"|\"b\"\n").expect("CSV needs a custom delimiter");
    fs::write(&next_path, b"| next |\n| --- |\n| value |\n").expect("Markdown is written");
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let document_id = harness.state().workspace.active_document.unwrap();
    let table_id = harness.state().workspace.documents[&document_id]
        .active_table()
        .unwrap()
        .id;
    let background_cell = CellAddress { row: 0, column: 1 };
    harness
        .state_mut()
        .workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .view_mut(table_id)
        .unwrap()
        .set_selection(CellRange::single(background_cell));
    harness.step();
    let table_token = harness.state().accesskit_table_tokens[&(document_id, table_id)];
    let target = grid_accesskit_cell_id(table_token, CellAddress::default(), 0)
        .expect("the cell has an accessibility ID")
        .accesskit_id();
    let before = harness.state().workspace.documents[&document_id]
        .view(table_id)
        .unwrap()
        .selection;
    harness
        .state_mut()
        .queue_open_paths([choice_path, next_path.clone()]);
    let deadline = Instant::now() + Duration::from_secs(5);
    let choice_document = loop {
        super::scenario_tests::send_batch(&mut harness, []);
        if let Some(document_id) = harness
            .state()
            .pending_csv_opens
            .iter()
            .find_map(|(document_id, request)| request.prompt.as_ref().map(|_| *document_id))
        {
            break document_id;
        }
        assert!(
            Instant::now() < deadline,
            "the real CSV load shows settings"
        );
        std::thread::sleep(Duration::from_millis(3));
    };
    assert!(
        harness.state().pending_csv_opens[&choice_document]
            .prompt
            .is_some()
    );
    super::scenario_tests::send_batch(
        &mut harness,
        [egui::Event::AccessKitActionRequest(
            egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Click,
                target_tree: egui::accesskit::TreeId::ROOT,
                target_node: target,
                data: None,
            },
        )],
    );
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .view(table_id)
            .unwrap()
            .selection,
        before,
        "the prompt prevents a background cell click from changing selection"
    );
    assert!(harness.state().pending_accesskit_focus.is_none());

    let cancel = context_text(&harness.ctx, "action-cancel");
    let cancel_rect = harness
        .query_all(By::new().role(egui::Role::Button).label_contains(&cancel))
        .next()
        .expect("the CSV prompt exposes Cancel")
        .rect();
    super::scenario_tests::click_point(&mut harness, cancel_rect.center());
    for _ in 0..80 {
        super::scenario_tests::send_batch(&mut harness, []);
        if harness
            .state()
            .user_file_paths
            .values()
            .any(|path| path == &next_path)
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(
        harness
            .state()
            .user_file_paths
            .values()
            .any(|path| path == &next_path)
            && harness
                .state()
                .workspace
                .documents
                .values()
                .any(|document| document.name == "next.md"),
        "canceling the choice releases the next queued file"
    );
}

#[test]
// Change the encoding, refresh its preview, and load the selected delimiter.
fn csv_settings_reprobe_encoding_and_load_custom_delimiter() {
    let directory = TestDirectory::new();
    let path = directory.0.join("cp932.csv");
    fs::write(&path, [0xC2, 0xA2, b';', b'1', b'\n']).expect("write CSV test input");
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let document_id = harness.state().workspace.active_document.unwrap();
    harness.state_mut().pending_csv_opens.insert(
        document_id,
        PendingCsvOpen {
            path,
            encoding: EncodingChoice::Auto,
            delimiter: None,
            prompt: Some(CsvLoadPrompt::Settings(CsvLoadSettingsPrompt {
                candidates: vec![';'],
                delimiter_text: ";".to_owned(),
                previews: BTreeMap::new(),
                selected_encoding: EncodingChoice::Auto,
                needs_encoding: true,
                error: None,
            })),
            job_id: 1,
            retry_submission: None,
        },
    );
    harness.run_steps(2);
    let auto_label = crate::i18n::context_text(&harness.ctx, "dialog-csv-encoding-auto");
    let encoding_combo = harness
        .query_all(By::new().role(egui::Role::ComboBox).value(&auto_label))
        .next()
        .expect("the encoding choice is exposed");
    encoding_combo.click();
    harness.run();
    let cp932_label = crate::i18n::context_text(&harness.ctx, "dialog-csv-encoding-cp932");
    harness
        .query_all(By::new().role(egui::Role::Button).label(&cp932_label))
        .next()
        .expect("the CP932 option is exposed after opening the choice")
        .click();
    harness.run();
    let original_job = 1;
    let mut preview_refreshed = false;
    for _ in 0..80 {
        harness.step();
        preview_refreshed = harness
            .state()
            .pending_csv_opens
            .get(&document_id)
            .is_some_and(|pending| {
                pending.job_id != original_job
                    && matches!(pending.prompt.as_ref(), Some(CsvLoadPrompt::Settings(settings))
                        if settings.selected_encoding == EncodingChoice::ShiftJis
                            && !settings.previews.is_empty())
            });
        if preview_refreshed {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(
        preview_refreshed,
        "the selected encoding produces a fresh preview"
    );
    let prompt = harness.state().pending_csv_opens[&document_id]
        .prompt
        .as_ref()
        .expect("the selected encoding was analyzed again");
    let CsvLoadPrompt::Settings(settings) = prompt;
    assert_eq!(settings.selected_encoding, EncodingChoice::ShiftJis);
    assert_eq!(settings.delimiter_text, ";");

    click_csv_load(&mut harness);
    wait_for_csv_load(&mut harness, document_id);
    let cell = harness.state().workspace.documents[&document_id].tables[0]
        .cell(CellAddress { row: 0, column: 0 })
        .expect("first custom-delimited cell loaded");
    assert_eq!(cell.input, CellInput::Text("ﾂ｢".to_owned()));
    assert_eq!(
        harness.state().workspace.documents[&document_id].tables[0]
            .cell(CellAddress { row: 0, column: 1 })
            .unwrap()
            .input,
        CellInput::Text("1".to_owned())
    );
}

/// Shows the localized name for a CSV encoding choice.
fn csv_encoding_label(ctx: &egui::Context, encoding: EncodingChoice) -> String {
    context_text(
        ctx,
        match encoding {
            EncodingChoice::Auto => "dialog-csv-encoding-auto",
            EncodingChoice::Utf8 => "dialog-csv-encoding-utf8",
            EncodingChoice::ShiftJis => "dialog-csv-encoding-cp932",
        },
    )
}

/// Selects another encoding through the CSV options dialog.
fn choose_csv_encoding(harness: &mut Harness<'_, TexcelApp>, next: EncodingChoice) {
    for _ in 0..2 {
        super::scenario_tests::send_batch(harness, []);
    }
    harness
        .query_all(By::new().role(egui::Role::ComboBox))
        .next()
        .expect("the encoding choice is shown")
        .click();
    // The worker wakes the UI for each preview, so frames do not settle at a fixed count.
    harness.run_steps(6);
    let next_label = csv_encoding_label(&harness.ctx, next);
    harness
        .query_all(By::new().role(egui::Role::Button).label(&next_label))
        .next()
        .expect("the next encoding is available")
        .click();
    harness.run_steps(6);
}

/// Confirms CP932 before loading and keeps its bytes and CRLF on save.
#[test]
fn cp932_sample_is_preselected_then_roundtrips_after_edit() {
    let directory = TestDirectory::new();
    let path = directory.0.join("inventory.csv");
    fs::write(
        &path,
        include_bytes!("../../../samples/cp932-inventory.csv"),
    )
    .expect("copy the CP932 sample");
    let original = fs::read(&path).expect("read original sample bytes");
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let context = harness.ctx.clone();
    harness.state_mut().language = crate::i18n::Language::Japanese;
    crate::i18n::set_context_language(&context, crate::i18n::Language::Japanese);
    let document_id = harness
        .state_mut()
        .start_path_load(path.clone())
        .expect("CSV path is supported");

    let mut prompt_ready = false;
    for _ in 0..80 {
        super::scenario_tests::send_batch(&mut harness, []);
        prompt_ready = harness
            .state()
            .pending_csv_opens
            .get(&document_id)
            .is_some_and(|pending| pending.prompt.is_some());
        if prompt_ready {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(prompt_ready, "the CP932 load asks for confirmation");
    let pending = &harness.state().pending_csv_opens[&document_id];
    let Some(CsvLoadPrompt::Settings(settings)) = pending.prompt.as_ref() else {
        panic!("the encoding prompt is visible");
    };
    assert_eq!(settings.selected_encoding, EncodingChoice::ShiftJis);
    assert_eq!(settings.delimiter_text, ",");
    assert_eq!(
        settings.previews[&','][0],
        vec!["品名".to_owned(), "数量".to_owned()]
    );
    assert!(
        !harness
            .state()
            .workspace
            .documents
            .contains_key(&document_id)
    );

    let first_job = harness.state().pending_csv_opens[&document_id].job_id;
    choose_csv_encoding(&mut harness, EncodingChoice::Utf8);
    let mut utf8_rejected = false;
    for _ in 0..80 {
        super::scenario_tests::send_batch(&mut harness, []);
        utf8_rejected = harness
            .state()
            .pending_csv_opens
            .get(&document_id)
            .is_some_and(|pending| {
                pending.job_id != first_job
                    && matches!(pending.prompt.as_ref(), Some(CsvLoadPrompt::Settings(settings))
                        if settings.selected_encoding == EncodingChoice::Utf8
                            && settings.error == Some("status-csv-encoding-invalid"))
            });
        if utf8_rejected {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(
        utf8_rejected,
        "an invalid encoding keeps the prompt and selected value available"
    );
    for _ in 0..2 {
        super::scenario_tests::send_batch(&mut harness, []);
    }
    let load_label = context_text(&harness.ctx, "dialog-csv-load");
    let load_button = harness
        .query_all(
            By::new()
                .role(egui::Role::Button)
                .label_contains(&load_label),
        )
        .next()
        .expect("the load action remains visible for a retry");
    assert!(
        load_button.accesskit_node().is_disabled(),
        "an invalid selected encoding disables Load"
    );
    super::scenario_tests::send_event(
        &mut harness,
        egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: Some(egui::Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    assert!(
        matches!(harness.state().pending_csv_opens[&document_id].prompt.as_ref(),
            Some(CsvLoadPrompt::Settings(settings))
                if settings.selected_encoding == EncodingChoice::Utf8
                    && settings.error == Some("status-csv-encoding-invalid")),
        "Enter cannot dismiss an invalid encoding prompt"
    );

    let delimiter_rect = harness
        .query_all(By::new().role(egui::Role::TextInput))
        .find(|node| node.value().as_deref() == Some(","))
        .expect("the delimiter field remains editable")
        .rect();
    super::scenario_tests::click_point(&mut harness, delimiter_rect.center());
    let command = super::scenario_tests::command_modifiers(&harness.ctx);
    super::scenario_tests::send_event(
        &mut harness,
        super::scenario_tests::key_event(egui::Key::A, command),
    );
    super::scenario_tests::send_event(&mut harness, egui::Event::Text(";".to_owned()));
    assert!(
        matches!(harness.state().pending_csv_opens[&document_id].prompt.as_ref(),
            Some(CsvLoadPrompt::Settings(settings))
                if settings.selected_encoding == EncodingChoice::Utf8
                    && settings.delimiter_text == ";"
                    && settings.error == Some("status-csv-encoding-invalid")),
        "changing the delimiter cannot erase an invalid encoding result"
    );
    for _ in 0..2 {
        super::scenario_tests::send_batch(&mut harness, []);
    }
    assert!(
        harness
            .query_all(
                By::new()
                    .role(egui::Role::Button)
                    .label_contains(&load_label),
            )
            .next()
            .expect("Load remains visible after changing the delimiter")
            .accesskit_node()
            .is_disabled(),
        "an invalid encoding keeps Load disabled after a delimiter edit"
    );
    super::scenario_tests::send_event(
        &mut harness,
        egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: Some(egui::Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    assert!(
        harness.state().pending_csv_opens[&document_id]
            .prompt
            .is_some(),
        "Enter still cannot load after changing to another delimiter"
    );

    let retry_job = harness.state().pending_csv_opens[&document_id].job_id;
    choose_csv_encoding(&mut harness, EncodingChoice::ShiftJis);
    let mut cp932_preview_restored = false;
    for _ in 0..80 {
        super::scenario_tests::send_batch(&mut harness, []);
        cp932_preview_restored = harness
            .state()
            .pending_csv_opens
            .get(&document_id)
            .is_some_and(|pending| {
                pending.job_id != retry_job
                    && matches!(pending.prompt.as_ref(), Some(CsvLoadPrompt::Settings(settings))
                        if settings.selected_encoding == EncodingChoice::ShiftJis
                            && !settings.previews.is_empty()
                            && settings.error.is_none())
            });
        if cp932_preview_restored {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(
        cp932_preview_restored,
        "CP932 can be selected again after failure"
    );

    let mut load_visible = false;
    for _ in 0..8 {
        super::scenario_tests::send_batch(&mut harness, []);
        load_visible = harness
            .query_all(
                By::new()
                    .role(egui::Role::Button)
                    .label_contains(&load_label),
            )
            .next()
            .is_some();
        if load_visible {
            break;
        }
    }
    assert!(
        load_visible,
        "the CP932 confirmation has a visible Load action"
    );

    click_csv_load(&mut harness);
    wait_for_csv_load(&mut harness, document_id);
    super::scenario_tests::click_cell(&mut harness, "A2");
    super::scenario_tests::send_batch(&mut harness, [egui::Event::Text("鉛筆改".to_owned())]);
    super::scenario_tests::send_event(
        &mut harness,
        egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: Some(egui::Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    for _ in 0..80 {
        super::scenario_tests::send_batch(&mut harness, []);
        if harness.state().workspace.documents[&document_id].tables[0]
            .cell(CellAddress { row: 1, column: 0 })
            .is_some_and(|cell| cell.input == CellInput::Text("鉛筆改".to_owned()))
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(
        harness.state().workspace.documents[&document_id].tables[0]
            .cell(CellAddress { row: 1, column: 0 })
            .unwrap()
            .input,
        CellInput::Text("鉛筆改".to_owned())
    );
    let modifiers = super::scenario_tests::command_modifiers(&harness.ctx);
    super::scenario_tests::send_event(
        &mut harness,
        super::scenario_tests::key_event(egui::Key::S, modifiers),
    );
    super::scenario_tests::wait_until(&mut harness, |app| {
        !app.pending_saves.contains_key(&document_id)
            && !app.document_has_unsaved_changes(document_id)
    });
    assert!(!harness.state().document_has_unsaved_changes(document_id));

    let output = fs::read(&path).expect("read saved CP932 bytes");
    let (old_cell, _, old_has_errors) = encoding_rs::SHIFT_JIS.encode("鉛筆");
    let (new_cell, _, new_has_errors) = encoding_rs::SHIFT_JIS.encode("鉛筆改");
    assert!(!old_has_errors && !new_has_errors);
    let locations = original
        .windows(old_cell.len())
        .enumerate()
        .filter_map(|(index, bytes)| (bytes == old_cell.as_ref()).then_some(index))
        .collect::<Vec<_>>();
    assert_eq!(locations.len(), 1, "the sample has one target cell");
    let target_start = locations[0];
    let mut expected = original[..target_start].to_vec();
    expected.extend_from_slice(&new_cell);
    expected.extend_from_slice(&original[target_start + old_cell.len()..]);
    assert_eq!(output, expected, "only A2's CP932 bytes changed");
    let reopened = crate::csv_adapter::CsvDocument::parse(output, EncodingChoice::ShiftJis, ',')
        .expect("saved file remains CP932 CSV");
    assert_eq!(reopened.newline, crate::csv_adapter::NewlineStyle::CrLf);
    assert_eq!(reopened.records[1].fields[0].value, "鉛筆改");
    assert_eq!(reopened.records[1].fields[1].value, "12");
}

/// Shows a clear encoding choice when neither UTF-8 nor CP932 can decode the file.
#[test]
fn csv_encoding_probe_failure_shows_a_selection_prompt() {
    let directory = TestDirectory::new();
    let path = directory.0.join("invalid.csv");
    fs::write(&path, [0x81]).expect("write a byte invalid in both supported encodings");
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let document_id = harness
        .state_mut()
        .start_path_load(path)
        .expect("CSV path is supported");

    let mut prompt_ready = false;
    for _ in 0..80 {
        super::scenario_tests::send_batch(&mut harness, []);
        prompt_ready = harness
            .state()
            .pending_csv_opens
            .get(&document_id)
            .is_some_and(|pending| {
                matches!(pending.prompt.as_ref(), Some(CsvLoadPrompt::Settings(settings))
                    if settings.selected_encoding == EncodingChoice::Auto
                        && settings.needs_encoding
                        && settings.previews.is_empty()
                        && settings.error == Some("status-csv-encoding-undetermined"))
            });
        if prompt_ready {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(
        prompt_ready,
        "unrecognized bytes show a visible encoding selection state"
    );
    for _ in 0..2 {
        super::scenario_tests::send_batch(&mut harness, []);
    }
    let select_label = context_text(&harness.ctx, "dialog-csv-encoding-select");
    assert!(
        harness
            .query_all(By::new().role(egui::Role::ComboBox).value(&select_label))
            .next()
            .is_some(),
        "the closed encoding control asks the user to choose a value"
    );
    let load_label = context_text(&harness.ctx, "dialog-csv-load");
    assert!(
        harness
            .query_all(
                By::new()
                    .role(egui::Role::Button)
                    .label_contains(&load_label)
            )
            .next()
            .expect("Load remains visible while the encoding is unknown")
            .accesskit_node()
            .is_disabled(),
        "Load is disabled while the encoding is unknown"
    );
    super::scenario_tests::send_event(
        &mut harness,
        egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: Some(egui::Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    assert!(
        harness.state().pending_csv_opens[&document_id]
            .prompt
            .is_some(),
        "Enter cannot close the prompt before an encoding is selected"
    );
    assert!(
        harness
            .query_all(By::new().label_contains(&context_text(
                &harness.ctx,
                "status-csv-encoding-undetermined",
            )),)
            .next()
            .is_some(),
        "the dialog explains that the encoding could not be determined"
    );
}

/// Opens the BOM sample, edits its first table, and preserves every other byte.
#[test]
fn bom_sample_edits_and_reopens_all_three_tables() {
    let directory = TestDirectory::new();
    let path = directory.0.join("bom-tables.md");
    fs::write(&path, include_bytes!("../../../samples/bom-tables.md"))
        .expect("copy the BOM Markdown sample");
    let original = fs::read(&path).expect("read original sample bytes");
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let context = harness.ctx.clone();
    harness.state_mut().language = crate::i18n::Language::Japanese;
    crate::i18n::set_context_language(&context, crate::i18n::Language::Japanese);
    let document_id = harness
        .state_mut()
        .start_path_load(path.clone())
        .expect("the Markdown sample opens through the product loader");
    for _ in 0..80 {
        super::scenario_tests::send_batch(&mut harness, []);
        if harness
            .state()
            .workspace
            .documents
            .contains_key(&document_id)
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let document = &harness.state().workspace.documents[&document_id];
    assert_eq!(
        document
            .tables
            .iter()
            .map(|table| table.name.as_str())
            .collect::<Vec<_>>(),
        ["表1", "追加注文", "表3"]
    );
    assert_eq!(
        document.tables[0]
            .cell(CellAddress { row: 1, column: 1 })
            .unwrap()
            .input,
        CellInput::Text("12".to_owned())
    );

    super::scenario_tests::click_cell(&mut harness, "B2");
    super::scenario_tests::send_batch(&mut harness, [egui::Event::Text("13".to_owned())]);
    super::scenario_tests::send_event(
        &mut harness,
        egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: Some(egui::Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    super::scenario_tests::wait_until(&mut harness, |app| {
        app.workspace.documents[&document_id].tables[0]
            .cell(CellAddress { row: 1, column: 1 })
            .is_some_and(|cell| cell.input == CellInput::Text("13".to_owned()))
    });
    let modifiers = super::scenario_tests::command_modifiers(&harness.ctx);
    super::scenario_tests::send_event(
        &mut harness,
        super::scenario_tests::key_event(egui::Key::S, modifiers),
    );
    super::scenario_tests::wait_until(&mut harness, |app| {
        !app.pending_saves.contains_key(&document_id)
            && !app.document_has_unsaved_changes(document_id)
    });

    let old_cell = b"12";
    let offsets = original
        .windows(old_cell.len())
        .enumerate()
        .filter_map(|(index, bytes)| (bytes == old_cell).then_some(index))
        .collect::<Vec<_>>();
    assert_eq!(offsets.len(), 1, "the sample has one value equal to 12");
    let mut expected = original[..offsets[0]].to_vec();
    expected.extend_from_slice(b"13");
    expected.extend_from_slice(&original[offsets[0] + old_cell.len()..]);
    let saved = fs::read(&path).expect("read the normally saved Markdown file");
    assert_eq!(saved, expected, "only the edited value changed");

    let mut reopened = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let context = reopened.ctx.clone();
    reopened.state_mut().language = crate::i18n::Language::Japanese;
    crate::i18n::set_context_language(&context, crate::i18n::Language::Japanese);
    let reopened_id = reopened
        .state_mut()
        .start_path_load(path)
        .expect("the normally saved BOM file can be reopened");
    for _ in 0..80 {
        super::scenario_tests::send_batch(&mut reopened, []);
        if reopened
            .state()
            .workspace
            .documents
            .contains_key(&reopened_id)
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let reopened_doc = &reopened.state().workspace.documents[&reopened_id];
    assert_eq!(reopened_doc.tables.len(), 3);
    assert_eq!(reopened_doc.tables[0].name, "表1");
    assert_eq!(reopened_doc.tables[1].name, "追加注文");
    assert_eq!(reopened_doc.tables[2].name, "表3");
    assert_eq!(
        reopened_doc.tables[0]
            .cell(CellAddress { row: 1, column: 1 })
            .unwrap()
            .input,
        CellInput::Text("13".to_owned())
    );
}

#[test]
// Sends a real Edit-menu action back to the focused CSV delimiter field.
fn edit_menu_select_all_keeps_focus_in_the_csv_text_field() {
    let directory = TestDirectory::new();
    let path = directory.0.join("ambiguous.csv");
    fs::write(&path, b"a,b;c\nd,e;f\n").expect("ambiguous CSV is written");
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let background_document = harness.state().workspace.active_document.unwrap();
    harness.state_mut().queue_open_paths([path]);
    let document_id = (0..80)
        .find_map(|_| {
            harness.step();
            if let Some((document_id, _pending)) = harness
                .state()
                .pending_csv_opens
                .iter()
                .find(|(_, pending)| pending.prompt.is_some())
            {
                return Some(*document_id);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
            None
        })
        .expect("ambiguous CSV reaches its settings prompt");
    let prompt_id = Id::unique(egui::IdSalt::new(("csv_load_prompt", document_id)));
    let prompt_rect = harness
        .ctx
        .memory(|memory| memory.area_rect(prompt_id))
        .expect("the CSV settings window has a position");
    let prompt_area = egui::AreaState {
        pivot_pos: Some(egui::pos2(340.0, 240.0)),
        size: Some(prompt_rect.size()),
        ..Default::default()
    };
    harness.ctx.memory_mut(|memory| {
        memory.areas_mut().set_state(
            egui::LayerId::new(egui::Order::Middle, prompt_id),
            prompt_area,
        );
    });
    harness.step();
    let delimiter_id = Id::unique(egui::IdSalt::new(("csv_custom_delimiter", document_id)));
    harness
        .ctx
        .memory_mut(|memory| memory.request_focus(delimiter_id));
    harness.step();
    assert_eq!(
        harness.ctx.memory(|memory| memory.focused()),
        Some(delimiter_id),
        "the actual CSV dialog TextEdit has focus"
    );
    assert_eq!(
        harness.state().last_modal_text_input_focus,
        Some(delimiter_id),
        "the app remembers this modal field as the text owner"
    );

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let menu_state = harness.state().native_menu_states_for_frame(
            crate::ui::top::TopActionState::default(),
            true,
            &harness.ctx,
        );
        assert!(menu_state["edit.select_all"].enabled);
        let target = harness.state().native_action_target(&harness.ctx, true);
        harness.state_mut().native_menu_events_for_test =
            vec![crate::native_menu::NativeMenuEvent {
                id: "edit.select_all".to_owned(),
                recent_path: None,
                target,
            }];
        harness.step();
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let edit = context_text(&harness.ctx, "menu-edit");
        let edit_rect = harness
            .query_all(By::new().role(egui::Role::Button).label_contains(&edit))
            .next()
            .expect("Edit menu is visible")
            .rect();
        harness.event(Event::PointerMoved(edit_rect.center()));
        harness.step();
        harness.event(Event::PointerButton {
            pos: edit_rect.center(),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        });
        harness.step();
        assert_eq!(
            harness.state().modal_menu_text_focus,
            Some(delimiter_id),
            "pointer-down on Edit retains the focused modal field"
        );
        harness.event(Event::PointerButton {
            pos: edit_rect.center(),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        harness.step();
        harness.run_steps(2);
        let select_all = context_text(&harness.ctx, "action-select-all");
        harness
            .query_all(
                By::new()
                    .role(egui::Role::Button)
                    .label_contains(&select_all),
            )
            .next()
            .expect("Edit menu offers the text Select All command")
            .click();
        harness.step();
    }
    assert_eq!(
        harness.state().command_input_owner(&harness.ctx, true),
        (
            crate::ui::commands::InputOwner::OtherText,
            Some(delimiter_id)
        ),
        "the menu sees the real focused modal TextEdit"
    );
    assert_eq!(
        harness.ctx.memory(|memory| memory.focused()),
        Some(delimiter_id),
        "the native text command keeps focus in the same CSV field"
    );
    harness.step();
    harness.event(Event::Text("!".to_owned()));
    harness.step();

    assert!(matches!(
        harness.state().pending_csv_opens[&document_id]
            .prompt
            .as_ref()
            .expect("the settings dialog remains open"),
        CsvLoadPrompt::Settings(settings) if settings.delimiter_text == "!"
    ));
    assert!(!harness.state().workspace.documents[&background_document].can_undo());
}

#[test]
// Ignore progress and completion from an older request without committing a live edit.
fn stale_load_result_does_not_commit_another_document_edit() {
    let directory = TestDirectory::new();
    let path = directory.0.join("stale.csv");
    fs::write(&path, b"a,b\n1,2\n").expect("write CSV test input");
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let document_id = harness.state().workspace.active_document.unwrap();
    let document_revision = harness.state().workspace.documents[&document_id].revision;
    let old_token = JobToken {
        document_id,
        revision: document_revision,
        job_id: 900,
    };
    {
        let app = harness.state_mut();
        let _ = edit_blank_cell(app);
        app.edit.buffer = "preserve this edit".to_owned();
        app.pending_csv_opens.insert(
            document_id,
            PendingCsvOpen {
                path: path.clone(),
                encoding: EncodingChoice::Auto,
                delimiter: Some(','),
                prompt: None,
                job_id: old_token.job_id,
                retry_submission: None,
            },
        );
        app.file_worker
            .load_csv(old_token, path, EncodingChoice::Auto, Some(','))
            .expect("submit stale load");
        app.pending_csv_opens
            .get_mut(&document_id)
            .expect("current CSV request remains open")
            .job_id = old_token.job_id + 1;
    }

    for _ in 0..80 {
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    assert!(harness.state().pending_load_previews.is_empty());
    assert_eq!(harness.state().workspace.order, vec![document_id]);
    assert_eq!(
        harness.state().workspace.documents[&document_id].revision,
        document_revision
    );
    assert_eq!(harness.state().edit.buffer, "preserve this edit");
}

/// Clicks the load action exposed by the CSV settings window.
fn click_csv_load(harness: &mut Harness<'_, TexcelApp>) {
    let label = context_text(&harness.ctx, "dialog-csv-load");
    harness
        .query_all(By::new().role(egui::Role::Button).label_contains(&label))
        .next()
        .expect("CSV load action is visible")
        .click();
    harness.step();
}

/// Waits until the custom CSV load result is installed in the workspace.
fn wait_for_csv_load(harness: &mut Harness<'_, TexcelApp>, document_id: DocumentId) {
    for _ in 0..400 {
        harness.step();
        // The loaded document shows once its first column fit is done.
        let app = harness.state();
        if !app.pending_csv_opens.contains_key(&document_id)
            && !app.initial_auto_fits.contains_key(&document_id)
        {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("CSV load did not finish");
}

/// Keeps recovery decisions from recording a background grid focus or scroll request.
#[test]
fn modal_dialog_blocks_background_accesskit_focus_and_scroll() {
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    harness.step();
    let (document_id, table_id) = {
        let document = harness
            .state()
            .workspace
            .active_document()
            .expect("the default document is active");
        (
            document.id,
            document
                .active_table()
                .expect("the default table is active")
                .id,
        )
    };
    let address = CellAddress { row: 0, column: 0 };
    let token = harness.state().accesskit_table_tokens[&(document_id, table_id)];
    let target = grid_accesskit_cell_id(token, address, 0)
        .expect("the active cell has an accessibility ID")
        .accesskit_id();
    harness.state_mut().save_conflict_dialog = Some(crate::ui::save_conflict::SaveConflictPrompt {
        document_id,
        close_after_save: false,
    });
    harness.state_mut().pending_accesskit_focus = Some((document_id, table_id, address));
    harness.state_mut().request_scroll_to_address = Some((document_id, table_id, address));
    harness.event(egui::Event::AccessKitActionRequest(
        egui::accesskit::ActionRequest {
            action: egui::accesskit::Action::Click,
            target_tree: egui::accesskit::TreeId::ROOT,
            target_node: target,
            data: None,
        },
    ));
    harness.step();
    assert!(harness.state().pending_accesskit_focus.is_none());
    assert!(harness.state().request_scroll_to_address.is_none());
    harness.event(egui::Event::AccessKitActionRequest(
        egui::accesskit::ActionRequest {
            action: egui::accesskit::Action::ScrollIntoView,
            target_tree: egui::accesskit::TreeId::ROOT,
            target_node: target,
            data: None,
        },
    ));
    harness.step();
    assert!(harness.state().pending_accesskit_focus.is_none());
    assert!(harness.state().request_scroll_to_address.is_none());
}

/// Keeps the detail editor usable while rejecting accessibility actions behind it.
#[test]
fn detail_editor_modal_keeps_text_and_blocks_background_accesskit_actions() {
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    harness.step();
    let document_id = harness.state().workspace.active_document.unwrap();
    let table_id = harness.state().workspace.documents[&document_id]
        .active_table()
        .unwrap()
        .id;
    harness
        .state_mut()
        .workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .tables[0]
        .set_dimensions(2, 2);
    harness.step();
    let target_address = CellAddress { row: 1, column: 1 };
    let token = harness.state().accesskit_table_tokens[&(document_id, table_id)];
    let target = grid_accesskit_cell_id(token, target_address, 0)
        .unwrap()
        .accesskit_id();
    harness
        .state_mut()
        .begin_cell_edit(table_id, CellAddress { row: 0, column: 0 });
    {
        let app = harness.state_mut();
        app.edit
            .switch_target(crate::document::EditTarget::DetailPanel);
        app.edit.buffer = "draft".to_owned();
        app.long_text_panel_open = true;
        app.request_editor_focus = true;
    }
    harness.step();
    let editor = editor_id(document_id, harness.state().edit.session_id);
    assert!(harness.ctx.memory(|memory| memory.has_focus(editor)));
    assert_eq!(
        harness.state().modal_text_session(),
        Some((9, document_id, harness.state().edit.session_id)),
        "the native Edit menu can identify this modal editor session"
    );
    assert_eq!(
        harness.state().command_input_owner(&harness.ctx, true),
        (crate::ui::commands::InputOwner::OtherText, Some(editor)),
        "the modal detail editor is a text owner, not the grid editor"
    );

    harness.input_mut().events.extend([
        egui::Event::Text("x".to_owned()),
        egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
            action: egui::accesskit::Action::Click,
            target_tree: egui::accesskit::TreeId::ROOT,
            target_node: target,
            data: None,
        }),
        egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
            action: egui::accesskit::Action::ScrollIntoView,
            target_tree: egui::accesskit::TreeId::ROOT,
            target_node: target,
            data: None,
        }),
    ]);
    harness.step();
    assert!(harness.state().edit.buffer.contains('x'));
    assert!(harness.state().pending_accesskit_focus.is_none());
    assert!(harness.state().request_scroll_to_address.is_none());

    harness.input_mut().events.extend([
        egui::Event::ModifiersChanged(egui::Modifiers::COMMAND),
        egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: Some(egui::Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        },
    ]);
    harness.step();
    assert!(!harness.state().edit.is_editing());
    assert!(
        harness.state().workspace.documents[&document_id].tables[0]
            .cell(CellAddress { row: 0, column: 0 })
            .unwrap()
            .input
            .source_text()
            .contains('x')
    );
}

/// Sends Escape only to the app-close modal above an active detail draft.
#[test]
fn app_close_escape_keeps_the_detail_draft_open() {
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    harness.step();
    let document_id = harness.state().workspace.active_document.unwrap();
    let table_id = harness.state().workspace.documents[&document_id]
        .active_table()
        .unwrap()
        .id;
    {
        let app = harness.state_mut();
        app.begin_cell_edit(table_id, CellAddress { row: 0, column: 0 });
        app.edit
            .switch_target(crate::document::EditTarget::DetailPanel);
        app.edit.buffer = "keep this draft".to_owned();
        app.long_text_panel_open = true;
        app.request_editor_focus = true;
    }
    harness.step();
    harness.state_mut().app_close_dialog_open = true;
    harness.step();
    harness.input_mut().events.push(egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: Some(egui::Key::Escape),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.step();

    assert!(!harness.state().app_close_dialog_open);
    assert!(harness.state().edit.is_editing());
    assert!(harness.state().long_text_panel_open);
    assert_eq!(harness.state().edit.buffer, "keep this draft");
}

/// Keeps accessibility actions available behind a non-modal load preview.
#[test]
fn load_preview_does_not_block_existing_document_accessibility() {
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    harness.step();
    let document_id = harness.state().workspace.active_document.unwrap();
    let table_id = harness.state().workspace.documents[&document_id]
        .active_table()
        .unwrap()
        .id;
    harness
        .state_mut()
        .workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .tables[0]
        .set_dimensions(2, 2);
    harness.step();
    let target_address = CellAddress { row: 1, column: 1 };
    let token = harness.state().accesskit_table_tokens[&(document_id, table_id)];
    let target = grid_accesskit_cell_id(token, target_address, 0)
        .unwrap()
        .accesskit_id();
    harness
        .state_mut()
        .pending_load_previews
        .insert(document_id, preview_state(document_id, 3, 2));
    harness.step();
    assert!(!harness.state().modal_input_blocked());
    assert_ne!(
        harness.state().workspace.documents[&document_id]
            .view(table_id)
            .unwrap()
            .active_cell(),
        target_address
    );

    harness
        .input_mut()
        .events
        .push(egui::Event::AccessKitActionRequest(
            egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Click,
                target_tree: egui::accesskit::TreeId::ROOT,
                target_node: target,
                data: None,
            },
        ));
    harness.step();
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .view(table_id)
            .unwrap()
            .active_cell(),
        target_address
    );
}
