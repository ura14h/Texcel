use super::*;
use std::path::Path;
use std::time::Instant;

struct InputFixture {
    extension: &'static str,
    bytes: &'static [u8],
}

const INPUTS: [InputFixture; 4] = [
    InputFixture {
        extension: "md",
        bytes: include_bytes!("../../../samples/io-roundtrip.md"),
    },
    InputFixture {
        extension: "csv",
        bytes: include_bytes!("../../../samples/io-roundtrip.csv"),
    },
    InputFixture {
        extension: "html",
        bytes: include_bytes!("../../../samples/io-roundtrip.html"),
    },
    InputFixture {
        extension: "xlsx",
        bytes: include_bytes!("../../../samples/io-roundtrip.xlsx"),
    },
];

#[test]
// Loads, edits, saves, and reloads every supported input and output route.
fn production_io_routes_edit_b2_and_keep_import_sources_untouched() {
    for input in &INPUTS {
        for output_extension in ["md", "csv"] {
            run_route(input, output_extension);
        }
    }
}

#[test]
// Keeps numeric-looking text intact through production load, edit, save, and reload.
fn long_text_values_survive_load_edit_save_and_reload() {
    let directory = TestDirectory::new();
    let input_path = directory.0.join("cell-display.md");
    let output_path = directory.0.join("cell-display-edited.md");
    let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/cell-display.md");
    std::fs::copy(&sample, &input_path).expect("copy the display fixture to a temporary file");

    let context = egui::Context::default();
    let mut app = new_app(&context);
    app.settings_loaded = true;
    let document_id = app
        .start_path_load(input_path)
        .expect("the production Markdown loader accepts the fixture");
    wait_for_route_load(&mut app, document_id);
    let table_id = app.workspace.documents[&document_id]
        .active_table()
        .expect("the loaded display table is active")
        .id;
    assert_eq!(
        app.workspace.documents[&document_id].tables[0]
            .cell(CellAddress { row: 1, column: 0 })
            .unwrap()
            .input,
        CellInput::Text("00123".to_owned())
    );
    assert_eq!(
        app.workspace.documents[&document_id].tables[0]
            .cell(CellAddress { row: 2, column: 0 })
            .unwrap()
            .input,
        CellInput::Text("12345678901234567890".to_owned())
    );

    app.begin_cell_edit(table_id, CellAddress { row: 4, column: 0 });
    app.edit.buffer = "saved".to_owned();
    app.commit_edit(None);
    app.save_as_to_path(document_id, output_path.clone());
    finish_product_save(&mut app, document_id, &output_path, true);

    let reopened_id = app
        .start_path_load(output_path)
        .expect("the saved Markdown file reopens through the product loader");
    wait_for_route_load(&mut app, reopened_id);
    let table = app.workspace.documents[&reopened_id]
        .active_table()
        .expect("the reloaded table is active");
    assert_eq!(
        table.cell(CellAddress { row: 1, column: 0 }).unwrap().input,
        CellInput::Text("00123".to_owned())
    );
    assert_eq!(
        table.cell(CellAddress { row: 2, column: 0 }).unwrap().input,
        CellInput::Text("12345678901234567890".to_owned())
    );
    assert_eq!(
        table.cell(CellAddress { row: 4, column: 0 }).unwrap().input,
        CellInput::Text("saved".to_owned())
    );
}

#[test]
// Opens an ODS file through the shared path loader as a Markdown import with its own report.
fn ods_path_load_imports_markdown_and_opens_report() {
    let directory = TestDirectory::new();
    let path = directory.0.join("workbook.ods");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/workbook.ods"),
        &path,
    )
    .expect("copy the ODS fixture");
    let context = egui::Context::default();
    let mut app = new_app(&context);
    app.settings_loaded = true;
    let id = app
        .start_path_load(path.clone())
        .expect("the production loader accepts ODS");
    wait_for_route_load(&mut app, id);
    let (owner, report) = app
        .active_import_report
        .as_ref()
        .expect("the merged cell's content loss opens the report");
    assert_eq!(*owner, id);
    assert_eq!(report.file_name, "workbook.ods");
    assert_eq!(report.input_format, "ODS");
    let document = &app.workspace.documents[&id];
    let names = document
        .tables
        .iter()
        .map(|table| table.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, ["売上", "型と書式"]);
    assert!(
        app.document_display_name(&context, id)
            .contains("workbook.ods"),
        "the window title keeps the imported extension"
    );
}

