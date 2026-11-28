use super::*;
use crate::command_menu::{
    MENUS, MenuAction, MenuCheck, MenuEntry, MenuShortcut, MenuSubmenu, MenuTree,
};
use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{IsMenuItem, PredefinedMenuItem};

enum Node {
    Item(String, MenuItem),
    Check(String, CheckMenuItem),
    Separator(PredefinedMenuItem),
    Submenu(Submenu),
}

impl Node {
    // Shares native menu nodes through the menu library's item interface.
    fn item(&self) -> &dyn IsMenuItem {
        match self {
            Self::Item(_, item) => item,
            Self::Check(_, item) => item,
            Self::Separator(item) => item,
            Self::Submenu(item) => item,
        }
    }

    // Keeps a live handle only for rows whose state changes later.
    fn handle(&self) -> Option<(String, Handle)> {
        match self {
            Self::Item(id, item) => Some((id.clone(), Handle::Item(item.clone()))),
            Self::Check(id, item) => Some((id.clone(), Handle::Check(item.clone()))),
            Self::Separator(_) | Self::Submenu(_) => None,
        }
    }
}

#[derive(Default)]
struct BuildState {
    handles: HashMap<String, Handle>,
    item_keys: HashMap<String, &'static str>,
    submenus: HashMap<String, Submenu>,
    recent: Option<Submenu>,
    window_list: Option<Submenu>,
}

// Builds a translated action from the shared menu definition.
fn action_item(language: crate::i18n::Language, action: &'static MenuAction) -> Node {
    let label = crate::i18n::text(language, action.label);
    let accelerator = accelerator(action.shortcut);
    if action.check != MenuCheck::None {
        Node::Check(
            action.id.to_owned(),
            CheckMenuItem::with_id(action.id, label, false, false, accelerator),
        )
    } else {
        Node::Item(
            action.id.to_owned(),
            MenuItem::with_id(action.id, label, false, accelerator),
        )
    }
}

// Builds nested rows and stores their labels and update handles.
fn entry_node(
    language: crate::i18n::Language,
    entry: MenuEntry,
    state: &mut BuildState,
) -> Result<Node, String> {
    match entry {
        MenuEntry::Action(action) => {
            let node = action_item(language, action);
            state.item_keys.insert(action.id.to_owned(), action.label);
            Ok(node)
        }
        MenuEntry::Separator => Ok(Node::Separator(PredefinedMenuItem::separator())),
        MenuEntry::RecentFiles => recent_submenu(language, state),
        MenuEntry::WindowList => window_list_submenu(language, state),
        MenuEntry::Submenu(submenu) => submenu_node(language, submenu, state),
    }
}

// Builds one shared submenu and records its label for language changes.
fn submenu_node(
    language: crate::i18n::Language,
    definition: &'static MenuSubmenu,
    state: &mut BuildState,
) -> Result<Node, String> {
    let nodes = definition
        .entries
        .iter()
        .copied()
        .map(|entry| entry_node(language, entry, state))
        .collect::<Result<Vec<_>, _>>()?;
    let items = nodes.iter().map(Node::item).collect::<Vec<_>>();
    let submenu = Submenu::with_items(crate::i18n::text(language, definition.label), true, &items)
        .map_err(|error| format!("could not create menu {}: {error}", definition.id))?;
    state
        .item_keys
        .insert(definition.id.to_owned(), definition.label);
    state
        .submenus
        .insert(definition.id.to_owned(), submenu.clone());
    state.handles.extend(nodes.iter().filter_map(Node::handle));
    Ok(Node::Submenu(submenu))
}

// Keeps the recent-files submenu replaceable after startup.
fn recent_submenu(language: crate::i18n::Language, state: &mut BuildState) -> Result<Node, String> {
    let empty = MenuItem::with_id(
        "file.recent.empty",
        crate::i18n::text(language, "menu-file-recent-empty"),
        false,
        None,
    );
    let items: [&dyn IsMenuItem; 1] = [&empty];
    let menu = Submenu::with_items(
        crate::i18n::text(language, "menu-file-open-recent"),
        true,
        &items,
    )
    .map_err(|error| format!("could not create the recent-files menu: {error}"))?;
    state
        .item_keys
        .insert("menu.file.recent".to_owned(), "menu-file-open-recent");
    state
        .submenus
        .insert("menu.file.recent".to_owned(), menu.clone());
    state.recent = Some(menu.clone());
    Ok(Node::Submenu(menu))
}

