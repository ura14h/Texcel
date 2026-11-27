use super::*;
use egui_kittest::{
    Harness,
    kittest::{By, NodeT, Queryable},
};
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static NEXT_SCENARIO_DIRECTORY: AtomicU64 = AtomicU64::new(1);

pub(super) struct ScenarioDirectory(pub(super) PathBuf);

impl ScenarioDirectory {
    /// Creates a private directory for one set of UI scenarios.
    pub(super) fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let serial = NEXT_SCENARIO_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "texcel-scenario-{}-{stamp}-{serial}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create scenario directory");
        Self(path)
    }
}

impl Drop for ScenarioDirectory {
    /// Removes only the temporary files created by this scenario.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Creates the app through the normal settings and document constructor.
pub(super) fn scenario_harness(settings_path: PathBuf) -> Harness<'static, TexcelApp> {
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .with_max_steps(10)
        .build_eframe(move |creation| {
            TexcelApp::new_with_settings_path(creation, Some(settings_path))
        });
    wait_until(&mut harness, |app| app.settings_loaded);
    set_test_language(&mut harness);
    harness.step();
    harness
}

/// Uses English labels so each scenario reaches the same visible control.
fn set_test_language(harness: &mut Harness<'_, TexcelApp>) {
    let ctx = harness.ctx.clone();
    let app = harness.state_mut();
    app.language = crate::i18n::Language::English;
    app.language_choice = LanguageChoice::English;
    crate::i18n::set_context_language(&ctx, app.language);
}

/// Sends ordered input through Texcel's production raw-input hook and UI frame.
pub(super) fn send_batch(
    harness: &mut Harness<'_, TexcelApp>,
    events: impl IntoIterator<Item = egui::Event>,
) {
    harness.input_mut().events.extend(events);
    let ctx = harness.ctx.clone();
    let mut raw = std::mem::take(harness.input_mut());
    eframe::App::raw_input_hook(harness.state_mut(), &ctx, &mut raw);
    *harness.input_mut() = raw;
    harness.step();
}

/// Sends one key or text event through the same input path used by the app.
pub(super) fn send_event(harness: &mut Harness<'_, TexcelApp>, event: egui::Event) {
    let key_details = match &event {
        egui::Event::Key {
            key,
            physical_key,
            modifiers,
            ..
        } => Some((*key, *physical_key, *modifiers)),
        _ => None,
    };
    if let Some((key, physical_key, modifiers)) = key_details {
        send_batch(harness, [egui::Event::ModifiersChanged(modifiers), event]);
        send_batch(
            harness,
            [
                egui::Event::Key {
                    key,
                    physical_key,
                    pressed: false,
                    repeat: false,
                    modifiers,
                },
                egui::Event::ModifiersChanged(egui::Modifiers::NONE),
            ],
        );
    } else {
        send_batch(harness, [event]);
    }
}

/// Builds the legacy terminal notification used to verify explicit IME cancellation.
#[allow(deprecated)]
fn ime_disabled_event() -> egui::Event {
    egui::Event::Ime(egui::ImeEvent::Disabled)
}

