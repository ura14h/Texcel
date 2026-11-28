//! Keeps the Help window from acting as a text field, so macOS does not pop up the
//! current input mode when Help becomes the active window.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use objc2::msg_send_id;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Sel};
use objc2_app_kit::NSWindow;

// Pinned winit 0.30.13 draws each window into this private view class.
const WINIT_VIEW_CLASS: &str = "WinitView";

/// NSView's own `inputContext`, which the override calls for every other window.
static ORIGINAL_INPUT_CONTEXT: OnceLock<InputContextFn> = OnceLock::new();
static INSTALLED: AtomicBool = AtomicBool::new(false);

type InputContextFn = unsafe extern "C" fn(*mut AnyObject, Sel) -> *mut AnyObject;

/// Answers `inputContext` for winit's views: none inside Help, the usual one elsewhere.
unsafe extern "C" fn input_context(view: *mut AnyObject, selector: Sel) -> *mut AnyObject {
    // SAFETY: AppKit sends this message to a live WinitView on the main thread.
    let window: Option<Retained<NSWindow>> = unsafe { msg_send_id![view, window] };
    if window.is_some_and(|window| crate::i18n::is_help_window_title(&window.title().to_string())) {
        return std::ptr::null_mut();
    }
    match ORIGINAL_INPUT_CONTEXT.get() {
        // SAFETY: the stored function is NSView's implementation of this same selector.
        Some(original) => unsafe { original(view, selector) },
        None => std::ptr::null_mut(),
    }
}

/// Adds the override to winit's view class once; leaves it unchanged if winit already
/// defines its own `inputContext`.
pub(super) fn install() {
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
