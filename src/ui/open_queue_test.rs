use super::*;
use std::fs;
use std::time::{Duration, Instant};

struct OpenTestDirectory(PathBuf);

impl OpenTestDirectory {
    // Creates a private directory for file-entry tests.
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time is available")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("texcel-open-queue-{unique}"));
        fs::create_dir_all(&path).expect("test directory is created");
        Self(path)
    }
}

impl Drop for OpenTestDirectory {
    // Removes all input files after a file-entry test.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn new_open_test_app() -> TexcelApp {
    let ctx = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(ctx);
    TexcelApp::new_with_settings_path(&creation, None)
}

fn poll_until(
    app: &mut TexcelApp,
    ctx: &egui::Context,
    mut ready: impl FnMut(&mut TexcelApp) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        // Give each worker poll a fresh test frame for the bounded preview queue.
        app.last_load_progress_frame = None;
        app.process_load_progress(ctx);
        app.process_file_results();
        if ready(app) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!(
        "file worker did not reach the expected open state: stopped={}, status={:?}, csv={:?}, markdown={}, backing={}, previews={}",
        app.file_worker.is_stopped(),
        app.status.as_ref().map(Message::key),
        app.pending_csv_opens
            .iter()
            .map(|(id, pending)| (*id, pending.job_id, pending.prompt.is_some()))
            .collect::<Vec<_>>(),
        app.pending_markdown_loads.len(),
        app.file_backings.len(),
        app.pending_load_previews.len()
    );
}

#[test]
// Opens a clear CSV through the normal load worker without asking for options.
fn queued_csv_with_a_clear_delimiter_opens_without_a_settings_prompt() {
    let directory = OpenTestDirectory::new();
    let path = directory.0.join("clear.csv");
    fs::write(&path, b"name,score\nAki,12\n").expect("CSV input is written");
    let ctx = egui::Context::default();
    let mut app = new_open_test_app();
    app.queue_open_paths([path.clone()]);
    app.advance_queued_open(&ctx);
    let document_id = app.active_queued_open.expect("queued CSV starts");

    poll_until(&mut app, &ctx, |app| {
        !app.pending_csv_opens.contains_key(&document_id)
            && app.file_backings.contains_key(&document_id)
    });
    assert!(!app.pending_csv_opens.contains_key(&document_id));
    assert_eq!(app.recent_files.front(), Some(&path));
    assert_eq!(app.queued_open_paths.len(), 0);
}

#[test]
// Keeps repeated startup deliveries from opening the same lexical file path twice.
fn repeated_startup_path_is_loaded_into_one_document() {
    let directory = OpenTestDirectory::new();
    let path = directory.0.join("same.md");
    fs::write(&path, b"| value |\n| --- |\n| one |\n").expect("Markdown input is written");
    let ctx = egui::Context::default();
    let mut app = new_open_test_app();
    app.queue_open_paths([path.clone()]);
    app.queue_open_paths([directory.0.join(".").join("same.md")]);
    assert_eq!(app.queued_open_paths.len(), 1);

    app.advance_queued_open(&ctx);
    let loaded_id = app
        .active_queued_open
        .expect("the first path starts loading");
    poll_until(&mut app, &ctx, |app| {
        !app.pending_markdown_loads.contains_key(&loaded_id)
            && app.file_backings.contains_key(&loaded_id)
    });
    app.advance_queued_open(&ctx);
    app.queue_open_paths([path]);
    app.advance_queued_open(&ctx);

    assert_eq!(app.workspace.order, [loaded_id]);
    assert!(app.queued_open_paths.is_empty());
}

