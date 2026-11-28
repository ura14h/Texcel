use super::*;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "app_host/file_results.rs"]
mod file_results;
#[path = "app_host/placement.rs"]
mod placement;
#[path = "app_host/root_window.rs"]
mod root_window;

static NEXT_HOST_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static HOSTS: RefCell<BTreeMap<u64, HostRuntime>> = const { RefCell::new(BTreeMap::new()) };
}

/// Holds the non-Send application core on the event-loop thread.
pub struct TexcelAppHost {
    id: u64,
}

#[cfg(test)]
#[path = "app_host_test.rs"]
mod tests;

struct HostRuntime {
    app: TexcelApp,
    root_viewport: egui::ViewportId,
    root_document: Option<DocumentId>,
    document_viewports: BTreeMap<DocumentId, egui::ViewportId>,
    retired_viewports: BTreeSet<egui::ViewportId>,
    /// Where each child window first appears.
    placements: BTreeMap<egui::ViewportId, placement::WindowPlacement>,
    last_focused_viewport: Option<egui::ViewportId>,
    /// The window placed last, which the next new window follows until focus moves.
    last_placement: Option<placement::WindowPlacement>,
    /// Where the opening progress window appeared, kept while it shows.
    opening_position: Option<egui::Pos2>,
}

impl TexcelAppHost {
    /// Registers the core before eframe creates its first native viewport.
    pub fn new(mut app: TexcelApp, root_viewport: egui::ViewportId) -> Self {
        let id = NEXT_HOST_ID.fetch_add(1, Ordering::Relaxed);
        let root_document = app.workspace.active_document;
        app.multi_window_mode = true;
        app.host_result_dispatch = true;
        app.enable_window_state(root_viewport);
        let mut document_viewports = BTreeMap::new();
        if let Some(document_id) = root_document {
            document_viewports.insert(document_id, root_viewport);
        }
        HOSTS.with(|hosts| {
            hosts.borrow_mut().insert(
                id,
                HostRuntime {
                    app,
                    root_viewport,
                    root_document,
                    document_viewports,
                    retired_viewports: BTreeSet::new(),
                    placements: BTreeMap::new(),
                    last_focused_viewport: None,
                    last_placement: None,
                    opening_position: None,
                },
            );
        });
        Self { id }
    }
}

impl Drop for TexcelAppHost {
    fn drop(&mut self) {
        HOSTS.with(|hosts| {
            hosts.borrow_mut().remove(&self.id);
        });
    }
}

impl eframe::App for TexcelAppHost {
    /// Advances shared workers once from the root viewport.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        with_host(self.id, |host| {
            let viewport = host.root_viewport;
            let close_requested = ctx.input(|input| input.viewport().close_requested());
            if host.app.app_exit_ready {
                host.request_close_all_windows(ctx);
                return;
            }
            #[cfg(target_os = "macos")]
            let native_terminate_pending = crate::macos_app::termination_requested();
            #[cfg(not(target_os = "macos"))]
            let native_terminate_pending = false;
            #[cfg(not(target_os = "macos"))]
            let last_document_window_closed = close_requested
                && host.root_document.is_none()
                && host.app.workspace.documents.is_empty();
            #[cfg(target_os = "macos")]
            let last_document_window_closed = false;
            let exit_request_pending = host.app.app_exit_start_pending
                || native_terminate_pending
                || last_document_window_closed;
            host.process_file_results(ctx);
            host.settle_root_window(ctx);
            host.advance_opening(ctx);
            let document = host.root_document;
            let no_document_windows = host.app.workspace.documents.is_empty();
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            let global_menu_events = if no_document_windows {
                host.app
                    .native_menu
                    .as_ref()
                    .map(crate::native_menu::NativeMenu::take_events)
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            let global_menu_events = Vec::new();
            #[cfg(target_os = "macos")]
            let open_paths = crate::macos_app::take_open_paths();
            #[cfg(not(target_os = "macos"))]
            let open_paths = Vec::new();
            host.app.with_window_state(viewport, document, |app| {
                if last_document_window_closed {
                    app.app_exit_start_pending = true;
                }
                if !open_paths.is_empty() {
                    app.queue_open_paths(open_paths);
                }
                let show_root_for_menu = no_document_windows
                    && app.apply_native_global_menu_events(global_menu_events, ctx);
                if show_root_for_menu
                    || no_document_windows
                        && (!app.queued_open_paths.is_empty()
                            || app.settings_window_open
                            || app.about_dialog_open
                            || app.app_close_dialog_open)
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                app.process_load_progress(ctx);
                if close_requested && !app.app_exit_transaction && !exit_request_pending {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    if let Some(document_id) = document {
                        app.request_close_document(document_id);
                    }
                } else {
                    if close_requested {
                        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    }
                    app.handle_app_close_request(ctx);
                }
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                if no_document_windows {
                    app.sync_native_menu_without_document(ctx);
                }
            });
            if host.app.app_exit_ready {
                host.request_close_all_windows(ctx);
            }
        });
    }

    /// Routes pointer, keyboard, and IME input into the owning window state.
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        let viewport = raw.viewport_id;
        with_host(self.id, |host| {
            let document = host.document_for_viewport(viewport);
            host.app.with_window_state(viewport, document, |app| {
                app.prepare_raw_input(ctx, raw);
            });
        });
    }

    /// Draws the root and registers deferred child viewports for other documents.
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        let (children, retired) = with_host(self.id, |host| {
            host.settle_root_window(&ctx);
            let viewport = host.root_viewport;
            let document = host.root_document;
            host.app.with_window_state(viewport, document, |app| {
                app.draw_contents(root);
            });
            host.settle_root_window(&ctx);
            host.note_focused_window(&ctx, host.root_viewport);
            host.place_new_windows(&ctx);
            host.deliver_pending_document_focus(&ctx);
            if host.app.app_exit_ready {
                host.request_close_all_windows(&ctx);
            }
            host.close_empty_root_if_needed(&ctx);
            (
                host.child_viewports(),
                std::mem::take(&mut host.retired_viewports),
            )
        });
        draw_child_viewports(self.id, &ctx, children, retired);
        draw_help_viewport(self.id, &ctx);
        draw_opening_viewport(self.id, &ctx);
    }
}

