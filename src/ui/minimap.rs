//! The minimap of a Markdown file: a small rendering of the whole file beside the sheet.
//! Each table is a box with its name instead of its cells, and switches to that table.

use super::markdown_view::{Block, MarkdownViewStyle, parse_markdown, show_markdown};
use super::*;
use std::sync::OnceLock;

/// Larger files keep the plain table list, so drawing the whole file stays quick.
pub(super) const MINIMAP_MAX_BYTES: usize = 512 * 1024;
/// Body text size of the minimap: small, but the words can still be made out.
const MINIMAP_TEXT_SIZE: f32 = 4.5;

/// A minimap parse and the source it was parsed from, which its table offsets refer to.
pub(super) struct MinimapParse {
    source: Arc<crate::markdown_adapter::MarkdownDocument>,
    blocks: Vec<Block>,
}

/// The minimap parse of one document's current source, made off the UI thread. Each
/// document keeps its own, so windows showing different files never replace each other's.
pub(super) struct MinimapCache {
    source: Arc<crate::markdown_adapter::MarkdownDocument>,
    parse: Arc<OnceLock<Arc<MinimapParse>>>,
    /// The last finished parse, shown until the newer one finishes.
    previous: Option<Arc<MinimapParse>>,
}

impl TexcelApp {
    /// Returns the Markdown source when the document is a Markdown file small enough for
    /// the minimap; other documents keep the table list.
    pub(super) fn minimap_source(
        &self,
        document_id: DocumentId,
    ) -> Option<Arc<crate::markdown_adapter::MarkdownDocument>> {
        let backing = self.file_backings.get(&document_id)?;
        let markdown_file = backing
            .path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown")
            });
        let document = self.workspace.documents.get(&document_id)?;
        match document.source.as_deref() {
            Some(SourceDocument::Markdown(markdown))
                if markdown_file && markdown.source_bytes().len() <= MINIMAP_MAX_BYTES =>
            {
                Some(Arc::clone(markdown))
            }
            _ => None,
        }
    }

    /// Draws the minimap panel at the right of the sheet and returns a table to switch to.
    pub(super) fn draw_minimap_panel(&mut self, root: &mut egui::Ui) -> Option<TableId> {
        let document_id = self.workspace.active_document?;
        if !self.minimap_open {
            return None;
        }
        let source = self.minimap_source(document_id)?;
        let parse = self.minimap_parse(root.ctx(), document_id, &source);
        let document = self.workspace.documents.get(&document_id)?;
        let active_table = document.active_table;
        // Offsets name tables only in the source they were parsed from; an older parse shown
        // while the newer one runs draws its boxes without names.
        let labels = parse
            .as_ref()
            .filter(|parse| Arc::ptr_eq(&parse.source, &source))
            .map(|_| {
                source
                    .tables
                    .iter()
                    .zip(&document.tables)
                    .map(|(markdown, table)| {
                        (markdown.source_range.start, (table.id, table.name.clone()))
                    })
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();
        let mut chosen = None;
        let palette = GridThemePalette::from_visuals(root.visuals());
        egui::Panel::right("texcel_minimap")
            .resizable(false)
            .exact_size(self.minimap_width.points())
            .frame(
                egui::Frame::new()
                    .fill(palette.header_background)
                    .inner_margin(egui::Margin::same(6)),
            )
            .show(root, |ui| {
                let Some(parse) = parse else {
                    ui.centered_and_justified(|ui| ui.spinner());
                    return;
                };
                egui::ScrollArea::vertical()
                    .id_salt(("texcel_minimap_scroll", document_id))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // The minimap is a picture of the file to navigate by, not text to
                        // copy, so dragging over it selects nothing.
                        ui.style_mut().interaction.selectable_labels = false;
                        let output = show_markdown(
                            ui,
                            &parse.blocks,
                            MarkdownViewStyle {
                                body_size: MINIMAP_TEXT_SIZE,
                                // Each table is a box with its name; its cells are not drawn.
                                table_placeholders: true,
                            },
                        );
                        for (start, rect) in &output.table_rects {
                            let label = labels.get(start).map(|(id, name)| (*id, name.as_str()));
                            let active = label.is_some_and(|(id, _)| id == active_table);
                            if table_box(ui, *rect, label, active).is_some_and(|r| r.clicked()) {
                                chosen = label.map(|(id, _)| id);
                            }
                        }
                    });
            });
        chosen
    }

    /// Returns the newest finished parse, starting a background parse when the document's
    /// source changed. Cell edits only change what the boxes would hide, so they keep it.
    fn minimap_parse(
        &mut self,
        ctx: &egui::Context,
        document_id: DocumentId,
        source: &Arc<crate::markdown_adapter::MarkdownDocument>,
    ) -> Option<Arc<MinimapParse>> {
        let cache = self.minimap_caches.get(&document_id);
        if cache.is_none_or(|cache| !Arc::ptr_eq(&cache.source, source)) {
            let previous = cache.and_then(MinimapCache::finished);
            let parse = Arc::new(OnceLock::new());
            let target = Arc::clone(&parse);
            let parsed_source = Arc::clone(source);
            // Wake the window that asked; a bare repaint from this thread would reach the root.
            let repaint = ctx.clone();
            let viewport = ctx.viewport_id();
            std::thread::spawn(move || {
                let text = String::from_utf8_lossy(parsed_source.source_bytes());
                let blocks = parse_markdown(&text);
                let _ = target.set(Arc::new(MinimapParse {
                    source: parsed_source,
                    blocks,
                }));
                repaint.request_repaint_of(viewport);
            });
            self.minimap_caches.insert(
                document_id,
                MinimapCache {
                    source: Arc::clone(source),
                    parse,
                    previous,
                },
            );
        }
        self.minimap_caches
            .get(&document_id)
            .and_then(MinimapCache::finished)
    }
}

impl MinimapCache {
    /// Returns the newest finished parse: the current source's, or the last one until then.
    fn finished(&self) -> Option<Arc<MinimapParse>> {
        self.parse.get().cloned().or_else(|| self.previous.clone())
    }
}

/// Draws one table's box: a button named after the table, or only an outline while the
/// table's name is not known yet.
fn table_box(
    ui: &mut egui::Ui,
    rect: Rect,
    label: Option<(TableId, &str)>,
    active: bool,
) -> Option<egui::Response> {
    let visuals = ui.visuals().clone();
    let outline = Stroke::new(1.0, visuals.selection.stroke.color);
    let Some((table_id, name)) = label else {
        ui.painter_at(rect)
            .rect_stroke(rect, 3.0, outline, egui::StrokeKind::Inside);
        return None;
    };
    let id = Id::unique(egui::IdSalt::new(("texcel_minimap_table", table_id)));
    let response = ui
        .interact(rect, id, Sense::click())
        .on_hover_text(name.to_owned());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name));
    let fill = if active {
        visuals.selection.bg_fill.gamma_multiply(0.45)
    } else if response.hovered() {
        visuals.selection.bg_fill.gamma_multiply(0.25)
    } else {
        visuals.extreme_bg_color.gamma_multiply(0.55)
    };
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, fill);
    painter.rect_stroke(rect, 3.0, outline, egui::StrokeKind::Inside);
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        name,
        FontId::new(12.0, grid_font_family(true)),
        visuals.strong_text_color(),
    );
    Some(response)
}
