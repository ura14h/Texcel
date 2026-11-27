use crate::command_menu::{MENUS, MenuEntry, MenuShortcut, MenuTree, action};

/// Finds a leaf ID through the nested shared menu tree.
fn contains(entries: &'static [MenuEntry], id: &str) -> bool {
    entries.iter().any(|entry| match entry {
        MenuEntry::Action(action) => action.id == id,
        MenuEntry::Submenu(submenu) => contains(submenu.entries, id),
        MenuEntry::Separator | MenuEntry::RecentFiles | MenuEntry::WindowList => false,
    })
}

/// Checks each shared menu level stays short and its established actions remain present.
#[test]
fn shared_menu_tree_keeps_grouped_actions_and_short_levels() {
    for menu in MENUS {
        assert!(menu_level_is_short(menu), "{} keeps a short level", menu.id);
    }
    for (menu, id) in [
        ("menu.edit", "edit.delete_rows"),
        ("menu.edit", "edit.duplicate_columns"),
        ("menu.edit", "edit.fill_series"),
        ("menu.edit", "edit.table_delete"),
        ("menu.edit", "edit.copy_as"),
        ("menu.view", "view.zoom_in"),
        ("menu.view", "view.wrap_off"),
        ("menu.insert", "insert.rows"),
        ("menu.edit", "edit.table_add"),
        ("menu.data", "data.sort"),
        ("menu.data", "data.replace_results"),
        ("menu.window", "window.minimize"),
        ("menu.window", "window.zoom"),
    ] {
        let definition = MENUS
            .iter()
            .find(|definition| definition.id == menu)
            .unwrap();
        assert!(contains(definition.entries, id), "{menu} contains {id}");
    }
    assert!(!all_actions_contain("view.current_table"));
    let edit_menu = MENUS
        .iter()
        .find(|menu| menu.id == "menu.edit")
        .expect("the shared Edit menu exists");
    // Keep each clipboard dialog next to the ordinary command that it extends.
    let commands = [
        "edit.copy",
        "edit.copy_as",
        "edit.paste",
        "edit.paste_special",
    ];
    assert!(edit_menu.entries.windows(commands.len()).any(|group| {
        group
            .iter()
            .zip(commands)
            .all(|(entry, id)| matches!(entry, MenuEntry::Action(action) if action.id == id))
    }));
}

/// Keeps the native Undo accelerator connected to its shared Cmd/Ctrl+Z metadata.
#[test]
fn native_undo_uses_the_shared_command_z_shortcut() {
    let undo = super::accelerator(MenuShortcut::Command('Z'))
        .expect("the shared Undo shortcut has a native accelerator");
    assert_eq!(undo.key(), muda::accelerator::Code::KeyZ);
    assert_eq!(undo.modifiers(), muda::accelerator::CMD_OR_CTRL);
}

/// Keeps the filter toggle's shared shortcut on its native menu item.
#[test]
fn native_filter_uses_command_shift_l() {
    let filter = super::accelerator(MenuShortcut::CommandShift('L'))
        .expect("the shared Filter shortcut has a native accelerator");
    assert_eq!(filter.key(), muda::accelerator::Code::KeyL);
    assert!(
        filter
            .modifiers()
            .contains(muda::accelerator::Modifiers::SHIFT)
    );
}

/// Keeps each text-format menu accelerator connected to its shared shortcut metadata.
#[test]
fn native_format_accelerators_use_the_shared_format_shortcuts() {
    for (id, key, shift) in [
        ("format.bold", muda::accelerator::Code::KeyB, false),
        ("format.italic", muda::accelerator::Code::KeyI, false),
        ("format.code", muda::accelerator::Code::KeyE, false),
        ("format.strike", muda::accelerator::Code::KeyX, true),
        ("insert.rows", muda::accelerator::Code::KeyR, true),
        ("insert.columns", muda::accelerator::Code::KeyC, true),
    ] {
        let definition = action(id).expect("the format action has shared shortcut metadata");
        let accelerator = super::accelerator(definition.shortcut)
            .expect("the native adapter supports the assigned format key");
        let expected_modifiers = if shift {
            muda::accelerator::CMD_OR_CTRL | muda::accelerator::Modifiers::SHIFT
        } else {
            muda::accelerator::CMD_OR_CTRL
        };
        assert_eq!(accelerator.key(), key, "{id} uses the assigned native key");
        assert_eq!(
            accelerator.modifiers(),
            expected_modifiers,
            "{id} uses the assigned native modifiers"
        );
    }
}

/// Limits visible action rows at each menu depth.
fn menu_level_is_short(menu: &MenuTree) -> bool {
    entries_are_short(menu.entries)
}

/// Checks nested submenus and counts only action rows, not separators or child menus.
fn entries_are_short(entries: &'static [MenuEntry]) -> bool {
    let direct_actions = entries
        .iter()
        .filter(|entry| matches!(entry, MenuEntry::Action(_)))
        .count();
    direct_actions <= 15
        && entries.iter().all(|entry| match entry {
            MenuEntry::Submenu(submenu) => entries_are_short(submenu.entries),
            MenuEntry::Action(_)
            | MenuEntry::Separator
            | MenuEntry::RecentFiles
            | MenuEntry::WindowList => true,
        })
}

/// Searches all shared menus for a stable action ID.
fn all_actions_contain(id: &str) -> bool {
    MENUS.iter().any(|menu| contains(menu.entries, id))
}
