//! Owns the macOS and Windows menu and forwards its actions to the app loop.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::PathBuf;
#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Mutex, OnceLock};

use eframe::egui::Context;
use muda::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, Submenu};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum NativeInputOwner {
    Grid,
    CellEditor,
    OtherText,
    Ime,
    Modal,
    #[default]
    Chrome,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct NativeActionTarget {
    pub(crate) document_id: Option<u64>,
    pub(crate) viewport: Option<eframe::egui::ViewportId>,
    pub(crate) table_id: Option<u64>,
    pub(crate) ranges: Vec<crate::document::CellRange>,
    pub(crate) active_cell: Option<crate::document::CellAddress>,
    pub(crate) owner: NativeInputOwner,
    pub(crate) focus: Option<eframe::egui::Id>,
    pub(crate) edit_session: Option<(u64, u64, Option<crate::document::CellAddress>)>,
    pub(crate) modal_session: Option<(u8, u64, u64)>,
}

#[path = "native_menu/build.rs"]
mod build;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ItemState {
    pub(crate) enabled: bool,
    pub(crate) checked: Option<bool>,
    pub(crate) label: Option<String>,
}

#[derive(Clone)]
enum Handle {
    Item(MenuItem),
    Check(CheckMenuItem),
    /// Only the macOS application menu holds system-provided items.
    #[cfg(target_os = "macos")]
    Predefined(muda::PredefinedMenuItem),
}

#[derive(Clone, Debug)]
pub(crate) struct NativeMenuEvent {
    pub(crate) id: String,
    pub(crate) recent_path: Option<PathBuf>,
    pub(crate) target: NativeActionTarget,
}

static MENU_EVENTS: OnceLock<Mutex<VecDeque<NativeMenuEvent>>> = OnceLock::new();
static APP_CONTEXT: OnceLock<Mutex<Option<Context>>> = OnceLock::new();
static RECENT_PATHS: OnceLock<Mutex<HashMap<String, PathBuf>>> = OnceLock::new();
static SYNCED_TARGET: OnceLock<Mutex<NativeActionTarget>> = OnceLock::new();
#[cfg(target_os = "windows")]
static WINDOW_ACCELERATOR: AtomicIsize = AtomicIsize::new(0);

pub(crate) struct NativeMenu {
    menu: Menu,
    handles: HashMap<String, Handle>,
    item_keys: HashMap<String, &'static str>,
    submenus: HashMap<String, Submenu>,
    last_states: BTreeMap<String, ItemState>,
    language: crate::i18n::Language,
    recent_menu: Submenu,
    shown_recent: Vec<PathBuf>,
    window_list_menu: Submenu,
    shown_windows: Vec<(u64, String, bool)>,
}

impl NativeMenu {
    /// Creates the OS menu after eframe has installed its native window.
    pub(crate) fn new(
        creation: &eframe::CreationContext<'_>,
        language: crate::i18n::Language,
    ) -> Result<Self, String> {
        install_event_handler(creation.egui_ctx.clone());
        let build::BuiltMenu {
            menu,
            handles,
            item_keys,
            submenus,
            recent: recent_menu,
            window_list: window_list_menu,
        } = build::make_menu(language)?;
        #[cfg(target_os = "macos")]
        {
            menu.init_for_nsapp();
            window_list_menu.set_as_windows_menu_for_nsapp();
        }
        #[cfg(target_os = "windows")]
        {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            let handle = creation
                .window_handle()
                .map_err(|error| format!("could not read the native window: {error}"))?;
            let RawWindowHandle::Win32(handle) = handle.as_raw() else {
                return Err("the Windows window did not provide a Win32 handle".to_owned());
            };
            // SAFETY: eframe owns this live Win32 window for the app lifetime.
            let hwnd = handle.hwnd.get();
            unsafe { menu.init_for_hwnd(hwnd) }
                .map_err(|error| format!("could not attach the native menu: {error}"))?;
            WINDOW_ACCELERATOR.store(menu.haccel(), Ordering::Release);
        }
        Ok(Self {
            menu,
            handles,
            item_keys,
            submenus,
            last_states: BTreeMap::new(),
            language,
            recent_menu,
            shown_recent: Vec::new(),
            window_list_menu,
            shown_windows: Vec::new(),
        })
    }

    /// Copies queued OS menu events into the current egui update.
    pub(crate) fn take_events(&self) -> Vec<NativeMenuEvent> {
        let mut events = event_queue()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        events.drain(..).collect()
    }

    /// Removes only events captured for the document drawn by this viewport.
    pub(crate) fn take_events_for_document(
        &self,
        document_id: Option<u64>,
    ) -> Vec<NativeMenuEvent> {
        let mut queue = event_queue()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        take_events_for_document(&mut queue, document_id)
    }

    /// Returns a shared handle to the menu so a window can be attached after app state is released.
    #[cfg(target_os = "windows")]
    pub(crate) fn windows_menu(&self) -> Menu {
        self.menu.clone()
    }

    /// Changes only menu labels and checks that differ from the last app state.
    pub(crate) fn sync(
        &mut self,
        language: crate::i18n::Language,
        states: &BTreeMap<String, ItemState>,
        recent_paths: &VecDeque<PathBuf>,
        target: NativeActionTarget,
        windows: &[(u64, String, bool)],
    ) -> Result<(), String> {
        self.sync_shared_lists(language, recent_paths, windows, true)?;
        *SYNCED_TARGET
            .get_or_init(|| Mutex::new(NativeActionTarget::default()))
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = target;
        let language_changed = language != self.language;
        self.language = language;
        for (id, handle) in &self.handles {
            let Some(state) = states.get(id) else {
                continue;
            };
            if self
                .last_states
                .get(id)
                .is_none_or(|old| old.enabled != state.enabled)
            {
                set_enabled(handle, state.enabled);
            }
            if language_changed
                || self.last_states.get(id).and_then(|old| old.label.as_ref())
                    != state.label.as_ref()
            {
                let text = state.label.as_ref().map_or_else(
                    || {
                        self.item_keys
                            .get(id)
                            .map_or_else(String::new, |key| crate::i18n::text(language, key))
                    },
                    Clone::clone,
                );
                set_text(handle, &text);
            }
            if let (Handle::Check(item), Some(checked)) = (handle, state.checked) {
                sync_checked_item(item, checked);
            }
        }
        if language_changed {
            for (id, submenu) in &self.submenus {
                if let Some(key) = self.item_keys.get(id) {
                    submenu.set_text(crate::i18n::text(language, key));
                }
            }
            #[cfg(target_os = "macos")]
            for (id, handle) in &self.handles {
                if let Handle::Predefined(item) = handle
                    && let Some(key) = self.item_keys.get(id)
                {
                    item.set_text(crate::i18n::text(language, key));
                }
            }
        }
        self.last_states.clone_from(states);
        let _ = &self.menu;
        Ok(())
    }

    /// Keeps app-wide menu lists current without changing the focused command target.
    pub(crate) fn sync_shared_lists(
        &mut self,
        language: crate::i18n::Language,
        recent_paths: &VecDeque<PathBuf>,
        windows: &[(u64, String, bool)],
        focused_window_state: bool,
    ) -> Result<(), String> {
        let recent_paths = recent_paths.iter().cloned().collect::<Vec<_>>();
        if self.shown_recent != recent_paths
            || (language != self.language && recent_paths.is_empty())
        {
            self.replace_recent_items(&recent_paths, language)?;
        }
        let windows = shared_window_rows(windows, &self.shown_windows, focused_window_state);
        if self.shown_windows != windows || (language != self.language && windows.is_empty()) {
            self.replace_window_items(&windows, language)?;
        }
        Ok(())
    }

    /// Keeps recent menu rows and event paths in the same order.
    fn replace_recent_items(
        &mut self,
        paths: &[PathBuf],
        language: crate::i18n::Language,
    ) -> Result<(), String> {
        while !self.recent_menu.items().is_empty() {
            self.recent_menu.remove_at(0);
        }
        self.handles.retain(|id, _| !id.starts_with("file.recent."));
        self.last_states
            .retain(|id, _| !id.starts_with("file.recent."));
        let mut binding = HashMap::new();
        if paths.is_empty() {
            let text = crate::i18n::text(language, "menu-file-recent-empty");
            let item = MenuItem::with_id("file.recent.empty", text, false, None);
            self.recent_menu
                .append(&item)
                .map_err(|error| format!("could not update the recent-files menu: {error}"))?;
        } else {
            for (index, path) in paths.iter().enumerate() {
                let id = format!("file.recent.{index}");
                let label = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                );
                let item = MenuItem::with_id(MenuId::new(id.as_str()), label, true, None);
                self.recent_menu
                    .append(&item)
                    .map_err(|error| format!("could not update the recent-files menu: {error}"))?;
                self.handles.insert(id.clone(), Handle::Item(item));
                binding.insert(id, path.clone());
            }
        }
        *RECENT_PATHS
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = binding;
        self.shown_recent = paths.to_vec();
        Ok(())
    }

    /// Rebuilds only the dynamic rows that activate open document windows.
    fn replace_window_items(
        &mut self,
        windows: &[(u64, String, bool)],
        language: crate::i18n::Language,
    ) -> Result<(), String> {
        while !self.window_list_menu.items().is_empty() {
            self.window_list_menu.remove_at(0);
        }
        if windows.is_empty() {
            let item = MenuItem::with_id(
                "window.list.empty",
                crate::i18n::text(language, "window-list-empty"),
                false,
                None,
            );
            self.window_list_menu
                .append(&item)
                .map_err(|error| format!("could not update the window list menu: {error}"))?;
        } else {
            for (document_id, label, active) in windows {
                let id = format!("window.activate.{document_id}");
                let item = CheckMenuItem::with_id(&id, label, true, *active, None);
                self.window_list_menu
                    .append(&item)
                    .map_err(|error| format!("could not update the window list menu: {error}"))?;
            }
        }
        self.shown_windows = windows.to_vec();
        Ok(())
    }
}