#[test]
// Separates a formatting-only XLSX report from an import that loses table content.
fn import_reports_auto_open_only_for_content_impact() {
    let directory = TestDirectory::new();
    let xlsx_path = directory.0.join("formatting-only.xlsx");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/formatting-only.xlsx"),
        &xlsx_path,
    )
    .expect("copy the XLSX formatting fixture");

    let context = egui::Context::default();
    let mut app = new_app(&context);
    app.settings_loaded = true;
    let xlsx_id = app
        .start_path_load(xlsx_path)
        .expect("the production XLSX loader accepts the fixture");
    wait_for_route_load(&mut app, xlsx_id);
    assert!(
        app.active_import_report.is_none(),
        "formatting-only fixture opened: {:?}",
        app.active_import_report
            .as_ref()
            .map(|(_, report)| &report.entries)
    );
    let report = app.import_reports[&xlsx_id].reopen();
    assert_eq!(report.file_name, "formatting-only.xlsx");
    assert!(!report.requires_dialog());
    assert!(report.has_formatting_notes());
    assert!(report.summary().omitted > 0);
    assert!(report.entries.iter().all(|entry| {
        entry.class == crate::conversion_report::ConversionClass::Retained
            || entry.impact == crate::conversion_report::ConversionImpact::Formatting
    }));
    assert_eq!(app.status, None);
    assert_eq!(
        app.import_notice.as_ref().map(|(owner, _)| *owner),
        Some(xlsx_id)
    );

    app.reopen_import_report(xlsx_id);
    let (owner, reopened) = app
        .active_import_report
        .as_ref()
        .expect("the formatting report remains available from the File menu");
    assert_eq!(*owner, xlsx_id);
    assert_eq!(reopened.file_name, "formatting-only.xlsx");

    let merged_path = directory.0.join("workbook.xlsx");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/workbook.xlsx"),
        &merged_path,
    )
    .expect("copy the merged-table content-loss fixture");
    let merged_context = egui::Context::default();
    let mut merged_app = new_app(&merged_context);
    merged_app.settings_loaded = true;
    let merged_id = merged_app
        .start_path_load(merged_path)
        .expect("the production XLSX loader accepts the merged fixture");
    wait_for_route_load(&mut merged_app, merged_id);
    let (_, merged_report) = merged_app
        .active_import_report
        .as_ref()
        .expect("the merged table's structure loss opens a report");
    assert!(merged_report.requires_dialog());
    assert!(merged_report.entries.iter().any(|entry| {
        entry.message.as_ref() == Some(&crate::i18n::Message::new("report-reason-merge-collapsed"))
            && entry.impact == crate::conversion_report::ConversionImpact::Content
    }));
    assert!(merged_report.entries.iter().any(|entry| {
        entry.message.as_ref()
            == Some(&crate::i18n::Message::new(
                "report-reason-markdown-header-first",
            ))
            && entry.impact == crate::conversion_report::ConversionImpact::Formatting
    }));

    let html_path = directory.0.join("html-nested-readonly.html");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/html-nested-readonly.html"),
        &html_path,
    )
    .expect("copy the HTML content-loss fixture");
    let second_context = egui::Context::default();
    let mut content_app = new_app(&second_context);
    content_app.settings_loaded = true;
    let html_id = content_app
        .start_path_load(html_path)
        .expect("the production HTML importer accepts the fixture");
    wait_for_route_load(&mut content_app, html_id);
    let (owner, report) = content_app
        .active_import_report
        .as_ref()
        .expect("omitted nested table content opens one report");
    assert_eq!(*owner, html_id);
    assert!(report.requires_dialog());
    assert!(report.entries.iter().any(|entry| {
        entry.impact == crate::conversion_report::ConversionImpact::Content
            && entry.class != crate::conversion_report::ConversionClass::Retained
    }));
}