/// Waits for a worker or settings result while the normal app keeps drawing.
pub(super) fn wait_until(harness: &mut Harness<'_, TexcelApp>, ready: impl Fn(&TexcelApp) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready(harness.state()) {
        send_batch(harness, []);
        assert!(
            Instant::now() < deadline,
            "scenario worker timed out: pending input {:?}, active document {:?}, edit document {:?}",
            harness.state().pending_edit_input,
            harness.state().workspace.active_document,
            harness.state().edit_document_id
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Clicks a visible accessible button through the production input hook.
fn click_button(harness: &mut Harness<'_, TexcelApp>, label: &str) {
    let nodes = harness
        .query_all(By::new().role(egui::Role::Button).label_contains(label))
        .collect::<Vec<_>>();
    let node = nodes
        .first()
        .unwrap_or_else(|| panic!("button {label:?} is visible; buttons: {nodes:?}"));
    let point = node.rect().center();
    click_point(harness, point);
    harness.step();
}

/// Waits for a modal button to keep one position before sending a real pointer click.
fn click_button_after_layout(harness: &mut Harness<'_, TexcelApp>, label: &str) {
    let mut previous = None;
    let mut current = None;
    for _ in 0..5 {
        harness.step();
        current = harness
            .query_all(By::new().role(egui::Role::Button).label(label))
            .next()
            .map(|node| node.rect());
        if current.is_some() && current == previous {
            break;
        }
        previous = current;
    }
    let point = current
        .unwrap_or_else(|| panic!("button {label:?} is visible after layout"))
        .center();
    click_point(harness, point);
    harness.step();
}

/// Opens Settings through the native menu adapter or the visible Linux menu.
fn open_settings(harness: &mut Harness<'_, TexcelApp>) {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
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
        click_button(harness, "File");
        click_button(harness, "Settings");
    }
}

/// Clicks the actual close choice rectangle reported by the drawn dialog.
fn click_close_choice(harness: &mut Harness<'_, TexcelApp>, choice: &'static str) {
    let mut previous = None;
    for _ in 0..4 {
        harness.step();
        let current = harness.state().test_close_button_rects.get(choice).copied();
        if current == previous {
            break;
        }
        previous = current;
    }
    let rect = harness
        .state()
        .test_close_button_rects
        .get(choice)
        .unwrap_or_else(|| panic!("close choice {choice:?} was drawn"))
        .to_owned();
    let point = rect.center();
    click_point(harness, point);
    harness.step();
}

/// Clicks a real app-exit action after its modal geometry stops moving.
fn click_app_close_choice(harness: &mut Harness<'_, TexcelApp>, choice: &'static str) {
    let mut previous = None;
    for _ in 0..5 {
        harness.step();
        let current = harness
            .state()
            .test_app_close_button_rects
            .get(choice)
            .copied();
        if current.is_some() && current == previous {
            break;
        }
        previous = current;
    }
    let point = harness
        .state()
        .test_app_close_button_rects
        .get(choice)
        .unwrap_or_else(|| panic!("app-close choice {choice:?} was drawn"))
        .center();
    click_point(harness, point);
    harness.step();
}

/// Sends a full pointer click through Texcel's raw input hook.
pub(super) fn click_point(harness: &mut Harness<'_, TexcelApp>, point: Pos2) {
    send_batch(harness, [egui::Event::PointerMoved(point)]);
    send_batch(
        harness,
        [egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    send_batch(
        harness,
        [egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
}

/// Selects a real cell before the edit shortcut is sent.
pub(super) fn click_cell(harness: &mut Harness<'_, TexcelApp>, label: &str) {
    let address = super::parse_cell_address(label);
    let point = harness
        .query_all(By::new().role(egui::Role::GridCell))
        .find(|node| {
            let accessible = node.accesskit_node();
            match address {
                Some(address) => {
                    crate::ui::grid_accessibility::decode_grid_cell_node_id(accessible.locate().0)
                        .is_some_and(|(_, actual, _)| actual == address)
                }
                None => accessible
                    .label()
                    .is_some_and(|accessible| accessible.contains(label)),
            }
        })
        .unwrap_or_else(|| panic!("cell {label:?} is visible"))
        .rect()
        .center();
    click_point(harness, point);
    assert!(
        harness.state().grid_focused,
        "the clicked cell owns the grid"
    );
    if let Some(expected) = address {
        let actual = harness
            .state()
            .workspace
            .active_document()
            .and_then(|document| {
                document
                    .active_table()
                    .and_then(|table| document.view(table.id))
                    .map(|view| view.active_cell())
            });
        assert_eq!(actual, Some(expected), "the clicked address is selected");
    }
}

#[test]
/// Selects the requested cell from coordinate metadata under Japanese labels.
fn click_cell_uses_grid_coordinates_in_japanese() {
    let directory = ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    let (document_id, table_id) =
        install_test_markdown_grid(harness.state_mut(), 2, 2, "Japanese grid coordinates");
    harness.state_mut().language = crate::i18n::Language::Japanese;
    harness.state_mut().language_choice = LanguageChoice::Japanese;
    crate::i18n::set_context_language(&harness.ctx, crate::i18n::Language::Japanese);
    harness.step();

    click_cell(&mut harness, "B2");
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .view(table_id)
            .unwrap()
            .active_cell(),
        CellAddress { row: 1, column: 1 }
    );
}

/// Moves to an address through the normal name-box input path.
pub(super) fn select_cell_by_name_box(harness: &mut Harness<'_, TexcelApp>, target: &str) {
    let current = harness
        .state()
        .workspace
        .active_document()
        .and_then(|document| {
            document
                .active_table()
                .and_then(|table| document.view(table.id))
                .map(|view| super::selection_name(view.selection))
        })
        .unwrap_or_else(|| panic!("the active grid has a current cell"));
    let rect = harness
        .query_all(By::new().role(egui::Role::TextInput))
        .find(|node| node.value().as_deref() == Some(current.as_str()))
        .unwrap_or_else(|| panic!("the name box shows {current}"))
        .rect();
    click_point(harness, rect.center());
    send_event(harness, key_event(Key::A, command_modifiers(&harness.ctx)));
    send_event(harness, egui::Event::Text(target.to_owned()));
    send_event(harness, key_event(Key::Enter, egui::Modifiers::NONE));
    if let Some(expected) = super::parse_cell_address(target) {
        let actual = harness
            .state()
            .workspace
            .active_document()
            .and_then(|document| {
                document
                    .active_table()
                    .and_then(|table| document.view(table.id))
                    .map(|view| view.active_cell())
            });
        assert_eq!(actual, Some(expected), "the name box selected {target}");
    }
}

/// Builds the platform command modifier used by a normal key press.
pub(super) fn command_modifiers(ctx: &egui::Context) -> egui::Modifiers {
    let mut modifiers = egui::Modifiers::COMMAND;
    if ctx.os().is_mac() {
        modifiers.mac_cmd = true;
    } else {
        modifiers.ctrl = true;
    }
    modifiers
}

/// Loads a user-selected path through the same FIFO used by the file chooser.
pub(super) fn queue_open_and_wait(harness: &mut Harness<'_, TexcelApp>, path: PathBuf) {
    harness.state_mut().queue_open_paths([path]);
    wait_until(harness, |app| {
        app.active_queued_open.is_none()
            && app.queued_open_paths.is_empty()
            && app.pending_markdown_loads.is_empty()
            && app.pending_html_loads.is_empty()
            && app.pending_csv_opens.is_empty()
            && app.pending_workbook_loads.is_empty()
    });
}

/// Creates a real key event with the requested modifiers.
pub(super) fn key_event(key: Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// Returns one cell's stored source text for an end-to-end assertion.
pub(super) fn cell_text(
    app: &TexcelApp,
    document_id: DocumentId,
    address: CellAddress,
) -> Option<String> {
    app.workspace
        .documents
        .get(&document_id)?
        .active_table()?
        .cell(address)
        .map(|cell| cell.input.source_text().to_owned())
}

/// Reads source text from a named table without changing the active table.
fn table_cell_text(
    app: &TexcelApp,
    document_id: DocumentId,
    table_id: TableId,
    address: CellAddress,
) -> Option<String> {
    app.workspace
        .documents
        .get(&document_id)?
        .tables
        .iter()
        .find(|table| table.id == table_id)?
        .cell(address)
        .map(|cell| cell.input.source_text().to_owned())
}

/// Requests a real root window close and keeps the resulting viewport command available.
fn request_viewport_close(harness: &mut Harness<'_, TexcelApp>) {
    harness
        .input_mut()
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .events
        .push(egui::ViewportEvent::Close);
    harness.step();
}

/// Confirms the first save using the format dialog and the supplied file picker path.
fn confirm_first_save_destination(harness: &mut Harness<'_, TexcelApp>) {
    wait_until(harness, |app| app.export_dialog.is_some());
    let label = context_text(&harness.ctx, "export-choose-destination");
    click_button_after_layout(harness, &label);
}

/// Waits for the host to finish settings and recovery writes before closing its windows.
fn wait_for_exit_transaction(harness: &mut Harness<'_, TexcelApp>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !harness.state().app_exit_ready
        && !viewport_command_sent(harness, egui::ViewportCommand::Close)
    {
        send_batch(harness, []);
        let app = harness.state();
        assert!(
            Instant::now() < deadline,
            "quit did not finish: status={:?}, transaction={}, prompt={}, settings_wait={}, recovery_wait={}, recovery_pending={}, start_pending={}",
            app.status,
            app.app_exit_transaction,
            app.app_close_dialog_open,
            app.close_after_settings_save,
            app.app_close_waiting_recovery_maintenance,
            app.recovery_maintenance_pending(),
            app.app_exit_start_pending
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Checks that the app answered the last root viewport close request.
fn viewport_command_sent(
    harness: &Harness<'_, TexcelApp>,
    expected: egui::ViewportCommand,
) -> bool {
    harness
        .output()
        .viewport_output
        .get(&egui::ViewportId::ROOT)
        .is_some_and(|output| output.commands.contains(&expected))
}

#[test]
/// Cancels the first close while Settings saves, then sends the final close response.
fn clean_startup_close_waits_for_settings_without_a_document_prompt() {
    let directory = ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    open_settings(&mut harness);
    harness.step();
    assert!(harness.state().settings_window_open);
    click_button(&mut harness, "Dark");
    assert_eq!(harness.state().theme, ThemeChoice::Dark);

    request_viewport_close(&mut harness);
    assert!(!harness.state().app_close_dialog_open);
    assert!(!harness.state().app_close_needs_save());
    assert!(viewport_command_sent(
        &harness,
        egui::ViewportCommand::CancelClose
    ));
    wait_until(&mut harness, |app| {
        !app.close_after_settings_save
            && app.settings_saved_generation == app.settings_save_generation
    });
    assert!(viewport_command_sent(
        &harness,
        egui::ViewportCommand::Close
    ));
    assert!(!harness.state().app_close_dialog_open);
}

#[test]
/// Puts every Settings choice back to the app defaults from the dialog's reset button.
fn settings_reset_restores_the_app_defaults() {
    let directory = ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    open_settings(&mut harness);
    harness.step();
    click_button(&mut harness, "Dark");
    harness.state_mut().show_invisibles = true;
    harness.state_mut().font_preset = crate::settings::FontPreset::NotoSansJp;
    harness.step();
    assert_eq!(harness.state().theme, ThemeChoice::Dark);

    click_button_after_layout(&mut harness, "Restore Defaults");
    let defaults = crate::settings::AppPreferences::default();
    let app = harness.state();
    assert!(app.settings_window_open, "reset keeps the dialog open");
    assert_eq!(app.theme, defaults.theme);
    assert_eq!(app.font_preset, defaults.font_preset);
    assert_eq!(app.language_choice, defaults.language);
    assert_eq!(app.show_invisibles, defaults.show_invisibles);
    assert_eq!(app.ui_font_points, defaults.ui_font_points);
}

#[test]
/// Accepts an untouched startup close without asking about a document.
fn untouched_startup_quit_is_accepted_without_a_document_prompt() {
    let directory = ScenarioDirectory::new();
    let settings_dir = directory.0.join("untouched-startup");
    fs::create_dir_all(&settings_dir).expect("create isolated settings directory");
    let mut harness = scenario_harness(settings_dir.join("settings.json"));
    let first_close_generation = harness.state().settings_save_generation;
    assert!(!harness.state().app_close_needs_save());
    assert!(!harness.state().app_close_dialog_open);

    request_viewport_close(&mut harness);
    let first_close_commands = harness.output().viewport_output[&egui::ViewportId::ROOT]
        .commands
        .clone();

    assert!(!harness.state().app_close_dialog_open);
    if first_close_commands
        .iter()
        .any(|command| matches!(command, egui::ViewportCommand::CancelClose))
    {
        wait_until(&mut harness, |app| {
            !app.close_after_settings_save
                && app.settings_saved_generation == first_close_generation
        });
        assert!(viewport_command_sent(
            &harness,
            egui::ViewportCommand::Close
        ));
    } else {
        // eframe accepts a root close request when the app does not cancel it.
        assert!(!harness.state().app_close_dialog_open);
    }
}

#[test]
/// Closes a newly created untouched document without a document close prompt.
fn new_document_closes_without_a_prompt() {
    let directory = ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    let command = command_modifiers(&harness.ctx);
    send_event(&mut harness, key_event(Key::N, command));
    assert_eq!(harness.state().workspace.documents.len(), 2);
    let new_document = harness.state().workspace.active_document.unwrap();
    send_event(&mut harness, key_event(Key::W, command));
    assert_eq!(harness.state().workspace.documents.len(), 1);
    assert!(
        !harness
            .state()
            .workspace
            .documents
            .contains_key(&new_document)
    );
    assert!(harness.state().pending_close_document.is_none());
    assert!(!harness.state().app_close_dialog_open);
}

#[test]
/// Replaces only untouched startup placeholders after successful file reads.
fn successful_failed_and_edited_startup_loads_keep_the_right_documents() {
    let directory = ScenarioDirectory::new();
    let valid_path = directory.0.join("opened.md");
    fs::write(&valid_path, "| A | B |\n| --- | --- |\n| abc | keep |\n")
        .expect("write a small Markdown document");

    let mut clean = scenario_harness(directory.0.join("clean-settings.json"));
    let placeholder = clean.state().workspace.active_document.unwrap();
    queue_open_and_wait(&mut clean, valid_path.clone());
    assert!(!clean.state().workspace.documents.contains_key(&placeholder));
    assert_eq!(clean.state().workspace.documents.len(), 1);

    let mut failed = scenario_harness(directory.0.join("failed-settings.json"));
    let failed_placeholder = failed.state().workspace.active_document.unwrap();
    let missing_path = directory.0.join("missing.md");
    failed.state_mut().queue_open_paths([missing_path]);
    wait_until(&mut failed, |app| {
        app.active_queued_open.is_none() && app.queued_open_paths.is_empty()
    });
    assert!(
        failed
            .state()
            .workspace
            .documents
            .contains_key(&failed_placeholder)
    );

    let mut edited = scenario_harness(directory.0.join("edited-settings.json"));
    let edited_placeholder = edited.state().workspace.active_document.unwrap();
    click_cell(&mut edited, "A1");
    send_event(&mut edited, key_event(Key::F2, egui::Modifiers::NONE));
    edited.step();
    send_event(&mut edited, egui::Event::Text("keep this draft".to_owned()));
    assert!(edited.state().edit.is_editing());
    queue_open_and_wait(&mut edited, valid_path);
    assert!(
        edited
            .state()
            .workspace
            .documents
            .contains_key(&edited_placeholder)
    );
    assert_eq!(edited.state().workspace.documents.len(), 2);
    assert_eq!(
        edited.state().workspace.documents[&edited_placeholder]
            .active_table()
            .unwrap()
            .cell(CellAddress::default())
            .unwrap()
            .input
            .source_text(),
        "keep this draft"
    );
}

#[test]
/// Keeps confirmed same-frame text and restores selected source text on IME cancel.
fn ime_cancel_preserves_confirmed_text_and_selected_source() {
    let directory = ScenarioDirectory::new();
    let source_path = directory.0.join("ime.md");
    fs::write(&source_path, "| A | B |\n| --- | --- |\n| abc | keep |\n")
        .expect("write a document with a selected cell value");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    queue_open_and_wait(&mut harness, source_path.clone());

    click_cell(&mut harness, "A2");
    send_event(&mut harness, key_event(Key::F2, egui::Modifiers::NONE));
    harness.step();
    let command = command_modifiers(&harness.ctx);
    send_event(&mut harness, key_event(Key::A, command));
    let editor = editor_id(
        harness.state().edit_document_id.unwrap(),
        harness.state().edit.session_id,
    );
    assert_eq!(harness.ctx.memory(|memory| memory.focused()), Some(editor));
    send_event(
        &mut harness,
        egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "にほん".to_owned(),
            active_range_chars: None,
        }),
    );
    assert!(harness.state().edit.composing);
    assert_eq!(harness.state().edit.buffer, "にほん");

    send_event(&mut harness, key_event(Key::Escape, egui::Modifiers::NONE));
    assert!(!harness.state().edit.composing);
    assert_eq!(harness.state().edit.buffer, "abc");
    assert!(harness.state().edit.is_editing());
    assert_eq!(harness.ctx.memory(|memory| memory.focused()), Some(editor));
    assert!(
        !harness
            .state()
            .document_has_unsaved_changes(harness.state().edit_document_id.unwrap())
    );

    send_event(
        &mut harness,
        key_event(Key::ArrowRight, egui::Modifiers::NONE),
    );
    send_event(&mut harness, egui::Event::Text("!".to_owned()));
    assert_eq!(harness.state().edit.buffer, "abc!");
    assert!(
        harness
            .state()
            .document_has_unsaved_changes(harness.state().edit_document_id.unwrap())
    );

    let b2 = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("B2"))
        .next()
        .expect("B2 is visible")
        .rect();
    let header_point = Pos2::new(b2.center().x, b2.top() - HEADER_HEIGHT / 2.0);
    click_point(&mut harness, header_point);
    let document_id = harness.state().workspace.active_document.unwrap();
    let table_id = harness.state().workspace.documents[&document_id].active_table;
    assert!(!harness.state().edit.is_editing());
    assert_eq!(
        cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 0 }
        )
        .as_deref(),
        Some("abc!")
    );
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .view(table_id)
            .unwrap()
            .selection
            .anchor
            .column,
        1
    );

    let next_cell = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("B2"))
        .next()
        .expect("the selected column still exposes B2")
        .rect()
        .center();
    send_batch(
        &mut harness,
        [
            egui::Event::PointerMoved(next_cell),
            egui::Event::PointerButton {
                pos: next_cell,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos: next_cell,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    send_event(&mut harness, key_event(Key::F2, egui::Modifiers::NONE));
    let command = command_modifiers(&harness.ctx);
    send_event(&mut harness, key_event(Key::A, command));
    send_event(&mut harness, egui::Event::Text("next".to_owned()));
    assert!(harness.state().edit.is_editing());
    assert_eq!(
        harness.state().edit.address,
        Some(CellAddress { row: 1, column: 1 })
    );
    assert_eq!(harness.state().edit.buffer, "next");
    send_event(&mut harness, key_event(Key::Enter, egui::Modifiers::NONE));
    wait_until(&mut harness, |app| app.pending_structures.is_empty());
    assert_eq!(
        cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 1 }
        )
        .as_deref(),
        Some("next")
    );

    let command = command_modifiers(&harness.ctx);
    send_event(&mut harness, key_event(Key::S, command));
    wait_until(&mut harness, |app| app.pending_saves.is_empty());
    let saved = crate::markdown_adapter::MarkdownDocument::parse(
        fs::read(&source_path).expect("saved Markdown remains available"),
    )
    .expect("saved Markdown can be parsed");
    assert_eq!(saved.tables[0].rows[1][0].value, "abc!");
    assert_eq!(saved.tables[0].rows[1][1].value, "next");
    let mut reopened = scenario_harness(directory.0.join("reopen-settings.json"));
    queue_open_and_wait(&mut reopened, source_path);
    let reopened_id = reopened.state().workspace.active_document.unwrap();
    assert_eq!(
        cell_text(
            reopened.state(),
            reopened_id,
            CellAddress { row: 1, column: 0 }
        )
        .as_deref(),
        Some("abc!")
    );
    assert_eq!(
        cell_text(
            reopened.state(),
            reopened_id,
            CellAddress { row: 1, column: 1 }
        )
        .as_deref(),
        Some("next")
    );
}

#[test]
/// Uses real event order to keep ordinary text before an IME preedit on cancellation.
fn same_frame_text_before_preedit_survives_escape() {
    let directory = ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    click_cell(&mut harness, "A1");
    send_event(&mut harness, key_event(Key::F2, egui::Modifiers::NONE));
    harness.step();
    let (document_id, session_id) = {
        let app = harness.state();
        (app.edit_document_id.unwrap(), app.edit.session_id)
    };
    let editor = editor_id(document_id, session_id);
    assert_eq!(harness.ctx.memory(|memory| memory.focused()), Some(editor));

    send_batch(
        &mut harness,
        [
            egui::Event::Text("confirmed".to_owned()),
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: "にほん".to_owned(),
                active_range_chars: None,
            }),
        ],
    );
    assert!(harness.state().edit.composing);
    assert_eq!(
        harness.state().ime_composition.as_ref().unwrap().buffer,
        "confirmed"
    );
    send_event(&mut harness, key_event(Key::Escape, egui::Modifiers::NONE));
    assert!(!harness.state().edit.composing);
    assert_eq!(harness.state().edit.buffer, "confirmed");
    assert!(harness.state().edit.is_editing());
}

