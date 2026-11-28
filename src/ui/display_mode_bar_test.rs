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

#[test]
/// Runs the formula bar and the switch from window edge to window edge, with one faint
/// vertical line between them.
fn bars_reach_the_window_edges_with_a_line_between() {
    let directory = ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    harness.run_steps(2);
    let window = harness.ctx.content_rect();
    let bar_fill = harness.ctx.global_style().visuals.extreme_bg_color;
    let shapes = &harness.output().shapes;
    let bar_rects = shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Rect(rect) if rect.fill == bar_fill && rect.rect.top() < 60.0 => {
                Some(rect.rect)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let left = bar_rects
        .iter()
        .map(|rect| rect.left())
        .fold(f32::MAX, f32::min);
    let right = bar_rects
        .iter()
        .map(|rect| rect.right())
        .fold(f32::MIN, f32::max);
    assert!(
        left <= window.left() + 0.5,
        "the formula bar starts at the left edge"
    );
    assert!(
        right >= window.right() - 0.5,
        "the switch ends at the right edge"
    );
    let line_color = harness
        .ctx
        .global_style()
        .visuals
        .widgets
        .noninteractive
        .bg_stroke
        .color;
    let separators = shapes
        .iter()
        .filter(|clipped| match &clipped.shape {
            egui::Shape::LineSegment { points, stroke } => {
                stroke.color == line_color
                    && points[0].x == points[1].x
                    && points[0].x > window.center().x
                    && points[0].y < 60.0
            }
            _ => false,
        })
        .count();
    assert_eq!(separators, 1, "one line separates the bars");
}