// Creates a replaceable child menu for the current open windows.
fn window_list_submenu(
    language: crate::i18n::Language,
    state: &mut BuildState,
) -> Result<Node, String> {
    let placeholder = MenuItem::with_id(
        "window.list.empty",
        crate::i18n::text(language, "window-list-empty"),
        false,
        None,
    );
    let items: [&dyn IsMenuItem; 1] = [&placeholder];
    let menu = Submenu::with_items(
        crate::i18n::text(language, "menu-window-open"),
        true,
        &items,
    )
    .map_err(|error| format!("could not create the window list menu: {error}"))?;
    state
        .item_keys
        .insert("menu.window.list".to_owned(), "menu-window-open");
    state
        .submenus
        .insert("menu.window.list".to_owned(), menu.clone());
    state.window_list = Some(menu.clone());
    Ok(Node::Submenu(menu))
}

// Applies platform shortcut differences from the shared key metadata.
fn accelerator(shortcut: MenuShortcut) -> Option<Accelerator> {
    if shortcut == MenuShortcut::PasteSpecial {
        let modifiers = if cfg!(target_os = "macos") {
            Modifiers::META | Modifiers::CONTROL
        } else {
            Modifiers::CONTROL | Modifiers::ALT
        };
        return Some(Accelerator::new(modifiers, Code::KeyV));
    }
    let (key, shift) = match shortcut {
        MenuShortcut::None => return None,
        MenuShortcut::Command('N') => (Code::KeyN, false),
        MenuShortcut::Command('O') => (Code::KeyO, false),
        MenuShortcut::Command('S') => (Code::KeyS, false),
        MenuShortcut::Command('W') => (Code::KeyW, false),
        MenuShortcut::Command('X') => (Code::KeyX, false),
        MenuShortcut::Command('C') => (Code::KeyC, false),
        MenuShortcut::Command('V') => (Code::KeyV, false),
        MenuShortcut::Command('A') => (Code::KeyA, false),
        MenuShortcut::Command('B') => (Code::KeyB, false),
        MenuShortcut::Command('E') => (Code::KeyE, false),
        MenuShortcut::Command('F') => (Code::KeyF, false),
        MenuShortcut::Command('I') => (Code::KeyI, false),
        MenuShortcut::Command('Z') => (Code::KeyZ, false),
        MenuShortcut::Command('D') => (Code::KeyD, false),
        MenuShortcut::Command('M') => (Code::KeyM, false),
        MenuShortcut::Command('-') => (Code::Minus, false),
        MenuShortcut::Command('0') => (Code::Digit0, false),
        MenuShortcut::Command('/') => (Code::Slash, false),
        MenuShortcut::CommandShift('S') => (Code::KeyS, true),
        MenuShortcut::CommandShift('Z') => (Code::KeyZ, true),
        MenuShortcut::CommandShift('D') => (Code::KeyD, true),
        MenuShortcut::CommandShift('N') => (Code::KeyN, true),
        MenuShortcut::CommandShift('L') => (Code::KeyL, true),
        MenuShortcut::CommandShift('X') => (Code::KeyX, true),
        MenuShortcut::CommandShift('V') => (Code::KeyV, true),
        MenuShortcut::CommandShift('R') => (Code::KeyR, true),
        MenuShortcut::CommandShift('C') => (Code::KeyC, true),
        MenuShortcut::Replace => (Code::KeyH, cfg!(target_os = "macos")),
        MenuShortcut::Redo if cfg!(target_os = "windows") => (Code::KeyY, false),
        MenuShortcut::Redo => (Code::KeyZ, true),
        MenuShortcut::ZoomIn => (Code::Equal, true),
        MenuShortcut::Settings if cfg!(target_os = "macos") => (Code::Comma, false),
        _ => return None,
    };
    let mut modifiers = muda::accelerator::CMD_OR_CTRL;
    if shift {
        modifiers |= Modifiers::SHIFT;
    }
    Some(Accelerator::new(modifiers, key))
}

// Builds one top-level menu and stores every leaf's update handle.
fn top_menu(
    language: crate::i18n::Language,
    definition: &'static MenuTree,
    state: &mut BuildState,
) -> Result<Submenu, String> {
    // macOS keeps Settings, Quit, and About in the application menu only.
    let entries = if cfg!(target_os = "macos") && matches!(definition.id, "menu.file" | "menu.help")
    {
        let mut entries = definition
            .entries
            .iter()
            .copied()
            .filter(|entry| {
                !matches!(entry, MenuEntry::Action(action)
                    if matches!(action.id, "app.settings" | "app.quit" | "help.about"))
            })
            .collect::<Vec<_>>();
        if entries.last() == Some(&MenuEntry::Separator) {
            entries.pop();
        }
        entries
    } else {
        definition.entries.to_vec()
    };
    let nodes = entries
        .into_iter()
        .map(|entry| entry_node(language, entry, state))
        .collect::<Result<Vec<_>, _>>()?;
    let items = nodes.iter().map(Node::item).collect::<Vec<_>>();
    let menu = Submenu::with_items(crate::i18n::text(language, definition.label), true, &items)
        .map_err(|error| format!("could not create menu {}: {error}", definition.id))?;
    state.handles.extend(nodes.iter().filter_map(Node::handle));
    state
        .item_keys
        .insert(definition.id.to_owned(), definition.label);
    state
        .submenus
        .insert(definition.id.to_owned(), menu.clone());
    Ok(menu)
}