#[test]
/// Starts IME editing from a selected cell without F2 and restores its old source on cancel.
fn direct_ime_start_preserves_an_existing_cell_value() {
    let directory = ScenarioDirectory::new();
    let source_path = directory.0.join("direct-ime.md");
    fs::write(&source_path, "| A | B |\n| --- | --- |\n| abc | keep |\n")
        .expect("write a cell with existing text");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    queue_open_and_wait(&mut harness, source_path);

    click_cell(&mut harness, "A2");
    send_event(
        &mut harness,
        egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "にほん".to_owned(),
            active_range_chars: None,
        }),
    );
    let document_id = harness.state().workspace.active_document.unwrap();
    assert!(harness.state().edit.is_editing());
    assert!(harness.state().edit.composing);
    assert_eq!(harness.state().edit.mode, EditMode::Input);
    assert_eq!(harness.state().edit.anchor_byte, 0);
    assert_eq!(
        harness.state().edit.buffer,
        "にほん",
        "the first preedit replaces the selected source in the active editor"
    );
    assert_eq!(harness.state().edit.cursor_byte, "にほん".len());
    assert_eq!(
        harness.state().ime_composition.as_ref().unwrap().buffer,
        "abc"
    );
    assert_eq!(
        harness
            .state()
            .ime_composition
            .as_ref()
            .unwrap()
            .anchor_byte,
        0
    );
    assert_eq!(
        harness
            .state()
            .ime_composition
            .as_ref()
            .unwrap()
            .cursor_byte,
        3
    );

    send_event(&mut harness, key_event(Key::Escape, egui::Modifiers::NONE));
    assert_eq!(harness.state().edit.buffer, "abc");
    assert!(!harness.state().edit.composing);
    assert!(!harness.state().document_has_unsaved_changes(document_id));
    let b2 = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("B2"))
        .next()
        .expect("B2 is visible")
        .rect();
    click_point(
        &mut harness,
        Pos2::new(b2.center().x, b2.top() - HEADER_HEIGHT / 2.0),
    );
    assert_eq!(
        cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 0 }
        )
        .as_deref(),
        Some("abc")
    );
}

#[test]
/// Replaces selected source through direct IME input and saves only its committed text.
fn direct_ime_commit_replaces_existing_text_and_survives_reload() {
    let directory = ScenarioDirectory::new();
    let source_path = directory.0.join("direct-ime-commit.md");
    fs::write(&source_path, "| A | B |\n| --- | --- |\n| abc | keep |\n")
        .expect("write a cell with existing text");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    queue_open_and_wait(&mut harness, source_path.clone());

    click_cell(&mut harness, "A2");
    send_event(
        &mut harness,
        egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "にほん".to_owned(),
            active_range_chars: None,
        }),
    );
    assert_eq!(harness.state().edit.mode, EditMode::Input);
    assert_eq!(harness.state().edit.anchor_byte, 0);
    assert_eq!(harness.state().edit.buffer, "にほん");
    assert_eq!(harness.state().edit.cursor_byte, "にほん".len());
    send_event(
        &mut harness,
        egui::Event::Ime(egui::ImeEvent::Commit("日本".to_owned())),
    );
    assert_eq!(harness.state().edit.buffer, "日本");

    send_event(&mut harness, key_event(Key::Enter, egui::Modifiers::NONE));
    wait_until(&mut harness, |app| app.pending_structures.is_empty());
    let document_id = harness.state().workspace.active_document.unwrap();
    assert_eq!(
        cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 0 }
        )
        .as_deref(),
        Some("日本")
    );
    let command = command_modifiers(&harness.ctx);
    send_event(&mut harness, key_event(Key::S, command));
    wait_until(&mut harness, |app| app.pending_saves.is_empty());
    let saved = crate::markdown_adapter::MarkdownDocument::parse(
        fs::read(&source_path).expect("the source path receives the saved edit"),
    )
    .expect("the committed value remains valid Markdown");
    assert_eq!(saved.tables[0].rows[1][0].value, "日本");

    let mut reopened = scenario_harness(directory.0.join("reopened-settings.json"));
    queue_open_and_wait(&mut reopened, source_path);
    let reopened_id = reopened.state().workspace.active_document.unwrap();
    assert_eq!(
        cell_text(
            reopened.state(),
            reopened_id,
            CellAddress { row: 1, column: 0 }
        )
        .as_deref(),
        Some("日本")
    );
}

#[test]
/// Starts IME directly from grid selection and blocks a same-frame app shortcut.
fn direct_ime_start_keeps_same_frame_app_shortcut_with_the_grid() {
    let directory = ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    click_cell(&mut harness, "A1");
    let document_id = harness.state().workspace.active_document.unwrap();
    let document_count = harness.state().workspace.order.len();
    let modifiers = command_modifiers(&harness.ctx);

    send_batch(
        &mut harness,
        [
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: "にほん".to_owned(),
                active_range_chars: None,
            }),
            egui::Event::ModifiersChanged(modifiers),
            key_event(Key::N, modifiers),
            egui::Event::Key {
                key: Key::N,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers,
            },
            egui::Event::ModifiersChanged(egui::Modifiers::NONE),
        ],
    );

    assert_eq!(harness.state().workspace.order.len(), document_count);
    assert_eq!(harness.state().workspace.active_document, Some(document_id));
    assert!(harness.state().edit.is_editing());
    assert!(harness.state().edit.composing);
    assert_eq!(harness.state().edit.buffer, "にほん");
    assert!(
        harness
            .state()
            .ime_composition
            .as_ref()
            .unwrap()
            .buffer
            .is_empty()
    );
    assert!(
        cell_text(harness.state(), document_id, CellAddress::default())
            .is_none_or(|text| text.is_empty())
    );

    send_event(
        &mut harness,
        egui::Event::Ime(egui::ImeEvent::Commit("日本".to_owned())),
    );
    assert!(!harness.state().edit.composing);
    assert_eq!(harness.state().edit.buffer, "日本");
    assert!(
        cell_text(harness.state(), document_id, CellAddress::default())
            .is_none_or(|text| text.is_empty())
    );
    send_event(&mut harness, key_event(Key::Enter, egui::Modifiers::NONE));
    assert!(!harness.state().edit.is_editing());
    assert!(harness.state().document_has_unsaved_changes(document_id));

    let saved_path = directory.0.join("committed-ime.md");
    harness.state_mut().test_save_path_selection = Some(saved_path.clone());
    let modifiers = command_modifiers(&harness.ctx);
    send_event(&mut harness, key_event(Key::S, modifiers));
    confirm_first_save_destination(&mut harness);
    wait_until(&mut harness, |app| {
        !app.pending_saves.contains_key(&document_id)
            && !app.document_has_unsaved_changes(document_id)
    });
    queue_open_and_wait(&mut harness, saved_path);
    let reloaded_document = harness.state().workspace.active_document.unwrap();
    assert_eq!(
        cell_text(harness.state(), reloaded_document, CellAddress::default()).as_deref(),
        Some("日本")
    );
}

