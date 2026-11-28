//! Registers the child windows each frame: document windows, Help, and opening progress.
//!
//! A new viewport's native window is created right after the pass that first registers it.
//! At most one new viewport is registered per pass, with its role announced beforehand, so
//! the platform code can tell which native window is which (see `native_window`).

use super::*;
use crate::native_window::{self, WindowRole};

#[cfg(test)]
#[path = "viewports_test.rs"]
mod tests;

/// One child window to register this frame.
pub(super) struct ChildWindow {
    pub(super) document_id: DocumentId,
    pub(super) viewport: egui::ViewportId,
    pub(super) title: String,
    pub(super) placement: Option<placement::WindowPlacement>,
}

/// What to register this frame, decided while the host is borrowed.
pub(super) struct ViewportPlan {
    children: Vec<ChildWindow>,
    help: Option<egui::ViewportBuilder>,
    opening: Option<egui::ViewportBuilder>,
    retired: BTreeSet<egui::ViewportId>,
}

impl HostRuntime {
    /// Decides which child windows to register this frame, admitting at most one new one.
    pub(super) fn plan_viewports(&mut self, ctx: &egui::Context) -> ViewportPlan {
        // The window created after the last pass gets its role before the next one is made.
        #[cfg(target_os = "macos")]
        crate::macos_app::identify_new_window();
        let children = self.child_viewports();
        let help = self
            .app
            .help_viewport_open
            .then(|| self.app.help_viewport_builder());
        let opening = self.opening_viewport_builder(ctx);
        let mut wanted = children
            .iter()
            .map(|child| child.viewport)
            .collect::<BTreeSet<_>>();
        if help.is_some() {
            wanted.insert(help_window::help_viewport_id());
        }
        if opening.is_some() {
            wanted.insert(opening_progress::opening_viewport_id());
        }
        self.forget_unwanted_viewports(&wanted);
        let mut waiting = false;
        let children = children
            .into_iter()
            .filter(|child| {
                let admitted = self.admit_viewport(child.viewport, WindowRole::Document);
                waiting |= !admitted;
                admitted
            })
            .collect();
        let help = help.filter(|_| {
            let admitted = self.admit_viewport(help_window::help_viewport_id(), WindowRole::Help);
            waiting |= !admitted;
            admitted
        });
        let opening = opening.filter(|_| {
            let admitted = self.admit_viewport(
                opening_progress::opening_viewport_id(),
                WindowRole::Progress,
            );
            waiting |= !admitted;
            admitted
        });
        if waiting || native_window::new_window_pending() {
            // The next window is created once the one being created now is identified.
            ctx.request_repaint();
        }
        let retired = std::mem::take(&mut self.retired_viewports);
        for viewport in &retired {
            self.window_creation.retire(*viewport);
        }
        ViewportPlan {
            children,
            help,
            opening,
            retired,
        }
    }

    /// Drops closed windows from the created set and withdraws an announced window whose
    /// viewport is no longer wanted.
    fn forget_unwanted_viewports(&mut self, wanted: &BTreeSet<egui::ViewportId>) {
        if self.window_creation.forget_unwanted(wanted) {
            native_window::cancel_expected_window();
        }
    }

    /// Lets a viewport register this pass, announcing its role when its window is to be made.
    fn admit_viewport(&mut self, viewport: egui::ViewportId, role: WindowRole) -> bool {
        match self
            .window_creation
            .admit(viewport, !native_window::new_window_pending())
        {
            Admission::Registered => true,
            Admission::Create => {
                native_window::expect_new_window(role);
                true
            }
            Admission::Wait => false,
        }
    }
}

/// What a viewport may do this pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Admission {
    /// Its window exists or is being created; register it as before.
    Registered,
    /// Register it for the first time; its window is created after this pass.
    Create,
    /// Another window is being created; register it on a later pass.
    Wait,
}

/// Which viewports have windows, and the one window being created.
#[derive(Default)]
pub(super) struct WindowCreation {
    created: BTreeSet<egui::ViewportId>,
    creating: Option<egui::ViewportId>,
}

impl WindowCreation {
    /// Forgets windows no longer wanted; returns whether the one being created was dropped.
    pub(super) fn forget_unwanted(&mut self, wanted: &BTreeSet<egui::ViewportId>) -> bool {
        self.created.retain(|viewport| wanted.contains(viewport));
        let dropped = self
            .creating
            .is_some_and(|viewport| !wanted.contains(&viewport));
        if dropped {
            self.creating = None;
        }
        dropped
    }

    /// Admits a viewport: one with a window, the one being created, or a new one when no
    /// window is being created. `identified` says the window being created was identified.
    pub(super) fn admit(&mut self, viewport: egui::ViewportId, identified: bool) -> Admission {
        if identified && let Some(creating) = self.creating.take() {
            self.created.insert(creating);
        }
        if self.created.contains(&viewport) || self.creating == Some(viewport) {
            return Admission::Registered;
        }
        if self.creating.is_some() {
            return Admission::Wait;
        }
        self.creating = Some(viewport);
        Admission::Create
    }

    /// Forgets a closed window.
    pub(super) fn retire(&mut self, viewport: egui::ViewportId) {
        self.created.remove(&viewport);
    }
}

/// Registers the planned child windows after releasing the host registry borrow.
pub(super) fn show_viewports(host_id: u64, ctx: &egui::Context, plan: ViewportPlan) {
    for ChildWindow {
        document_id,
        viewport,
        title,
        placement,
    } in plan.children
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
    for viewport in plan.retired {
        ctx.show_viewport_deferred(
            viewport,
            egui::ViewportBuilder::default().with_visible(false),
            move |ui, _class| {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            },
        );
    }
    if let Some(builder) = plan.help {
        ctx.show_viewport_deferred(
            help_window::help_viewport_id(),
            builder,
            move |ui, _class| {
                with_host(host_id, |host| host.app.draw_help_window_contents(ui));
            },
        );
    }
    if let Some(builder) = plan.opening {
        ctx.show_viewport_deferred(
            opening_progress::opening_viewport_id(),
            builder,
            move |ui, _class| {
                with_host(host_id, |host| host.app.draw_opening_progress_contents(ui));
            },
        );
    }
}
