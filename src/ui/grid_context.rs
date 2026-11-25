use super::*;

/// The table one grid shows, the layout of its view, and the Markdown source behind it.
#[derive(Clone, Copy)]
pub(super) struct GridTable<'a> {
    pub(super) document: &'a Document,
    pub(super) table: &'a crate::document::Table,
    /// The view's layout at the sheet zoom; it may differ from the layout saved with the table.
    pub(super) layout: ScaledLayout<'a>,
    /// True when the document is Markdown, so cell text is Markdown to format.
    pub(super) is_markdown: bool,
    pub(super) markdown_source: Option<&'a crate::markdown_adapter::MarkdownDocument>,
    /// The table's position in the Markdown source, used to look up parsed cells.
    pub(super) markdown_table_index: Option<usize>,
}

impl<'a> GridTable<'a> {
    /// Collects the view layout at a grid scale and the Markdown source of one table.
    pub(super) fn new(
        document: &'a Document,
        table: &'a crate::document::Table,
        scale: f32,
    ) -> Self {
        let markdown_source = match document.source.as_deref() {
            Some(SourceDocument::Markdown(markdown)) => Some(markdown.as_ref()),
            Some(SourceDocument::Csv(_)) | None => None,
        };
        Self {
            document,
            table,
            layout: ScaledLayout::new(
                document
                    .view(table.id)
                    .map_or(&table.layout, |view| view.layout.as_ref()),
                scale,
            ),
            is_markdown: markdown_source.is_some(),
            markdown_source,
            markdown_table_index: markdown_source.and_then(|_| {
                document
                    .tables
                    .iter()
                    .position(|candidate| candidate.id == table.id)
            }),
        }
    }
}

/// How the grid displays cell text. These settings never change the document.
#[derive(Clone, Copy, Debug)]
pub(super) struct GridDisplay {
    /// Live editing shows results and formatting; source editing shows the raw text.
    pub(super) mode: CellDisplayMode,
    pub(super) show_invisibles: bool,
    /// The table's wrap setting, or `None` to follow each cell's own style.
    pub(super) wrap_text: bool,
    /// Changes whenever the fonts change, so cached text measurements are redone.
    pub(super) font_layout_generation: u64,
    /// Sheet zoom relative to the app's own scale; every grid size is multiplied by it.
    pub(super) scale: f32,
}

/// What the user may do in the grid this frame, and drags or jumps already under way.
pub(super) struct GridInput<'a> {
    /// False while the document is locked, for example during a save.
    pub(super) allow_edit: bool,
    pub(super) accesskit_table_token: Option<u32>,
    pub(super) navigation_blocked: bool,
    pub(super) calculation_pending: bool,
    /// Why row, column, and fill commands are unavailable, if they are.
    pub(super) structure_block_reason: Option<&'a str>,
    pub(super) context_commands_enabled: [bool; 6],
    /// True while a formula is being edited and clicks insert cell references.
    pub(super) formula_reference_mode: bool,
    pub(super) formula_reference_overlays: &'a [FormulaReferenceOverlay],
    pub(super) fill_handle_drag: Option<FillHandleDrag>,
    pub(super) selection_drag: Option<GridSelectionDrag>,
    pub(super) scroll_to_address: Option<CellAddress>,
}

/// App state that the grid reads and updates while it draws.
pub(super) struct GridState<'a> {
    pub(super) edit: &'a mut EditSession,
    pub(super) request_editor_focus: &'a mut bool,
    pub(super) request_grid_focus: &'a mut bool,
    pub(super) request_scroll_to_selection: &'a mut bool,
    pub(super) header_resize_drag: &'a mut Option<HeaderResizeDrag>,
    /// Parsed Markdown for edited cells, shared with the background preview worker.
    pub(super) markdown_cell_displays:
        &'a mut BTreeMap<MarkdownCellDisplayKey, CachedMarkdownCellDisplay>,
    /// Edited cells whose Markdown still needs to be parsed by the worker.
    pub(super) markdown_display_requests: &'a mut Vec<(u64, CellAddress)>,
}