#[test]
/// Keeps an empty preedit restore point for Escape, disable, and pointer departure.
fn empty_preedit_cancel_and_click_keep_the_selected_source_text() {
    let directory = ScenarioDirectory::new();
    for ending in ["escape", "disabled", "click"] {
        let source_path = directory.0.join(format!("empty-preedit-{ending}.md"));
        fs::write(&source_path, "| A | B |\n| --- | --- |\n| abc | keep |\n")
            .expect("write a cell with selected source text");
        let mut harness = scenario_harness(directory.0.join(format!("settings-{ending}.json")));
        queue_open_and_wait(&mut harness, source_path);
        click_cell(&mut harness, "A2");
        send_event(&mut harness, key_event(Key::F2, egui::Modifiers::NONE));
        let command = command_modifiers(&harness.ctx);
        send_event(&mut harness, key_event(Key::A, command));
        send_event(
            &mut harness,
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: String::new(),
                active_range_chars: None,
            }),
        );
        let document_id = harness.state().workspace.active_document.unwrap();
        assert_eq!(
            harness.state().ime_composition.as_ref().unwrap().buffer,
            "abc"
        );
        assert!(
            !harness
                .state()
                .ime_composition
                .as_ref()
                .unwrap()
                .preedit_active
        );

        match ending {
            "escape" => {
                send_event(&mut harness, key_event(Key::Escape, egui::Modifiers::NONE));
            }
            "disabled" => {
                send_event(&mut harness, ime_disabled_event());
            }
            "click" => {
                let b2 = harness
                    .query_all(By::new().role(egui::Role::GridCell).label_contains("B2"))
                    .next()
                    .expect("B2 is visible")
                    .rect();
                click_point(
                    &mut harness,
                    Pos2::new(b2.center().x, b2.top() - HEADER_HEIGHT / 2.0),
                );
            }
            _ => unreachable!(),
        }
        assert_eq!(
            cell_text(
                harness.state(),
                document_id,
                CellAddress { row: 1, column: 0 }
            )
            .as_deref(),
            Some("abc")
        );
        assert!(!harness.state().document_has_unsaved_changes(document_id));
        if ending != "click" {
            assert!(harness.state().edit.is_editing());
            assert_eq!(harness.state().edit.buffer, "abc");
        } else {
            assert!(!harness.state().edit.is_editing());
        }
    }
}

#[test]
/// Keeps a new preedit active after an unmarked editor session expires.
fn fresh_preedit_is_not_interrupted_by_the_previous_unmark() {
    let directory = ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    click_cell(&mut harness, "A1");
    send_batch(
        &mut harness,
        [egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "old".to_owned(),
            active_range_chars: None,
        })],
    );
    send_batch(
        &mut harness,
        [egui::Event::Ime(egui::ImeEvent::Preedit {
            text: String::new(),
            active_range_chars: None,
        })],
    );
    assert!(!harness.state().edit.composing);
    assert!(harness.state().ime_composition.is_some());

    send_batch(
        &mut harness,
        [
            egui::Event::Text("x".to_owned()),
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: "new".to_owned(),
                active_range_chars: None,
            }),
        ],
    );

    assert!(harness.state().edit.composing);
    assert_eq!(
        harness
            .state()
            .ime_composition
            .as_ref()
            .map(|state| state.buffer.as_str()),
        Some("x"),
        "ordinary text before the new preedit remains confirmed"
    );
    assert!(!harness.state().ime_interrupt_pending);
    let ime_output = harness
        .output()
        .platform_output
        .ime
        .as_ref()
        .expect("the active editor keeps an IME output");
    assert!(!ime_output.should_interrupt_composition);
}

#[test]
/// Requests a platform IME interrupt after pointer and window-focus cancellation.
fn cancelled_cell_preedit_reaches_the_platform_output() {
    let directory = ScenarioDirectory::new();
    let source_path = directory.0.join("ime-output.md");
    fs::write(&source_path, "| A | B |\n| --- | --- |\n|  | keep |\n")
        .expect("write a two-column document for focus changes");

    for destination in ["cell", "header", "mode-bar", "other-window"] {
        let mut harness =
            scenario_harness(directory.0.join(format!("settings-{destination}.json")));
        queue_open_and_wait(&mut harness, source_path.clone());
        click_cell(&mut harness, "A2");
        send_batch(
            &mut harness,
            [egui::Event::Ime(egui::ImeEvent::Preedit {
                text: "あ".to_owned(),
                active_range_chars: None,
            })],
        );
        assert!(harness.state().edit.composing);

        let mut pointer_position = None;
        if destination == "other-window" {
            send_batch(&mut harness, [egui::Event::WindowFocused(false)]);
        } else {
            let point = match destination {
                "cell" => {
                    let cell = harness
                        .query_all(By::new().role(egui::Role::GridCell).label_contains("B2"))
                        .next()
                        .expect("the next cell is visible")
                        .rect();
                    cell.center()
                }
                "header" => {
                    let first_row = harness
                        .query_all(By::new().role(egui::Role::GridCell).label_contains("B1"))
                        .next()
                        .expect("the first body row is visible")
                        .rect();
                    Pos2::new(first_row.center().x, first_row.top() - HEADER_HEIGHT / 2.0)
                }
                "mode-bar" => harness
                    .query_all(By::new().role(egui::Role::Button).label("Source Editing"))
                    .next()
                    .expect("the source mode control is visible beside the formula bar")
                    .rect()
                    .center(),
                _ => unreachable!(),
            };
            pointer_position = Some(point);
            send_batch(&mut harness, [egui::Event::PointerMoved(point)]);
            send_batch(
                &mut harness,
                [egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }

        let ime_output = harness.output().platform_output.ime.as_ref();
        assert!(
            ime_output.is_none_or(|ime| ime.should_interrupt_composition),
            "a focused TextEdit must expose the interrupt request for {destination}"
        );
        assert!(!harness.state().ime_interrupt_pending);
        assert!(
            !harness.state().edit.composing,
            "the application editor restores its confirmed state for {destination}"
        );

        if destination != "other-window" {
            let point = pointer_position.expect("pointer destinations have a position");
            send_batch(
                &mut harness,
                [egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            if destination == "header" {
                let document = harness.state().workspace.active_document().unwrap();
                let table = document.active_table().unwrap();
                let selection = &document.view(table.id).unwrap().selection;
                assert_eq!(selection.anchor.column, 1);
                assert_eq!(selection.head.column, 1);
                assert_eq!(selection.anchor.row.min(selection.head.row), 0);
                assert_eq!(
                    selection.anchor.row.max(selection.head.row),
                    MAX_ROWS - 1,
                    "the header click selects the full column"
                );
            }
        }
    }
}

#[test]
/// Applies an empty preedit followed by a normal IME commit only once.
fn empty_preedit_then_commit_replaces_the_selected_cell_text() {
    let directory = ScenarioDirectory::new();
    let source_path = directory.0.join("empty-then-commit.md");
    fs::write(&source_path, "| A | B |\n| --- | --- |\n| abc | keep |\n")
        .expect("write a cell with selected source text");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    queue_open_and_wait(&mut harness, source_path);
    click_cell(&mut harness, "A2");
    send_event(&mut harness, key_event(Key::F2, egui::Modifiers::NONE));
    let command = command_modifiers(&harness.ctx);
    send_event(&mut harness, key_event(Key::A, command));
    send_event(
        &mut harness,
        egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "にほん".to_owned(),
            active_range_chars: None,
        }),
    );

    send_event(
        &mut harness,
        egui::Event::Ime(egui::ImeEvent::Preedit {
            text: String::new(),
            active_range_chars: None,
        }),
    );
    assert!(!harness.state().edit.composing);
    assert_eq!(harness.state().edit.buffer, "abc");
    assert_eq!(
        (
            harness
                .state()
                .edit
                .anchor_byte
                .min(harness.state().edit.cursor_byte),
            harness
                .state()
                .edit
                .anchor_byte
                .max(harness.state().edit.cursor_byte),
        ),
        (0, 3)
    );
    send_event(
        &mut harness,
        egui::Event::Ime(egui::ImeEvent::Commit("日本".to_owned())),
    );
    assert_eq!(harness.state().edit.buffer, "日本");
    assert!(!harness.state().edit.composing);
    send_event(&mut harness, key_event(Key::Enter, egui::Modifiers::NONE));
    let document_id = harness.state().workspace.active_document.unwrap();
    assert_eq!(
        cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 0 }
        )
        .as_deref(),
        Some("日本")
    );
}

#[test]
/// Restores text committed before a later same-frame preedit is canceled.
fn same_frame_commit_then_preedit_cancel_keeps_the_committed_prefix() {
    let directory = ScenarioDirectory::new();
    let source_path = directory.0.join("commit-then-preedit.md");
    fs::write(&source_path, "| A | B |\n| --- | --- |\n| abc | keep |\n")
        .expect("write a cell with selected source text");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    queue_open_and_wait(&mut harness, source_path);
    click_cell(&mut harness, "A2");
    send_event(&mut harness, key_event(Key::F2, egui::Modifiers::NONE));
    let command = command_modifiers(&harness.ctx);
    send_event(&mut harness, key_event(Key::A, command));
    send_event(
        &mut harness,
        egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "にほん".to_owned(),
            active_range_chars: None,
        }),
    );

    send_batch(
        &mut harness,
        [
            egui::Event::Ime(egui::ImeEvent::Commit("日本".to_owned())),
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: "ご".to_owned(),
                active_range_chars: None,
            }),
        ],
    );
    assert_eq!(
        harness.state().ime_composition.as_ref().unwrap().buffer,
        "日本"
    );
    send_event(&mut harness, key_event(Key::Escape, egui::Modifiers::NONE));
    assert_eq!(harness.state().edit.buffer, "日本");
    assert!(!harness.state().edit.composing);
    send_event(&mut harness, key_event(Key::Enter, egui::Modifiers::NONE));
    let document_id = harness.state().workspace.active_document.unwrap();
    assert_eq!(
        cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 0 }
        )
        .as_deref(),
        Some("日本")
    );
}

