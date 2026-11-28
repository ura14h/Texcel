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

    // The outline at the left lists the headings on an opaque fill; choosing one scrolls the
    // guide to it.
    let outline_fills = harness
        .output()
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            // The pane spans the window's height at the outline's width.
            egui::Shape::Rect(rect)
                if (rect.rect.width() - 200.0).abs() < 2.0 && rect.rect.height() > 500.0 =>
            {
                Some(rect.fill)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!outline_fills.is_empty(), "the outline pane is drawn");
    assert!(
        outline_fills.iter().all(|fill| fill.a() == 255),
        "the outline fill is opaque: {outline_fills:?}"
    );
    let appendix = "使える関数";
    let heading_top = |harness: &Harness<'_, TexcelApp>| {
        harness
            .query_all(By::new().role(egui::Role::Label).label(appendix))
            .next()
            .map(|node| node.rect().top())
    };
    let outline_entry = harness
        .query_all(By::new().role(egui::Role::Button).label(appendix))
        .next()
        .expect("the outline lists the appendix")
        .rect();
    let before = heading_top(&harness).unwrap_or(f32::MAX);
    harness.event(egui::Event::PointerMoved(outline_entry.center()));
    harness.event(egui::Event::PointerButton {
        pos: outline_entry.center(),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    harness.event(egui::Event::PointerButton {
        pos: outline_entry.center(),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run_steps(30);
    let after = heading_top(&harness).expect("the appendix heading is drawn");
    assert!(
        after < before && after < 150.0,
        "the appendix heading moves to the top: {before} -> {after}"
    );
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

/// Describes exactly the functions that formulas calculate, in both languages.
#[test]
fn help_appendix_lists_every_calculated_function() {
    for (guide, heading) in [
        (
            include_str!("../../../assets/help/help.en.md"),
            "## Functions",
        ),
        (
            include_str!("../../../assets/help/help.ja.md"),
            "## 使える関数",
        ),
    ] {
        let appendix = &guide[guide.find(heading).expect("the guide has the appendix")..];
        let mut listed = appendix
            .lines()
            .filter_map(|line| line.split('`').nth(1))
            .filter_map(|syntax| syntax.split_once('('))
            .map(|(name, _)| name.to_owned())
            .collect::<Vec<_>>();
        listed.sort();
        let mut supported = crate::calc::SUPPORTED_FUNCTIONS
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>();
        supported.sort();
        assert_eq!(
            listed, supported,
            "{heading} lists the calculated functions"
        );
    }
}
