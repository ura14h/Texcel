use super::*;
use std::time::{Duration, Instant};

#[path = "ui_io_routes_test.rs"]
mod io_routes_test;
#[path = "ui_save_as_results_test.rs"]
mod result_output_test;

/// Waits for the worker results that settle one Save As transition.
fn wait_for_save_as(app: &mut TexcelApp, settled: impl Fn(&TexcelApp) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !settled(app) {
        app.process_file_results();
        assert!(Instant::now() < deadline, "Save As worker did not settle");
        std::thread::sleep(Duration::from_millis(3));
    }
    app.process_file_results();
}

#[test]
/// Keeps the Markdown document dirty after a confirmed single-table CSV export.
fn markdown_save_as_partial_csv_keeps_source_and_history() {
    let directory = TestDirectory::new();
    let destination = directory.0.join("selected.csv");
    let context = egui::Context::default();
    let mut app = new_app(&context);
    app.language = crate::i18n::Language::English;
    let document_id = app.workspace.active_document.unwrap();
    let first_table = app.workspace.documents[&document_id].active_table;
    app.workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .set_cell(
            first_table,
            CellAddress { row: 1, column: 0 },
            Some(Cell::text("first table")),
        )
        .expect("edit the original table");
    app.request_table_management(StructureAction::AddTable, None, None);
    wait_for_save_as(&mut app, |app| {
        !app.pending_structures.contains_key(&document_id)
    });
    let active_table = app.workspace.documents[&document_id].active_table;
    assert_ne!(active_table, first_table);
    let saved_revision = app.workspace.documents[&document_id].saved_revision;
    let history_was_available = app.workspace.documents[&document_id].can_undo();
    let recent_before = app.recent_files.clone();

    app.save_as_to_path(document_id, destination.clone());
    wait_for_save_as(&mut app, |app| {
        app.active_export_confirmation
            .as_ref()
            .is_some_and(|export| export.document_id == document_id)
    });
    let export = app
        .active_export_confirmation
        .as_ref()
        .expect("partial CSV output needs a confirmation report");
    assert_eq!(export.options.scope, ExportScope::Selected(active_table));
    assert!(export.prepared.report.summary().omitted > 0);
    assert!(
        export
            .prepared
            .report
            .settings_summary
            .resolve(crate::i18n::Language::English)
            .contains(
                &app.workspace.documents[&document_id]
                    .active_table()
                    .unwrap()
                    .name
            )
    );

    let export = app.active_export_confirmation.take().unwrap();
    app.submit_prepared_export(export);
    wait_for_save_as(&mut app, |app| {
        !app.pending_export_writes.contains_key(&document_id)
    });
    let document = &app.workspace.documents[&document_id];
    assert!(matches!(
        document.source.as_deref(),
        Some(SourceDocument::Markdown(_))
    ));
    assert!(document.is_dirty());
    assert_eq!(document.saved_revision, saved_revision);
    assert_eq!(document.can_undo(), history_was_available);
    assert!(!app.file_backings.contains_key(&document_id));
    assert_eq!(app.recent_files, recent_before);
    assert!(!app.recent_files.contains(&destination));
    assert!(
        !std::fs::read_to_string(&destination)
            .expect("partial CSV was written")
            .contains("first table")
    );
}

