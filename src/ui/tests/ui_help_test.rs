use super::*;
use egui_kittest::{
    Harness,
    kittest::{By, Queryable},
};

/// Opens Help through the native action adapter and checks its guide and formula list.
#[test]
fn help_menu_opens_the_localized_guide_and_about_page() {
    let mut harness = Harness::builder()
        .with_size([980.0, 680.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    harness.state_mut().settings_window_open = false;
    harness.step();

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let menu_state = harness.state().native_menu_states_for_frame(
            crate::ui::top::TopActionState::default(),
            false,
            &harness.ctx,
        );
        assert!(menu_state["help.open"].enabled);
        let target = harness.state().native_action_target(&harness.ctx, false);
        harness.state_mut().native_menu_events_for_test =
            vec![crate::native_menu::NativeMenuEvent {
                id: "help.open".to_owned(),
                recent_path: None,
                target,
            }];
        harness.step();
        harness.step();
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let menu_label = crate::i18n::context_text(&harness.ctx, "menu-help");
        harness
            .query_all(By::new().role(egui::Role::Button).label(&menu_label))
            .next()
            .expect("the Help menu is visible")
            .click_accesskit();
        harness.run_steps(2);
        let open_label = crate::i18n::context_text(&harness.ctx, "menu-help-open");
        harness
            .query_all(By::new().label(&open_label))
            .next()
            .expect("the Help menu offers its guide")
            .click();
        harness.run_steps(2);
    }

    assert!(harness.state().help_window_open);
    let title = crate::i18n::context_text(&harness.ctx, "help-title");
    assert!(
        harness.query_all(By::new().label(&title)).next().is_some(),
        "the guide has its localized title"
    );
    let formula_heading = crate::i18n::context_text(&harness.ctx, "help-formulas-heading");
    assert!(
        harness
            .query_all(By::new().label(&formula_heading))
            .next()
            .is_some(),
        "the guide explains the supported formula set"
    );
    let undo_shortcut = crate::i18n::context_text(
        &harness.ctx,
        if cfg!(target_os = "macos") {
            "help-undo-command"
        } else {
            "help-undo-control"
        },
    );
    assert!(
        harness
            .query_all(By::new().label(&undo_shortcut))
            .next()
            .is_some(),
        "the guide shows the platform's Undo key"
    );
    assert!(
        harness
            .query_all(By::new().label_contains("Texcel 0.3.0"))
            .next()
            .is_some(),
        "the guide shows the application version"
    );
}