// Joins menu bars in the order shared with the egui implementation.
fn root_menu(items: &[&dyn IsMenuItem]) -> Result<Menu, String> {
    Menu::with_items(items).map_err(|error| format!("could not create the native menu: {error}"))
}

/// The native menu bar with the lookups the app needs to update and route it.
pub(super) struct BuiltMenu {
    pub(super) menu: Menu,
    pub(super) handles: HashMap<String, Handle>,
    pub(super) item_keys: HashMap<String, &'static str>,
    pub(super) submenus: HashMap<String, Submenu>,
    pub(super) recent: Submenu,
    pub(super) window_list: Submenu,
}

/// Creates the native tree from the shared definitions.
pub(super) fn make_menu(language: crate::i18n::Language) -> Result<BuiltMenu, String> {
    let mut state = BuildState::default();
    #[cfg_attr(
        not(target_os = "macos"),
        expect(unused_mut, reason = "only macOS adds a menu")
    )]
    let mut menus = MENUS
        .iter()
        .map(|definition| top_menu(language, definition, &mut state))
        .collect::<Result<Vec<_>, _>>()?;
    #[cfg(target_os = "macos")]
    menus.insert(0, application_submenu(language, &mut state)?);
    let items = menus
        .iter()
        .map(|menu| menu as &dyn IsMenuItem)
        .collect::<Vec<_>>();
    let menu = root_menu(&items)?;
    let recent = state
        .recent
        .ok_or_else(|| "the shared File menu has no recent-files submenu".to_owned())?;
    let window_list = state
        .window_list
        .ok_or_else(|| "the shared Window menu has no window list".to_owned())?;
    Ok(BuiltMenu {
        menu,
        handles: state.handles,
        item_keys: state.item_keys,
        submenus: state.submenus,
        recent,
        window_list,
    })
}

#[cfg(target_os = "macos")]
// Adds the app's shared Settings action around native system commands.
fn application_submenu(
    language: crate::i18n::Language,
    state: &mut BuildState,
) -> Result<Submenu, String> {
    let about_metadata = muda::AboutMetadata {
        name: Some("Texcel".to_owned()),
        version: Some(env!("CARGO_PKG_VERSION").to_owned()),
        // The same value for the build number makes AppKit show "Version 0.3.0" without a
        // parenthesized build after it.
        short_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
        copyright: Some(crate::ui::APP_COPYRIGHT.to_owned()),
        ..Default::default()
    };
    let about = PredefinedMenuItem::about(
        Some(&crate::i18n::text(language, "menu-app-about")),
        Some(about_metadata),
    );
    let settings = action_item(language, crate::command_menu::APP_SETTINGS_ACTION);
    state.handles.extend(settings.handle());
    state.item_keys.insert(
        "app.settings".to_owned(),
        crate::command_menu::APP_SETTINGS_ACTION.label,
    );
    let services =
        PredefinedMenuItem::services(Some(&crate::i18n::text(language, "menu-app-services")));
    let hide = PredefinedMenuItem::hide(Some(&crate::i18n::text(language, "menu-app-hide")));
    let hide_others =
        PredefinedMenuItem::hide_others(Some(&crate::i18n::text(language, "menu-app-hide-others")));
    let show_all =
        PredefinedMenuItem::show_all(Some(&crate::i18n::text(language, "menu-app-show-all")));
    let quit = PredefinedMenuItem::quit(Some(&crate::i18n::text(language, "menu-app-quit")));
    for (id, key, item) in [
        ("localized.app.about", "menu-app-about", &about),
        ("localized.app.services", "menu-app-services", &services),
        ("localized.app.hide", "menu-app-hide", &hide),
        (
            "localized.app.hide_others",
            "menu-app-hide-others",
            &hide_others,
        ),
        ("localized.app.show_all", "menu-app-show-all", &show_all),
        ("localized.app.quit", "menu-app-quit", &quit),
    ] {
        state
            .handles
            .insert(id.to_owned(), Handle::Predefined(item.clone()));
        state.item_keys.insert(id.to_owned(), key);
    }
    let separator_one = PredefinedMenuItem::separator();
    let separator_two = PredefinedMenuItem::separator();
    let separator_three = PredefinedMenuItem::separator();
    let items: [&dyn IsMenuItem; 10] = [
        &about,
        settings.item(),
        &separator_one,
        &services,
        &separator_two,
        &hide,
        &hide_others,
        &show_all,
        &separator_three,
        &quit,
    ];
    Submenu::with_items("Texcel", true, &items)
        .map_err(|error| format!("could not create the Texcel application menu: {error}"))
}

#[cfg(test)]
#[path = "build_test.rs"]
mod tests;
