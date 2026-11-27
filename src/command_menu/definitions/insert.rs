use super::{MenuAction, MenuCheck, MenuEntry, MenuShortcut, action};
use MenuEntry::Action as A;
use MenuShortcut::CommandShift as Shift;

// R for rows and C for columns; plain Ctrl/Cmd+R and +C already fill right and copy.
const INSERT_ROWS: MenuAction = action("insert.rows", "insert-row", Shift('R'), MenuCheck::None);
const INSERT_COLUMNS: MenuAction = action(
    "insert.columns",
    "insert-column",
    Shift('C'),
    MenuCheck::None,
);

pub(super) const INSERT: &[MenuEntry] = &[A(&INSERT_ROWS), A(&INSERT_COLUMNS)];
