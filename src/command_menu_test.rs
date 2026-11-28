use super::*;
use std::collections::HashSet;

const ENGLISH_MESSAGES: &str = include_str!("../locales/en-US/main.ftl");
const JAPANESE_MESSAGES: &str = include_str!("../locales/ja-JP/main.ftl");

/// Confirms that every command in the shared tree has one stable ID.
#[test]
fn shared_menu_action_ids_are_unique() {
    let ids = action_ids();
    assert_eq!(ids.len(), ids.iter().collect::<HashSet<_>>().len());
}

/// Confirms that all shared menu labels exist in both supported languages.
#[test]
fn shared_menu_labels_exist_in_both_locales() {
    let mut keys = Vec::new();
    for menu in MENUS {
        keys.push(menu.label);
        collect_label_keys(menu.entries, &mut keys);
    }
    keys.push("menu-file-open-recent");
    keys.sort_unstable();
    keys.dedup();

    for key in keys {
        assert!(
            has_message(ENGLISH_MESSAGES, key),
            "missing English key {key}"
        );
        assert!(
            has_message(JAPANESE_MESSAGES, key),
            "missing Japanese key {key}"
        );
    }
}

/// Confirms that known keyboard shortcuts come from the shared action metadata.
#[test]
fn shared_menu_shortcuts_match_existing_bindings() {
    assert_eq!(
        action("edit.undo").unwrap().shortcut,
        MenuShortcut::Command('Z')
    );
    assert_eq!(
        action("edit.table_duplicate").unwrap().shortcut,
        MenuShortcut::CommandShift('D')
    );
    assert_eq!(
        action("edit.table_add").unwrap().shortcut,
        MenuShortcut::CommandShift('N')
    );
    assert_eq!(
        action("format.bold").unwrap().shortcut,
        MenuShortcut::Command('B')
    );
    assert_eq!(
        action("format.italic").unwrap().shortcut,
        MenuShortcut::Command('I')
    );
    assert_eq!(
        action("format.strike").unwrap().shortcut,
        MenuShortcut::CommandShift('X')
    );
    assert_eq!(
        action("format.code").unwrap().shortcut,
        MenuShortcut::Command('E')
    );
    assert_eq!(
        action("insert.rows").unwrap().shortcut,
        MenuShortcut::CommandShift('R')
    );
    assert_eq!(
        action("insert.columns").unwrap().shortcut,
        MenuShortcut::CommandShift('C')
    );
    assert_eq!(
        action("window.minimize").unwrap().shortcut,
        MenuShortcut::Command('M')
    );
}

#[test]
/// Keeps format-specific copy and paste choices next to their regular commands.
fn copy_and_paste_options_follow_their_standard_commands() {
    let edit = MENUS
        .iter()
        .find(|menu| menu.id == "menu.edit")
        .expect("the Edit menu exists");
    let ids = edit
        .entries
        .iter()
        .filter_map(|entry| match entry {
            MenuEntry::Action(action) => Some(action.id),
            _ => None,
        })
        .collect::<Vec<_>>();

    let copy = ids.iter().position(|id| *id == "edit.copy").unwrap();
    let copy_options = ids.iter().position(|id| *id == "edit.copy_as").unwrap();
    let paste = ids.iter().position(|id| *id == "edit.paste").unwrap();
    let paste_options = ids
        .iter()
        .position(|id| *id == "edit.paste_special")
        .unwrap();
    assert_eq!(copy_options, copy + 1);
    assert_eq!(paste_options, paste + 1);
}

/// Collects action and submenu labels without counting separators.
fn collect_label_keys(entries: &[MenuEntry], keys: &mut Vec<&'static str>) {
    for entry in entries {
        match entry {
            MenuEntry::Action(action) => keys.push(action.label),
            MenuEntry::Submenu(submenu) => {
                keys.push(submenu.label);
                collect_label_keys(submenu.entries, keys);
            }
            MenuEntry::Separator | MenuEntry::RecentFiles | MenuEntry::WindowList => (),
        }
    }
}

/// Finds an exact Fluent message key in one locale file.
fn has_message(source: &str, key: &str) -> bool {
    source.lines().any(|line| {
        line.split_once('=')
            .is_some_and(|(candidate, _)| candidate.trim() == key)
    })
}

/// Shows punctuation shortcuts with their symbols, not egui's key names.
#[test]
fn punctuation_shortcuts_use_symbols() {
    let ctx = egui::Context::default();
    // Formatting a shortcut checks the fonts, which exist after the first frame.
    ctx.run_ui(egui::RawInput::default(), |_| ())
        .textures_delta
        .clear();
    for (shortcut, symbol) in [
        (MenuShortcut::Command('/'), "/"),
        (MenuShortcut::Command('-'), "-"),
        (MenuShortcut::ZoomIn, "+"),
    ] {
        let text = shortcut_text(&ctx, shortcut);
        assert!(text.ends_with(symbol), "{text} ends with {symbol}");
        assert!(!text.contains("Slash") && !text.contains("Minus") && !text.contains("Plus"));
    }
}