#[test]
/// Drops an old commit after a canceled composition and a different editor starts.
fn late_ime_commit_cannot_enter_a_new_cell_editor() {
    let directory = ScenarioDirectory::new();
    let source_path = directory.0.join("late-commit.md");
    fs::write(&source_path, "| A | B |\n| --- | --- |\n| abc | keep |\n")
        .expect("write two editable cells");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    queue_open_and_wait(&mut harness, source_path);
    click_cell(&mut harness, "A2");
    send_event(&mut harness, key_event(Key::F2, egui::Modifiers::NONE));
    send_event(
        &mut harness,
        egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "にほん".to_owned(),
            active_range_chars: None,
        }),
    );
    send_event(&mut harness, key_event(Key::Escape, egui::Modifiers::NONE));
    assert_eq!(harness.state().edit.buffer, "abc");
    click_cell(&mut harness, "B2");
    send_event(&mut harness, key_event(Key::F2, egui::Modifiers::NONE));
    let new_editor = editor_id(
        harness.state().edit_document_id.unwrap(),
        harness.state().edit.session_id,
    );
    assert_eq!(harness.state().edit.buffer, "keep");

    send_batch(
        &mut harness,
        [
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: String::new(),
                active_range_chars: None,
            }),
            egui::Event::Ime(egui::ImeEvent::Commit("遅れ".to_owned())),
        ],
    );
    assert_eq!(harness.state().edit.buffer, "keep");
    assert_eq!(
        harness.ctx.memory(|memory| memory.focused()),
        Some(new_editor)
    );
}

#[test]
/// Drops an old empty-preedit terminal across frames and accepts a new preedit.
fn late_empty_preedit_terminal_stays_with_old_editor_across_frames() {
    let directory = ScenarioDirectory::new();
    let source_path = directory.0.join("late-empty-terminal.md");
    fs::write(&source_path, "| A | B |\n| --- | --- |\n| abc | keep |\n")
        .expect("write two editable cells");
    let mut stale = scenario_harness(directory.0.join("stale-settings.json"));
    queue_open_and_wait(&mut stale, source_path.clone());
    click_cell(&mut stale, "A2");
    send_event(&mut stale, key_event(Key::F2, egui::Modifiers::NONE));
    send_event(
        &mut stale,
        egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "にほん".to_owned(),
            active_range_chars: None,
        }),
    );
    send_event(&mut stale, key_event(Key::Escape, egui::Modifiers::NONE));
    click_cell(&mut stale, "B2");
    send_event(&mut stale, key_event(Key::F2, egui::Modifiers::NONE));
    send_event(
        &mut stale,
        egui::Event::Ime(egui::ImeEvent::Preedit {
            text: String::new(),
            active_range_chars: None,
        }),
    );
    assert_eq!(stale.state().edit.buffer, "keep");
    send_event(
        &mut stale,
        egui::Event::Ime(egui::ImeEvent::Commit("遅れ".to_owned())),
    );
    assert_eq!(stale.state().edit.buffer, "keep");

    let mut fresh = scenario_harness(directory.0.join("fresh-settings.json"));
    queue_open_and_wait(&mut fresh, source_path);
    click_cell(&mut fresh, "A2");
    send_event(&mut fresh, key_event(Key::F2, egui::Modifiers::NONE));
    send_event(
        &mut fresh,
        egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "旧変換".to_owned(),
            active_range_chars: None,
        }),
    );
    send_event(&mut fresh, key_event(Key::Escape, egui::Modifiers::NONE));
    click_cell(&mut fresh, "B2");
    send_event(&mut fresh, key_event(Key::F2, egui::Modifiers::NONE));
    let command = command_modifiers(&fresh.ctx);
    send_event(&mut fresh, key_event(Key::A, command));
    send_event(
        &mut fresh,
        egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "かな".to_owned(),
            active_range_chars: None,
        }),
    );
    assert!(fresh.state().edit.composing);
    assert_eq!(fresh.state().edit.buffer, "かな");
    send_event(
        &mut fresh,
        egui::Event::Ime(egui::ImeEvent::Commit("新".to_owned())),
    );
    assert_eq!(fresh.state().edit.buffer, "新");
}

#[test]
/// Cancels an active preedit on unmark, restores its selection, and resumes Tab.
fn active_preedit_unmark_only_restores_text_and_releases_navigation() {
    let directory = ScenarioDirectory::new();
    let source_path = directory.0.join("unmark-only.md");
    fs::write(&source_path, "| A | B |\n| --- | --- |\n| abc | keep |\n")
        .expect("write selected source text");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    queue_open_and_wait(&mut harness, source_path);
    let document_id = harness.state().workspace.active_document.unwrap();
    click_cell(&mut harness, "A2");
    send_event(&mut harness, key_event(Key::F2, egui::Modifiers::NONE));
    let command = command_modifiers(&harness.ctx);
    send_event(&mut harness, key_event(Key::A, command));
    send_event(
        &mut harness,
        egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "にほん".to_owned(),
            active_range_chars: None,
        }),
    );
    send_event(
        &mut harness,
        egui::Event::Ime(egui::ImeEvent::Preedit {
            text: String::new(),
            active_range_chars: None,
        }),
    );

    assert!(!harness.state().edit.composing);
    assert_eq!(harness.state().edit.buffer, "abc");
    assert_eq!(
        (
            harness
                .state()
                .edit
                .anchor_byte
                .min(harness.state().edit.cursor_byte),
            harness
                .state()
                .edit
                .anchor_byte
                .max(harness.state().edit.cursor_byte),
        ),
        (0, 3)
    );
    send_event(&mut harness, key_event(Key::Tab, egui::Modifiers::NONE));
    assert!(!harness.state().edit.is_editing());
    assert_eq!(
        cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 0 }
        )
        .as_deref(),
        Some("abc")
    );
    assert!(!harness.state().document_has_unsaved_changes(document_id));
}

#[test]
/// Preserves a commit before a header click when no OtherText field owns IME.
fn ime_commit_before_header_pointer_is_not_dropped_for_missing_modal_owner() {
    let directory = ScenarioDirectory::new();
    let source_path = directory.0.join("commit-before-header.md");
    fs::write(&source_path, "| A | B |\n| --- | --- |\n| abc | keep |\n")
        .expect("write a selected value");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    queue_open_and_wait(&mut harness, source_path);
    let document_id = harness.state().workspace.active_document.unwrap();
    let first_row_rect = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("B1"))
        .next()
        .expect("B1 is visible before editing")
        .rect();
    click_cell(&mut harness, "A2");
    send_event(&mut harness, key_event(Key::F2, egui::Modifiers::NONE));
    let command = command_modifiers(&harness.ctx);
    send_event(&mut harness, key_event(Key::A, command));
    send_event(
        &mut harness,
        egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "にほん".to_owned(),
            active_range_chars: None,
        }),
    );
    let pointer = Pos2::new(
        first_row_rect.center().x,
        first_row_rect.top() - HEADER_HEIGHT / 2.0,
    );

    send_batch(
        &mut harness,
        [
            egui::Event::PointerMoved(pointer),
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: String::new(),
                active_range_chars: None,
            }),
            egui::Event::Ime(egui::ImeEvent::Commit("日本".to_owned())),
            egui::Event::PointerButton {
                pos: pointer,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos: pointer,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );

    assert!(!harness.state().edit.is_editing());
    assert_eq!(
        cell_text(
            harness.state(),
            document_id,
            CellAddress { row: 1, column: 0 }
        )
        .as_deref(),
        Some("日本")
    );
    let document = &harness.state().workspace.documents[&document_id];
    let table_id = document.active_table;
    let selection = document.view(table_id).unwrap().selection;
    let last_row = MAX_ROWS - 1;
    assert_eq!(
        selection,
        CellRange {
            anchor: CellAddress { row: 0, column: 1 },
            head: CellAddress {
                row: last_row,
                column: 1,
            },
        }
    );
}

#[test]
/// Cancels a cell preedit on window focus loss and accepts text after focus returns.
fn window_focus_loss_cancels_preedit_and_keeps_the_confirmed_buffer() {
    let directory = ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    click_cell(&mut harness, "A1");
    send_event(&mut harness, key_event(Key::F2, egui::Modifiers::NONE));
    send_event(&mut harness, egui::Event::Text("confirmed".to_owned()));
    send_event(
        &mut harness,
        egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "にほん".to_owned(),
            active_range_chars: None,
        }),
    );
    assert!(harness.state().edit.composing);

    send_event(&mut harness, egui::Event::WindowFocused(false));
    assert!(!harness.state().edit.composing);
    assert_eq!(harness.state().edit.buffer, "confirmed");
    assert!(harness.state().edit.is_editing());
    send_event(&mut harness, egui::Event::WindowFocused(true));
    send_event(&mut harness, egui::Event::Text("!".to_owned()));
    assert!(!harness.state().edit.composing);
    assert_eq!(harness.state().edit.buffer, "confirmed!");
}

#[test]
/// Commits the current edit before a table tab switch and routes the next key to the new table.
fn table_tab_switch_commits_the_old_editor_before_the_next_input() {
    let directory = ScenarioDirectory::new();
    let source_path = directory.0.join("two-tables.md");
    fs::write(
        &source_path,
        "# First table\n\n| A | B |\n| --- | --- |\n| first | keep |\n\n# Second table\n\n| A | B |\n| --- | --- |\n| second | stay |\n",
    )
    .expect("write a document with two tables");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    queue_open_and_wait(&mut harness, source_path.clone());
    let document_id = harness.state().workspace.active_document.unwrap();
    let (first_table, second_table, second_table_name) = {
        let document = &harness.state().workspace.documents[&document_id];
        (
            document.tables[0].id,
            document.tables[1].id,
            document.tables[1].name.clone(),
        )
    };

    click_cell(&mut harness, "A2");
    send_event(&mut harness, key_event(Key::F2, egui::Modifiers::NONE));
    let command = command_modifiers(&harness.ctx);
    send_event(&mut harness, key_event(Key::A, command));
    send_event(&mut harness, egui::Event::Text("draft".to_owned()));
    click_button(&mut harness, &second_table_name);
    assert!(!harness.state().edit.is_editing());
    assert_eq!(
        table_cell_text(
            harness.state(),
            document_id,
            first_table,
            CellAddress { row: 1, column: 0 }
        )
        .as_deref(),
        Some("draft")
    );
    assert_eq!(
        harness.state().workspace.documents[&document_id].active_table,
        second_table
    );

    click_cell(&mut harness, "A2");
    send_event(&mut harness, key_event(Key::F2, egui::Modifiers::NONE));
    let command = command_modifiers(&harness.ctx);
    send_event(&mut harness, key_event(Key::A, command));
    send_event(&mut harness, egui::Event::Text("target".to_owned()));
    assert!(harness.state().edit.is_editing());
    assert_eq!(harness.state().edit.table_id, Some(second_table));
    assert_eq!(harness.state().edit.buffer, "target");
    send_event(&mut harness, key_event(Key::Enter, egui::Modifiers::NONE));
    wait_until(&mut harness, |app| app.pending_structures.is_empty());
    assert_eq!(
        table_cell_text(
            harness.state(),
            document_id,
            first_table,
            CellAddress { row: 1, column: 0 }
        ),
        Some("draft".to_owned())
    );
    assert_eq!(
        table_cell_text(
            harness.state(),
            document_id,
            second_table,
            CellAddress { row: 1, column: 0 }
        )
        .as_deref(),
        Some("target")
    );
    let command = command_modifiers(&harness.ctx);
    send_event(&mut harness, key_event(Key::S, command));
    wait_until(&mut harness, |app| app.pending_saves.is_empty());
    let saved = crate::markdown_adapter::MarkdownDocument::parse(
        fs::read(source_path).expect("saving keeps both selected tables"),
    )
    .expect("saved document is valid Markdown");
    assert_eq!(saved.tables[0].rows[1][0].value, "draft");
    assert_eq!(saved.tables[1].rows[1][0].value, "target");
}