impl HostRuntime {
    /// Assigns each loaded document one stable viewport and retires closed windows.
    fn sync_document_viewports(&mut self) {
        self.root_document = self
            .app
            .window_document(self.root_viewport)
            .filter(|document_id| self.app.workspace.documents.contains_key(document_id));
        self.document_viewports.retain(|document_id, viewport| {
            if self.app.workspace.documents.contains_key(document_id) {
                true
            } else {
                if *viewport != self.root_viewport {
                    self.retired_viewports.insert(*viewport);
                }
                false
            }
        });
        if let Some(document_id) = self.root_document {
            self.document_viewports
                .insert(document_id, self.root_viewport);
        }

        let unassigned = self
            .app
            .workspace
            .order
            .iter()
            .copied()
            .filter(|document_id| !self.document_viewports.contains_key(document_id))
            .collect::<Vec<_>>();
        for document_id in unassigned {
            let viewport = egui::ViewportId::from_hash_of(("texcel-document", document_id));
            self.document_viewports.insert(document_id, viewport);
            self.app.set_window_document(viewport, Some(document_id));
            // Bring the new window forward. A file dropped from Explorer or Finder leaves
            // that app in front, and Windows would keep the new window inactive; the menu
            // bar would then still act on the window that last had focus.
            self.app.pending_document_focus = Some(document_id);
        }
    }

    /// Returns the document assigned to an input viewport.
    fn document_for_viewport(&self, viewport: egui::ViewportId) -> Option<DocumentId> {
        if viewport == self.root_viewport {
            self.root_document
        } else {
            self.document_viewports
                .iter()
                .find_map(|(document, owner)| (*owner == viewport).then_some(*document))
        }
    }

    /// Brings an existing document forward after a duplicate open was resolved.
    fn deliver_pending_document_focus(&mut self, ctx: &egui::Context) {
        let Some(document_id) = self.app.pending_document_focus else {
            return;
        };
        // A window waiting for its first column fit does not exist yet.
        if self.app.opening_fits.contains_key(&document_id) {
            return;
        }
        if let Some(viewport) = self.document_viewports.get(&document_id).copied() {
            ctx.send_viewport_cmd_to(viewport, egui::ViewportCommand::Focus);
            self.app.pending_document_focus = None;
        }
    }

    /// Hides the root window once no document is left (or closes the app outside macOS).
    /// While documents remain, the root has already taken one over.
    fn close_empty_root_if_needed(&self, ctx: &egui::Context) {
        let close_dialog_open =
            self.app
                .window_states
                .get(&self.root_viewport)
                .is_some_and(|state| {
                    state.app_close_dialog_open
                        || state.settings_window_open
                        || state.about_dialog_open
                });
        if self.root_document.is_some()
            || close_dialog_open
            || self.app.active_queued_open.is_some()
            || !self.app.queued_open_paths.is_empty()
            // An opened document is having its columns fitted and comes to the root next.
            || !self.app.opening_fits.is_empty()
        {
            return;
        }
        #[cfg(not(target_os = "macos"))]
        if self.app.workspace.documents.is_empty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        #[cfg(target_os = "macos")]
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }

    /// Closes every native viewport only after the app-wide save and recovery gate passes.
    fn request_close_all_windows(&self, ctx: &egui::Context) {
        let mut viewports = self
            .document_viewports
            .values()
            .copied()
            .collect::<BTreeSet<_>>();
        viewports.insert(self.root_viewport);
        for viewport in viewports {
            ctx.send_viewport_cmd_to(viewport, egui::ViewportCommand::Close);
        }
    }

    /// Collects child window identifiers, titles, and first places after root state is current.
    fn child_viewports(&self) -> Vec<ChildWindow> {
        self.document_viewports
            .iter()
            // A new window opens once its first column fit is done.
            .filter(|(document_id, viewport)| {
                **viewport != self.root_viewport && !self.app.opening_fits.contains_key(document_id)
            })
            .map(|(document_id, viewport)| ChildWindow {
                document_id: *document_id,
                viewport: *viewport,
                title: self.window_title(*document_id),
                placement: self.placements.get(viewport).copied(),
            })
            .collect()
    }

