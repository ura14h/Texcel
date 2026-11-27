use super::*;

#[test]
// Reads headings, marks, nested lists, code, tables, and `<br>` breaks from GFM text.
fn parse_markdown_builds_blocks_for_the_supported_syntax() {
    let blocks = parse_markdown(
        "# Title\n\nSome **bold** and `code`.\n\n- one\n- two\n  1. inner\n\n```\nx = 1\n```\n\n| A | B |\n| :-- | --: |\n| a<br>b | 2 |\n\n---\n",
    );
    assert!(matches!(&blocks[0], Block::Heading(1, inline) if inline[0].text == "Title"));
    let Block::Paragraph(paragraph) = &blocks[1] else {
        panic!("a paragraph follows the heading");
    };
    assert!(
        paragraph
            .iter()
            .any(|span| span.bold && span.text == "bold")
    );
    assert!(
        paragraph
            .iter()
            .any(|span| span.code && span.text == "code")
    );
    let Block::List { start: None, items } = &blocks[2] else {
        panic!("a bullet list follows");
    };
    assert_eq!(items.len(), 2);
    assert!(
        items[1]
            .iter()
            .any(|block| matches!(block, Block::List { start: Some(1), .. }))
    );
    assert_eq!(blocks[3], Block::Code("x = 1".to_owned()));
    let Block::Table { alignments, rows } = &blocks[4] else {
        panic!("a table follows");
    };
    assert_eq!(alignments.len(), 2);
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[1][0]
            .iter()
            .map(|span| span.text.as_str())
            .collect::<String>(),
        "a\nb"
    );
    assert_eq!(blocks[5], Block::Rule);
}