/// Preserves the active mark while a background viewport refreshes shared rows.
fn shared_window_rows(
    windows: &[(u64, String, bool)],
    shown_windows: &[(u64, String, bool)],
    focused_window_state: bool,
) -> Vec<(u64, String, bool)> {
    if focused_window_state {
        return windows.to_vec();
    }
    let active_document = shown_windows
        .iter()
        .find_map(|(id, _, active)| active.then_some(*id));
    windows
        .iter()
        .map(|(id, label, _)| (*id, label.clone(), Some(*id) == active_document))
        .collect()
}

/// Keeps the menu callback small and asks the app to repaint only after an action.
fn install_event_handler(context: Context) {
    let context_slot = APP_CONTEXT.get_or_init(|| Mutex::new(None));
    *context_slot
        .lock()
        .unwrap_or_else(|poison| poison.into_inner()) = Some(context);
    let _ = MENU_EVENTS.get_or_init(|| Mutex::new(VecDeque::new()));
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        MenuEvent::set_event_handler(Some(|event: MenuEvent| {
            let id = event.id.0;
            let recent_path = RECENT_PATHS
                .get()
                .and_then(|paths| paths.lock().ok())
                .and_then(|paths| paths.get(&id).cloned());
            let target = SYNCED_TARGET
                .get()
                .and_then(|slot| slot.lock().ok())
                .map(|target| target.clone())
                .unwrap_or_default();
            let repaint_viewport = target.viewport;
            event_queue()
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .push_back(NativeMenuEvent {
                    id,
                    recent_path,
                    target,
                });
            if let Some(context) = APP_CONTEXT
                .get()
                .and_then(|slot| slot.lock().ok())
                .and_then(|slot| slot.as_ref().cloned())
            {
                if let Some(viewport) = repaint_viewport {
                    context.request_repaint_of(viewport);
                } else {
                    context.request_repaint();
                }
            }
        }));
    });
}

