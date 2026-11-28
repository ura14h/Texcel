//! Turns GitHub-flavored Markdown into a small block tree for drawing.

use crate::markdown_adapter::{MARKDOWN_OPTIONS, markdown_body};
use pulldown_cmark::{Alignment, Event, HeadingLevel, Parser, Tag, TagEnd};

/// One run of inline text with its marks.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Span {
    pub(crate) text: String,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub(crate) strike: bool,
    pub(crate) code: bool,
    pub(crate) link: bool,
}

/// Inline content of a paragraph, heading, or table cell.
pub(crate) type Inline = Vec<Span>;

/// A block of Markdown content, in document order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Block {
    Heading(u8, Inline),
    Paragraph(Inline),
    /// Items of a list; `start` is the first number of an ordered list.
    List {
        start: Option<u64>,
        items: Vec<Vec<Block>>,
    },
    Code(String),
    Quote(Vec<Block>),
    /// A table: column alignments and rows, the first row being the header.
    Table {
        /// Byte offset where the table starts in the parsed text, to match it with a
        /// document table by position rather than by order.
        source_start: usize,
        alignments: Vec<Alignment>,
        rows: Vec<Vec<Inline>>,
    },
    Rule,
}

// Open containers while the event stream is read.
enum Frame {
    Blocks(Vec<Block>),
    Quote(Vec<Block>),
    List(Option<u64>, Vec<Vec<Block>>),
    Item(Vec<Block>),
}

