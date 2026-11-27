//! Keeps menu labels, action IDs, shortcuts, and checks in one tree.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MenuShortcut {
    None,
    Command(char),
    CommandShift(char),
    Replace,
    Redo,
    ZoomIn,
    Settings,
    /// Ctrl+Alt+V, or Cmd+Ctrl+V on macOS, like Excel's Paste Special.
    PasteSpecial,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MenuCheck {
    None,
    Radio,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MenuAction {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) shortcut: MenuShortcut,
    pub(crate) check: MenuCheck,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MenuSubmenu {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) entries: &'static [MenuEntry],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MenuEntry {
    Action(&'static MenuAction),
    Submenu(&'static MenuSubmenu),
    Separator,
    RecentFiles,
    WindowList,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MenuTree {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) entries: &'static [MenuEntry],
}

const fn action(
    id: &'static str,
    label: &'static str,
    shortcut: MenuShortcut,
    check: MenuCheck,
) -> MenuAction {
    MenuAction {
        id,
        label,
        shortcut,
        check,
    }
}

const fn submenu(
    id: &'static str,
    label: &'static str,
    entries: &'static [MenuEntry],
) -> MenuSubmenu {
    MenuSubmenu { id, label, entries }
}

#[path = "definitions/data.rs"]
mod data;
#[path = "definitions/edit.rs"]
mod edit;
#[path = "definitions/file.rs"]
mod file;
#[path = "definitions/format.rs"]
mod format;
#[path = "definitions/help.rs"]
mod help;
#[path = "definitions/insert.rs"]
mod insert;
#[path = "definitions/view.rs"]
mod view;
#[path = "definitions/window.rs"]
mod window;

use data::DATA;
use edit::EDIT;
use file::FILE;
use format::FORMAT;
use help::HELP;
use insert::INSERT;
use view::VIEW;
use window::WINDOW;

pub(crate) const MENUS: &[MenuTree] = &[
    MenuTree {
        id: "menu.file",
        label: "menu-file",
        entries: FILE,
    },
    MenuTree {
        id: "menu.edit",
        label: "menu-edit",
        entries: EDIT,
    },
    MenuTree {
        id: "menu.view",
        label: "menu-view",
        entries: VIEW,
    },
    MenuTree {
        id: "menu.insert",
        label: "menu-insert",
        entries: INSERT,
    },
    MenuTree {
        id: "menu.format",
        label: "menu-format",
        entries: FORMAT,
    },
    MenuTree {
        id: "menu.data",
        label: "menu-data",
        entries: DATA,
    },
    MenuTree {
        id: "menu.help",
        label: "menu-help",
        entries: HELP,
    },
    MenuTree {
        id: "menu.window",
        label: "menu-window",
        entries: WINDOW,
    },
];

#[cfg(target_os = "macos")]
pub(crate) const APP_SETTINGS_ACTION: &MenuAction = file::APP_SETTINGS_ACTION;