#[cfg(unix)]
#[test]
// Keeps the user's alias while deduplicating by the worker-resolved file path.
fn symlink_open_path_reuses_the_existing_document() {
    use std::os::unix::fs::symlink;

    let directory = OpenTestDirectory::new();
    let path = directory.0.join("source.md");
    let alias = directory.0.join("alias.md");
    let second_alias = directory.0.join("second-alias.md");
    fs::write(&path, b"| value |\n| --- |\n| one |\n").expect("Markdown input is written");
    symlink(&path, &alias).expect("a symlink alias is created");
    symlink(&path, &second_alias).expect("a second symlink alias is created");
    let mut app = new_open_test_app();
    app.settings_loaded = true;
    app.queue_open_paths([alias.clone(), second_alias]);
    assert_eq!(app.queued_open_paths.len(), 2);

    let ctx = egui::Context::default();
    app.advance_queued_open(&ctx);
    let first_id = app
        .active_queued_open
        .expect("the original file starts loading");
    poll_until(&mut app, &ctx, |app| {
        !app.pending_markdown_loads.contains_key(&first_id)
            && app.file_backings.contains_key(&first_id)
    });
    app.advance_queued_open(&ctx);
    let alias_id = app
        .active_queued_open
        .expect("the second alias is checked by the file worker");
    poll_until(&mut app, &ctx, |app| {
        !app.pending_markdown_loads.contains_key(&alias_id)
    });
    app.advance_queued_open(&ctx);

    assert_eq!(app.workspace.order, [first_id]);
    assert_eq!(
        app.file_backings[&first_id].path,
        fs::canonicalize(&path).unwrap()
    );
    assert_eq!(app.user_file_paths[&first_id], alias);
    assert_eq!(app.recent_files.front(), app.user_file_paths.get(&first_id));
    assert_eq!(app.workspace.documents[&first_id].name, "alias.md");
    assert_eq!(app.workspace.active_document, Some(first_id));
    assert!(
        fs::symlink_metadata(&alias)
            .expect("the chosen alias remains present")
            .file_type()
            .is_symlink()
    );

    let table_id = app.workspace.documents[&first_id]
        .active_table()
        .expect("the source table remains loaded")
        .id;
    app.workspace
        .documents
        .get_mut(&first_id)
        .unwrap()
        .set_cell(
            table_id,
            CellAddress { row: 1, column: 0 },
            Some(Cell::text("saved through target")),
        )
        .expect("the source cell changes through document history");
    let target = app.file_backings[&first_id].path.clone();
    app.save_document_to_path(first_id, target.clone());
    assert!(app.pending_saves.contains_key(&first_id));
    poll_until(&mut app, &ctx, |app| {
        !app.pending_saves.contains_key(&first_id)
    });
    assert!(
        fs::read_to_string(&target)
            .expect("the resolved source target remains readable")
            .contains("saved through target")
    );
    assert!(
        fs::symlink_metadata(&alias)
            .expect("saving does not replace the chosen alias")
            .file_type()
            .is_symlink()
    );
}

#[cfg(unix)]
#[test]
// Lets the loader resolve parent components after a symlink without changing their meaning.
fn symlink_parent_component_is_preserved_until_worker_loads_the_file() {
    use std::os::unix::fs::symlink;

    let directory = OpenTestDirectory::new();
    let target_directory = directory.0.join("target");
    fs::create_dir_all(target_directory.join("inner")).expect("target directory is created");
    symlink(target_directory.join("inner"), directory.0.join("link"))
        .expect("the intermediate directory is linked");
    fs::write(
        directory.0.join("file.md"),
        b"| value |\n| --- |\n| lexical |\n",
    )
    .expect("lexical target is written");
    fs::write(
        target_directory.join("file.md"),
        b"| value |\n| --- |\n| resolved |\n",
    )
    .expect("symlink-resolved target is written");
    let requested = directory.0.join("link/../file.md");
    let mut app = new_open_test_app();
    app.queue_open_paths([requested.clone()]);
    assert_eq!(app.queued_open_paths.front(), Some(&requested));

    let ctx = egui::Context::default();
    app.advance_queued_open(&ctx);
    let document_id = app
        .active_queued_open
        .expect("the requested Markdown file starts loading");
    poll_until(&mut app, &ctx, |app| {
        !app.pending_markdown_loads.contains_key(&document_id)
            && app.file_backings.contains_key(&document_id)
    });

    assert_eq!(
        app.file_backings[&document_id].path,
        fs::canonicalize(target_directory.join("file.md")).unwrap()
    );
    assert_eq!(
        app.workspace.documents[&document_id].tables[0]
            .cell(crate::document::CellAddress { row: 1, column: 0 })
            .unwrap()
            .input,
        crate::document::CellInput::Text("resolved".to_owned())
    );
}