// Returns the shared queue used by native menu callbacks.
fn event_queue() -> &'static Mutex<VecDeque<NativeMenuEvent>> {
    MENU_EVENTS.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// Keeps other windows' commands queued until their owner draws.
fn take_events_for_document(
    queue: &mut VecDeque<NativeMenuEvent>,
    document_id: Option<u64>,
) -> Vec<NativeMenuEvent> {
    let mut selected = Vec::new();
    let mut retained = VecDeque::new();
    while let Some(event) = queue.pop_front() {
        if event.target.document_id == document_id {
            selected.push(event);
        } else {
            retained.push_back(event);
        }
    }
    *queue = retained;
    selected
}

// Applies enabled state to every native item kind that supports it.
fn set_enabled(handle: &Handle, enabled: bool) {
    match handle {
        Handle::Item(item) => item.set_enabled(enabled),
        Handle::Check(item) => item.set_enabled(enabled),
        #[cfg(target_os = "macos")]
        Handle::Predefined(_) => (),
    }
}

// Updates the label of a stored native menu item.
fn set_text(handle: &Handle, text: &str) {
    match handle {
        Handle::Item(item) => item.set_text(text),
        Handle::Check(item) => item.set_text(text),
        #[cfg(target_os = "macos")]
        Handle::Predefined(item) => item.set_text(text),
    }
}

