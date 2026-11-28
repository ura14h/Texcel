//! Places each new document window like other desktop apps do: the same size as the window
//! the user was working in, moved down and to the right by one step.

use super::*;

#[cfg(test)]
#[path = "placement_test.rs"]
mod tests;

/// How far each new window moves down and to the right of the one it follows.
const CASCADE_STEP: f32 = 24.0;
/// The size of a new window when no other window can be measured yet.
pub(super) const DEFAULT_WINDOW_SIZE: Vec2 = Vec2::new(1120.0, 760.0);

/// Where a new window first appears: its outer top-left corner and its inner size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct WindowPlacement {
    pub(super) position: Pos2,
    pub(super) size: Vec2,
}

impl HostRuntime {
    /// Remembers the document window that has keyboard focus, which new windows follow.
    pub(super) fn note_focused_window(&mut self, ctx: &egui::Context, viewport: egui::ViewportId) {
        let focused = ctx.input(|input| {
            input
                .raw
                .viewports
                .get(&viewport)
                .and_then(|info| info.focused)
        });
        if focused == Some(true) && self.last_focused_viewport != Some(viewport) {
            self.last_focused_viewport = Some(viewport);
            // A newly focused window starts a new cascade.
            self.last_placement = None;
        }
    }

    /// Chooses a place for every child window that has none yet, before it is created.
    pub(super) fn place_new_windows(&mut self, ctx: &egui::Context) {
        let new_windows = self
            .document_viewports
            .values()
            .copied()
            .filter(|viewport| {
                *viewport != self.root_viewport && !self.placements.contains_key(viewport)
            })
            .collect::<Vec<_>>();
        for viewport in new_windows {
            let base = self
                .last_placement
                .or_else(|| self.focused_window_placement(ctx));
            let monitor = ctx.input(|input| input.viewport().monitor_size);
            let placement = base.map_or(
                WindowPlacement {
                    position: Pos2::new(CASCADE_STEP, CASCADE_STEP),
                    size: DEFAULT_WINDOW_SIZE,
                },
                |base| cascade_after(base, monitor),
            );
            self.placements.insert(viewport, placement);
            self.last_placement = Some(placement);
        }
        let live = self
            .document_viewports
            .values()
            .copied()
            .collect::<BTreeSet<_>>();
        self.placements
            .retain(|viewport, _| live.contains(viewport));
    }

    /// Measures the window new windows follow: the last focused one, or else the root.
    fn focused_window_placement(&self, ctx: &egui::Context) -> Option<WindowPlacement> {
        let viewport = self
            .last_focused_viewport
            .filter(|viewport| {
                self.document_viewports
                    .values()
                    .any(|owner| owner == viewport)
            })
            .unwrap_or(self.root_viewport);
        ctx.input(|input| {
            let info = input.raw.viewports.get(&viewport)?;
            Some(WindowPlacement {
                position: info.outer_rect?.min,
                size: info.inner_rect?.size(),
            })
        })
    }
}

/// Moves one step down and to the right of `base`, keeping its size, and goes back to the
/// screen's top-left corner when the window would run off the screen.
pub(super) fn cascade_after(base: WindowPlacement, monitor: Option<Vec2>) -> WindowPlacement {
    let position = base.position + Vec2::splat(CASCADE_STEP);
    let fits = monitor.is_none_or(|monitor| {
        position.x + base.size.x <= monitor.x && position.y + base.size.y <= monitor.y
    });
    WindowPlacement {
        position: if fits {
            position
        } else {
            Pos2::new(CASCADE_STEP, CASCADE_STEP)
        },
        size: base.size,
    }
}