#[test]
/// Changes the backing format only when all Markdown content fits in one CSV table.
fn markdown_save_as_complete_csv_updates_source_and_preserves_saved_identity_on_undo() {
    let directory = TestDirectory::new();
    let destination = directory.0.join("complete.csv");
    let context = egui::Context::default();
    let mut app = new_app(&context);
    let document_id = app.workspace.active_document.unwrap();
    let table_id = app.workspace.documents[&document_id].active_table;
    app.workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .set_cell(
            table_id,
            CellAddress { row: 0, column: 0 },
            Some(Cell::text("saved value")),
        )
        .expect("edit the Markdown header");
    app.workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .set_cell(
            table_id,
            CellAddress { row: 1, column: 0 },
            Some(Cell::formula("=1+1", None)),
        )
        .expect("set formula source");

    app.save_as_to_path(document_id, destination.clone());
    wait_for_save_as(&mut app, |app| {
        app.pending_saves.contains_key(&document_id)
            || app
                .active_export_confirmation
                .as_ref()
                .is_some_and(|export| export.document_id == document_id)
    });
    if let Some(export) = app.active_export_confirmation.take() {
        assert!(export.prepared.report.entries.iter().any(|entry| {
            entry
                .message
                .as_ref()
                .is_some_and(|message| message.key() == "report-reason-formula-csv-text")
        }));
        assert!(!app.pending_export_writes.contains_key(&document_id));
        app.submit_prepared_export(export);
    }
    wait_for_save_as(&mut app, |app| app.pending_saves.contains_key(&document_id));
    wait_for_save_as(&mut app, |app| {
        !app.pending_saves.contains_key(&document_id)
            && app
                .workspace
                .documents
                .get(&document_id)
                .is_some_and(|document| {
                    matches!(document.source.as_deref(), Some(SourceDocument::Csv(_)))
                })
    });
    assert!(
        std::fs::read_to_string(&destination)
            .expect("complete CSV was written")
            .contains("saved value")
    );
    assert!(
        std::fs::read_to_string(&destination)
            .expect("complete CSV remains available")
            .contains("=1+1")
    );
    assert!(!app.document_requires_save(document_id));
    let identity = app.file_backings[&document_id].identity.clone();
    let saved_path = app.file_backings[&document_id].path.clone();

    app.apply_undo(false);
    let document = &app.workspace.documents[&document_id];
    assert!(matches!(
        document.source.as_deref(),
        Some(SourceDocument::Csv(_))
    ));
    assert!(document.is_dirty());
    assert!(app.document_requires_save(document_id));
    assert_eq!(app.file_backings[&document_id].path, saved_path);
    assert_eq!(app.file_backings[&document_id].identity, identity);
}

#[test]
/// Promotes CSV and saves Markdown while preserving the first-row address and Undo source.
fn csv_save_as_markdown_promotes_first_row_and_keeps_identity_out_of_undo() {
    let directory = TestDirectory::new();
    let destination = directory.0.join("promoted.md");
    let context = egui::Context::default();
    let mut app = new_app(&context);
    let (document_id, table_id) =
        install_test_csv_document(&mut app, b",\n,\n".to_vec(), "Promotion");
    let document = app.workspace.documents.get_mut(&document_id).unwrap();
    document
        .set_cell(
            table_id,
            CellAddress { row: 0, column: 0 },
            Some(Cell::text("Header")),
        )
        .expect("set first row");
    document
        .set_cell(
            table_id,
            CellAddress { row: 1, column: 0 },
            Some(Cell::text("2")),
        )
        .expect("set body value");
    document
        .set_cell(
            table_id,
            CellAddress { row: 1, column: 1 },
            Some(Cell::formula("=A2+1", None)),
        )
        .expect("set coordinate-sensitive formula");

    app.save_as_to_path(document_id, destination.clone());
    wait_for_save_as(&mut app, |app| app.pending_saves.contains_key(&document_id));
    wait_for_save_as(&mut app, |app| {
        !app.pending_saves.contains_key(&document_id)
            && app
                .workspace
                .documents
                .get(&document_id)
                .is_some_and(|document| {
                    matches!(
                        document.source.as_deref(),
                        Some(SourceDocument::Markdown(_))
                    )
                })
    });
    let markdown_bytes = std::fs::read(&destination).expect("Markdown was saved");
    let markdown = crate::markdown_adapter::MarkdownDocument::parse(markdown_bytes)
        .expect("saved source is a valid Markdown table");
    assert_eq!(markdown.tables[0].rows[0][0].value, "Header");
    assert_eq!(markdown.tables[0].rows[1][0].value, "2");
    assert_eq!(markdown.tables[0].rows[1][1].value, "=A2+1");
    assert!(app.active_import_report.is_none());
    let saved_report = app.import_reports[&document_id].reopen();
    assert!(!saved_report.requires_dialog());
    assert!(saved_report.groups().iter().any(|group| {
        group.class == crate::conversion_report::ConversionClass::Converted
            && group.impact == crate::conversion_report::ConversionImpact::Formatting
            && group
                .message
                .as_ref()
                .is_some_and(|message| message.key() == "report-reason-markdown-header-first")
    }));
    app.reopen_import_report(document_id);
    let (report_owner, manually_opened_report) = app
        .active_import_report
        .as_ref()
        .expect("the formatting report can be reopened manually");
    assert_eq!(*report_owner, document_id);
    assert!(!manually_opened_report.requires_dialog());
    let saved_identity = app.file_backings[&document_id].identity.clone();
    app.apply_undo(false);
    wait_for_save_as(&mut app, |app| {
        !app.pending_history_restores.contains_key(&document_id)
    });
    assert!(matches!(
        app.workspace.documents[&document_id].source.as_deref(),
        Some(SourceDocument::Csv(_))
    ));
    assert!(app.document_requires_save(document_id));
    assert_eq!(app.file_backings[&document_id].path, destination);
    assert_eq!(app.file_backings[&document_id].identity, saved_identity);
}

