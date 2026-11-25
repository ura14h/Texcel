use super::*;

// Matches the colons the Markdown separator row uses for each column alignment.
#[test]
fn alignment_marks_follow_the_markdown_separator() {
    assert_eq!(alignment_marks(None), (false, false));
    assert_eq!(
        alignment_marks(Some(MarkdownTableAlignment::Left)),
        (true, false)
    );
    assert_eq!(
        alignment_marks(Some(MarkdownTableAlignment::Center)),
        (true, true)
    );
    assert_eq!(
        alignment_marks(Some(MarkdownTableAlignment::Right)),
        (false, true)
    );
}
