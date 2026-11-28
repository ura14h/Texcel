//! The app-wide Help window: a resizable, modeless window with the built-in guide and an
//! outline of its headings beside it.

use super::markdown_view::{Block, MarkdownViewStyle, parse_markdown, show_markdown};
use super::*;

/// Width of the outline pane left of the guide.
const OUTLINE_WIDTH: f32 = 200.0;
/// How far below the top of the guide a heading still counts as the current section.
const CURRENT_HEADING_SLACK: f32 = 24.0;

/// Which heading the outline marks as current, and which one it asked the guide to show.
#[derive(Default)]
pub(super) struct HelpOutline {
    current: Option<usize>,
    scroll_to: Option<usize>,
}

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
            .with_inner_size([920.0, 680.0])
            .with_min_inner_size([560.0, 280.0])
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
        if let Some(index) = draw_help_outline(ui, &blocks, self.help_outline.current) {
            self.help_outline.scroll_to = Some(index);
        }
        // The scroll bar runs along the window edge; the text keeps its margin inside it.
        egui::Frame::new()
            .fill(ui.visuals().panel_fill)
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                egui::ScrollArea::vertical()
                    .id_salt("texcel_help_body")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let view_top = ui.clip_rect().top();
                        let output = egui::Frame::new()
                            .inner_margin(egui::Margin {
                                left: 28,
                                right: 28,
                                top: 20,
                                bottom: 28,
                            })
                            .show(ui, |ui| {
                                show_markdown(
                                    ui,
                                    &blocks,
                                    MarkdownViewStyle {
                                        body_size: visual::BODY_SIZE,
                                        table_placeholders: false,
                                    },
                                )
                            })
                            .inner;
                        if let Some(target) = self.help_outline.scroll_to.take()
                            && let Some((_, rect)) = output
                                .heading_rects
                                .iter()
                                .find(|(index, _)| *index == target)
                        {
                            ui.scroll_to_rect(*rect, Some(egui::Align::TOP));
                        }
                        // The current section is the last heading at or above the view's top.
                        let current = output
                            .heading_rects
                            .iter()
                            .take_while(|(_, rect)| rect.top() <= view_top + CURRENT_HEADING_SLACK)
                            .last()
                            .or(output.heading_rects.first())
                            .map(|(index, _)| *index);
                        if current != self.help_outline.current {
                            self.help_outline.current = current;
                            ui.ctx().request_repaint();
                        }
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

/// Lists the guide's headings as a tree in a pane at the left and returns the one clicked.
fn draw_help_outline(ui: &mut egui::Ui, blocks: &[Block], current: Option<usize>) -> Option<usize> {
    let mut chosen = None;
    egui::Panel::left("texcel_help_outline")
        .resizable(false)
        .exact_size(OUTLINE_WIDTH)
        .frame(
            egui::Frame::new()
                .fill(ui.visuals().faint_bg_color)
                .inner_margin(egui::Margin::symmetric(0, 12)),
        )
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("texcel_help_outline")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    visual::prepare_context_menu(ui);
                    for (index, block) in blocks.iter().enumerate() {
                        let Block::Heading(level, inline) = block else {
                            continue;
                        };
                        let title = inline
                            .iter()
                            .map(|span| span.text.as_str())
                            .collect::<String>();
                        // The title is the root; each deeper level is indented under it.
                        let indent = 12.0 + 14.0 * f32::from(level.saturating_sub(1));
                        let selected = current == Some(index);
                        let (rect, response) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), ui.spacing().interact_size.y),
                            Sense::click(),
                        );
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(egui::Role::Button, true, selected, &title)
                        });
                        let visuals = ui.style().interact_selectable(&response, selected);
                        if selected || response.hovered() {
                            ui.painter().rect_filled(rect, 0.0, visuals.weak_bg_fill);
                        }
                        let font = if *level <= 1 {
                            FontId::new(visual::BODY_SIZE, grid_font_family(true))
                        } else {
                            FontId::proportional(visual::BODY_SIZE)
                        };
                        ui.painter()
                            .with_clip_rect(rect.intersect(ui.clip_rect()))
                            .text(
                                egui::pos2(rect.left() + indent, rect.center().y),
                                Align2::LEFT_CENTER,
                                &title,
                                font,
                                visuals.text_color(),
                            );
                        if response.clicked() {
                            chosen = Some(index);
                        }
                    }
                });
        });
    chosen
}
