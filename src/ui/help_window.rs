//! The app-wide Help window: a resizable, modeless window with the built-in guide.

use super::markdown_view::{Block, MarkdownViewStyle, parse_markdown, show_markdown};
use super::*;

/// Returns the viewport that every document window shares for Help.
pub(super) fn help_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("texcel-help")
}

/// Returns the built-in guide for one language.
fn help_markdown(language: Language) -> &'static str {
    match language {
        Language::English => include_str!("../../assets/help/help.en.md"),
        Language::Japanese => include_str!("../../assets/help/help.ja.md"),
    }
}

impl TexcelApp {
    /// Opens the Help window, or brings it forward when it is already open.
    pub(super) fn open_help_window(&mut self, ctx: &egui::Context) {
        if self.help_viewport_open {
            ctx.send_viewport_cmd_to(help_viewport_id(), egui::ViewportCommand::Focus);
        }
        self.help_viewport_open = true;
        ctx.request_repaint();
    }

    /// Describes the Help window's native frame.
    pub(super) fn help_viewport_builder(&self) -> egui::ViewportBuilder {
        egui::ViewportBuilder::default()
            .with_title(text(self.language, "help-window-title"))
            .with_inner_size([720.0, 680.0])
            .with_min_inner_size([360.0, 280.0])
            .with_resizable(true)
    }

    /// Draws the guide in the current language and closes the window on request.
    pub(super) fn draw_help_window_contents(&mut self, ui: &mut egui::Ui) {
        if ui.input(|input| input.viewport().close_requested()) {
            self.help_viewport_open = false;
            return;
        }
        let language = self.language;
        let blocks = match &self.help_blocks {
            Some((cached, blocks)) if *cached == language => Arc::clone(blocks),
            _ => {
                let blocks = Arc::new(parse_markdown(help_markdown(language)));
                self.help_blocks = Some((language, Arc::clone(&blocks)));
                blocks
            }
        };
        egui::Frame::new()
            .fill(ui.visuals().panel_fill)
            .inner_margin(egui::Margin::symmetric(24, 16))
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                egui::ScrollArea::vertical()
                    .id_salt("texcel_help_body")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        show_markdown(
                            ui,
                            &blocks,
                            MarkdownViewStyle {
                                body_size: visual::BODY_SIZE,
                            },
                        );
                    });
            });
    }

    /// Shows Help as an immediate viewport when no window host owns the viewports.
    pub(super) fn draw_help_viewport_immediate(&mut self, ctx: &egui::Context) {
        if !self.help_viewport_open {
            return;
        }
        let builder = self.help_viewport_builder();
        ctx.show_viewport_immediate(help_viewport_id(), builder, |ui, _class| {
            self.draw_help_window_contents(ui);
        });
    }
}

/// Parsed guide cached with the language it was parsed for.
pub(super) type HelpBlocks = Option<(Language, Arc<Vec<Block>>)>;
