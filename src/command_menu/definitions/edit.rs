use super::{MenuAction, MenuCheck, MenuEntry, MenuShortcut, MenuSubmenu, action, submenu};
use MenuEntry::{Action as A, Separator as Sep, Submenu as S};
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
const DELETE_ROWS: MenuAction = action("edit.delete_rows", "delete-row", NoKey, MenuCheck::None);
const DUPLICATE_ROWS: MenuAction = action(
    "edit.duplicate_rows",
    "duplicate-row",
    NoKey,
    MenuCheck::None,
);
const MOVE_ROWS_UP: MenuAction = action("edit.move_rows_up", "move-row-up", NoKey, MenuCheck::None);
const MOVE_ROWS_DOWN: MenuAction = action(
    "edit.move_rows_down",
    "move-row-down",
    NoKey,
    MenuCheck::None,
);
const DELETE_COLUMNS: MenuAction = action(
    "edit.delete_columns",
    "delete-column",
    NoKey,
    MenuCheck::None,
);
const DUPLICATE_COLUMNS: MenuAction = action(
    "edit.duplicate_columns",
    "duplicate-column",
    NoKey,
    MenuCheck::None,
);
const MOVE_COLUMNS_LEFT: MenuAction = action(
    "edit.move_columns_left",
    "move-column-left",
    NoKey,
    MenuCheck::None,
);
const MOVE_COLUMNS_RIGHT: MenuAction = action(
    "edit.move_columns_right",
    "move-column-right",
    NoKey,
    MenuCheck::None,
);
const FILL_COPY: MenuAction = action("edit.fill_copy", "insert-fill-copy", NoKey, MenuCheck::None);
const FILL_SERIES: MenuAction = action(
    "edit.fill_series",
    "insert-fill-series",
    NoKey,
    MenuCheck::None,
);
const TABLE_ADD: MenuAction = action("edit.table_add", "table-add", Shift('N'), MenuCheck::None);
const TABLE_DUPLICATE: MenuAction = action(
    "edit.table_duplicate",
    "table-duplicate",
    Shift('D'),
    MenuCheck::None,
);
const TABLE_DELETE: MenuAction =
    action("edit.table_delete", "table-delete", NoKey, MenuCheck::None);

const ROWS: MenuSubmenu = submenu(
    "menu.edit.rows",
    "menu-edit-rows",
    &[
        A(&DELETE_ROWS),
        A(&DUPLICATE_ROWS),
        A(&MOVE_ROWS_UP),
        A(&MOVE_ROWS_DOWN),
    ],
);
const COLUMNS: MenuSubmenu = submenu(
    "menu.edit.columns",
    "menu-edit-columns",
    &[
        A(&DELETE_COLUMNS),
        A(&DUPLICATE_COLUMNS),
        A(&MOVE_COLUMNS_LEFT),
        A(&MOVE_COLUMNS_RIGHT),
    ],
);
const FILL: MenuSubmenu = submenu(
    "menu.edit.fill",
    "menu-edit-fill",
    &[A(&FILL_COPY), A(&FILL_SERIES)],
);
const TABLES: MenuSubmenu = submenu(
    "menu.edit.tables",
    "menu-edit-tables",
    &[A(&TABLE_ADD), A(&TABLE_DUPLICATE), A(&TABLE_DELETE)],
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
    Sep,
    S(&ROWS),
    S(&COLUMNS),
    S(&FILL),
    S(&TABLES),
];
