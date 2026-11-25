use super::{MenuAction, MenuCheck, MenuEntry, MenuShortcut, MenuSubmenu, action, submenu};
use MenuCheck::Radio;
use MenuEntry::{Action as A, Separator as Sep, Submenu as S};
use MenuShortcut::{Command as Cmd, None as NoKey};

const VIEW_LIVE: MenuAction = action("view.live", "view-live", NoKey, Radio);
const VIEW_SOURCE: MenuAction = action("view.source", "view-source", NoKey, Radio);
const VIEW_ZOOM_IN: MenuAction = action(
    "view.zoom_in",
    "view-zoom-in",
    MenuShortcut::ZoomIn,
    MenuCheck::None,
);
const VIEW_ZOOM_OUT: MenuAction =
    action("view.zoom_out", "view-zoom-out", Cmd('-'), MenuCheck::None);
const VIEW_ZOOM_RESET: MenuAction = action(
    "view.zoom_reset",
    "view-zoom-reset",
    Cmd('0'),
    MenuCheck::None,
);
const VIEW_WRAP_ON: MenuAction = action("view.wrap_on", "settings-table-wrap-on", NoKey, Radio);
const VIEW_WRAP_OFF: MenuAction = action("view.wrap_off", "settings-table-wrap-off", NoKey, Radio);
const VIEW_COLUMN_WIDTH: MenuAction = action(
    "view.column_width",
    "view-column-width",
    NoKey,
    MenuCheck::None,
);
const VIEW_AUTOFIT_COLUMNS: MenuAction = action(
    "view.autofit_columns",
    "view-autofit-columns",
    NoKey,
    MenuCheck::None,
);
const VIEW_FREEZE_ROWS: MenuAction = action(
    "view.freeze_rows",
    "view-freeze-rows",
    NoKey,
    MenuCheck::None,
);
const VIEW_FREEZE_COLUMNS: MenuAction = action(
    "view.freeze_columns",
    "view-freeze-columns",
    NoKey,
    MenuCheck::None,
);
const VIEW_UNFREEZE: MenuAction = action("view.unfreeze", "view-unfreeze", NoKey, MenuCheck::None);

const ZOOM: MenuSubmenu = submenu(
    "menu.view.zoom",
    "menu-view-zoom",
    &[A(&VIEW_ZOOM_IN), A(&VIEW_ZOOM_OUT), A(&VIEW_ZOOM_RESET)],
);
const WRAP: MenuSubmenu = submenu(
    "menu.view.wrap",
    "menu-view-wrap",
    &[A(&VIEW_WRAP_ON), A(&VIEW_WRAP_OFF)],
);
const FREEZE: MenuSubmenu = submenu(
    "menu.view.freeze",
    "menu-view-freeze",
    &[
        A(&VIEW_FREEZE_ROWS),
        A(&VIEW_FREEZE_COLUMNS),
        A(&VIEW_UNFREEZE),
    ],
);

pub(super) const VIEW: &[MenuEntry] = &[
    A(&VIEW_LIVE),
    A(&VIEW_SOURCE),
    Sep,
    S(&ZOOM),
    S(&WRAP),
    Sep,
    A(&VIEW_COLUMN_WIDTH),
    A(&VIEW_AUTOFIT_COLUMNS),
    S(&FREEZE),
];