/// Parses Markdown text into blocks. Raw HTML other than `<br>` is kept as plain text.
/// It reads the text the way the document's own parser does (byte-order mark and GFM
/// extensions), so both find the same tables at the same offsets.
pub(crate) fn parse_markdown(text: &str) -> Vec<Block> {
    let (body, body_offset) = markdown_body(text);
    let mut stack = vec![Frame::Blocks(Vec::new())];
    let mut inline: Option<Inline> = None;
    let mut marks = Span::default();
    let mut heading = None;
    let mut code: Option<String> = None;
    let mut table: Option<(usize, Vec<Alignment>, Vec<Vec<Inline>>)> = None;
    for (event, range) in Parser::new_ext(body, MARKDOWN_OPTIONS).into_offset_iter() {
        match event {
            Event::Start(Tag::Paragraph) => inline = Some(Vec::new()),
            Event::End(TagEnd::Paragraph) => {
                let content = inline.take().unwrap_or_default();
                push_block(&mut stack, Block::Paragraph(content));
            }
            Event::Start(Tag::Heading { level, .. }) => {
                heading = Some(heading_number(level));
                inline = Some(Vec::new());
            }
            Event::End(TagEnd::Heading(_)) => {
                let content = inline.take().unwrap_or_default();
                push_block(
                    &mut stack,
                    Block::Heading(heading.take().unwrap_or(1), content),
                );
            }
            Event::Start(Tag::BlockQuote(_)) => stack.push(Frame::Quote(Vec::new())),
            Event::End(TagEnd::BlockQuote(_)) => {
                if let Some(Frame::Quote(blocks)) = stack.pop() {
                    push_block(&mut stack, Block::Quote(blocks));
                }
            }
            Event::Start(Tag::List(start)) => stack.push(Frame::List(start, Vec::new())),
            Event::End(TagEnd::List(_)) => {
                if let Some(Frame::List(start, items)) = stack.pop() {
                    push_block(&mut stack, Block::List { start, items });
                }
            }
            Event::Start(Tag::Item) => stack.push(Frame::Item(Vec::new())),
            Event::End(TagEnd::Item) => {
                // A tight list item keeps its text outside a paragraph.
                if let Some(content) = inline.take().filter(|content| !content.is_empty()) {
                    push_block(&mut stack, Block::Paragraph(content));
                }
                if let Some(Frame::Item(blocks)) = stack.pop()
                    && let Some(Frame::List(_, items)) = stack.last_mut()
                {
                    items.push(blocks);
                }
            }
            Event::Start(Tag::CodeBlock(_)) => code = Some(String::new()),
            Event::End(TagEnd::CodeBlock) => {
                let text = code.take().unwrap_or_default();
                push_block(
                    &mut stack,
                    Block::Code(text.trim_end_matches('\n').to_owned()),
                );
            }
            Event::Start(Tag::Table(alignments)) => {
                table = Some((range.start + body_offset, alignments, Vec::new()))
            }
            Event::End(TagEnd::Table) => {
                if let Some((source_start, alignments, rows)) = table.take() {
                    push_block(
                        &mut stack,
                        Block::Table {
                            source_start,
                            alignments,
                            rows,
                        },
                    );
                }
            }
            Event::Start(Tag::TableHead | Tag::TableRow) => {
                if let Some((_, _, rows)) = table.as_mut() {
                    rows.push(Vec::new());
                }
            }
            Event::Start(Tag::TableCell) => inline = Some(Vec::new()),
            Event::End(TagEnd::TableCell) => {
                let content = inline.take().unwrap_or_default();
                if let Some(row) = table.as_mut().and_then(|(_, _, rows)| rows.last_mut()) {
                    row.push(content);
                }
            }
            Event::Start(Tag::Strong) => marks.bold = true,
            Event::End(TagEnd::Strong) => marks.bold = false,
            Event::Start(Tag::Emphasis) => marks.italic = true,
            Event::End(TagEnd::Emphasis) => marks.italic = false,
            Event::Start(Tag::Strikethrough) => marks.strike = true,
            Event::End(TagEnd::Strikethrough) => marks.strike = false,
            Event::Start(Tag::Link { .. }) => marks.link = true,
            Event::End(TagEnd::Link) => marks.link = false,
            Event::Rule => push_block(&mut stack, Block::Rule),
            Event::Text(text) => {
                if let Some(code) = code.as_mut() {
                    code.push_str(&text);
                } else {
                    push_span(&mut inline, &marks, &text, false);
                }
            }
            Event::Code(text) => push_span(&mut inline, &marks, &text, true),
            Event::SoftBreak => push_span(&mut inline, &marks, " ", false),
            Event::HardBreak => push_span(&mut inline, &marks, "\n", false),
            Event::InlineHtml(html) | Event::Html(html) => {
                let break_tag = html
                    .trim()
                    .trim_start_matches('<')
                    .trim_end_matches('>')
                    .trim_end_matches('/')
                    .trim()
                    .eq_ignore_ascii_case("br");
                push_span(
                    &mut inline,
                    &marks,
                    if break_tag { "\n" } else { &html },
                    false,
                );
            }
            Event::TaskListMarker(done) => {
                push_span(
                    &mut inline,
                    &marks,
                    if done { "[x] " } else { "[ ] " },
                    false,
                );
            }
            _ => (),
        }
    }
    match stack.into_iter().next() {
        Some(Frame::Blocks(blocks)) => blocks,
        _ => Vec::new(),
    }
}

// Add finished text to the open inline run, starting one inside a tight list item.
fn push_span(inline: &mut Option<Inline>, marks: &Span, text: &str, code: bool) {
    let spans = inline.get_or_insert_with(Vec::new);
    let span = Span {
        text: text.to_owned(),
        code,
        ..marks.clone()
    };
    match spans.last_mut() {
        Some(last) if same_marks(last, &span) => last.text.push_str(text),
        _ => spans.push(span),
    }
}

// Report whether two runs carry the same marks, so they can be joined.
fn same_marks(left: &Span, right: &Span) -> bool {
    (left.bold, left.italic, left.strike, left.code, left.link)
        == (
            right.bold,
            right.italic,
            right.strike,
            right.code,
            right.link,
        )
}

// Append a finished block to the innermost open container.
fn push_block(stack: &mut [Frame], block: Block) {
    match stack.last_mut() {
        Some(Frame::Blocks(blocks) | Frame::Quote(blocks) | Frame::Item(blocks)) => {
            blocks.push(block)
        }
        Some(Frame::List(_, items)) => items.push(vec![block]),
        None => (),
    }
}

// Convert a heading level to its number.
fn heading_number(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}
