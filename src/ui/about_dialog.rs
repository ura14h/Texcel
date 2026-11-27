use super::*;

/// The copyright line shown with the app name and version.
pub(crate) const APP_COPYRIGHT: &str = "© 2026 Hiroki Ishiura";

impl TexcelApp {
    /// Shows the app name, version, and copyright in a small dialog.
    pub(super) fn draw_about_dialog(&mut self, ctx: &egui::Context) {
        if !self.about_dialog_open {
            return;
        }
        let language = self.language;
        let mut close = false;
        let modal_id = Id::unique("texcel_about");
        let close_key = self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Escape, false)
            || self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Enter, false);
        egui::Modal::new(modal_id).show(ctx, |ui| {
            ui.set_min_width(280.0);
            visual::dialog_surface(ui, |ui| {
                visual::prepare_dialog_body(ui);
                ui.vertical_centered(|ui| {
                    visual::dialog_title(ui, "Texcel");
                    ui.label(
                        Message::new("about-version")
                            .with_text("version", env!("CARGO_PKG_VERSION"))
                            .resolve(language),
                    );
                    visual::caption(ui, APP_COPYRIGHT);
                });
                visual::group_gap(ui);
                ui.separator();
                visual::dialog_footer(ui, |ui| {
                    close |= ui
                        .add_sized(
                            [112.0, visual::BUTTON_HEIGHT],
                            egui::Button::new(text(language, "action-close")),
                        )
                        .clicked();
                });
            });
        });
        if close || close_key {
            self.about_dialog_open = false;
        }
    }
}