#[test]
/// Keeps escaped Markdown text and real apostrophes when applying code formatting.
fn markdown_code_action_preserves_inline_meaning_and_literal_apostrophes() {
    let context = egui::Context::default();
    let mut app = new_app(&context);
    app.create_new_document("Markdown code".to_owned())
        .expect("create Markdown document");
    let document_id = app.workspace.active_document.unwrap();
    let source = Arc::new(
        crate::markdown_adapter::MarkdownDocument::parse(
            br"| a\*b | ''=1+2 |
| --- | --- |
"
            .to_vec(),
        )
        .expect("Markdown code fixture parses"),
    );
    let mut document = source.to_document(document_id, "Markdown code");
    document.set_source(Some(Arc::new(SourceDocument::Markdown(Arc::clone(
        &source,
    )))));
    let table_id = document.active_table;
    app.workspace.documents.insert(document_id, document);

    for (address, expected) in [
        (CellAddress { row: 0, column: 0 }, "a*b"),
        (CellAddress { row: 0, column: 1 }, "'=1+2"),
    ] {
        app.begin_cell_edit(table_id, address);
        let count = app.edit.buffer.chars().count();
        app.text_selection = Some(TextSelection {
            document_id,
            table_id,
            address,
            session_id: app.edit.session_id,
            buffer_revision: app.edit.buffer_revision,
            start: 0,
            end: count,
        });
        app.apply_markdown_modifier(CellFormatAction::Code, &context);
        if address.column == 0 {
            let selection = app.text_selection.expect("Code keeps its new selection");
            assert_eq!(selection.start, 0);
            assert_eq!(selection.end, app.edit.buffer.chars().count());
            let content_end = app.edit.buffer.chars().count() - 1;
            app.text_selection = Some(TextSelection {
                document_id,
                table_id,
                address,
                session_id: app.edit.session_id,
                buffer_revision: app.edit.buffer_revision,
                start: 1,
                end: content_end,
            });
            app.apply_markdown_modifier(CellFormatAction::Code, &context);
            assert_eq!(app.edit.buffer, "a\\*b");
            assert_eq!(
                app.text_selection.unwrap().end,
                app.edit.buffer.chars().count()
            );
            app.apply_markdown_modifier(CellFormatAction::Code, &context);
            assert!(app.edit.buffer.starts_with('`') && app.edit.buffer.ends_with('`'));
        }
        app.commit_edit(None);

        let cell = app.workspace.documents[&document_id].tables[0]
            .cell(address)
            .unwrap();
        let CellInput::Text(markup) = &cell.input else {
            panic!("formatted Markdown cell stays text");
        };
        let display = crate::markdown_adapter::MarkdownCellDisplay::parse_fragment(markup);
        assert_eq!(display.plain_text, expected);
        assert!(display.runs.iter().any(|run| run.code));
    }
}

