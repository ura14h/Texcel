//! Stores each Texcel window's role in its NSWindow identifier, and keeps the Help window
//! from acting as a text field, so macOS does not pop up the current input mode there.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use objc2::ClassType;
use objc2::msg_send_id;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Sel};
use objc2_app_kit::{NSApplication, NSUserInterfaceItemIdentification, NSWindow};
use objc2_foundation::NSObjectProtocol;
use objc2_foundation::{MainThreadMarker, NSString};

use crate::native_window::{self, WindowRole};

// Pinned winit 0.30.13 draws each window into these private classes.
const WINIT_VIEW_CLASS: &str = "WinitView";
const WINIT_WINDOW_CLASS: &str = "WinitWindow";
/// Prefix of the identifier that names a window's role.
const ROLE_PREFIX: &str = "net.homeunix.hio.app.Texcel.role.";

/// Reads the role stored on a window, if Texcel identified it.
fn window_role(window: &NSWindow) -> Option<WindowRole> {
    // SAFETY: reading the identifier of a live window on the main thread.
    let identifier = unsafe { window.identifier()? }.to_string();
    WindowRole::from_name(identifier.strip_prefix(ROLE_PREFIX)?)
}

/// Stores a role on a window.
fn set_window_role(window: &NSWindow, role: WindowRole) {
    let identifier = NSString::from_str(&format!("{ROLE_PREFIX}{}", role.name()));
    // SAFETY: setting the identifier of a live window on the main thread.
    unsafe { window.setIdentifier(Some(&identifier)) };
}

/// Lists winit's windows that carry no identifier yet.
fn unidentified_winit_windows(main_thread: MainThreadMarker) -> Vec<Retained<NSWindow>> {
    let Some(winit_class) = AnyClass::get(WINIT_WINDOW_CLASS) else {
        return Vec::new();
    };
    NSApplication::sharedApplication(main_thread)
        .windows()
        .iter()
        // SAFETY: reading the identifier of a live window on the main thread.
        .filter(|window| {
            window.isKindOfClass(winit_class) && unsafe { window.identifier() }.is_none()
        })
        .map(|window| window.retain())
        .collect()
}

/// Gives the window eframe created for the announced viewport its role. Called each frame
/// while a window is announced; the window appears right after the pass that registered it.
pub(crate) fn identify_new_window() {
    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };
    if !native_window::new_window_pending() {
        return;
    }
    let windows = unidentified_winit_windows(main_thread);
    let Some(window) = windows.first() else {
        return;
    };
    let Some(role) = native_window::take_expected_role() else {
        return;
    };
    set_window_role(window, role);
    if role == WindowRole::Help {
        // The window may have become key before it was identified; let AppKit ask its view
        // for an input context again so the input mode indicator goes away.
        let view = window.contentView();
        let _ = window.makeFirstResponder(None);
        let _ = window.makeFirstResponder(view.as_deref().map(|view| &**view));
    }
}

/// NSView's own `inputContext`, which the override calls for every other window.
static ORIGINAL_INPUT_CONTEXT: OnceLock<InputContextFn> = OnceLock::new();
static INSTALLED: AtomicBool = AtomicBool::new(false);

type InputContextFn = unsafe extern "C" fn(*mut AnyObject, Sel) -> *mut AnyObject;

/// Answers `inputContext` for winit's views: none inside Help, the usual one elsewhere.
unsafe extern "C" fn input_context(view: *mut AnyObject, selector: Sel) -> *mut AnyObject {
    // SAFETY: AppKit sends this message to a live WinitView on the main thread.
    let window: Option<Retained<NSWindow>> = unsafe { msg_send_id![view, window] };
    if window.is_some_and(|window| window_role(&window) == Some(WindowRole::Help)) {
        return std::ptr::null_mut();
    }
    match ORIGINAL_INPUT_CONTEXT.get() {
        // SAFETY: the stored function is NSView's implementation of this same selector.
        Some(original) => unsafe { original(view, selector) },
        None => std::ptr::null_mut(),
    }
}

/// Identifies the first window as a document window and adds the `inputContext` override to
/// winit's view class once; leaves the class unchanged if winit defines its own.
pub(super) fn install(main_thread: MainThreadMarker) {
    for window in unidentified_winit_windows(main_thread) {
        set_window_role(&window, WindowRole::Document);
    }
    if INSTALLED.swap(true, Ordering::AcqRel) {
        return;
    }
    let (Some(view_class), Some(ns_view)) =
        (AnyClass::get(WINIT_VIEW_CLASS), AnyClass::get("NSView"))
    else {
        return;
    };
    let selector = objc2::sel!(inputContext);
    let view_ptr = view_class as *const AnyClass as *mut objc2::ffi::objc_class;
    let base_ptr = ns_view as *const AnyClass as *const objc2::ffi::objc_class;
    // SAFETY: both classes are registered and the selector is a plain NSView method.
    let base = unsafe { objc2::ffi::class_getInstanceMethod(base_ptr, selector.as_ptr()) };
    let current = unsafe { objc2::ffi::class_getInstanceMethod(view_ptr, selector.as_ptr()) };
    if base.is_null() || current != base {
        return;
    }
    let Some(original) = (unsafe { objc2::ffi::method_getImplementation(base) }) else {
        return;
    };
    let encoding = unsafe { objc2::ffi::method_getTypeEncoding(base) };
    if encoding.is_null() {
        return;
    }
    // SAFETY: `inputContext` takes no arguments and returns an object, as InputContextFn does.
    let original: InputContextFn = unsafe { std::mem::transmute(original) };
    let _ = ORIGINAL_INPUT_CONTEXT.set(original);
    let implementation: objc2::ffi::IMP = Some(unsafe {
        std::mem::transmute::<InputContextFn, unsafe extern "C" fn()>(input_context)
    });
    // SAFETY: the selector is inherited, not defined on WinitView, and the encoding matches.
    unsafe {
        objc2::ffi::class_addMethod(view_ptr, selector.as_ptr(), implementation, encoding);
    }
}