#[test]
/// Keeps Enter and Escape with a search field until its IME sends a terminal event.
fn modal_search_ime_keeps_shortcuts_out_of_the_dialog_on_the_next_frame() {
    let directory = ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    let command = command_modifiers(&harness.ctx);
    send_event(&mut harness, key_event(Key::F, command));
    wait_until(&mut harness, |app| app.search_dialog.is_some());
    harness.step();
    let focused = harness.ctx.memory(|memory| memory.focused()).unwrap();
    assert!(harness.ctx.text_edit_focused());

    send_event(
        &mut harness,
        egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "にほん".to_owned(),
            active_range_chars: None,
        }),
    );
    assert_eq!(harness.state().modal_ime_composition, Some(focused));
    assert_eq!(
        harness.state().search_dialog.as_ref().unwrap().query,
        "にほん"
    );

    send_event(&mut harness, key_event(Key::Enter, egui::Modifiers::NONE));
    assert!(harness.state().search_dialog.is_some());
    assert!(harness.state().pending_searches.is_empty());
    assert_eq!(harness.ctx.memory(|memory| memory.focused()), Some(focused));
    assert!(harness.ctx.text_edit_focused());
    send_event(&mut harness, key_event(Key::Escape, egui::Modifiers::NONE));
    assert!(harness.state().search_dialog.is_some());
    assert_eq!(harness.ctx.memory(|memory| memory.focused()), Some(focused));
    assert!(harness.ctx.text_edit_focused());

    send_event(
        &mut harness,
        egui::Event::Ime(egui::ImeEvent::Commit("日本".to_owned())),
    );
    assert!(harness.state().modal_ime_composition.is_none());
    assert_eq!(
        harness.state().search_dialog.as_ref().unwrap().query,
        "日本"
    );
    send_event(&mut harness, key_event(Key::Escape, egui::Modifiers::NONE));
    assert!(harness.state().search_dialog.is_none());

    let mut pointer_close = scenario_harness(directory.0.join("pointer-settings.json"));
    let command = command_modifiers(&pointer_close.ctx);
    send_event(&mut pointer_close, key_event(Key::F, command));
    wait_until(&mut pointer_close, |app| app.search_dialog.is_some());
    pointer_close.step();
    let grid_editor = pointer_close.ctx.memory(|memory| memory.focused()).unwrap();
    send_event(
        &mut pointer_close,
        egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "にほん".to_owned(),
            active_range_chars: None,
        }),
    );
    assert_eq!(
        pointer_close.state().modal_ime_composition,
        Some(grid_editor)
    );
    click_button_after_layout(&mut pointer_close, "Close");
    assert!(pointer_close.state().search_dialog.is_none());
    pointer_close.step();
    assert!(pointer_close.state().grid_focused);
    assert!(!pointer_close.ctx.text_edit_focused());
}

#[test]
/// Gives each close choice its own real draft and visible confirmation button.
fn changed_document_close_supports_save_discard_and_cancel() {
    let directory = ScenarioDirectory::new();
    for choice in ["Cancel", "Discard Changes", "Save and Close"] {
        let choice_directory = directory.0.join(choice.replace(' ', "_"));
        fs::create_dir_all(&choice_directory).expect("create isolated choice directory");
        let settings_path = choice_directory.join("settings.json");
        let mut harness = scenario_harness(settings_path);
        let document_id = harness.state().workspace.active_document.unwrap();
        click_cell(&mut harness, "A1");
        send_event(&mut harness, key_event(Key::F2, egui::Modifiers::NONE));
        harness.step();
        send_event(&mut harness, egui::Event::Text(format!("draft-{choice}")));
        assert!(harness.state().document_has_unsaved_changes(document_id));

        let command = command_modifiers(&harness.ctx);
        send_event(&mut harness, key_event(Key::W, command));
        assert_eq!(harness.state().pending_close_document, Some(document_id));
        if choice == "Save and Close" {
            harness.state_mut().test_save_path_selection =
                Some(choice_directory.join("saved-on-close.md"));
        }
        let close_choice = match choice {
            "Cancel" => "cancel",
            "Discard Changes" => "discard",
            "Save and Close" => "save",
            _ => unreachable!(),
        };
        click_close_choice(&mut harness, close_choice);

        match choice {
            "Cancel" => {
                assert!(
                    harness
                        .state()
                        .workspace
                        .documents
                        .contains_key(&document_id)
                );
                assert!(harness.state().edit.is_editing());
                assert_eq!(harness.state().edit.buffer, "draft-Cancel");
            }
            "Discard Changes" => {
                assert!(
                    !harness
                        .state()
                        .workspace
                        .documents
                        .contains_key(&document_id)
                );
                assert!(harness.state().pending_close_document.is_none());
            }
            "Save and Close" => {
                confirm_first_save_destination(&mut harness);
                wait_until(&mut harness, |app| {
                    !app.workspace.documents.contains_key(&document_id)
                        && app.pending_saves.is_empty()
                });
                assert!(
                    fs::read_to_string(choice_directory.join("saved-on-close.md"))
                        .expect("save choice writes a file")
                        .contains("draft-Save and Close")
                );
            }
            _ => unreachable!(),
        }
    }
}

