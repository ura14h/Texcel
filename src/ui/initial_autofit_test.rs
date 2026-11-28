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
