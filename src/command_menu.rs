//! Defines the shared menu tree used by native and in-window menus.

#[cfg(any(test, not(any(target_os = "macos", target_os = "windows"))))]
use eframe::egui;

#[path = "command_menu/definitions.rs"]
mod definitions;
#[cfg(target_os = "macos")]
pub(crate) use definitions::APP_SETTINGS_ACTION;
pub(crate) use definitions::{
    MENUS, MenuAction, MenuCheck, MenuEntry, MenuShortcut, MenuSubmenu, MenuTree,
};

/// Returns every stable action ID in menu order.
pub(crate) fn action_ids() -> Vec<&'static str> {
    let mut ids = Vec::new();
    for menu in MENUS {
        collect_action_ids(menu.entries, &mut ids);
    }
    ids
}

/// Finds shortcut metadata by stable action ID.
pub(crate) fn action(id: &str) -> Option<&'static MenuAction> {
    MENUS.iter().find_map(|menu| find_action(menu.entries, id))
}

/// Formats an assigned shortcut with the current platform's key names.
#[cfg(any(test, not(any(target_os = "macos", target_os = "windows"))))]
pub(crate) fn shortcut_text(ctx: &egui::Context, shortcut: MenuShortcut) -> String {
    use egui::Key;
    let (key, shift) = match shortcut {
        MenuShortcut::None | MenuShortcut::Settings if !ctx.os().is_mac() => return String::new(),
        MenuShortcut::None => return String::new(),
        MenuShortcut::Settings => (Key::Comma, false),
        MenuShortcut::Replace => (Key::H, ctx.os().is_mac()),
        MenuShortcut::Redo if !ctx.os().is_mac() => (Key::Y, false),
        MenuShortcut::Redo => (Key::Z, true),
        MenuShortcut::ZoomIn => {
            return key_symbols(ctx.format_shortcut(&egui::gui_zoom::kb_shortcuts::ZOOM_IN));
        }
        MenuShortcut::PasteSpecial => {
            let modifiers = if ctx.os().is_mac() {
                egui::Modifiers::MAC_CMD | egui::Modifiers::CTRL
            } else {
                egui::Modifiers::CTRL | egui::Modifiers::ALT
            };
            return ctx.format_shortcut(&egui::KeyboardShortcut::new(modifiers, Key::V));
        }
        MenuShortcut::Command(character) => {
            let Some(key) = key_for_character(character) else {
                return String::new();
            };
            (key, false)
        }
        MenuShortcut::CommandShift(character) => {
            let Some(key) = key_for_character(character) else {
                return String::new();
            };
            (key, true)
        }
    };
    let modifiers = egui::Modifiers::COMMAND
        | if shift {
            egui::Modifiers::SHIFT
        } else {
            egui::Modifiers::NONE
        };
    key_symbols(ctx.format_shortcut(&egui::KeyboardShortcut::new(modifiers, key)))
}

/// Shows punctuation keys as their symbols, as menus usually do: "Ctrl+/" rather than
/// egui's "Ctrl+Slash".
#[cfg(any(test, not(any(target_os = "macos", target_os = "windows"))))]
fn key_symbols(text: String) -> String {
    for (name, symbol) in [("Slash", "/"), ("Plus", "+"), ("Minus", "-")] {
        if let Some(prefix) = text.strip_suffix(name) {
            return format!("{prefix}{symbol}");
        }
    }
    text
}

/// Walks nested entries while preserving the order shown to users.
fn collect_action_ids(entries: &'static [MenuEntry], ids: &mut Vec<&'static str>) {
    for entry in entries {
        match entry {
            MenuEntry::Action(action) => ids.push(action.id),
            MenuEntry::Submenu(submenu) => collect_action_ids(submenu.entries, ids),
            MenuEntry::Separator | MenuEntry::RecentFiles | MenuEntry::WindowList => (),
        }
    }
}

/// Searches one menu level and its nested submenus.
fn find_action(entries: &'static [MenuEntry], id: &str) -> Option<&'static MenuAction> {
    entries.iter().find_map(|entry| match entry {
        MenuEntry::Action(action) if action.id == id => Some(*action),
        MenuEntry::Submenu(submenu) => find_action(submenu.entries, id),
        MenuEntry::Action(_)
        | MenuEntry::Separator
        | MenuEntry::RecentFiles
        | MenuEntry::WindowList => None,
    })
}

/// Maps supported shortcut characters without inventing a fallback key.
#[cfg(any(test, not(any(target_os = "macos", target_os = "windows"))))]
fn key_for_character(character: char) -> Option<egui::Key> {
    match character {
        'N' => Some(egui::Key::N),
        'O' => Some(egui::Key::O),
        'S' => Some(egui::Key::S),
        'B' => Some(egui::Key::B),
        'I' => Some(egui::Key::I),
        'E' => Some(egui::Key::E),
        'L' => Some(egui::Key::L),
        'M' => Some(egui::Key::M),
        'Z' => Some(egui::Key::Z),
        'W' => Some(egui::Key::W),
        'X' => Some(egui::Key::X),
        'C' => Some(egui::Key::C),
        'V' => Some(egui::Key::V),
        'A' => Some(egui::Key::A),
        'F' => Some(egui::Key::F),
        'D' => Some(egui::Key::D),
        'R' => Some(egui::Key::R),
        '-' => Some(egui::Key::Minus),
        '0' => Some(egui::Key::Num0),
        '/' => Some(egui::Key::Slash),
        _ => None,
    }
}

#[cfg(test)]
#[path = "command_menu_test.rs"]
mod tests;