#[test]
/// Preserves formula-like text when Markdown formatting is added or removed.
fn markdown_modifiers_keep_literal_apostrophes() {
    let context = egui::Context::default();
    let mut app = new_app(&context);
    app.language = crate::i18n::Language::English;
    let document_id = app.workspace.active_document.unwrap();
    let source = Arc::new(
        crate::markdown_adapter::MarkdownDocument::parse(
            b"| Add | Remove | Escape |\n| --- | --- | --- |\n| ''=A1 | **''=A1** | '=A1 |\n"
                .to_vec(),
        )
        .expect("literal modifier fixture parses"),
    );
    let mut document = source.to_document(document_id, "Literal modifiers");
    document.set_source(Some(Arc::new(SourceDocument::Markdown(Arc::clone(
        &source,
    )))));
    let table_id = document.active_table;
    app.workspace.documents.insert(document_id, document);

    let add_address = CellAddress { row: 1, column: 0 };
    app.begin_cell_edit(table_id, add_address);
    app.text_selection = Some(TextSelection {
        document_id,
        table_id,
        address: add_address,
        session_id: app.edit.session_id,
        buffer_revision: app.edit.buffer_revision,
        start: 0,
        end: app.edit.buffer.chars().count(),
    });
    app.apply_markdown_modifier(CellFormatAction::Bold, &context);
    assert_eq!(app.edit.buffer, "**''=A1**");
    assert_eq!(
        crate::markdown_adapter::MarkdownCellDisplay::parse_fragment(&app.edit.buffer).plain_text,
        "'=A1"
    );
    let selection = app
        .text_selection
        .expect("selection follows marker insertion");
    assert_eq!((selection.start, selection.end), (2, 7));
    assert_eq!(&app.edit.buffer[selection.start..selection.end], "''=A1");
    app.apply_markdown_modifier(CellFormatAction::Bold, &context);
    assert_eq!(app.edit.buffer, "''=A1");
    assert_eq!(
        crate::markdown_adapter::MarkdownCellDisplay::parse_fragment(&app.edit.buffer).plain_text,
        "'=A1"
    );
    app.commit_edit(None);

    let remove_address = CellAddress { row: 1, column: 1 };
    app.begin_cell_edit(table_id, remove_address);
    app.text_selection = Some(TextSelection {
        document_id,
        table_id,
        address: remove_address,
        session_id: app.edit.session_id,
        buffer_revision: app.edit.buffer_revision,
        start: 2,
        end: 7,
    });
    app.apply_markdown_modifier(CellFormatAction::Bold, &context);
    assert_eq!(app.edit.buffer, "''=A1");
    assert_eq!(
        crate::markdown_adapter::MarkdownCellDisplay::parse_fragment(&app.edit.buffer).plain_text,
        "'=A1"
    );
    app.commit_edit(None);

    let escape_address = CellAddress { row: 1, column: 2 };
    app.begin_cell_edit(table_id, escape_address);
    app.text_selection = Some(TextSelection {
        document_id,
        table_id,
        address: escape_address,
        session_id: app.edit.session_id,
        buffer_revision: app.edit.buffer_revision,
        start: 0,
        end: app.edit.buffer.chars().count(),
    });
    app.apply_markdown_modifier(CellFormatAction::Bold, &context);
    assert_eq!(app.edit.buffer, "**'=A1**");
    let display = crate::markdown_adapter::MarkdownCellDisplay::parse_fragment(&app.edit.buffer);
    assert_eq!(display.plain_text, "=A1");
    assert!(display.literal_marker_consumed);
    app.commit_edit(None);
}

