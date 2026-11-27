use super::scenario_tests::{
    ScenarioDirectory, command_modifiers, key_event, queue_open_and_wait, scenario_harness,
    select_cell_by_name_box, send_batch, send_event,
};
use super::*;
use std::fs;
use std::time::Duration;

type AppHarness = egui_kittest::Harness<'static, TexcelApp>;

/// Lets workers finish while the app keeps drawing frames.
fn settle(harness: &mut AppHarness, frames: usize) {
    for _ in 0..frames {
        send_batch(harness, []);
        std::thread::sleep(Duration::from_millis(3));
    }
}

/// Returns the color of the first painted label that reads exactly `label`.
fn label_color(harness: &AppHarness, label: &str) -> Color32 {
    harness
        .output()
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) if text.galley.job.text == label => text
                .galley
                .job
                .sections
                .first()
                .map(|section| section.format.color),
            _ => None,
        })
        .unwrap_or_else(|| panic!("header label {label} is painted"))
}

#[test]
/// Headers inside the saved range are strong, and follow typing and Undo right away.
fn saved_range_headers_follow_edits_and_undo() {
    let directory = ScenarioDirectory::new();
    let path = directory.0.join("extent.md");
    fs::write(&path, "| x | x |\n| --- | --- |\n| x | x |\n").expect("write the input file");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    queue_open_and_wait(&mut harness, path);
    settle(&mut harness, 20);
    // Move the selection away from the headers being compared.
    select_cell_by_name_box(&mut harness, "A1");
    settle(&mut harness, 10);

    let strong = label_color(&harness, "B");
    let weak = label_color(&harness, "D");
    assert_ne!(strong, weak, "saved and unsaved column headers differ");
    assert_eq!(
        label_color(&harness, "2"),
        strong,
        "saved row header is strong"
    );
    assert_eq!(
        label_color(&harness, "4"),
        weak,
        "row past the saved range is weak"
    );

    // Typing into C4 grows the saved range to C and 4 at once.
    select_cell_by_name_box(&mut harness, "C4");
    send_event(&mut harness, egui::Event::Text("y".to_owned()));
    send_event(&mut harness, key_event(Key::Enter, egui::Modifiers::NONE));
    settle(&mut harness, 60);
    select_cell_by_name_box(&mut harness, "A1");
    settle(&mut harness, 10);
    assert_eq!(label_color(&harness, "C"), strong);
    assert_eq!(label_color(&harness, "4"), strong);
    assert_eq!(label_color(&harness, "D"), weak);

    // Undo shrinks the saved range back.
    let command = command_modifiers(&harness.ctx);
    send_event(&mut harness, key_event(Key::Z, command));
    settle(&mut harness, 60);
    select_cell_by_name_box(&mut harness, "A1");
    settle(&mut harness, 10);
    assert_eq!(label_color(&harness, "C"), weak);
    assert_eq!(label_color(&harness, "4"), weak);
}