#[test]
/// Saves a dirty backed document even when an untouched editor remains open elsewhere.
fn app_close_save_all_ignores_an_untouched_editor_on_another_document() {
    let directory = ScenarioDirectory::new();
    let saved_path = directory.0.join("saved.md");
    let untouched_path = directory.0.join("untouched.md");
    fs::write(&saved_path, "| A | B |\n| --- | --- |\n| old | keep |\n")
        .expect("write the dirty backed document");
    fs::write(
        &untouched_path,
        "| A | B |\n| --- | --- |\n| stay | safe |\n",
    )
    .expect("write the clean document");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    queue_open_and_wait(&mut harness, saved_path.clone());
    let saved_document = harness.state().workspace.active_document.unwrap();
    click_cell(&mut harness, "A2");
    send_event(&mut harness, key_event(Key::F2, egui::Modifiers::NONE));
    let command = command_modifiers(&harness.ctx);
    send_event(&mut harness, key_event(Key::A, command));
    send_event(&mut harness, egui::Event::Text("changed".to_owned()));
    send_event(&mut harness, key_event(Key::Enter, egui::Modifiers::NONE));
    assert!(harness.state().document_has_unsaved_changes(saved_document));

    queue_open_and_wait(&mut harness, untouched_path);
    let untouched_document = harness.state().workspace.active_document.unwrap();
    click_cell(&mut harness, "B2");
    send_event(&mut harness, key_event(Key::F2, egui::Modifiers::NONE));
    assert!(harness.state().edit.is_editing());
    assert_eq!(harness.state().edit_document_id, Some(untouched_document));
    assert!(
        !harness
            .state()
            .document_has_unsaved_changes(untouched_document)
    );

    request_viewport_close(&mut harness);
    assert!(harness.state().app_close_dialog_open);
    assert!(viewport_command_sent(
        &harness,
        egui::ViewportCommand::CancelClose
    ));
    click_app_close_choice(&mut harness, "save");
    assert_eq!(harness.state().edit_document_id, Some(untouched_document));
    assert!(harness.state().edit.is_editing());
    assert!(harness.state().pending_saves.contains_key(&saved_document));
    let saves_finished = |app: &TexcelApp| {
        app.app_close_save_tokens.is_empty()
            && app.pending_saves.is_empty()
            && app.pending_save_intents.is_empty()
            && !app.app_close_dialog_open
            && !app.app_close_save_failed
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    while !saves_finished(harness.state()) {
        harness.step();
        assert!(
            Instant::now() < deadline,
            "Save All stalled: tokens={:?}, pending={:?}, intents={:?}, dialog={}, failed={}, needs_save={}, pending_operations={}, recovery_maintenance={}, editing={}, edit_doc={:?}, status={:?}",
            harness.state().app_close_save_tokens,
            harness.state().pending_saves,
            harness.state().pending_save_intents,
            harness.state().app_close_dialog_open,
            harness.state().app_close_save_failed,
            harness.state().app_close_needs_save(),
            harness.state().app_close_has_pending_operations(),
            harness.state().recovery_maintenance_pending(),
            harness.state().edit.is_editing(),
            harness.state().edit_document_id,
            harness.state().status
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    wait_until(&mut harness, |app| {
        !app.app_close_after_documents
            && !app.close_after_settings_save
            && app.settings_saved_generation == app.settings_save_generation
    });
    let saved = crate::markdown_adapter::MarkdownDocument::parse(
        fs::read(saved_path).expect("Save All writes the backed file"),
    )
    .expect("Save All writes valid Markdown");
    assert_eq!(saved.tables[0].rows[1][0].value, "changed");
    assert_eq!(harness.state().edit_document_id, Some(untouched_document));
    assert!(harness.state().edit.is_editing());
    assert!(viewport_command_sent(
        &harness,
        egui::ViewportCommand::Close
    ));
}

#[test]
/// Cancels Save All before destination choice without committing the active cell draft.
fn cancelling_save_all_destination_keeps_editor_selection_and_history() {
    let directory = ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    let document_id = harness.state().workspace.active_document.unwrap();
    let revision = harness.state().workspace.documents[&document_id].revision;
    let history = harness.state().workspace.documents[&document_id].history_state_id();
    let can_undo = harness.state().workspace.documents[&document_id].can_undo();

    click_cell(&mut harness, "A1");
    send_event(&mut harness, egui::Event::Text("draft".to_owned()));
    send_event(&mut harness, key_event(Key::F2, egui::Modifiers::NONE));
    send_event(
        &mut harness,
        key_event(Key::ArrowLeft, egui::Modifiers::SHIFT),
    );
    let edit_before = harness.state().edit.clone();
    assert_eq!(edit_before.buffer, "draft");
    assert_ne!(edit_before.anchor_byte, edit_before.cursor_byte);

    request_viewport_close(&mut harness);
    assert!(harness.state().app_close_dialog_open);
    click_app_close_choice(&mut harness, "save");
    assert!(harness.state().export_dialog.is_some());
    assert_eq!(harness.state().edit.buffer, edit_before.buffer);
    assert_eq!(harness.state().edit.anchor_byte, edit_before.anchor_byte);
    assert_eq!(harness.state().edit.cursor_byte, edit_before.cursor_byte);
    assert_eq!(harness.state().edit.session_id, edit_before.session_id);

    let cancel = crate::i18n::context_text(&harness.ctx, "action-cancel");
    click_button_after_layout(&mut harness, &cancel);
    let labels = harness
        .query_all(By::new().role(egui::Role::Button))
        .filter_map(|node| node.accesskit_node().label())
        .collect::<Vec<_>>();
    assert!(
        harness.state().export_dialog.is_none(),
        "Cancel closes the save dialog: labels={labels:?}, status={:?}, failure={}",
        harness.state().status,
        harness.state().app_close_save_failed
    );
    assert!(harness.state().app_close_dialog_open);
    assert!(!harness.state().app_close_save_batch_active);
    assert_eq!(harness.state().edit.buffer, edit_before.buffer);
    assert_eq!(harness.state().edit.anchor_byte, edit_before.anchor_byte);
    assert_eq!(harness.state().edit.cursor_byte, edit_before.cursor_byte);
    assert_eq!(harness.state().edit.session_id, edit_before.session_id);
    let document = &harness.state().workspace.documents[&document_id];
    assert_eq!(document.revision, revision);
    assert_eq!(document.history_state_id(), history);
    assert_eq!(document.can_undo(), can_undo);
}

#[test]
/// Keeps a chosen Save All path while source growth waits for its worker acknowledgement.
fn app_close_save_all_keeps_the_chosen_path_across_editor_growth() {
    let directory = ScenarioDirectory::new();
    let destination = directory.0.join("grown-at-exit.md");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    let document_id = harness.state().workspace.active_document.unwrap();
    select_cell_by_name_box(&mut harness, "A3");
    send_event(
        &mut harness,
        egui::Event::Text("saved after growth".to_owned()),
    );
    assert!(harness.state().edit.is_editing());
    assert_eq!(harness.state().edit.buffer, "saved after growth");
    assert!(harness.state().document_has_unsaved_changes(document_id));

    request_viewport_close(&mut harness);
    click_app_close_choice(&mut harness, "save");
    assert!(harness.state().export_dialog.is_some());
    harness.state_mut().test_save_path_selection = Some(destination.clone());
    let choose = crate::i18n::context_text(&harness.ctx, "export-choose-destination");
    click_button_after_layout(&mut harness, &choose);
    assert!(harness.state().export_dialog.is_none());
    assert!(harness.state().test_save_path_selection.is_none());
    assert!(
        destination.is_file()
            || harness
                .state()
                .pending_structures
                .contains_key(&document_id)
            || harness
                .state()
                .pending_save_intents
                .contains_key(&document_id)
            || harness.state().pending_saves.contains_key(&document_id)
            || harness
                .state()
                .app_close_save_tokens
                .contains_key(&document_id),
        "chosen path starts a save or its boundary worker: doc={:?}, edit={:?}, growth={:?}, structure={:?}, intents={:?}, saves={:?}, close_batch={}, failed={}, backing={:?}, status={:?}",
        harness
            .state()
            .workspace
            .documents
            .get(&document_id)
            .map(|doc| (doc.revision, doc.saved_revision, doc.is_dirty())),
        harness.state().edit,
        harness.state().pending_text_growth_sessions,
        harness.state().pending_structures,
        harness.state().pending_save_intents,
        harness.state().pending_saves,
        harness.state().app_close_save_batch_active,
        harness.state().app_close_save_failed,
        harness
            .state()
            .file_backings
            .get(&document_id)
            .map(|backing| &backing.path),
        harness.state().status,
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !destination.is_file()
        || !harness.state().pending_structures.is_empty()
        || !harness.state().pending_text_growth_sessions.is_empty()
        || !harness.state().pending_saves.is_empty()
        || !harness.state().pending_save_intents.is_empty()
    {
        send_batch(&mut harness, []);
        assert!(
            Instant::now() < deadline,
            "Save All after growth stalled: exit_ready={}, exit_after_docs={}, close_dialog={}, transaction={}, documents={:?}, output={}, backing={:?}, structures={:?}, growth={:?}, saves={:?}, intents={:?}, tokens={:?}, queued={:?}, targets={:?}, collecting={}, active={}, failed={}, status={:?}",
            harness.state().app_exit_ready,
            harness.state().app_close_after_documents,
            harness.state().app_close_dialog_open,
            harness.state().app_exit_transaction,
            harness
                .state()
                .workspace
                .order
                .iter()
                .map(|id| (
                    *id,
                    harness.state().workspace.documents[id].revision,
                    harness.state().workspace.documents[id].saved_revision,
                    harness.state().document_has_unsaved_changes(*id)
                ))
                .collect::<Vec<_>>(),
            destination.is_file(),
            harness
                .state()
                .file_backings
                .get(&document_id)
                .map(|backing| &backing.path),
            harness.state().pending_structures,
            harness.state().pending_text_growth_sessions,
            harness.state().pending_saves,
            harness.state().pending_save_intents,
            harness.state().app_close_save_tokens,
            harness.state().pending_app_close_save_queue,
            harness.state().pending_app_close_save_targets,
            harness.state().app_close_save_collecting_targets,
            harness.state().app_close_save_batch_active,
            harness.state().app_close_save_failed,
            harness.state().status,
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    let saved = crate::markdown_adapter::MarkdownDocument::parse(
        std::fs::read(&destination).expect("the chosen Save All path is written"),
    )
    .expect("the grown output is valid Markdown");
    assert_eq!(saved.tables[0].rows[2][0].value, "saved after growth");
    assert!(harness.state().test_save_path_selection.is_none());
}

#[test]
/// Loads defaults from a missing settings file, saves through Settings and restores on restart.
fn first_run_missing_settings_directory_saves_on_close_and_restores() {
    let directory = ScenarioDirectory::new();
    let settings_path = directory.0.join("new").join("nested").join("settings.json");
    assert!(!settings_path.parent().unwrap().exists());

    let mut first = scenario_harness(settings_path.clone());
    assert!(first.state().status.is_none());
    open_settings(&mut first);
    first.step();
    click_button(&mut first, "Dark");
    assert_eq!(first.state().theme, ThemeChoice::Dark);
    request_viewport_close(&mut first);
    wait_until(&mut first, |app| {
        !app.close_after_settings_save
            && app.settings_saved_generation == app.settings_save_generation
    });
    assert!(!first.state().app_close_dialog_open);
    assert!(settings_path.parent().unwrap().is_dir());
    let saved: SettingsFile = serde_json::from_slice(
        &fs::read(&settings_path).expect("first settings save creates its parent path"),
    )
    .expect("settings file is valid JSON");
    assert_eq!(saved.app.theme, ThemeChoice::Dark);

    let restarted = scenario_harness(settings_path);
    assert_eq!(restarted.state().theme, ThemeChoice::Dark);
    assert!(restarted.state().status.is_none());
}

#[cfg(unix)]
#[test]
/// Keeps a failed cleanup on disk and retries it after a real close attempt.
fn close_waits_for_recovery_removal_ack_and_can_retry_failure() {
    use std::os::unix::fs::PermissionsExt;

    let directory = ScenarioDirectory::new();
    let case_dir = directory.0.join("cleanup-close");
    fs::create_dir_all(&case_dir).expect("create isolated settings directory");
    let settings_path = case_dir.join("settings.json");
    let source_path = case_dir.join("source.md");
    fs::write(&source_path, "| A | B |\n| --- | --- |\n| base | keep |\n")
        .expect("write a source for the recovery record");

    let mut first = scenario_harness(settings_path.clone());
    queue_open_and_wait(&mut first, source_path);
    let document_id = first.state().workspace.active_document.unwrap();
    click_cell(&mut first, "A2");
    send_event(&mut first, egui::Event::Text("recoverable".to_owned()));
    send_event(&mut first, key_event(Key::Enter, egui::Modifiers::NONE));
    wait_until(&mut first, |app| {
        app.recovery_schedules
            .get(&document_id)
            .is_some_and(|schedule| schedule.last_success.is_some())
    });
    let key = first.state().recovery_keys[&document_id].clone();
    drop(first);

    let recovery_dir = case_dir.join("recovery");
    let manifest = recovery_dir.join(format!("{key}.json"));
    assert!(manifest.is_file());
    let original_mode = fs::metadata(&recovery_dir)
        .expect("read recovery folder permissions")
        .permissions()
        .mode();
    let mut readonly = fs::metadata(&recovery_dir)
        .expect("read recovery folder permissions")
        .permissions();
    readonly.set_mode(original_mode & !0o222);
    fs::set_permissions(&recovery_dir, readonly).expect("make recovery cleanup fail");

    // The restart reopens the work; discarding it asks the worker to remove the record.
    let mut restarted = scenario_harness(settings_path);
    wait_until(&mut restarted, |app| {
        app.recovery_keys.values().any(|saved| saved == &key)
    });
    let recovered_id = restarted
        .state()
        .recovery_keys
        .iter()
        .find_map(|(id, saved)| (saved == &key).then_some(*id))
        .expect("the recovered document keeps its record key");
    restarted
        .state_mut()
        .discard_and_close_document(recovered_id);
    request_viewport_close(&mut restarted);
    assert!(viewport_command_sent(
        &restarted,
        egui::ViewportCommand::CancelClose
    ));
    wait_until(&mut restarted, |app| {
        !app.app_close_waiting_recovery_maintenance
            && app.failed_recovery_maintenance.contains(&key)
    });
    let close_was_cancelled = !viewport_command_sent(&restarted, egui::ViewportCommand::Close);
    let cleanup_is_retryable =
        restarted.state().pending_recovery_removals.contains(&key) && manifest.is_file();

    let mut restored_permissions = fs::metadata(&recovery_dir)
        .expect("read recovery folder permissions")
        .permissions();
    restored_permissions.set_mode(original_mode);
    fs::set_permissions(&recovery_dir, restored_permissions)
        .expect("restore recovery folder permissions");
    assert!(
        close_was_cancelled,
        "the failed cleanup must not close the app"
    );
    assert!(
        cleanup_is_retryable,
        "the failed request and recovery record remain"
    );

    request_viewport_close(&mut restarted);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !viewport_command_sent(&restarted, egui::ViewportCommand::Close) {
        send_batch(&mut restarted, []);
        assert!(Instant::now() < deadline, "cleanup retry did not finish");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        !manifest.exists(),
        "the successful retry removes the old recovery"
    );
    assert!(restarted.state().pending_recovery_removals.is_empty());
    assert!(restarted.state().in_flight_recovery_maintenance.is_empty());
}

#[test]
/// Closes untouched and IME-cancelled clean apps without creating recovery candidates.
fn clean_first_run_exit_has_no_recovery_candidate() {
    let directory = ScenarioDirectory::new();

    for ending in ["untouched", "ime-cancel", "xlsx-import"] {
        let case_dir = directory.0.join(ending);
        fs::create_dir_all(&case_dir).expect("create an isolated settings directory");
        let settings_path = case_dir.join("settings.json");
        let mut first = scenario_harness(settings_path.clone());
        if ending == "xlsx-import" {
            queue_open_and_wait(
                &mut first,
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("samples/workbook.xlsx"),
            );
        }
        if first.state().active_import_report.is_some() {
            let close = context_text(&first.ctx, "action-close");
            click_button_after_layout(&mut first, &close);
            assert!(first.state().active_import_report.is_none());
        }
        let document_id = first.state().workspace.active_document.unwrap();
        if ending == "ime-cancel" {
            click_cell(&mut first, "A1");
            send_batch(
                &mut first,
                [egui::Event::Ime(egui::ImeEvent::Preedit {
                    text: "あ".to_owned(),
                    active_range_chars: None,
                })],
            );
            send_event(&mut first, key_event(Key::Escape, egui::Modifiers::NONE));
            assert!(!first.state().edit.composing);
            assert!(!first.state().document_has_unsaved_changes(document_id));
        }
        assert!(!first.state().app_close_needs_save());
        let idle_deadline = Instant::now() + Duration::from_millis(2_300);
        while Instant::now() < idle_deadline {
            send_batch(&mut first, []);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(
            first
                .state()
                .recovery_schedules
                .get(&document_id)
                .is_none_or(|schedule| {
                    schedule.signature.is_none()
                        && schedule.last_success.is_none()
                        && schedule.in_flight.is_none()
                })
        );
        assert_eq!(
            fs::read_dir(case_dir.join("recovery"))
                .ok()
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
                .count(),
            0,
            "a clean untitled document does not produce a recovery manifest"
        );
        // The production host closes its windows when this transaction becomes ready.
        first.state_mut().multi_window_mode = true;
        quit_app_from_menu(&mut first);
        assert!(!first.state().app_close_dialog_open);
        wait_for_exit_transaction(&mut first);
        drop(first);

        let mut restarted = scenario_harness(settings_path);
        for _ in 0..30 {
            send_batch(&mut restarted, []);
            std::thread::sleep(Duration::from_millis(4));
        }
        assert!(restarted.state().recovery_restore_pending.is_empty());
        assert!(restarted.state().recovery_items.is_empty());
        let recovery_root = case_dir.join("recovery");
        let manifests = fs::read_dir(&recovery_root)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
            .count();
        assert_eq!(manifests, 0, "a clean exit leaves no recovery manifest");
    }
}

#[test]
/// Recreates a deleted file on Save without asking, and keeps edits when its folder is gone.
fn deleted_csv_path_is_recreated_on_save() {
    let directory = ScenarioDirectory::new();
    let csv_directory = directory.0.join("csv");
    fs::create_dir_all(&csv_directory).expect("create CSV folder");
    let csv_path = csv_directory.join("missing.csv");
    fs::write(&csv_path, b"Name,Score\nAki,12\n").expect("write CSV source");
    let mut csv = scenario_harness(directory.0.join("csv-settings.json"));
    queue_open_and_wait(&mut csv, csv_path.clone());
    let csv_id = csv
        .state()
        .workspace
        .active_document
        .expect("the CSV file has an adopted document");
    click_cell(&mut csv, "A2");
    send_event(&mut csv, egui::Event::Text("Local".to_owned()));
    send_event(&mut csv, key_event(Key::Enter, egui::Modifiers::NONE));
    assert!(csv.state().workspace.documents[&csv_id].is_dirty());

    fs::remove_file(&csv_path).expect("remove the edited CSV source");
    let save_modifiers = command_modifiers(&csv.ctx);
    send_event(&mut csv, key_event(Key::S, save_modifiers));
    wait_until(&mut csv, |app| {
        !app.pending_saves.contains_key(&csv_id)
            && csv_path.exists()
            && !app.workspace.documents[&csv_id].is_dirty()
    });
    assert!(
        String::from_utf8_lossy(&fs::read(&csv_path).expect("read recreated CSV"))
            .contains("Local")
    );
    assert!(csv.state().save_conflict_dialog.is_none());

    fs::remove_file(&csv_path).expect("remove the recreated CSV");
    fs::remove_dir(&csv_directory).expect("remove the original CSV folder");
    click_cell(&mut csv, "A2");
    send_event(&mut csv, egui::Event::Text("Later".to_owned()));
    send_event(&mut csv, key_event(Key::Enter, egui::Modifiers::NONE));
    let save_modifiers = command_modifiers(&csv.ctx);
    send_event(&mut csv, key_event(Key::S, save_modifiers));
    wait_until(&mut csv, |app| {
        !app.pending_saves.contains_key(&csv_id)
            && app
                .status
                .as_ref()
                .is_some_and(|status| status.key() == "status-save-parent-folder-missing")
    });
    assert!(csv.state().workspace.documents[&csv_id].is_dirty());
    assert!(!csv_path.exists());
}

/// Closes through the platform menu action used by the app's File menu.
fn quit_app_from_menu(harness: &mut Harness<'_, TexcelApp>) {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let target = harness.state().native_action_target(&harness.ctx, false);
        harness.state_mut().native_menu_events_for_test =
            vec![crate::native_menu::NativeMenuEvent {
                id: "app.quit".to_owned(),
                recent_path: None,
                target,
            }];
        harness.step();
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        click_button(harness, "File");
        click_button(harness, "Quit Texcel");
    }
}

#[cfg(target_os = "macos")]
/// Sends a native quit request into the app's macOS close handler.
fn request_native_termination(harness: &mut Harness<'_, TexcelApp>) -> egui::FullOutput {
    crate::macos_app::request_terminate_for_test();
    assert!(crate::macos_app::terminate_request_pending_for_test());
    let context = harness.ctx.clone();
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 620.0))),
        ..egui::RawInput::default()
    };
    let output = context.run_ui(raw, |_| {
        harness.state_mut().handle_app_close_request(&context);
    });
    assert!(!crate::macos_app::terminate_request_pending_for_test());
    output
}

#[cfg(target_os = "macos")]
#[test]
/// Routes native termination through close choices and removes recovery on discard.
fn macos_native_termination_uses_the_app_close_flow() {
    let directory = ScenarioDirectory::new();
    let source_path = directory.0.join("native-quit.md");
    fs::write(&source_path, "| A | B |\n| --- | --- |\n| base | keep |\n")
        .expect("write a Markdown source for close scenarios");

    let clean_dir = directory.0.join("clean");
    let dirty_dir = directory.0.join("dirty");
    fs::create_dir_all(&clean_dir).expect("create clean case directory");
    fs::create_dir_all(&dirty_dir).expect("create dirty case directory");
    let mut clean = scenario_harness(clean_dir.join("settings.json"));
    let output = request_native_termination(&mut clean);
    assert!(!clean.state().app_close_dialog_open);
    assert!(
        output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::Close)
    );
    drop(clean);

    let mut dirty = scenario_harness(dirty_dir.join("settings.json"));
    queue_open_and_wait(&mut dirty, source_path.clone());
    let document_id = dirty.state().workspace.active_document.unwrap();
    click_cell(&mut dirty, "A2");
    send_event(&mut dirty, egui::Event::Text("changed".to_owned()));
    send_event(&mut dirty, key_event(Key::Enter, egui::Modifiers::NONE));
    wait_until(&mut dirty, |app| {
        app.recovery_schedules
            .get(&document_id)
            .is_some_and(|schedule| schedule.last_success.is_some())
    });

    let output = request_native_termination(&mut dirty);
    assert!(dirty.state().app_close_dialog_open);
    assert!(
        !output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::Close)
    );
    click_app_close_choice(&mut dirty, "discard");
    wait_until(&mut dirty, |app| {
        app.app_close_discard_exit_ready
            && app.pending_recovery_removals.is_empty()
            && app.in_flight_recovery_removals.is_empty()
    });
    let recovery_key = dirty.state().recovery_keys.get(&document_id).cloned();
    let recovery_root = dirty_dir.join("recovery");
    if let Some(key) = recovery_key {
        assert!(!recovery_root.join(format!("{key}.json")).exists());
    }
    assert!(viewport_command_sent(&dirty, egui::ViewportCommand::Close));
    drop(dirty);

    let mut restarted = scenario_harness(dirty_dir.join("settings.json"));
    for _ in 0..30 {
        send_batch(&mut restarted, []);
        std::thread::sleep(Duration::from_millis(4));
    }
    assert!(restarted.state().recovery_items.is_empty());
    assert!(restarted.state().recovery_restore_pending.is_empty());
}

