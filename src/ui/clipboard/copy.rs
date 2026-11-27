use super::*;

#[path = "copy_markdown.rs"]
mod markdown;
use markdown::{attach_markdown_copy_content, markdown_source_cell_copy_content};

impl TexcelApp {
    // Copy the selected grid ranges to the system clipboard.
    pub(in crate::ui) fn copy_selected_grid(
        &mut self,
        ctx: &egui::Context,
        format: CopyFormat,
    ) -> bool {
        let (output, content) = match format {
            CopyFormat::DisplayTsv => (CopyFormat::DisplayTsv, CopyContentChoice::Standard),
            output => (output, CopyContentChoice::Source),
        };
        self.copy_selected_grid_with_content(ctx, output, content)
    }

    /// Copies one selection using independent content and output choices.
    pub(in crate::ui) fn copy_selected_grid_with_content(
        &mut self,
        ctx: &egui::Context,
        format: CopyFormat,
        content_choice: CopyContentChoice,
    ) -> bool {
        let Some(document) = self.workspace.active_document() else {
            self.status = Some(Message::new("status-copy-table-required"));
            return false;
        };
        let Some(table) = document.active_table() else {
            return false;
        };
        let Some(view) = document.view(table.id) else {
            return false;
        };
        let table_id = table.id;
        let document_id = document.id;
        let document_revision = document.revision;
        let selected_ranges = view.selected_ranges().collect::<Vec<_>>();
        let extent_end = document.extent_end(table_id).unwrap_or_default();
        let extent_rows = extent_end.row.saturating_add(1);
        let Some(selected_ranges) = bounded_copy_ranges(
            &selected_ranges,
            extent_end,
            crate::clipboard::MAX_CLIPBOARD_CELLS,
            extent_rows,
        ) else {
            self.status = Some(Message::new("status-copy-range-too-large"));
            return false;
        };
        let markdown_source = match document.source.as_deref() {
            Some(SourceDocument::Markdown(source)) => Some(Arc::clone(source)),
            _ => None,
        };
        let markdown_table_index = markdown_source.as_ref().and_then(|_| {
            document
                .tables
                .iter()
                .position(|candidate| candidate.id == table_id)
        });
        let markdown_alignments = if format == CopyFormat::MarkdownTable {
            selected_ranges
                .iter()
                .map(|range| {
                    copied_markdown_alignments(
                        markdown_source.as_deref(),
                        markdown_table_index,
                        *range,
                    )
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let copied_ranges = selected_ranges.clone();
        let origins = copied_ranges
            .iter()
            .map(|range| range.normalized().0)
            .collect::<Vec<_>>();
        let mut pending_markdown_copy = Vec::new();
        let mut unavailable_formula_result = false;
        let mut internal_ranges = Vec::with_capacity(selected_ranges.len());
        let mut visible_ranges = Vec::with_capacity(selected_ranges.len());
        for range in selected_ranges {
            let (start, end) = range.normalized();
            let mut internal_matrix = Vec::new();
            let mut visible_matrix = Vec::new();
            for row in selected_rows(range, extent_rows) {
                let mut internal_row = Vec::with_capacity(end.column - start.column + 1);
                let mut visible_row = Vec::with_capacity(end.column - start.column + 1);
                for column in start.column..=end.column {
                    let address = CellAddress { row, column };
                    let original = table.cell(address).cloned().unwrap_or_else(Cell::empty);
                    let calculation = document.calculation_result_for(table_id, address);
                    let formula_source =
                        crate::structure::is_formula_cell(table, address, &original);
                    let source_for_copy = original.clone();
                    let mut internal = ClipboardCell::from_cell_with_formula_context(
                        &source_for_copy,
                        formula_source,
                        calculation.as_ref(),
                    );
                    internal.raw_markdown_source =
                        content_choice != CopyContentChoice::Display && markdown_source.is_some();
                    let markdown_content = if content_choice != CopyContentChoice::Source
                        && let Some(markdown_source) = markdown_source.as_deref()
                        && !formula_source
                        && let CellInput::Text(raw) = &source_for_copy.input
                    {
                        markdown_source_cell_copy_content(
                            markdown::MarkdownCopyContext {
                                document,
                                source: markdown_source,
                                table_index: markdown_table_index,
                                table_id,
                                language: crate::i18n::context_language(ctx),
                                cache: &self.markdown_cell_displays,
                            },
                            address,
                            raw,
                            &mut pending_markdown_copy,
                        )
                    } else {
                        None
                    };
                    if content_choice == CopyContentChoice::Standard
                        && let Some(content) = &markdown_content
                    {
                        attach_markdown_copy_content(&mut internal, content);
                    }
                    let display_text = (content_choice != CopyContentChoice::Source).then(|| {
                        copy_content::displayed_text(
                            &source_for_copy,
                            calculation.as_ref(),
                            markdown_content
                                .as_ref()
                                .map(|content| content.plain_text.as_str()),
                        )
                    });
                    if content_choice == CopyContentChoice::Display
                        && let Some(display_text) = &display_text
                    {
                        unavailable_formula_result |= !copy_content::replace_with_displayed_result(
                            &mut internal,
                            display_text,
                        );
                    }
                    if format == CopyFormat::Rich && content_choice != CopyContentChoice::Source {
                        internal.rich_text = markdown_content
                            .as_ref()
                            .filter(|content| content.has_features)
                            .map(|content| content.rich_text.clone());
                    }
                    let mut visible = internal.clone();
                    if let Some(display_text) = display_text {
                        visible.input = CellInput::Text(display_text);
                        visible.rich_text = if content_choice == CopyContentChoice::Source {
                            None
                        } else {
                            markdown_content
                                .as_ref()
                                .filter(|content| content.has_features)
                                .map(|content| content.rich_text.clone())
                        };
                    } else if content_choice == CopyContentChoice::Source {
                        visible.rich_text = None;
                    }
                    if format != CopyFormat::Rich {
                        visible.clear_pasted_style();
                    }
                    internal_row.push(internal);
                    visible_row.push(visible);
                }
                internal_matrix.push(internal_row);
                visible_matrix.push(visible_row);
            }
            if internal_matrix.is_empty() {
                self.status = Some(Message::new("status-filtered-copy-rows-empty"));
                return false;
            }
            internal_ranges.push(internal_matrix);
            visible_ranges.push(visible_matrix);
        }
        if unavailable_formula_result {
            self.status = Some(Message::new("status-copy-result-not-fresh"));
            return false;
        }
        if !pending_markdown_copy.is_empty() {
            for (address, cell_input_revision, text) in pending_markdown_copy {
                let key = (document_id, table_id, address);
                if self
                    .pending_markdown_displays
                    .get(&key)
                    .is_some_and(|(_, revision)| *revision == cell_input_revision)
                {
                    continue;
                }
                let Some(source) = markdown_source.as_ref() else {
                    continue;
                };
                let Some(markdown_table_index) = markdown_table_index else {
                    continue;
                };
                let token = self.next_job_token(document_id, document_revision);
                if self
                    .file_worker
                    .prepare_markdown_cell_display(
                        token,
                        table_id,
                        address,
                        cell_input_revision,
                        Arc::clone(source),
                        crate::markdown_adapter::MarkdownCellChange {
                            table: markdown_table_index,
                            row: address.row,
                            column: address.column,
                            value: text,
                        },
                    )
                    .is_ok()
                {
                    self.pending_markdown_displays
                        .insert(key, (token, cell_input_revision));
                }
            }
            self.status = Some(Message::new("status-markdown-display-copy-pending"));
            return false;
        }
        let text = if format == CopyFormat::MarkdownTable {
            format_ranges_plain_text_with_alignments(&visible_ranges, format, &markdown_alignments)
        } else {
            format_ranges_plain_text(&visible_ranges, format)
        };
        let html = match create_html_clipboard(&internal_ranges, &visible_ranges, &origins, &text) {
            Ok(html) => html,
            Err(error) => {
                self.status = Some(error.0);
                return false;
            }
        };
        #[cfg(test)]
        if super::copy_options_tests::capture_if_requested(ctx, &html, &text) {
            return true;
        }
        match write_system_clipboard(&html, &text) {
            Ok(()) => {
                self.status = None;
                self.set_clipboard_mark(
                    crate::clipboard::owned_payload_owner_id(),
                    document_id,
                    table_id,
                    copied_ranges,
                    document_revision,
                );
                true
            }
            Err(error) => {
                // Without Texcel data a later paste cannot move or outline these cells.
                self.clipboard_mark = None;
                if write_system_text(&text).is_ok() {
                    self.status = Some(
                        Message::new("status-clipboard-html-failed")
                            .with_message("detail", error.0),
                    );
                    true
                } else {
                    ctx.copy_text(text);
                    self.status = Some(
                        Message::new("status-clipboard-text-failed")
                            .with_message("detail", error.0),
                    );
                    false
                }
            }
        }
    }

    // Submit clipboard text for parsing and paste it into the active selection.
}

/// Reads the current source alignment for each column in one copied range.
fn copied_markdown_alignments(
    source: Option<&crate::markdown_adapter::MarkdownDocument>,
    table_index: Option<usize>,
    range: CellRange,
) -> Vec<Option<crate::markdown_adapter::MarkdownTableAlignment>> {
    let (start, end) = range.normalized();
    (start.column..=end.column)
        .map(|column| source.and_then(|source| source.column_alignment(table_index?, column)))
        .collect()
}

/// Bounds whole-axis copies to the document extent before copy allocates a matrix.
fn bounded_copy_ranges(
    ranges: &[CellRange],
    extent_end: CellAddress,
    max_cells: usize,
    extent_rows: usize,
) -> Option<Vec<CellRange>> {
    let mut bounded = Vec::with_capacity(ranges.len());
    let mut area = 0_usize;
    for range in ranges {
        let (start, end) = range.normalized();
        let whole_axis = (start.row == 0 && end.row == MAX_ROWS - 1)
            || (start.column == 0 && end.column == MAX_COLUMNS - 1);
        let effective = if whole_axis {
            let end = CellAddress {
                row: end.row.min(extent_end.row),
                column: end.column.min(extent_end.column),
            };
            if end.row < start.row || end.column < start.column {
                return None;
            }
            CellRange {
                anchor: start,
                head: end,
            }
        } else {
            *range
        };
        let (start, end) = effective.normalized();
        let effective_range = CellRange {
            anchor: start,
            head: end,
        };
        area = area.saturating_add(
            selected_row_count(effective_range, extent_rows)
                .saturating_mul(end.column.saturating_sub(start.column).saturating_add(1)),
        );
        if area > max_cells {
            return None;
        }
        bounded.push(effective_range);
    }
    Some(bounded)
}

#[cfg(test)]
#[path = "copy_test.rs"]
mod tests;