#[test]
/// Keeps a real apostrophe when Code formatting selects rich text inside a cell.
fn markdown_code_selection_keeps_interior_apostrophe_literal() {
    let context = egui::Context::default();
    let mut app = new_app(&context);
    let document_id = app.workspace.active_document.unwrap();
    let source = Arc::new(
        crate::markdown_adapter::MarkdownDocument::parse(
            b"| Value |\n| --- |\n| note **'=A1** |\n".to_vec(),
        )
        .expect("interior literal fixture parses"),
    );
    let mut document = source.to_document(document_id, "Interior literal");
    document.set_source(Some(Arc::new(SourceDocument::Markdown(Arc::clone(
        &source,
    )))));
    let table_id = document.active_table;
    app.workspace.documents.insert(document_id, document);
    let address = CellAddress { row: 1, column: 0 };
    app.begin_cell_edit(table_id, address);
    app.text_selection = Some(TextSelection {
        document_id,
        table_id,
        address,
        session_id: app.edit.session_id,
        buffer_revision: app.edit.buffer_revision,
        start: 5,
        end: 13,
    });

    app.apply_markdown_modifier(CellFormatAction::Code, &context);

    assert_ne!(
        app.status.as_ref().map(Message::key),
        Some("status-markdown-code-unrepresentable")
    );
    let display = crate::markdown_adapter::MarkdownCellDisplay::parse_fragment(&app.edit.buffer);
    assert_eq!(display.plain_text, "note '=A1");
    assert!(
        display
            .runs
            .iter()
            .any(|run| { run.code && run.text.as_deref() == Some("'=A1") })
    );
    app.commit_edit(None);
}

#[test]
/// Writes Markdown display text as a protected CSV literal, not a new formula.
fn markdown_formula_looking_display_exports_as_literal_csv_text() {
    let directory = TestDirectory::new();
    let destination = directory.0.join("literal.csv");
    let context = egui::Context::default();
    let mut app = new_app(&context);
    let document_id = app.workspace.active_document.unwrap();
    let table_id = app.workspace.documents[&document_id].active_table;
    app.workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .set_cell(
            table_id,
            CellAddress { row: 1, column: 0 },
            Some(Cell::text("**=1+1**")),
        )
        .expect("set formatted formula-looking text");

    app.start_export_preflight(
        document_id,
        ExportOptions {
            format: ExportFormat::Csv,
            scope: ExportScope::Selected(table_id),
            formulas: FormulaOutput::Formula,
            modifiers: ModifierOutput::Convert,
            markdown_header: MarkdownHeader::UseFirstRow,
        },
        destination.clone(),
    );
    wait_for_save_as(&mut app, |app| {
        app.active_export_confirmation
            .as_ref()
            .is_some_and(|export| export.document_id == document_id)
    });
    let export = app.active_export_confirmation.take().unwrap();
    app.submit_prepared_export(export);
    wait_for_save_as(&mut app, |app| {
        app.pending_export_writes.contains_key(&document_id)
    });
    wait_for_save_as(&mut app, |app| {
        !app.pending_export_writes.contains_key(&document_id)
    });

    let csv = crate::csv_adapter::CsvDocument::parse(
        std::fs::read(destination).expect("CSV output was written"),
        crate::csv_adapter::EncodingChoice::Auto,
        ',',
    )
    .expect("output parses as CSV");
    let value = &csv.records[1].fields[0].value;
    assert_eq!(value, "'=1+1");
    assert_eq!(crate::document::literal_text_value(value), "=1+1");
    assert!(crate::calc::formula_source(&CellInput::Text(value.clone()), false).is_none());
}

#[test]
/// Keeps a partial CSV export acknowledgement from authorizing app exit.
fn app_close_does_not_treat_partial_csv_export_as_a_document_save() {
    let directory = TestDirectory::new();
    let destination = directory.0.join("partial-on-exit.csv");
    let context = egui::Context::default();
    let mut app = new_app(&context);
    let document_id = app.workspace.active_document.unwrap();
    app.request_table_management(StructureAction::AddTable, None, None);
    wait_for_save_as(&mut app, |app| {
        !app.pending_structures.contains_key(&document_id)
    });

    app.app_close_dialog_open = true;
    app.save_all_before_exit_with_path_picker(|_, _| Some(destination.clone()));
    wait_for_save_as(&mut app, |app| {
        app.active_export_confirmation
            .as_ref()
            .is_some_and(|export| export.document_id == document_id)
    });
    assert!(app.app_close_save_failed);
    assert!(!app.app_close_after_documents);
    assert!(!app.app_close_save_tokens.contains_key(&document_id));

    let export = app.active_export_confirmation.take().unwrap();
    app.submit_prepared_export(export);
    wait_for_save_as(&mut app, |app| {
        !app.pending_export_writes.contains_key(&document_id)
    });
    assert!(!app.app_close_after_documents);
    assert!(app.document_requires_save(document_id));
    assert!(app.workspace.documents.contains_key(&document_id));
    assert!(app.pending_close_document.is_none());
    assert!(!app.recent_files.contains(&destination));
}

