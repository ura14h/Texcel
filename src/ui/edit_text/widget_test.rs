use super::*;

#[test]
// Shows raw line endings as one marker each so egui caret indexes still match the buffer.
fn line_break_markers_keep_one_character_per_line_ending() {
    let text = "a\nb\r\nc";
    let marked = line_break_markers(text);
    assert_eq!(marked, "a↲b↲↲c");
    assert_eq!(marked.chars().count(), text.chars().count());
    assert!(!has_line_break("a<br>b"));
    assert!(has_line_break(text));
}
