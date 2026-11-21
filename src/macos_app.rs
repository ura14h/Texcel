//! Routes macOS quit requests through Texcel's normal close confirmation.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::CStr;
use std::path::PathBuf;
use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyClass;
use objc2::{ClassType, DeclaredClass, declare_class, mutability};
use objc2_app_kit::{
    NSApplication, NSApplicationDelegate, NSApplicationDelegateReply, NSApplicationTerminateReply,
    NSApplicationWillFinishLaunchingNotification, NSUserInterfaceItemIdentification, NSWindow,
};
use objc2_foundation::{
    MainThreadMarker, NSArray, NSNotification, NSNotificationCenter, NSObject, NSObjectProtocol,
    NSString,
};

// Pinned winit 0.30.13 uses this private class; review the hook on upgrades.
const WINIT_DELEGATE_CLASS: &str = "WinitApplicationDelegate";

static TERMINATE_REQUESTED: AtomicBool = AtomicBool::new(false);
static TERMINATE_METHOD_INSTALLED: AtomicBool = AtomicBool::new(false);
static OPEN_PATHS: OnceLock<Mutex<VecDeque<PathBuf>>> = OnceLock::new();
static OPEN_FILE_METHOD_INSTALLED: AtomicBool = AtomicBool::new(false);
static OPEN_FILES_METHOD_INSTALLED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static APP_CONTEXT: RefCell<Option<eframe::egui::Context>> = const { RefCell::new(None) };
    static OPEN_FILE_OBSERVER: RefCell<Option<Retained<NSObject>>> = const { RefCell::new(None) };
    #[cfg(test)]
    static TEST_TERMINATE_REQUESTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

// This typed class supplies the exact ABI for the method copied to winit.
declare_class!(
    struct TexcelTerminateMethod;

    unsafe impl ClassType for TexcelTerminateMethod {
        type Super = NSObject;
        type Mutability = mutability::MainThreadOnly;
        const NAME: &'static str = "TexcelTerminateMethod";
    }

    impl DeclaredClass for TexcelTerminateMethod {}

    unsafe impl NSObjectProtocol for TexcelTerminateMethod {}

    unsafe impl NSApplicationDelegate for TexcelTerminateMethod {
        // Cancels AppKit termination and forwards the request to egui.
        #[method(applicationShouldTerminate:)]
        unsafe fn application_should_terminate(
            &self,
            _application: &NSApplication,
        ) -> NSApplicationTerminateReply {
            request_terminate_after_recovery();
            NSApplicationTerminateReply::NSTerminateCancel
        }
    }
);

// This typed class supplies the exact ABI for both AppKit file-open callbacks.
declare_class!(
    struct TexcelOpenFileMethod;

    unsafe impl ClassType for TexcelOpenFileMethod {
        type Super = NSObject;
        type Mutability = mutability::MainThreadOnly;
        const NAME: &'static str = "TexcelOpenFileMethod";
    }

    impl DeclaredClass for TexcelOpenFileMethod {}

    unsafe impl NSObjectProtocol for TexcelOpenFileMethod {}

    unsafe impl NSApplicationDelegate for TexcelOpenFileMethod {
        // Queues one Finder path and reports that Texcel accepted it.
        #[method(application:openFile:)]
        unsafe fn application_open_file(
            &self,
            _application: &NSApplication,
            filename: &NSString,
        ) -> bool {
            enqueue_open_paths([PathBuf::from(filename.to_string())]);
            true
        }

        // Queues every Finder path in order and completes the AppKit reply.
        #[method(application:openFiles:)]
        unsafe fn application_open_files(
            &self,
            application: &NSApplication,
            filenames: &NSArray<NSString>,
        ) {
            let paths = (0..filenames.count()).map(|index| {
                let filename = unsafe { filenames.objectAtIndex(index) };
                PathBuf::from(filename.to_string())
            });
            enqueue_open_paths(paths);
            unsafe { application.replyToOpenOrPrint(NSApplicationDelegateReply::Success) };
        }
    }
);

