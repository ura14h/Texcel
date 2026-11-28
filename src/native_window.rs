//! Tells Texcel's native windows apart by the role each was created for.
//!
//! eframe creates a viewport's native window right after the pass that first registers it,
//! and hands out no handle for child windows. The window host therefore registers at most
//! one new viewport per pass and announces its role here first. The platform code then
//! stores that role on the next new native window it sees, as an identifier that lives and
//! dies with the window: the NSWindow identifier on macOS, a window property on Windows.

use std::sync::Mutex;

/// What a native window shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowRole {
    Document,
    Help,
    Progress,
}

#[cfg(target_os = "macos")]
impl WindowRole {
    /// The stable name stored on the native window.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Document => "document",
            Self::Help => "help",
            Self::Progress => "progress",
        }
    }

    /// Reads a role back from its stored name.
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        [Self::Document, Self::Help, Self::Progress]
            .into_iter()
            .find(|role| role.name() == name)
    }
}

/// The role of the window eframe is about to create, until the platform code takes it.
static EXPECTED: Mutex<Option<WindowRole>> = Mutex::new(None);

/// Tells whether native windows are identified at all. Tests and Linux create no native
/// window menus to tell apart, so their viewports need not wait for one another.
pub(crate) fn identification_active() -> bool {
    cfg!(all(
        not(test),
        any(target_os = "macos", target_os = "windows")
    ))
}

/// Announces the role of the one window created after this pass.
pub(crate) fn expect_new_window(role: WindowRole) {
    if identification_active() {
        *EXPECTED.lock().unwrap_or_else(|poison| poison.into_inner()) = Some(role);
    }
}

/// Tells whether an announced window has not been identified yet.
pub(crate) fn new_window_pending() -> bool {
    EXPECTED
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .is_some()
}

/// Takes the announced role for a newly seen native window.
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) fn take_expected_role() -> Option<WindowRole> {
    EXPECTED
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .take()
}

/// Withdraws an announcement whose viewport was dropped before its window was created.
pub(crate) fn cancel_expected_window() {
    *EXPECTED.lock().unwrap_or_else(|poison| poison.into_inner()) = None;
}
