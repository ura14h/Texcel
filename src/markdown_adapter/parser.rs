use super::*;

/// Converts a parser range into offsets within the original byte-backed text.
fn offset_range(range: Range<usize>, offset: usize) -> Range<usize> {
    range.start + offset..range.end + offset
}

impl MarkdownDocument {
    // Finds tables with pulldown-cmark GFM extensions and keeps cell ranges as UTF-8 byte offsets.
    pub fn parse(bytes: Vec<u8>) -> Result<Self, MarkdownError> {
        Self::parse_with_progress(bytes, |_, _, _| true)?.ok_or(MarkdownError::OutputMismatch)
    }

    /// Parses each completed table and sends its source rows to a read-only preview.
    pub(crate) fn parse_with_progress(
        bytes: Vec<u8>,
        mut progress: impl FnMut(usize, usize, &[MarkdownCell]) -> bool,
    ) -> Result<Option<Self>, MarkdownError> {
        if bytes.len() > MAX_MARKDOWN_BYTES {
            return Err(MarkdownError::ResourceLimit(
                crate::i18n::ResourceLimit::FileSize,
            ));
        }
        let text = std::str::from_utf8(&bytes).map_err(|_| MarkdownError::InvalidUtf8)?;
        let parser_offset = usize::from(text.starts_with('\u{feff}')) * '\u{feff}'.len_utf8();
        let parser_text = text.get(parser_offset..).unwrap_or(text);
        let options = Options::ENABLE_TABLES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_GFM;
        let mut table_builder: Option<TableBuilder> = None;
        let mut raw_tables = Vec::<(
            usize,
            usize,
            Vec<Vec<MarkdownCell>>,
            usize,
            Vec<MarkdownRowSyntax>,
            Vec<Option<MarkdownTableAlignment>>,
        )>::new();
        let mut total_rows = 0_usize;
        let mut total_cells = 0_usize;
        let mut max_columns = 0_usize;

        for (event, range) in Parser::new_ext(parser_text, options).into_offset_iter() {
            match event {
                Event::Start(Tag::Table(alignments)) => {
                    table_builder = Some(TableBuilder {
                        start: range.start + parser_offset,
                        alignments: alignments
                            .into_iter()
                            .map(|alignment| match alignment {
                                pulldown_cmark::Alignment::None => None,
                                pulldown_cmark::Alignment::Left => {
                                    Some(MarkdownTableAlignment::Left)
                                }
                                pulldown_cmark::Alignment::Center => {
                                    Some(MarkdownTableAlignment::Center)
                                }
                                pulldown_cmark::Alignment::Right => {
                                    Some(MarkdownTableAlignment::Right)
                                }
                            })
                            .collect(),
                        ..TableBuilder::default()
                    });
                }
                Event::Start(Tag::TableHead | Tag::TableRow) => {
                    if let Some(builder) = &mut table_builder {
                        builder.current_row = Some(Vec::new());
                        builder.current_row_start = Some(range.start + parser_offset);
                    }
                }
                Event::Start(Tag::TableCell) => {
                    if let Some(builder) = &mut table_builder
                        && let Some(row) = &mut builder.current_row
                    {
                        total_cells = total_cells.saturating_add(1);
                        if total_cells > MAX_MARKDOWN_CELLS {
                            return Err(MarkdownError::ResourceLimit(
                                crate::i18n::ResourceLimit::CellCount,
                            ));
                        }
                        let cell_range = trim_cell_range(text, offset_range(range, parser_offset));
                        row.push(MarkdownCell {
                            value: text[cell_range.clone()].to_owned(),
                            source_range: cell_range,
                            has_source: false,
                        });
                    }
                }
                Event::End(TagEnd::TableHead | TagEnd::TableRow) => {
                    if let Some(builder) = &mut table_builder
                        && let Some(row) = builder.current_row.take()
                    {
                        let row_start = builder
                            .current_row_start
                            .take()
                            .unwrap_or(range.start + parser_offset);
                        let syntax = row_syntax(text, row_start..range.end + parser_offset);
                        total_rows = total_rows.saturating_add(1);
                        if total_rows > MAX_MARKDOWN_ROWS {
                            return Err(MarkdownError::ResourceLimit(
                                crate::i18n::ResourceLimit::RowCount,
                            ));
                        }
                        if row.len() > MAX_MARKDOWN_COLUMNS {
                            return Err(MarkdownError::ResourceLimit(
                                crate::i18n::ResourceLimit::ColumnCount,
                            ));
                        }
                        let mut row = row;
                        if builder.rows.is_empty() {
                            builder.parser_width = row.len();
                        }
                        for (index, cell) in row.iter_mut().enumerate() {
                            cell.has_source = index < syntax.source_cell_count;
                        }
                        // pulldown exposes only the separator-defined width. Keep lexical cells
                        // beyond that width in the model so the grid and explicit normalization
                        // can show and preserve them.
                        for column in row.len()..syntax.source_cell_count {
                            total_cells = total_cells.saturating_add(1);
                            if total_cells > MAX_MARKDOWN_CELLS {
                                return Err(MarkdownError::ResourceLimit(
                                    crate::i18n::ResourceLimit::CellCount,
                                ));
                            }
                            if row.len() >= MAX_MARKDOWN_COLUMNS {
                                return Err(MarkdownError::ResourceLimit(
                                    crate::i18n::ResourceLimit::ColumnCount,
                                ));
                            }
                            let raw_range = syntax
                                .cell_ranges
                                .get(column)
                                .cloned()
                                .ok_or(MarkdownError::OutputMismatch)?;
                            let cell_range = trim_cell_range(text, raw_range);
                            row.push(MarkdownCell {
                                value: text[cell_range.clone()].to_owned(),
                                source_range: cell_range,
                                has_source: true,
                            });
                        }
                        max_columns = max_columns.max(row.len());
                        builder.row_syntax.push(syntax);
                        builder.rows.push(row);
                    }
                }
                Event::End(TagEnd::Table) => {
                    if let Some(builder) = table_builder.take() {
                        let table_index = raw_tables.len();
                        for (row_index, row) in builder.rows.iter().enumerate() {
                            if !progress(table_index, row_index, row) {
                                return Ok(None);
                            }
                        }
                        raw_tables.push((
                            builder.start,
                            range.end + parser_offset,
                            builder.rows,
                            builder.parser_width,
                            builder.row_syntax,
                            builder.alignments,
                        ));
                    }
                }
                _ => (),
            }
        }

        let mut tables = Vec::with_capacity(raw_tables.len());
        let mut heading_ranges = Vec::<(Range<usize>, String)>::new();
        let mut current = None::<(usize, String)>;
        let mut displays = Vec::<Vec<Vec<MarkdownCellDisplay>>>::new();
        let mut display_table = None::<Vec<Vec<MarkdownCellDisplay>>>;
        let mut display_row = None::<Vec<MarkdownCellDisplay>>;
        let mut display_cell = None::<InlineDisplayBuilder>;
        let mut inline_style = InlineStyleState::default();
        for (event, range) in Parser::new_ext(parser_text, options).into_offset_iter() {
            match event {
                Event::Start(Tag::Heading { .. }) => {
                    current = Some((range.start + parser_offset, String::new()))
                }
                Event::Text(value) | Event::Code(value) if current.is_some() => {
                    if let Some((_, title)) = &mut current {
                        title.push_str(&value);
                    }
                }
                Event::End(TagEnd::Heading(_)) => {
                    if let Some((start, title)) = current.take() {
                        heading_ranges
                            .push((start..range.end + parser_offset, title.trim().to_owned()));
                    }
                }
                Event::Start(Tag::Table(_)) => display_table = Some(Vec::new()),
                Event::Start(Tag::TableHead | Tag::TableRow) => display_row = Some(Vec::new()),
                Event::Start(Tag::TableCell) => {
                    display_cell = Some(InlineDisplayBuilder {
                        source_range: trim_cell_range(text, offset_range(range, parser_offset)),
                        ..InlineDisplayBuilder::default()
                    });
                }
                Event::Start(Tag::Strong) => inline_style.bold += 1,
                Event::Start(Tag::Emphasis) => inline_style.italic += 1,
                Event::Start(Tag::Strikethrough) => inline_style.strike += 1,
                Event::Start(Tag::Link {
                    dest_url, title, ..
                }) => {
                    let link = crate::document::CellHyperlink {
                        target: dest_url.to_string(),
                        tooltip: (!title.is_empty()).then(|| title.to_string()),
                        location: false,
                    };
                    if let Some(cell) = display_cell.as_mut()
                        && cell.first_link_target.is_none()
                    {
                        cell.first_link_target = Some(link.target.clone());
                        cell.first_link_tooltip = link.tooltip.clone();
                    }
                    inline_style.links.push(link);
                }
                Event::End(TagEnd::Strong) => {
                    inline_style.bold = inline_style.bold.saturating_sub(1)
                }
                Event::End(TagEnd::Emphasis) => {
                    inline_style.italic = inline_style.italic.saturating_sub(1);
                }
                Event::End(TagEnd::Strikethrough) => {
                    inline_style.strike = inline_style.strike.saturating_sub(1);
                }
                Event::End(TagEnd::Link) => {
                    inline_style.links.pop();
                }
                Event::Text(value) if display_cell.is_some() => {
                    if let Some(cell) = display_cell.as_mut() {
                        cell.push(
                            &value,
                            inline_style.run(false),
                            Some(offset_range(range, parser_offset)),
                            Some(text),
                            true,
                        );
                    }
                }
                Event::Code(value) if display_cell.is_some() => {
                    if let Some(cell) = display_cell.as_mut() {
                        cell.push(
                            &value,
                            inline_style.run(true),
                            Some(offset_range(range, parser_offset)),
                            Some(text),
                            true,
                        );
                    }
                }
                Event::SoftBreak if display_cell.is_some() => {
                    if let Some(cell) = display_cell.as_mut() {
                        cell.push(" ", inline_style.run(false), None, None, false);
                    }
                }
                Event::HardBreak if display_cell.is_some() => {
                    if let Some(cell) = display_cell.as_mut() {
                        cell.push("\n", inline_style.run(false), None, None, false);
                    }
                }
                Event::InlineHtml(html) | Event::Html(html) if display_cell.is_some() => {
                    if is_markdown_break(&html)
                        && let Some(cell) = display_cell.as_mut()
                    {
                        cell.push("\n", InlineDisplayStyle::default(), None, None, false);
                    }
                }
                Event::End(TagEnd::Image) if display_cell.is_some() => {
                    if let Some(cell) = display_cell.as_mut() {
                        cell.push_image();
                    }
                }
                Event::TaskListMarker(checked) if display_cell.is_some() => {
                    if let Some(cell) = display_cell.as_mut() {
                        cell.push(
                            if checked { "☑ " } else { "☐ " },
                            InlineDisplayStyle::default(),
                            None,
                            None,
                            false,
                        );
                    }
                }
                Event::End(TagEnd::TableCell) => {
                    if let (Some(cell), Some(row)) = (display_cell.take(), display_row.as_mut()) {
                        let source = text.get(cell.source_range.clone()).unwrap_or_default();
                        row.push(cell.finish(source));
                    }
                }
                Event::End(TagEnd::TableHead | TagEnd::TableRow) => {
                    if let (Some(row), Some(table)) = (display_row.take(), display_table.as_mut()) {
                        table.push(row);
                    }
                }
                Event::End(TagEnd::Table) => {
                    if let Some(table) = display_table.take() {
                        displays.push(table);
                    }
                }
                _ => (),
            }
        }
        let table_ranges = raw_tables
            .iter()
            .map(|(start, end, ..)| *start..*end)
            .collect::<Vec<_>>();
        let names = table_names::name_tables(text, &heading_ranges, &table_ranges);
        for (index, ((start, end, rows, parser_width, row_syntax, alignments), name)) in
            raw_tables.into_iter().zip(names).enumerate()
        {
            let header_end = row_syntax.first().map_or(start, |syntax| syntax.range.end);
            let after_header = text.get(header_end..end).unwrap_or_default();
            let separator_start = after_header
                .find('\n')
                .map_or(0, |offset| offset.saturating_add(1));
            let separator_rest = after_header.get(separator_start..).unwrap_or_default();
            let separator_end = separator_rest.find('\n').unwrap_or(separator_rest.len());
            let raw_separator = &separator_rest[..separator_end];
            let separator_line = raw_separator
                .strip_suffix('\r')
                .unwrap_or(raw_separator)
                .to_owned();
            let separator_range =
                header_end + separator_start..header_end + separator_start + raw_separator.len();
            let newline = if text.get(header_end..header_end + 2) == Some("\r\n") {
                "\r\n"
            } else {
                "\n"
            }
            .to_owned();
            tables.push(MarkdownTable {
                name_generated: name.name.is_none(),
                name: name.name.unwrap_or_else(|| format!("表{}", index + 1)),
                rows,
                alignments,
                source_range: start..end,
                heading_range: name.heading_range,
                parser_width,
                row_syntax,
                separator_line,
                separator_range,
                newline,
            });
        }
        let source_extents = tables
            .iter()
            .map(|table| {
                (
                    table.rows.len().max(1),
                    table
                        .row_syntax
                        .iter()
                        .map(|row| row.source_cell_count)
                        .max()
                        .unwrap_or(0)
                        .max(1),
                )
            })
            .collect();
        Ok(Some(Self {
            bytes,
            tables,
            displays,
            source_extents,
            parsed_row_count: total_rows,
            parsed_cell_count: total_cells,
            parsed_max_column_count: max_columns,
        }))
    }
}
