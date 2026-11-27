use super::*;
use egui_kittest::{
    Harness,
    kittest::{By, Queryable},
};

/// Sends one Help menu command through the native adapter or the in-window menu.
fn choose_help_item(harness: &mut Harness<'_, TexcelApp>, id: &str, label_key: &str) {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let _ = label_key;
        let target = harness.state().native_action_target(&harness.ctx, false);
        harness.state_mut().native_menu_events_for_test =
            vec![crate::native_menu::NativeMenuEvent {
                id: id.to_owned(),
                recent_path: None,
                target,
            }];
        harness.step();
        harness.step();
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = id;
        let menu_label = crate::i18n::context_text(&harness.ctx, "menu-help");
        harness
            .query_all(By::new().role(egui::Role::Button).label(&menu_label))
            .next()
            .expect("the Help menu is visible")
            .click_accesskit();
        harness.run_steps(2);
        let item_label = crate::i18n::context_text(&harness.ctx, label_key);
        harness
            .query_all(By::new().label(&item_label))
            .next()
            .expect("the Help menu offers the item")
            .click();
        harness.run_steps(2);
    }
}

/// Opens the guide in its own window, separate from About.
#[test]
fn help_opens_the_built_in_guide_in_its_own_window() {
    let mut harness = Harness::builder()
        .with_size([980.0, 680.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    harness.state_mut().language = crate::i18n::Language::English;
    crate::i18n::set_context_language(&harness.ctx, crate::i18n::Language::English);
    harness.step();

    choose_help_item(&mut harness, "help.open", "menu-help-open");
    let app = harness.state();
    assert!(app.help_viewport_open);
    assert!(
        !app.about_dialog_open,
        "Help no longer opens the About dialog"
    );
    harness.run_steps(2);
    assert!(
        harness
            .query_all(By::new().label_contains("Selecting and moving"))
            .next()
            .is_some(),
        "the guide shows its sections"
    );

    // The guide follows the app language.
    harness.state_mut().language = crate::i18n::Language::Japanese;
    crate::i18n::set_context_language(&harness.ctx, crate::i18n::Language::Japanese);
    harness.run_steps(2);
    assert!(
        harness
            .query_all(By::new().label_contains("セルの選択と移動"))
            .next()
            .is_some(),
        "the Japanese guide replaces the English one"
    );
    if let Ok(image) = harness.render() {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("kittest-renders");
        let _ = std::fs::create_dir_all(&directory);
        let _ = image.save(directory.join("help-window-ja.png"));
    }
}

/// Shows the app name, version, and copyright in the About dialog.
#[test]
fn about_shows_the_name_version_and_copyright() {
    let mut harness = Harness::builder()
        .with_size([980.0, 680.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    harness.state_mut().language = crate::i18n::Language::English;
    crate::i18n::set_context_language(&harness.ctx, crate::i18n::Language::English);
    harness.step();
    // macOS opens About from the application menu's standard panel instead.
    #[cfg(target_os = "macos")]
    {
        harness.state_mut().about_dialog_open = true;
        harness.run_steps(2);
    }
    #[cfg(not(target_os = "macos"))]
    choose_help_item(&mut harness, "help.about", "menu-app-about");

    assert!(harness.state().about_dialog_open);
    for text in [
        format!("Version {}", env!("CARGO_PKG_VERSION")),
        APP_COPYRIGHT.to_owned(),
    ] {
        assert!(
            harness.query_all(By::new().label(&text)).next().is_some(),
            "About shows {text}"
        );
    }
}
