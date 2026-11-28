use super::scenario_tests::{ScenarioDirectory, queue_open_and_wait, scenario_harness};
use super::*;

/// Opens a Markdown file and returns its pixel column widths at 100%.
fn widths_after_open(
    contents: &str,
    columns: usize,
) -> (egui_kittest::Harness<'static, TexcelApp>, Vec<f64>) {
    let directory = ScenarioDirectory::new();
    let path = directory.0.join("fit.md");
    std::fs::write(&path, contents).expect("write the input");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    queue_open_and_wait(&mut harness, path);
    let widths = {
        let document = harness.state().workspace.active_document().unwrap();
        let layout = &document.view(document.active_table).unwrap().layout;
        (0..columns)
            .map(|column| layout.column_width_pixels(column))
            .collect()
    };
    (harness, widths)
}

#[test]
// Fits the columns of an opened file before it is shown.
fn opened_file_has_fitted_columns() {
    let long = "a long value that needs a wide column";
    let (_, widths) = widths_after_open(&format!("| A | B |\n| --- | --- |\n| x | {long} |\n"), 2);
    assert!(widths[0] < 112.0, "the short column narrows: {widths:?}");
    assert!(widths[1] > 112.0, "the long column widens: {widths:?}");
}

#[test]
// Measures only the first rows on opening; a later AutoFit measures every row.
fn opening_fit_reads_the_first_rows_only() {
    let mut contents = String::from("| A |\n| --- |\n");
    for row in 0..1_200 {
        if row == 1_100 {
            contents.push_str("| a long value far below the first thousand rows |\n");
        } else {
            contents.push_str("| x |\n");
        }
    }
    let (mut harness, widths) = widths_after_open(&contents, 1);
    assert!(
        widths[0] < 112.0,
        "the far row is not measured on opening: {widths:?}"
    );

    let ctx = harness.ctx.clone();
    let (document_id, table_id) = {
        let document = harness.state().workspace.active_document().unwrap();
        (document.id, document.active_table)
    };
    harness
        .state_mut()
        .request_columns_auto_fit(&ctx, document_id, table_id, vec![0]);
    super::scenario_tests::wait_until(&mut harness, |app| app.pending_auto_fits.is_empty());
    let width = harness.state().workspace.documents[&document_id]
        .view(table_id)
        .unwrap()
        .layout
        .column_width_pixels(0);
    assert!(width > 112.0, "AutoFit reads every row: {width}");
}

#[test]
// Fits every table of an opened file, not only the one shown first.
fn opening_fits_every_table() {
    let long = "a long value that needs a wide column";
    let directory = ScenarioDirectory::new();
    let path = directory.0.join("two.md");
    std::fs::write(
        &path,
        format!("| A |\n| --- |\n| x |\n\ntext\n\n| B |\n| --- |\n| {long} |\n"),
    )
    .expect("write the input");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    queue_open_and_wait(&mut harness, path);
    let document = harness.state().workspace.active_document().unwrap();
    let second = document.tables[1].id;
    assert_ne!(
        document.active_table, second,
        "the second table is not shown first"
    );
    let width = document.view(second).unwrap().layout.column_width_pixels(0);
    assert!(
        width > 112.0,
        "the second table's long column widens: {width}"
    );
}

#[test]
// Measures the header row in the bold it is drawn with, so a heading wider than the cells
// below it is not clipped. Bold Latin text comes from another font and is wider.
fn opening_fit_measures_the_header_row_in_bold() {
    let heading = "Shelf Count Note Item abcdefghijklmnopqrstuvwxyz";
    let directory = ScenarioDirectory::new();
    let path = directory.0.join("fit.md");
    std::fs::write(&path, format!("| {heading} |\n| --- |\n| x |\n")).expect("write the input");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    queue_open_and_wait(&mut harness, path);
    let app = harness.state();
    let document = app.workspace.active_document().unwrap();
    let width = document
        .view(document.active_table)
        .unwrap()
        .layout
        .column_width_pixels_with(0, app.grid_character_width);
    let style = CellTextStyle::default();
    let (mut job, _) = cell_layout_job(
        heading,
        CellTextFormat {
            style: &style,
            color: Color32::BLACK,
            width: f32::INFINITY,
            show_invisibles: false,
            scale: 1.0,
        },
    );
    apply_grid_bold(&mut job);
    let bold = f64::from(
        harness
            .ctx
            .fonts_mut(|fonts| fonts.layout_job(job))
            .size()
            .x,
    );
    assert!(
        width >= bold + crate::document::COLUMN_TEXT_PADDING_PIXELS,
        "the column ({width}) holds the bold heading ({bold}) and its padding"
    );
}

#[test]
// A duplicated table is a copy of its table, so it keeps that table's fitted widths.
fn duplicated_table_keeps_the_widths_of_its_table() {
    let long = "a long value that needs a wide column";
    let (mut harness, widths) =
        widths_after_open(&format!("| A | B |\n| --- | --- |\n| x | {long} |\n"), 2);
    let (document_id, original) = {
        let document = harness.state().workspace.active_document().unwrap();
        (document.id, document.active_table)
    };
    harness.state_mut().request_table_management(
        StructureAction::DuplicateTable,
        Some(original),
        None,
    );
    super::scenario_tests::wait_until(&mut harness, |app| {
        app.pending_structures.is_empty() && app.workspace.documents[&document_id].tables.len() == 2
    });
    let document = &harness.state().workspace.documents[&document_id];
    let copy = document.tables[1].id;
    let layout = &document.view(copy).unwrap().layout;
    let copied = (0..2)
        .map(|column| layout.column_width_pixels(column))
        .collect::<Vec<_>>();
    assert_eq!(copied, widths, "the copy has its table's widths");

    // Undo removes the copy and Redo brings it back with the same widths.
    let widths_of_copy = |harness: &egui_kittest::Harness<'_, TexcelApp>| {
        let document = &harness.state().workspace.documents[&document_id];
        let layout = &document.view(copy)?.layout;
        Some(
            (0..2)
                .map(|column| layout.column_width_pixels(column))
                .collect::<Vec<_>>(),
        )
    };
    harness.state_mut().apply_undo(false);
    super::scenario_tests::wait_until(&mut harness, |app| {
        app.workspace.documents[&document_id].tables.len() == 1
    });
    harness.state_mut().apply_undo(true);
    super::scenario_tests::wait_until(&mut harness, |app| {
        app.workspace.documents[&document_id].tables.len() == 2
    });
    assert_eq!(
        widths_of_copy(&harness),
        Some(widths),
        "Redo keeps the widths"
    );
}
