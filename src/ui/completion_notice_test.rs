use super::*;
use crate::ui::scenario_tests as scenario;
use std::fs;

/// Reports a finished row insertion and a finished save, each after its job succeeds.
#[test]
fn finished_commands_report_their_completion() {
    let directory = scenario::ScenarioDirectory::new();
    let source = directory.0.join("notice.md");
    fs::write(&source, "| A |\n| --- |\n| one |\n").expect("write a small table");
    let mut harness = scenario::scenario_harness(directory.0.join("settings.json"));
    harness.state_mut().language = Language::English;
    crate::i18n::set_context_language(&harness.ctx, Language::English);
    scenario::queue_open_and_wait(&mut harness, source);
    let document_id = harness.state().workspace.active_document.unwrap();
    scenario::click_cell(&mut harness, "A2");

    harness
        .state_mut()
        .request_structure_action(StructureAction::InsertRows);
    assert_eq!(
        harness.state().status,
        None,
        "nothing is said while it runs"
    );
    scenario::wait_until(&mut harness, |app| app.pending_structures.is_empty());
    assert_eq!(
        harness
            .state()
            .status
            .as_ref()
            .map(|status| status.resolve(Language::English)),
        Some("Inserted 1 row.".to_owned())
    );

    let command = scenario::command_modifiers(&harness.ctx);
    scenario::send_event(&mut harness, scenario::key_event(Key::S, command));
    scenario::wait_until(&mut harness, |app| {
        !app.pending_saves.contains_key(&document_id)
    });
    assert_eq!(
        harness
            .state()
            .status
            .as_ref()
            .map(|status| status.resolve(Language::English)),
        Some("Saved “notice.md”.".to_owned())
    );
}

/// A notice registered for one job is never shown for a different job.
#[test]
fn completion_notice_belongs_to_its_own_job() {
    let ctx = egui::Context::default();
    let mut app = crate::ui::tests::new_app(&ctx);
    let document_id = app.workspace.active_document.unwrap();
    let first = app.next_job_token(document_id, 0);
    let second = app.next_job_token(document_id, 0);
    app.expect_completion(document_id, first, Some(Message::new("status-done-pasted")));
    assert_eq!(app.take_completion(document_id, second), None);
    assert_eq!(
        app.take_completion(document_id, first),
        None,
        "a mismatched result already dropped the notice"
    );
    app.expect_completion(
        document_id,
        second,
        Some(Message::new("status-done-pasted")),
    );
    assert!(app.take_completion(document_id, second).is_some());
}
