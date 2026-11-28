use super::super::scenario_tests::{ScenarioDirectory, queue_open_and_wait, scenario_harness};
use super::*;
use egui_kittest::kittest::{By, Queryable};

#[test]
/// One button names the current mode and switches to the other mode on each click.
fn one_switch_alternates_live_and_source_editing() {
    let directory = ScenarioDirectory::new();
    let path = directory.0.join("modes.md");
    std::fs::write(&path, "| A |\n| --- |\n| **x** |\n").expect("write a table");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    harness.state_mut().language = crate::i18n::Language::English;
    crate::i18n::set_context_language(&harness.ctx, crate::i18n::Language::English);
    queue_open_and_wait(&mut harness, path);
    let document_id = harness.state().workspace.active_document.unwrap();

    for (label, after) in [
        ("Live Editing", DocumentDisplayMode::Source),
        ("Source Editing", DocumentDisplayMode::Live),
    ] {
        assert_eq!(
            harness
                .query_all(By::new().role(egui::Role::Button).label(label))
                .count(),
            1,
            "one switch shows {label}"
        );
        harness
            .query_all(By::new().role(egui::Role::Button).label(label))
            .next()
            .expect("the switch is visible")
            .click();
        harness.run_steps(2);
        assert_eq!(
            harness.state().workspace.documents[&document_id].display_mode,
            after
        );
    }
}