/// Restores the app's value after muda toggles a native check item on click.
fn sync_checked_item(item: &CheckMenuItem, checked: bool) {
    if item.is_checked() != checked {
        item.set_checked(checked);
    }
}

#[cfg(test)]
#[path = "native_menu_test.rs"]
mod tests;

#[cfg(target_os = "windows")]
// Lets Windows handle a menu shortcut before the app sees the key event.
pub(crate) fn translate_windows_message(message: *const std::ffi::c_void) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MSG, TranslateAcceleratorW};
    if message.is_null() {
        return false;
    }
    // SAFETY: winit invokes this hook with a pointer to its current Win32 MSG.
    let message = unsafe { &*(message.cast::<MSG>()) };
    if !is_winit_window(message.hwnd) || is_help_window(message.hwnd) {
        return false;
    }
    let hwnd = message.hwnd as isize;
    let accelerator = crate::ui::attach_windows_native_menu(hwnd)
        .unwrap_or_else(|| WINDOW_ACCELERATOR.load(Ordering::Acquire));
    if accelerator == 0 {
        return false;
    }
    // SAFETY: the live menu owns this accelerator and the MSG targets its attached HWND.
    unsafe { TranslateAcceleratorW(message.hwnd, accelerator as _, message) != 0 }
}

/// Attaches the shared native menu to one additional Winit window and returns its accelerators.
/// The caller must not hold app state: `SetMenu` resizes the window, and eframe repaints it
/// synchronously on Windows, which runs the app again inside this call.
#[cfg(target_os = "windows")]
pub(crate) fn attach_windows_hwnd(menu: &Menu, hwnd: isize) -> Option<isize> {
    // SAFETY: the message hook verifies this is a live Winit HWND on the UI thread.
    if !unsafe { menu.is_visible_on_hwnd(hwnd) } {
        // SAFETY: the message hook verifies this is a live Winit HWND on the UI thread.
        match unsafe { menu.init_for_hwnd(hwnd) } {
            Ok(()) => (),
            // muda keeps a record for every window it was attached to, also after the
            // window closed. Windows reuses window handles, so a new window can match a
            // closed one's stale record; drop that record and attach again.
            Err(muda::Error::AlreadyInitialized) => {
                // SAFETY: as above; removing only clears muda's record and menu bar.
                let _ = unsafe { menu.remove_for_hwnd(hwnd) };
                // SAFETY: as above.
                unsafe { menu.init_for_hwnd(hwnd) }.ok()?;
            }
            Err(_) => return None,
        }
    }
    Some(menu.haccel())
}

/// Recognizes the Help window by its title, in either language, so it gets no menu bar.
#[cfg(target_os = "windows")]
fn is_help_window(hwnd: windows_sys::Win32::Foundation::HWND) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowTextW;
    let mut title = [0u16; 128];
    // SAFETY: the buffer is writable and `hwnd` is a live Winit window from the hook.
    let length = unsafe { GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32) };
    if length <= 0 {
        return false;
    }
    let title = String::from_utf16_lossy(&title[..length as usize]);
    [
        crate::i18n::Language::English,
        crate::i18n::Language::Japanese,
    ]
    .into_iter()
    .any(|language| crate::i18n::text(language, "help-window-title") == title)
}

/// Filters message-hook windows to Winit's owned top-level class.
#[cfg(target_os = "windows")]
fn is_winit_window(hwnd: windows_sys::Win32::Foundation::HWND) -> bool {
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetWindowThreadProcessId};
    if hwnd.is_null() {
        return false;
    }
    let mut class_name = [0u16; 64];
    // SAFETY: the buffer is writable and `hwnd` came from the current message hook.
    let length = unsafe { GetClassNameW(hwnd, class_name.as_mut_ptr(), class_name.len() as i32) };
    if length <= 0 || String::from_utf16_lossy(&class_name[..length as usize]) != "Window Class" {
        return false;
    }
    let mut process_id = 0;
    // SAFETY: the output pointer is valid and the HWND was checked above.
    unsafe { GetWindowThreadProcessId(hwnd, &mut process_id) };
    process_id == unsafe { GetCurrentProcessId() }
}