/// Installs the cold-start observer before winit enters AppKit's launch cycle.
pub(crate) fn register_open_file_observer() {
    let main_thread = MainThreadMarker::new()
        .expect("Texcel's macOS startup must register file events on the main thread");
    let _application = NSApplication::sharedApplication(main_thread);
    let center = unsafe { NSNotificationCenter::defaultCenter() };
    let observer = OPEN_FILE_OBSERVER.with(|slot| {
        if slot.borrow().is_some() {
            return None;
        }
        let block = RcBlock::new(|_notification: std::ptr::NonNull<NSNotification>| {
            let marker = MainThreadMarker::new()
                .expect("AppKit launch notifications must run on the main thread");
            install_open_file_methods(marker);
        });
        Some(unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(NSApplicationWillFinishLaunchingNotification),
                None,
                None,
                &block,
            )
        })
    });
    if let Some(observer) = observer {
        OPEN_FILE_OBSERVER.with(|slot| *slot.borrow_mut() = Some(observer));
    }
}

/// Keeps winit's delegate and adds only Texcel's quit veto to its class.
pub(crate) fn register(context: eframe::egui::Context) {
    let main_thread = MainThreadMarker::new()
        .expect("Texcel's macOS app setup must run on the AppKit main thread");
    let application = NSApplication::sharedApplication(main_thread);
    NSWindow::setAllowsAutomaticWindowTabbing(false, main_thread);
    let winit_class =
        AnyClass::get(WINIT_DELEGATE_CLASS).expect("fixed winit delegate class was not registered");
    let registered_delegate = unsafe { application.delegate() }
        .expect("winit must keep its application delegate registered");
    if !registered_delegate.isKindOfClass(winit_class) {
        panic!("NSApplication no longer uses winit's registered delegate");
    }

    APP_CONTEXT.with(|slot| *slot.borrow_mut() = Some(context));
    install_terminate_method(winit_class);
    install_open_file_methods(main_thread);
}

/// Updates saved-state dots on the NSWindows that belong to open documents.
pub(crate) fn sync_document_edited_states(states: &[(u64, bool)], focused_document: Option<u64>) {
    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };
    let application = NSApplication::sharedApplication(main_thread);
    if let Some(document_id) = focused_document
        && let Some(window) = application.keyWindow()
        && let Some((_, edited)) = states.iter().find(|(id, _)| *id == document_id)
    {
        unsafe { window.setIdentifier(Some(&document_window_identifier(document_id))) };
        window.setDocumentEdited(*edited);
    }
    for window in application.windows().iter() {
        let Some(document_id) = document_id_for_window(window) else {
            continue;
        };
        if let Some((_, edited)) = states.iter().find(|(id, _)| *id == document_id) {
            window.setDocumentEdited(*edited);
        }
    }
}

/// Gives each native document window a stable identity that does not depend on its title.
fn document_window_identifier(document_id: u64) -> objc2::rc::Retained<NSString> {
    NSString::from_str(&format!(
        "net.homeunix.hio.app.Texcel.document.{document_id}"
    ))
}

/// Reads the stable document identity attached when its window became focused.
fn document_id_for_window(window: &NSWindow) -> Option<u64> {
    let identifier = unsafe { window.identifier()? };
    identifier
        .to_string()
        .strip_prefix("net.homeunix.hio.app.Texcel.document.")?
        .parse()
        .ok()
}