#[test]
// Holds later files behind an ambiguous CSV and exposes samples for its choices.
fn queued_paths_pause_for_ambiguous_csv_preview() {
    let directory = OpenTestDirectory::new();
    let csv_path = directory.0.join("ambiguous.csv");
    let next_path = directory.0.join("next.md");
    fs::write(&csv_path, b"a,b;c\nd,e;f\n").expect("ambiguous CSV is written");
    fs::write(&next_path, b"| next |\n| --- |\n| value |\n").expect("Markdown is written");
    let ctx = egui::Context::default();
    let mut app = new_open_test_app();
    app.queue_open_paths([csv_path, next_path]);
    app.advance_queued_open(&ctx);
    let csv_id = app.active_queued_open.expect("first file starts");

    poll_until(&mut app, &ctx, |app| {
        app.pending_csv_opens
            .get(&csv_id)
            .is_some_and(|pending| pending.prompt.is_some())
    });
    app.advance_queued_open(&ctx);
    assert_eq!(app.active_queued_open, Some(csv_id));
    assert_eq!(app.queued_open_paths.len(), 1);
    assert!(app.pending_markdown_loads.is_empty());
    let CsvLoadPrompt::Settings(settings) = app.pending_csv_opens[&csv_id]
        .prompt
        .as_ref()
        .expect("CSV choice stays open");
    assert_eq!(settings.previews[&','][0], ["a", "b;c"]);
    assert_eq!(settings.previews[&';'][0], ["a,b", "c"]);
}

