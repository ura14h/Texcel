//! Texcel desktop editor: typed source data, common document commands, and UI state.

pub(crate) mod calc;
pub(crate) mod clipboard;
pub(crate) mod command_menu;
pub(crate) mod conversion_report;
pub(crate) mod csv_adapter;
pub(crate) mod document;
pub(crate) mod export;
pub(crate) mod file_worker;
pub(crate) mod html_adapter;
pub(crate) mod i18n;
#[cfg(target_os = "macos")]
mod macos_app;
pub(crate) mod markdown_adapter;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod native_menu;
pub(crate) mod numeric_text;
pub(crate) mod ods_adapter;
pub(crate) mod recovery;
pub(crate) mod settings;
pub(crate) mod source;
pub(crate) mod structure;
pub mod ui;
pub(crate) mod xlsx_adapter;

/// Registers the pre-launch AppKit observer for Finder document-open events.
#[cfg(target_os = "macos")]
pub fn prepare_macos_file_open() {
    macos_app::register_open_file_observer();
}

/// Routes Windows menu accelerators through muda before winit handles keyboard input.
#[cfg(target_os = "windows")]
pub fn translate_windows_menu_message(message: *const std::ffi::c_void) -> bool {
    native_menu::translate_windows_message(message)
}
