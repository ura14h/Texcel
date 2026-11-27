use super::{MenuAction, MenuCheck, MenuEntry, MenuShortcut, action};
use MenuEntry::{Action as A, Separator as Sep};
use MenuShortcut::{Command as Cmd, CommandShift as Shift, None as NoKey};

const EDIT_UNDO: MenuAction = action("edit.undo", "top-undo", Cmd('Z'), MenuCheck::None);
const EDIT_REDO: MenuAction = action("edit.redo", "top-redo", MenuShortcut::Redo, MenuCheck::None);
const EDIT_CUT: MenuAction = action("edit.cut", "action-cut", Cmd('X'), MenuCheck::None);
const EDIT_COPY: MenuAction = action("edit.copy", "action-copy", Cmd('C'), MenuCheck::None);
const EDIT_PASTE: MenuAction = action("edit.paste", "action-paste", Cmd('V'), MenuCheck::None);
const EDIT_PASTE_TEXT: MenuAction = action(
    "edit.paste_text",
    "action-paste-text",
    Shift('V'),
    MenuCheck::None,
);
const EDIT_CLEAR: MenuAction = action("edit.clear", "action-clear-cells", NoKey, MenuCheck::None);
const EDIT_SELECT_ALL: MenuAction = action(
    "edit.select_all",
    "action-select-all",
    Cmd('A'),
    MenuCheck::None,
);
const EDIT_SEARCH: MenuAction =
    action("edit.search", "menu-edit-search", Cmd('F'), MenuCheck::None);
const EDIT_REPLACE: MenuAction = action(
    "edit.replace",
    "menu-edit-replace",
    MenuShortcut::Replace,
    MenuCheck::None,
);
const EDIT_COPY_AS: MenuAction =
    action("edit.copy_as", "menu-edit-copy-as", NoKey, MenuCheck::None);
const PASTE_SPECIAL: MenuAction = action(
    "edit.paste_special",
    "top-paste-special-open",
    MenuShortcut::PasteSpecial,
    MenuCheck::None,
);
pub(super) const EDIT: &[MenuEntry] = &[
    A(&EDIT_UNDO),
    A(&EDIT_REDO),
    Sep,
    A(&EDIT_CUT),
    A(&EDIT_COPY),
    A(&EDIT_COPY_AS),
    A(&EDIT_PASTE),
    A(&PASTE_SPECIAL),
    A(&EDIT_PASTE_TEXT),
    A(&EDIT_CLEAR),
    A(&EDIT_SELECT_ALL),
    Sep,
    A(&EDIT_SEARCH),
    A(&EDIT_REPLACE),
];