#[test]
/// Saves an arbitrary single-table ID after a Markdown-to-CSV source change.
fn non_one_table_id_saves_after_complete_csv_save_as() {
    let directory = TestDirectory::new();
    let destination = directory.0.join("stable-id.csv");
    let context = egui::Context::default();
    let mut app = new_app(&context);
    let (document_id, _) = install_test_markdown_grid(&mut app, 1, 2, "Arbitrary table id");
    let first_table_id = app.workspace.documents[&document_id].active_table;
    app.request_table_management(StructureAction::AddTable, None, None);
    wait_for_save_as(&mut app, |app| {
        !app.pending_structures.contains_key(&document_id)
    });
    let survivor_table_id = app.workspace.documents[&document_id].active_table;
    app.request_table_management(StructureAction::DeleteTable, Some(first_table_id), None);
    wait_for_save_as(&mut app, |app| {
        !app.pending_structures.contains_key(&document_id)
    });
    assert_eq!(
        app.workspace.documents[&document_id].tables[0].id,
        survivor_table_id
    );
    app.workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .set_cell(
            survivor_table_id,
            CellAddress { row: 0, column: 0 },
            Some(Cell::text("surviving table")),
        )
        .expect("edit the remaining table");

    app.save_as_to_path(document_id, destination.clone());
    // A fast save can finish in the same poll that approves its preflight.
    wait_for_save_as(&mut app, |app| {
        app.active_export_confirmation.is_some()
            || matches!(
                app.workspace.documents[&document_id].source.as_deref(),
                Some(SourceDocument::Csv(_))
            )
    });
    if let Some(export) = app.active_export_confirmation.take() {
        assert_eq!(export.document_id, document_id);
        app.submit_prepared_export(export);
    }
    wait_for_save_as(&mut app, |app| {
        !app.pending_saves.contains_key(&document_id)
            && matches!(
                app.workspace.documents[&document_id].source.as_deref(),
                Some(SourceDocument::Csv(_))
            )
    });
    assert_eq!(
        app.workspace.documents[&document_id].tables[0].id,
        survivor_table_id
    );

    app.workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .set_cell(
            survivor_table_id,
            CellAddress { row: 0, column: 0 },
            Some(Cell::number(42.0)),
        )
        .expect("edit a typed number in the converted CSV source");
    let export_destination = directory.0.join("after-save-as-edit.csv");
    app.start_export_preflight(
        document_id,
        ExportOptions {
            format: ExportFormat::Csv,
            scope: ExportScope::Selected(survivor_table_id),
            formulas: FormulaOutput::Formula,
            modifiers: ModifierOutput::Convert,
            markdown_header: MarkdownHeader::UseFirstRow,
        },
        export_destination.clone(),
    );
    wait_for_save_as(&mut app, |app| {
        app.active_export_confirmation
            .as_ref()
            .is_some_and(|export| export.document_id == document_id)
    });
    let export = app
        .active_export_confirmation
        .take()
        .expect("a CSV export can materialize its stable table ID after Save As");
    assert!(export.prepared.report.can_export());
    app.submit_prepared_export(export);
    wait_for_save_as(&mut app, |app| {
        !app.pending_export_writes.contains_key(&document_id)
    });
    let export_output =
        std::fs::read_to_string(export_destination).expect("edited CSV export was written");
    assert!(export_output.contains("42"));

    app.save_document_to_path(document_id, destination.clone());
    wait_for_save_as(&mut app, |app| app.pending_saves.contains_key(&document_id));
    wait_for_save_as(&mut app, |app| {
        !app.pending_saves.contains_key(&document_id)
            && app.workspace.documents.contains_key(&document_id)
            && destination.is_file()
    });
    let output = std::fs::read_to_string(destination).expect("stable ID CSV was written");
    assert!(output.contains("42"));
}

