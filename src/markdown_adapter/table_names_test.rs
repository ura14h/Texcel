use super::super::MarkdownDocument;

/// Returns the names the parser gives the tables of `source`.
fn names(source: &str) -> Vec<String> {
    MarkdownDocument::parse(source.as_bytes().to_vec())
        .expect("the Markdown parses")
        .tables
        .into_iter()
        .map(|table| table.name)
        .collect()
}

const TABLE: &str = "| A |\n| --- |\n| 1 |\n";

#[test]
/// Names a table after its section's heading even when text lies between them.
fn a_table_takes_the_heading_of_its_section() {
    assert_eq!(
        names(&format!("# Title\n\n## Sales\n\nThe totals:\n\n{TABLE}")),
        ["Sales"]
    );
}

#[test]
/// Uses the nearest heading: a deeper heading further up is already closed by it.
fn the_nearest_heading_wins_over_a_closed_deeper_one() {
    assert_eq!(
        names(&format!(
            "## Part\n\n### Detail\n\nText\n\n## Summary\n\n{TABLE}"
        )),
        ["Summary"]
    );
}

#[test]
/// Numbers the tables of one section in order, including one after text with no heading.
fn tables_sharing_a_section_are_numbered() {
    let source = format!(
        "# Title\n\n{TABLE}\nParagraph\n\n{TABLE}\n## Next\n\n{TABLE}\n### Deeper\n\n{TABLE}\n{TABLE}"
    );
    assert_eq!(
        names(&source),
        ["Title (1)", "Title (2)", "Next", "Deeper (1)", "Deeper (2)"]
    );
}

#[test]
/// Falls back to the numbered default before any heading and under an empty heading.
fn tables_without_a_titled_heading_get_the_default_name() {
    assert_eq!(
        names(&format!("{TABLE}\n## Sales\n\n{TABLE}\n##\n\n{TABLE}")),
        ["表1", "Sales", "表3"]
    );
}

#[test]
/// Deletes a heading with its table only when the heading names that table alone, so the
/// other tables of a shared section keep their name.
fn deleting_a_table_keeps_a_heading_its_section_shares() {
    let source = format!("## Sales\n\n{TABLE}\n{TABLE}");
    let document = MarkdownDocument::parse(source.into_bytes()).expect("the Markdown parses");
    let removed = document.delete_table(0).expect("the table is deleted");
    assert_eq!(
        std::str::from_utf8(removed.source_bytes()).expect("UTF-8"),
        format!("## Sales\n\n{TABLE}")
    );
    assert_eq!(removed.tables[0].name, "Sales");
}