// Exercise one real product route without constructing documents in memory.
fn run_route(input: &InputFixture, output_extension: &str) {
    let directory = TestDirectory::new();
    let input_path = directory.0.join(format!("input.{}", input.extension));
    let output_path = directory.0.join(format!("output.{output_extension}"));
    std::fs::write(&input_path, input.bytes).expect("write input fixture");
    let original_bytes = std::fs::read(&input_path).expect("read input fixture");

    let context = egui::Context::default();
    let mut app = new_app(&context);
    app.settings_loaded = true;
    let document_id = app
        .start_path_load(input_path.clone())
        .expect("supported input starts through the product loader");
    wait_for_route_load(&mut app, document_id);

    let (table_id, source_kind) = {
        let document = &app.workspace.documents[&document_id];
        let table = document.active_table().expect("loaded table is active");
        assert_eq!(
            table.cell(CellAddress { row: 1, column: 1 }).unwrap().input,
            CellInput::Text("古い".to_owned()),
            "{} input exposes the shared B2 fixture value",
            input.extension
        );
        let kind = match document.source.as_deref() {
            Some(SourceDocument::Csv(_)) => "csv",
            Some(SourceDocument::Markdown(_)) => "markdown",
            other => panic!(
                "{} input adopted unexpected source: {other:?}",
                input.extension
            ),
        };
        (table.id, kind)
    };
    if input.extension == "csv" {
        assert_eq!(source_kind, "csv");
        assert!(
            !app.pending_csv_opens.contains_key(&document_id),
            "the clear CSV fixture loads without an options prompt"
        );
    } else {
        assert_eq!(source_kind, "markdown");
    }
    close_automatic_import_report(&mut app);

    let address = CellAddress { row: 1, column: 1 };
    app.begin_cell_edit(table_id, address);
    assert!(
        app.edit.is_editing(),
        "the loaded B2 cell enters normal editing"
    );
    app.edit.buffer = "新しい値".to_owned();
    app.commit_edit(None);
    assert!(!app.edit.is_editing(), "the common edit command commits B2");
    assert_eq!(
        app.workspace.documents[&document_id].tables[0]
            .cell(address)
            .unwrap()
            .input,
        CellInput::Text("新しい値".to_owned())
    );

    let complete_save = !(matches!(input.extension, "html" | "xlsx") && output_extension == "csv");
    app.save_as_to_path(document_id, output_path.clone());
    finish_product_save(&mut app, document_id, &output_path, complete_save);
    assert_eq!(
        std::fs::read(&input_path).expect("read original after Save As"),
        original_bytes,
        "{} import source bytes stay unchanged when saving as .{}",
        input.extension,
        output_extension
    );
    assert!(
        output_path.is_file(),
        "the .{output_extension} output exists"
    );
    if complete_save {
        assert_eq!(
            app.user_file_paths[&document_id], output_path,
            "complete Save As adopts the selected output path"
        );
        assert!(!app.document_requires_save(document_id));
    } else {
        assert_eq!(
            app.user_file_paths[&document_id], input_path,
            "a partial CSV save keeps the chosen input path"
        );
        assert!(app.workspace.documents[&document_id].is_dirty());
        assert!(app.document_requires_save(document_id));
    }

    // A fresh session must read the saved bytes to verify the output.
    drop(app);
    let mut app = new_app(&context);
    app.settings_loaded = true;
    let reloaded_id = app
        .start_path_load(output_path.clone())
        .expect("saved output reopens through the product loader");
    wait_for_route_load(&mut app, reloaded_id);
    let reloaded = &app.workspace.documents[&reloaded_id];
    assert_eq!(
        reloaded
            .active_table()
            .unwrap()
            .cell(address)
            .unwrap()
            .input,
        CellInput::Text("新しい値".to_owned()),
        "{}→{} route keeps the edited B2 after product reload",
        input.extension,
        output_extension
    );
    if output_extension == "csv" {
        assert!(
            !app.pending_csv_opens.contains_key(&reloaded_id),
            "the saved clear CSV reopens without an options prompt"
        );
        assert!(matches!(
            reloaded.source.as_deref(),
            Some(SourceDocument::Csv(_))
        ));
    } else {
        assert!(matches!(
            reloaded.source.as_deref(),
            Some(SourceDocument::Markdown(_))
        ));
    }
    assert_eq!(
        std::fs::read(&input_path).expect("read original after reload"),
        original_bytes,
        "reopening output still leaves the {} source untouched",
        input.extension
    );
}