#[test]
// Drops worker progress that arrives after the CSV load has asked for choices.
fn csv_choice_rejects_late_progress_for_its_finished_load_token() {
    let directory = OpenTestDirectory::new();
    let csv_path = directory.0.join("ambiguous.csv");
    let next_path = directory.0.join("next.md");
    fs::write(&csv_path, b"a,b;c\nd,e;f\n").expect("ambiguous CSV is written");
    fs::write(&next_path, b"| next |\n| --- |\n| value |\n").expect("Markdown is written");
    let ctx = egui::Context::default();
    let mut app = new_open_test_app();
    app.queue_open_paths([csv_path, next_path]);
    app.advance_queued_open(&ctx);
    let csv_id = app.active_queued_open.expect("CSV starts first");

    let deadline = Instant::now() + Duration::from_secs(5);
    while !app.pending_csv_opens[&csv_id].prompt.is_some() {
        app.process_file_results();
        assert!(Instant::now() < deadline, "CSV analysis returns its choice");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(app.pending_load_previews.is_empty());
    app.process_load_progress(&ctx);

    assert!(
        app.pending_load_previews.is_empty(),
        "the late ReadingSource event cannot recreate an old preview"
    );
    assert_eq!(app.active_queued_open, Some(csv_id));
    assert_eq!(app.queued_open_paths.len(), 1);
}

#[test]
// Continues after one missing path and keeps its error visible after a later load.
fn queued_missing_file_does_not_hide_its_error_or_block_the_next_file() {
    let directory = OpenTestDirectory::new();
    let missing_path = directory.0.join("missing.md");
    let next_path = directory.0.join("next.md");
    fs::write(&next_path, b"| next |\n| --- |\n| value |\n").expect("Markdown is written");
    let ctx = egui::Context::default();
    let mut app = new_open_test_app();
    app.queue_open_paths([missing_path, next_path.clone()]);
    app.advance_queued_open(&ctx);

    poll_until(&mut app, &ctx, |app| {
        app.advance_queued_open(&ctx);
        app.active_queued_open.is_none()
            && app.queued_open_paths.is_empty()
            && app
                .user_file_paths
                .values()
                .any(|user_path| user_path == &next_path)
    });

    let failure = app
        .queued_open_failure
        .as_ref()
        .expect("the first queued error stays available");
    assert_eq!(app.status.as_ref().map(Message::key), Some(failure.key()));
    assert_eq!(failure.key(), "status-file-open-error");
    let resolved = failure.resolve(crate::i18n::Language::English);
    assert!(
        resolved.contains("missing.md"),
        "the missing file name is shown"
    );
    assert!(
        !resolved.contains(&directory.0.display().to_string()),
        "the absolute temporary path is not shown"
    );
    let japanese = failure.resolve(crate::i18n::Language::Japanese);
    assert!(japanese.contains("missing.md"));
    assert!(!japanese.contains(&directory.0.display().to_string()));
    // Show the missing-file reason once in the user's language.
    assert!(resolved.contains("not found"), "{resolved}");
    assert!(japanese.contains("見つかりません"), "{japanese}");
    for message in [resolved, japanese] {
        assert_eq!(message.matches("missing.md").count(), 1);
        assert!(!message.contains("os error"));
    }
}

#[test]
// Clears a stopped request's preview before opening the next queued file.
fn stopped_open_clears_preview_so_next_queued_file_starts() {
    let directory = OpenTestDirectory::new();
    let next_path = directory.0.join("next.md");
    fs::write(&next_path, b"| next |\n| --- |\n| value |\n").expect("Markdown is written");
    let ctx = egui::Context::default();
    let mut app = new_open_test_app();
    let failed_id = app
        .workspace
        .active_document
        .expect("blank document exists");
    let token = JobToken {
        document_id: failed_id,
        revision: 0,
        job_id: 40,
    };
    app.pending_markdown_loads.insert(failed_id, token);
    app.pending_file_load_paths
        .insert(failed_id, directory.0.join("stopped.md"));
    app.pending_load_previews.insert(
        failed_id,
        LoadPreviewState {
            token,
            path: directory.0.join("stopped.md"),
            stage: LoadStage::ReadingSource,
            selected_table: 0,
            tables: BTreeMap::new(),
        },
    );
    app.active_queued_open = Some(failed_id);
    app.queue_open_paths([next_path.clone()]);

    app.fail_stopped_open(failed_id, token, FileOperation::LoadMarkdown);
    assert!(app.pending_load_previews.is_empty());
    app.advance_queued_open(&ctx);
    let next_id = app
        .active_queued_open
        .expect("the next path starts immediately");
    assert!(app.pending_markdown_loads.contains_key(&next_id));
    assert_eq!(app.queued_open_paths.len(), 0);

    poll_until(&mut app, &ctx, |app| {
        !app.pending_markdown_loads.contains_key(&next_id)
            && app.file_backings.contains_key(&next_id)
    });
    assert_eq!(app.user_file_paths[&next_id], next_path);
    assert!(app.queued_open_failure.is_some());
}

#[test]
// Keeps current-session paths ahead of loaded settings and removes duplicate paths.
fn recent_paths_opened_before_settings_load_are_kept_first() {
    let mut app = new_open_test_app();
    let recent = PathBuf::from("/tmp/session.csv");
    let saved = PathBuf::from("/tmp/saved.md");
    app.remember_recent_file(recent.clone());
    app.restore_recent_files(vec![saved.clone(), recent.clone()]);

    assert_eq!(
        app.recent_files.iter().cloned().collect::<Vec<_>>(),
        [recent, saved]
    );
    assert!(app.recent_files_changed_before_load);
}

#[test]
// Resolves relative incoming paths from a captured working directory without disk access.
fn relative_open_path_becomes_a_stable_absolute_path() {
    let current_dir = PathBuf::from("/workspace/project");

    assert_eq!(
        absolute_open_path(std::path::Path::new("../data/table.csv"), &current_dir),
        PathBuf::from("/workspace/project/../data/table.csv")
    );
}
