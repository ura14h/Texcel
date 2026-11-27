use super::{MenuAction, MenuCheck, MenuEntry, MenuShortcut, MenuSubmenu, action, submenu};
use MenuEntry::{Action as A, Separator as Sep, Submenu as S};
use MenuShortcut::{CommandShift as Shift, None as NoKey};

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
    "menu.table.rows",
    "menu-table-rows",
    &[
        A(&DELETE_ROWS),
        A(&DUPLICATE_ROWS),
        A(&MOVE_ROWS_UP),
        A(&MOVE_ROWS_DOWN),
    ],
);
const COLUMNS: MenuSubmenu = submenu(
    "menu.table.columns",
    "menu-table-columns",
    &[
        A(&DELETE_COLUMNS),
        A(&DUPLICATE_COLUMNS),
        A(&MOVE_COLUMNS_LEFT),
        A(&MOVE_COLUMNS_RIGHT),
    ],
);
const FILL: MenuSubmenu = submenu(
    "menu.table.fill",
    "menu-table-fill",
    &[A(&FILL_COPY), A(&FILL_SERIES)],
);

// The table actions sit directly in this menu so it never reads "Table > Tables".
pub(super) const TABLE: &[MenuEntry] = &[
    S(&ROWS),
    S(&COLUMNS),
    S(&FILL),
    Sep,
    A(&TABLE_ADD),
    A(&TABLE_DUPLICATE),
    A(&TABLE_DELETE),
];
