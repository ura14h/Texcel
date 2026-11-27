use super::{MenuAction, MenuCheck, MenuEntry, MenuShortcut, action};
use MenuEntry::{Action as A, Separator as Sep};
use MenuShortcut::None as NoKey;

const HELP_OPEN: MenuAction = action("help.open", "menu-help-open", NoKey, MenuCheck::None);
/// macOS shows this in the application menu instead, as the standard About panel.
const HELP_ABOUT: MenuAction = action("help.about", "menu-app-about", NoKey, MenuCheck::None);

pub(super) const HELP: &[MenuEntry] = &[A(&HELP_OPEN), Sep, A(&HELP_ABOUT)];