#[test]
/// Keeps a Save All request after growth but drops its exit intent when the user cancels.
fn cancelling_app_close_during_growth_keeps_late_alternate_save_without_exit() {
    let directory = TestDirectory::new();
    let destination = directory.0.join("growth-cancelled-exit.csv");
    let context = egui::Context::default();
    let mut app = new_app(&context);
    let document_id = app.workspace.active_document.unwrap();
    let table_id = app.workspace.documents[&document_id].active_table;
    app.begin_cell_edit(table_id, CellAddress { row: 2, column: 0 });
    app.edit.buffer = "saved after exit cancel".to_owned();
    app.commit_edit(None);
    assert!(app.pending_structures.contains_key(&document_id));
    wait_for_save_as(&mut app, |app| {
        !app.pending_structures.contains_key(&document_id)
            && !app.pending_text_growth_sessions.contains_key(&document_id)
    });

    app.app_close_dialog_open = true;
    app.save_all_before_exit_with_path_picker(|_, _| Some(destination.clone()));
    assert!(app.pending_save_intents.contains_key(&document_id));
    app.cancel_app_close();
    wait_for_save_as(&mut app, |app| {
        app.pending_saves.contains_key(&document_id)
            || app
                .active_export_confirmation
                .as_ref()
                .is_some_and(|export| export.document_id == document_id)
    });
    if let Some(export) = app.active_export_confirmation.take() {
        app.submit_prepared_export(export);
    }
    wait_for_save_as(&mut app, |app| {
        !app.pending_saves.contains_key(&document_id)
            && !app.pending_save_intents.contains_key(&document_id)
            && app.workspace.documents.contains_key(&document_id)
            && destination.is_file()
    });

    assert!(
        std::fs::read_to_string(destination)
            .expect("deferred alternate save completed")
            .contains("saved after exit cancel")
    );
    assert!(!app.app_close_after_documents);
    assert!(app.app_close_save_tokens.is_empty());
    assert!(app.workspace.documents.contains_key(&document_id));
}

#[test]
// Suspends the close prompt while a first Save As is open, then keeps the document on cancel.
fn save_as_close_does_not_leave_the_original_close_prompt_active() {
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    let mut app = TexcelApp::new_with_settings_path(&creation, None);
    app.create_new_document("Close after Save As".to_owned())
        .expect("create a Markdown document");
    let document_id = app.workspace.active_document.unwrap();
    let table_id = app.workspace.documents[&document_id].active_table;
    app.workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .set_cell(
            table_id,
            CellAddress::default(),
            Some(Cell::text("changed")),
        )
        .expect("make the document dirty");

    app.request_close_document(document_id);
    assert_eq!(app.pending_close_document, Some(document_id));
    app.save_and_close_document(document_id);

    assert!(app.pending_close_document.is_none());
    let intent = app
        .export_dialog
        .take()
        .and_then(|dialog| dialog.save_intent)
        .expect("first Save As keeps its close intent");
    assert!(intent.close_after_save);
    app.cancel_save_as_dialog(document_id, intent);
    assert!(app.pending_close_document.is_none());
    assert!(app.workspace.documents.contains_key(&document_id));
}

#[test]
/// Adds a complete Markdown-to-CSV Save As to recents after its save result.
fn full_markdown_to_csv_save_as_is_remembered_after_success() {
    let directory = TestDirectory::new();
    let destination = directory.0.join("complete.csv");
    let context = egui::Context::default();
    let mut app = new_app(&context);
    app.create_new_document("Recent CSV".to_owned())
        .expect("create Markdown document");
    let document_id = app.workspace.active_document.unwrap();

    app.save_as_to_path(document_id, destination.clone());
    assert!(!app.recent_files.contains(&destination));
    // Wait for the saved document and the written file, even if the worker finishes at once.
    wait_for_save_as(&mut app, |app| {
        destination.is_file()
            && matches!(
                app.workspace.documents[&document_id].source.as_deref(),
                Some(SourceDocument::Csv(_))
            )
            && !app.pending_saves.contains_key(&document_id)
    });

    assert_eq!(app.recent_files.front(), Some(&destination));
    assert!(matches!(
        app.workspace.documents[&document_id].source.as_deref(),
        Some(SourceDocument::Csv(_))
    ));
}

