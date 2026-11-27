//! The minimap of a Markdown file: a small rendering of the whole file beside the sheet,
//! with each table as a button that switches to it.

use super::markdown_view::{Block, MarkdownViewStyle, parse_markdown, show_markdown};
use super::*;
use std::sync::OnceLock;

/// Larger files keep the plain table list, so drawing the whole file stays quick.
pub(super) const MINIMAP_MAX_BYTES: usize = 512 * 1024;
/// Body text size of the minimap: small, but the words can still be made out.
const MINIMAP_TEXT_SIZE: f32 = 4.5;

/// Blocks parsed off the UI thread for one revision of one document.
pub(super) struct MinimapCache {
    document_id: DocumentId,
    revision: u64,
    blocks: Arc<OnceLock<Arc<Vec<Block>>>>,
    /// The last finished blocks, shown until the newer parse finishes.
    previous: Option<Arc<Vec<Block>>>,
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
        let blocks = self.minimap_blocks(root.ctx(), document_id, source);
        let document = self.workspace.documents.get(&document_id)?;
        let tables = document
            .tables
            .iter()
            .map(|table| (table.id, table.name.clone()))
            .collect::<Vec<_>>();
        let active_table = document.active_table;
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
                let Some(blocks) = blocks else {
                    ui.centered_and_justified(|ui| ui.spinner());
                    return;
                };
                egui::ScrollArea::vertical()
                    .id_salt(("texcel_minimap_scroll", document_id))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let output = show_markdown(
                            ui,
                            &blocks,
                            MarkdownViewStyle {
                                body_size: MINIMAP_TEXT_SIZE,
                            },
                        );
                        // Markdown tables are the document's tables in the same order.
                        for (rect, (table_id, name)) in output.table_rects.iter().zip(&tables) {
                            let active = *table_id == active_table;
                            if table_button(ui, *rect, *table_id, name, active).clicked() {
                                chosen = Some(*table_id);
                            }
                        }
                    });
            });
        chosen
    }

    /// Returns parsed blocks for the current revision, starting a background parse when the
    /// document changed. Unsaved cell edits are applied to the source in that background step.
    fn minimap_blocks(
        &mut self,
        ctx: &egui::Context,
        document_id: DocumentId,
        source: Arc<crate::markdown_adapter::MarkdownDocument>,
    ) -> Option<Arc<Vec<Block>>> {
        let document = self.workspace.documents.get(&document_id)?;
        let revision = document.revision;
        let current = self
            .minimap_cache
            .as_ref()
            .is_some_and(|cache| cache.document_id == document_id && cache.revision == revision);
        if !current {
            let changes = minimap_cell_changes(document);
            let previous = self.minimap_cache.as_ref().and_then(|cache| {
                (cache.document_id == document_id)
                    .then(|| {
                        cache
                            .blocks
                            .get()
                            .cloned()
                            .or_else(|| cache.previous.clone())
                    })
                    .flatten()
            });
            let blocks = Arc::new(OnceLock::new());
            let target = Arc::clone(&blocks);
            let repaint = ctx.clone();
            std::thread::spawn(move || {
                let updated = source.apply_changes(&changes).ok();
                let bytes = updated
                    .as_ref()
                    .map_or_else(|| source.source_bytes(), |updated| updated.source_bytes());
                let text = String::from_utf8_lossy(bytes);
                let _ = target.set(Arc::new(parse_markdown(&text)));
                repaint.request_repaint();
            });
            self.minimap_cache = Some(MinimapCache {
                document_id,
                revision,
                blocks,
                previous,
            });
        }
        self.minimap_cache.as_ref().and_then(|cache| {
            cache
                .blocks
                .get()
                .cloned()
                .or_else(|| cache.previous.clone())
        })
    }
}

/// Lists the cells edited since the source was last written, as Markdown cell changes.
fn minimap_cell_changes(
    document: &crate::document::Document,
) -> Vec<crate::markdown_adapter::MarkdownCellChange> {
    document
        .changed_cell_snapshot()
        .into_iter()
        .filter_map(|(table_id, address, cell)| {
            let table = document
                .tables
                .iter()
                .position(|table| table.id == table_id)?;
            Some(crate::markdown_adapter::MarkdownCellChange {
                table,
                row: address.row,
                column: address.column,
                value: cell.map_or_else(String::new, |cell| cell.input.source_text()),
            })
        })
        .collect()
}

/// Covers one drawn table with a button named after the table.
fn table_button(
    ui: &mut egui::Ui,
    rect: Rect,
    table_id: TableId,
    name: &str,
    active: bool,
) -> egui::Response {
    let id = Id::unique(egui::IdSalt::new(("texcel_minimap_table", table_id)));
    let response = ui
        .interact(rect, id, Sense::click())
        .on_hover_text(name.to_owned());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name));
    let visuals = ui.visuals();
    let fill = if active {
        visuals.selection.bg_fill.gamma_multiply(0.45)
    } else if response.hovered() {
        visuals.selection.bg_fill.gamma_multiply(0.25)
    } else {
        visuals.extreme_bg_color.gamma_multiply(0.55)
    };
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, fill);
    painter.rect_stroke(
        rect,
        3.0,
        Stroke::new(1.0, visuals.selection.stroke.color),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        name,
        FontId::new(12.0, grid_font_family(true)),
        visuals.strong_text_color(),
    );
    response
}