/// Drains paths delivered before the app creator or between UI frames.
pub(crate) fn take_open_paths() -> Vec<PathBuf> {
    OPEN_PATHS
        .get_or_init(|| Mutex::new(VecDeque::new()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .drain(..)
        .collect()
}

/// Adds one OS delivery batch to the same file FIFO used by other entry points.
fn enqueue_open_paths(paths: impl IntoIterator<Item = PathBuf>) {
    OPEN_PATHS
        .get_or_init(|| Mutex::new(VecDeque::new()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .extend(paths);
    APP_CONTEXT.with(|slot| {
        if let Some(context) = slot.borrow().as_ref() {
            context.request_repaint();
        }
    });
}

/// Extends only missing file callbacks on the winit delegate class.
fn install_open_file_methods(main_thread: MainThreadMarker) {
    let application = NSApplication::sharedApplication(main_thread);
    let winit_class =
        AnyClass::get(WINIT_DELEGATE_CLASS).expect("fixed winit delegate class was not registered");
    let registered_delegate = unsafe { application.delegate() }
        .expect("winit must keep its application delegate registered");
    if !registered_delegate.isKindOfClass(winit_class) {
        panic!("NSApplication no longer uses winit's registered delegate");
    }
    install_winit_method(
        winit_class,
        objc2::sel!(application:openFile:),
        TexcelOpenFileMethod::class(),
        &OPEN_FILE_METHOD_INSTALLED,
    );
    install_winit_method(
        winit_class,
        objc2::sel!(application:openFiles:),
        TexcelOpenFileMethod::class(),
        &OPEN_FILES_METHOD_INSTALLED,
    );
}

/// Copies a typed method to winit only while that selector remains absent.
fn install_winit_method(
    winit_class: &'static AnyClass,
    selector: objc2::runtime::Sel,
    method_class: &'static AnyClass,
    installed_flag: &AtomicBool,
) {
    let method_class_ptr = method_class as *const AnyClass as *const objc2::ffi::objc_class;
    let method =
        unsafe { objc2::ffi::class_getInstanceMethod(method_class_ptr, selector.as_ptr()) };
    if method.is_null() {
        panic!("Texcel's typed macOS file-open method was not registered");
    }
    let implementation = unsafe { objc2::ffi::method_getImplementation(method) };
    let encoding = unsafe { objc2::ffi::method_getTypeEncoding(method) };
    if implementation.is_none() || encoding.is_null() {
        panic!("Texcel's typed macOS file-open method has no implementation or ABI");
    }
    let winit_ptr = winit_class as *const AnyClass as *mut objc2::ffi::objc_class;
    let existing = unsafe { objc2::ffi::class_getInstanceMethod(winit_ptr, selector.as_ptr()) };
    if !existing.is_null() {
        if implementations_match(
            unsafe { objc2::ffi::method_getImplementation(existing) },
            implementation,
        ) && method_encodings_match(existing, encoding)
        {
            installed_flag.store(true, Ordering::Release);
            return;
        }
        panic!("winit already provides a different file-open handler");
    }
    if installed_flag.load(Ordering::Acquire) {
        panic!("winit's file-open handler changed after Texcel registered it");
    }
    // SAFETY: The selector is absent and the typed handler uses no prototype-class state.
    let added = unsafe {
        objc2::ffi::class_addMethod(winit_ptr, selector.as_ptr(), implementation, encoding)
    };
    // BOOL's underlying Rust type varies by arch (bool on aarch64, i8 elsewhere); normalize it.
    if !objc2::runtime::Bool::from_raw(added).as_bool() {
        panic!("could not add Texcel's file-open handler to winit's delegate");
    }
    let installed = unsafe { objc2::ffi::class_getInstanceMethod(winit_ptr, selector.as_ptr()) };
    if installed.is_null()
        || !implementations_match(
            unsafe { objc2::ffi::method_getImplementation(installed) },
            implementation,
        )
        || !method_encodings_match(installed, encoding)
    {
        panic!("winit's delegate did not retain Texcel's file-open handler");
    }
    installed_flag.store(true, Ordering::Release);
}

/// Adds the typed quit handler once and rejects unexpected delegate changes.
fn install_terminate_method(winit_class: &'static AnyClass) {
    let selector = objc2::sel!(applicationShouldTerminate:);
    let method_class = TexcelTerminateMethod::class();
    let method_class_ptr = method_class as *const AnyClass as *const objc2::ffi::objc_class;
    let method =
        unsafe { objc2::ffi::class_getInstanceMethod(method_class_ptr, selector.as_ptr()) };
    if method.is_null() {
        panic!("Texcel's typed macOS termination method was not registered");
    }
    let implementation = unsafe { objc2::ffi::method_getImplementation(method) };
    if implementation.is_none() {
        panic!("Texcel's typed macOS termination method has no implementation");
    }
    let encoding = unsafe { objc2::ffi::method_getTypeEncoding(method) };
    if encoding.is_null() {
        panic!("Texcel's typed macOS termination method has no ABI encoding");
    }

    let winit_ptr = winit_class as *const AnyClass as *mut objc2::ffi::objc_class;
    let existing = unsafe { objc2::ffi::class_getInstanceMethod(winit_ptr, selector.as_ptr()) };
    if !existing.is_null()
        && implementations_match(
            unsafe { objc2::ffi::method_getImplementation(existing) },
            implementation,
        )
        && method_encodings_match(existing, encoding)
    {
        TERMINATE_METHOD_INSTALLED.store(true, Ordering::Release);
        return;
    }
    if !existing.is_null() {
        panic!("winit already provides a different termination handler");
    }
    if TERMINATE_METHOD_INSTALLED.load(Ordering::Acquire) {
        panic!("winit's terminate handler changed after Texcel registered it");
    }

    // SAFETY: The selector is absent on the class hierarchy, and the typed
    // prototype handler uses no receiver state or ivars from its own class.
    let added = unsafe {
        objc2::ffi::class_addMethod(winit_ptr, selector.as_ptr(), implementation, encoding)
    };
    // BOOL's underlying Rust type varies by arch (bool on aarch64, i8 elsewhere); normalize it.
    if !objc2::runtime::Bool::from_raw(added).as_bool() {
        panic!("could not add Texcel's quit handler to winit's delegate");
    }

    let installed = unsafe { objc2::ffi::class_getInstanceMethod(winit_ptr, selector.as_ptr()) };
    if installed.is_null()
        || !implementations_match(
            unsafe { objc2::ffi::method_getImplementation(installed) },
            implementation,
        )
        || !method_encodings_match(installed, encoding)
    {
        panic!("winit's delegate did not retain Texcel's quit handler");
    }
    TERMINATE_METHOD_INSTALLED.store(true, Ordering::Release);
}

/// Compares method pointers without assuming function addresses are unique.
fn implementations_match(left: objc2::ffi::IMP, right: objc2::ffi::IMP) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => std::ptr::fn_addr_eq(left, right),
        _ => false,
    }
}

/// Compares two runtime signatures without decoding Objective-C encodings.
fn method_encodings_match(method: *const objc2::ffi::objc_method, expected: *const i8) -> bool {
    let actual = unsafe { objc2::ffi::method_getTypeEncoding(method) };
    if actual.is_null() || expected.is_null() {
        return false;
    }
    unsafe { CStr::from_ptr(actual) == CStr::from_ptr(expected) }
}

/// Consumes one quit request and lets the app loop decide when to close.
pub(crate) fn take_terminate_request() -> bool {
    #[cfg(test)]
    if TEST_TERMINATE_REQUESTED.with(|requested| requested.replace(false)) {
        return true;
    }
    TERMINATE_REQUESTED.swap(false, Ordering::AcqRel)
}

/// Lets any active viewport notice a native quit before consuming it.
pub(crate) fn termination_requested() -> bool {
    #[cfg(test)]
    if TEST_TERMINATE_REQUESTED.with(std::cell::Cell::get) {
        return true;
    }
    TERMINATE_REQUESTED.load(Ordering::Acquire)
}

/// Queues a native quit request after a recovery write has finished.
pub(crate) fn request_terminate_after_recovery() {
    // Independent test hosts must receive only their own request to finish quitting.
    #[cfg(test)]
    request_terminate_for_test();
    #[cfg(not(test))]
    TERMINATE_REQUESTED.store(true, Ordering::Release);
    APP_CONTEXT.with(|slot| {
        if let Some(context) = slot.borrow().as_ref() {
            context.request_repaint();
        }
    });
}

/// Lets macOS close scenarios exercise the same native terminate branch.
#[cfg(test)]
pub(crate) fn request_terminate_for_test() {
    TEST_TERMINATE_REQUESTED.with(|requested| requested.set(true));
}

/// Confirms a test-only quit request was consumed by the app close handler.
#[cfg(test)]
pub(crate) fn terminate_request_pending_for_test() -> bool {
    TEST_TERMINATE_REQUESTED.with(std::cell::Cell::get)
}
