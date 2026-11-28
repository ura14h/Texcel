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
// Restores the widths saved for a file before fitting, so a width the user set stays and
// only the other columns are measured.
fn reopening_keeps_saved_widths_and_fits_the_rest() {
    let directory = ScenarioDirectory::new();
    let path = directory.0.join("saved.md");
    let long = "a long value that needs a wide column";
    std::fs::write(
        &path,
        format!("| A | B |\n| --- | --- |\n| {long} | {long} |\n"),
    )
    .expect("write the input");
    let settings = directory.0.join("settings.json");
    let mut harness = scenario_harness(settings.clone());
    queue_open_and_wait(&mut harness, path.clone());
    let (document_id, table_id) = {
        let document = harness.state().workspace.active_document().unwrap();
        (document.id, document.active_table)
    };
    // The user narrows column A to 5 characters.
    harness
        .state_mut()
        .workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .view_mut(table_id)
        .unwrap()
        .set_layout_value(
            LayoutTarget::Column(0),
            LayoutValue::Column(Some(crate::document::ColumnLayout { width: Some(5.0) })),
        );
    harness.state_mut().mark_document_preference_change(
        document_id,
        table_id,
        TablePreferenceField::ViewLayout,
    );
    super::scenario_tests::wait_until(&mut harness, |app| {
        app.settings_saved_generation >= app.settings_save_generation
            && app.settings_pending_save.is_none()
            && app.settings_in_flight_generation.is_none()
    });
    drop(harness);

    let mut harness = scenario_harness(settings);
    queue_open_and_wait(&mut harness, path);
    let document = harness.state().workspace.active_document().unwrap();
    let layout = &document.view(document.active_table).unwrap().layout;
    assert_eq!(layout.column(0).and_then(|column| column.width), Some(5.0));
    let fitted = layout.column(1).and_then(|column| column.width);
    assert!(
        fitted.is_some_and(|width| width > 20.0),
        "column B is fitted: {fitted:?}"
    );
}