    /// Builds the title from the document name and its saved state.
    fn window_title(&self, document_id: DocumentId) -> String {
        let Some(document) = self.app.workspace.documents.get(&document_id) else {
            return "Texcel".to_owned();
        };
        let marker = if self.app.document_has_unsaved_changes(document_id) {
            " •"
        } else {
            ""
        };
        format!("{}{marker} — Texcel", document.name)
    }
}

/// One child window to register this frame.
struct ChildWindow {
    document_id: DocumentId,
    viewport: egui::ViewportId,
    title: String,
    placement: Option<placement::WindowPlacement>,
}

/// Registers child windows after releasing the host registry borrow.
fn draw_child_viewports(
    host_id: u64,
    ctx: &egui::Context,
    children: Vec<ChildWindow>,
    retired: BTreeSet<egui::ViewportId>,
) {
    for ChildWindow {
        document_id,
        viewport,
        title,
        placement,
    } in children
    {
        // The place only applies when the window is created; later moves are the user's.
        let mut builder = egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size(placement.map_or(placement::DEFAULT_WINDOW_SIZE, |place| place.size))
            .with_min_inner_size([760.0, 500.0])
            .with_drag_and_drop(true);
        if let Some(place) = placement {
            builder = builder.with_position(place.position);
        }
        ctx.show_viewport_deferred(viewport, builder, move |ui, _class| {
            draw_document_viewport(host_id, viewport, document_id, ui);
        });
    }
    for viewport in retired {
        ctx.show_viewport_deferred(
            viewport,
            egui::ViewportBuilder::default().with_visible(false),
            move |ui, _class| {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            },
        );
    }
}

/// Shows the app-wide Help window while it is open.
fn draw_help_viewport(host_id: u64, ctx: &egui::Context) {
    let Some(builder) = with_host(host_id, |host| {
        host.app
            .help_viewport_open
            .then(|| host.app.help_viewport_builder())
    }) else {
        return;
    };
    ctx.show_viewport_deferred(
        help_window::help_viewport_id(),
        builder,
        move |ui, _class| {
            with_host(host_id, |host| host.app.draw_help_window_contents(ui));
        },
    );
}

/// Shows the opening progress window while a file has been opening for a while.
fn draw_opening_viewport(host_id: u64, ctx: &egui::Context) {
    let Some(builder) = with_host(host_id, |host| host.opening_viewport_builder(ctx)) else {
        return;
    };
    ctx.show_viewport_deferred(
        opening_progress::opening_viewport_id(),
        builder,
        move |ui, _class| {
            with_host(host_id, |host| host.app.draw_opening_progress_contents(ui));
        },
    );
}

/// Handles one child viewport with the same input and close path in tests and the app.
fn draw_document_viewport(
    host_id: u64,
    viewport: egui::ViewportId,
    document_id: DocumentId,
    ui: &mut egui::Ui,
) {
    let ctx = ui.ctx().clone();
    #[cfg(target_os = "macos")]
    let native_terminate_pending = crate::macos_app::termination_requested();
    #[cfg(not(target_os = "macos"))]
    let native_terminate_pending = false;
    with_host(host_id, |host| {
        let close_requested = ctx.input(|input| input.viewport().close_requested());
        if close_requested && !host.app.app_exit_ready {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        host.app
            .with_window_state(viewport, Some(document_id), |app| {
                if close_requested && !app.app_exit_ready && !app.app_exit_transaction {
                    app.request_close_document(document_id);
                }
                if app.app_exit_start_pending || native_terminate_pending {
                    app.handle_app_close_request(&ctx);
                }
                app.draw_contents(ui);
            });
        host.settle_root_window(&ctx);
        host.note_focused_window(&ctx, viewport);
        host.deliver_pending_document_focus(&ctx);
        if host.app.app_exit_ready {
            host.request_close_all_windows(&ctx);
        }
        if !host.app.workspace.documents.contains_key(&document_id) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    });
}

/// Uses the main-thread registry without making native UI state Send.
fn with_host<R>(id: u64, run: impl FnOnce(&mut HostRuntime) -> R) -> R {
    HOSTS.with(|hosts| {
        let mut hosts = hosts.borrow_mut();
        run(hosts.get_mut(&id).expect("the app host remains registered"))
    })
}

/// Attaches the shared menu to a Winit child window from the message hook.
#[cfg(target_os = "windows")]
pub(crate) fn attach_native_menu_to_window(hwnd: isize) -> Option<isize> {
    // Release the registry before attaching: the synchronous repaint from `SetMenu`
    // re-enters `with_host`, which would otherwise panic on a second borrow.
    let menu = HOSTS.with(|hosts| {
        hosts.try_borrow().ok()?.values().find_map(|host| {
            host.app
                .native_menu
                .as_ref()
                .map(crate::native_menu::NativeMenu::windows_menu)
        })
    })?;
    crate::native_menu::attach_windows_hwnd(&menu, hwnd)
}