#[cfg(target_os = "macos")]
#[test]
/// Accepts native quit after Save or Undo returns a file to its saved version.
fn macos_native_termination_accepts_saved_file_states() {
    let directory = ScenarioDirectory::new();
    let source_path = directory.0.join("saved-source.md");
    fs::write(&source_path, "| A | B |\n| --- | --- |\n| base | keep |\n")
        .expect("write a saved Markdown source");

    for ending in ["saved", "undoed"] {
        let case_dir = directory.0.join(ending);
        fs::create_dir_all(&case_dir).expect("create an isolated settings directory");
        let mut harness = scenario_harness(case_dir.join("settings.json"));
        queue_open_and_wait(&mut harness, source_path.clone());
        let document_id = harness.state().workspace.active_document.unwrap();
        click_cell(&mut harness, "A2");
        send_event(&mut harness, egui::Event::Text("changed".to_owned()));
        send_event(&mut harness, key_event(Key::Enter, egui::Modifiers::NONE));

        if ending == "saved" {
            let command = command_modifiers(&harness.ctx);
            send_event(&mut harness, key_event(Key::S, command));
            wait_until(&mut harness, |app| {
                app.pending_saves.is_empty() && !app.document_has_unsaved_changes(document_id)
            });
        } else {
            let command = command_modifiers(&harness.ctx);
            send_event(&mut harness, key_event(Key::Z, command));
            assert!(!harness.state().document_has_unsaved_changes(document_id));
        }

        harness.state_mut().multi_window_mode = true;
        let output = request_native_termination(&mut harness);
        assert!(!harness.state().app_close_dialog_open);
        let closed_immediately = output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::Close);
        if !closed_immediately {
            wait_for_exit_transaction(&mut harness);
        }
    }
}