// Wait for a real input to be adopted or for a CSV choice to become visible.
fn wait_for_route_load(app: &mut TexcelApp, document_id: DocumentId) {
    wait_for_condition(app, |app| {
        app.workspace.documents.contains_key(&document_id)
            || app
                .pending_csv_opens
                .get(&document_id)
                .is_some_and(|pending| pending.prompt.is_some())
    });
    assert!(
        app.workspace.documents.contains_key(&document_id),
        "load did not adopt {document_id}; status={:?}, csv={:?}",
        app.status,
        app.pending_csv_opens.get(&document_id)
    );
}

// Closes an automatic import report through the same state change as its Close action.
fn close_automatic_import_report(app: &mut TexcelApp) {
    if app.active_import_report.take().is_some() {
        app.activate_next_import_report();
    }
}

// Accepts CSV conversion details and waits for a save or export acknowledgement.
fn finish_product_save(
    app: &mut TexcelApp,
    document_id: DocumentId,
    path: &Path,
    complete_save: bool,
) {
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    loop {
        app.process_file_results();
        if let Some(export) = app
            .active_export_confirmation
            .as_ref()
            .is_some_and(|export| export.document_id == document_id)
            .then(|| app.active_export_confirmation.take())
            .flatten()
        {
            app.submit_prepared_export(export);
        }

        let no_pending_save_work = |app: &TexcelApp| {
            !app.pending_saves.contains_key(&document_id)
                && !app.pending_save_intents.contains_key(&document_id)
                && !app.pending_structures.contains_key(&document_id)
                && !app.pending_table_management.contains(&document_id)
                && !app.pending_exports.contains_key(&document_id)
                && !app.pending_export_writes.contains_key(&document_id)
                && !app
                    .active_export_confirmation
                    .as_ref()
                    .is_some_and(|export| export.document_id == document_id)
        };
        let settled = |app: &TexcelApp| {
            no_pending_save_work(app)
                && path.is_file()
                && if complete_save {
                    app.user_file_paths
                        .get(&document_id)
                        .is_some_and(|user_path| user_path == path)
                        && !app.document_requires_save(document_id)
                } else {
                    app.document_requires_save(document_id)
                }
        };
        if settled(app) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Save As did not settle for {}; status={:?}, backing={:?}, dirty={:?}, saves={:?}, intents={:?}, structures={:?}, exports={:?}, writes={:?}, stopped={}",
            path.display(),
            app.status,
            app.file_backings
                .get(&document_id)
                .map(|backing| &backing.path),
            app.workspace.documents.get(&document_id).map(|document| (
                document.revision,
                document.saved_revision,
                document.is_dirty(),
            )),
            app.pending_saves,
            app.pending_save_intents,
            app.pending_structures,
            app.pending_exports,
            app.pending_export_writes,
            app.file_worker.is_stopped()
        );
        std::thread::sleep(std::time::Duration::from_millis(3));
    }
}