#[test]
/// Commits an adjacent Markdown cell before preparing its CSV destination.
fn markdown_save_as_waits_for_editor_growth_before_csv_preflight() {
    let directory = TestDirectory::new();
    let destination = directory.0.join("growth.csv");
    let context = egui::Context::default();
    let mut app = new_app(&context);
    let document_id = app.workspace.active_document.unwrap();
    let table_id = app.workspace.documents[&document_id].active_table;
    app.begin_cell_edit(table_id, CellAddress { row: 2, column: 0 });
    app.edit.buffer = "from editor".to_owned();

    app.save_as_to_path(document_id, destination.clone());
    assert!(app.pending_structures.contains_key(&document_id));
    assert!(app.pending_save_intents.contains_key(&document_id));
    wait_for_save_as(&mut app, |app| {
        app.pending_saves.contains_key(&document_id)
            || app
                .active_export_confirmation
                .as_ref()
                .is_some_and(|export| export.document_id == document_id)
    });
    if let Some(export) = app.active_export_confirmation.take() {
        app.submit_prepared_export(export);
    }
    wait_for_save_as(&mut app, |app| app.pending_saves.contains_key(&document_id));
    wait_for_save_as(&mut app, |app| {
        !app.pending_saves.contains_key(&document_id)
    });

    let output = std::fs::read_to_string(destination).expect("grown Markdown table was exported");
    assert!(output.contains("from editor"));
    assert!(matches!(
        app.workspace.documents[&document_id].source.as_deref(),
        Some(SourceDocument::Csv(_))
    ));
}

#[test]
/// Uses a tab delimiter when a Markdown document is saved as TSV.
fn markdown_save_as_tsv_uses_tab_delimiters() {
    let directory = TestDirectory::new();
    let destination = directory.0.join("table.tsv");
    let context = egui::Context::default();
    let mut app = new_app(&context);
    let document_id = app.workspace.active_document.unwrap();
    let table_id = app.workspace.documents[&document_id].active_table;
    app.workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .set_cell(
            table_id,
            CellAddress { row: 1, column: 0 },
            Some(Cell::text("left,right")),
        )
        .expect("set text with a comma");

    app.save_as_to_path(document_id, destination.clone());
    wait_for_save_as(&mut app, |app| app.pending_saves.contains_key(&document_id));
    wait_for_save_as(&mut app, |app| {
        !app.pending_saves.contains_key(&document_id)
    });
    let output = std::fs::read_to_string(destination).expect("TSV was written");
    assert!(output.contains("left,right"));
    assert!(!output.contains("\"left,right\""));
}

#[test]
/// Rewrites a comma CSV document with tabs when it is saved as TSV.
fn comma_csv_save_as_tsv_writes_tabs() {
    let directory = TestDirectory::new();
    let destination = directory.0.join("table.tsv");
    let context = egui::Context::default();
    let mut app = new_app(&context);
    let (document_id, _) = install_test_csv_document(&mut app, b"a,b\n1,2\n".to_vec(), "table.csv");
    assert_eq!(
        default_save_format(&app.workspace.documents[&document_id]),
        ExportFormat::Csv
    );

    app.save_as_to_path(document_id, destination.clone());
    wait_for_save_as(&mut app, |app| {
        destination.is_file()
            && !app.pending_saves.contains_key(&document_id)
            && !app.pending_exports.contains_key(&document_id)
            && !app.pending_export_writes.contains_key(&document_id)
    });
    assert_eq!(
        std::fs::read_to_string(&destination).expect("TSV was written"),
        "a\tb\n1\t2\n"
    );
}
