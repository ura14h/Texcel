use super::*;

/// Builds a zero-based cell address from an A1-style row and column.
fn at(row: usize, column: usize) -> CellAddress {
    CellAddress { row, column }
}

#[test]
fn copy_moves_relative_references_and_keeps_absolute_ones() {
    let mut mover = PasteFormulaMover::new().expect("engine");
    // B2 to D5 moves three rows down and two columns right.
    let cases = [
        ("=A1", "=C4"),
        ("=$A$1", "=$A$1"),
        ("=$A1", "=$A4"),
        ("=A$1", "=C$1"),
        ("=SUM(A1:A3)", "=SUM(C4:C6)"),
    ];
    for (formula, expected) in cases {
        assert_eq!(
            mover.copy(formula, at(1, 1), at(4, 3)).expect("copy"),
            expected
        );
    }
}

#[test]
fn copy_past_the_sheet_edge_becomes_a_reference_error() {
    let mut mover = PasteFormulaMover::new().expect("engine");
    let moved = mover.copy("=A2", at(1, 1), at(0, 1)).expect("copy");
    assert_eq!(moved, "=A1");
    let moved = mover.copy("=A1", at(1, 1), at(0, 0)).expect("copy");
    assert!(moved.contains("#REF!"), "{moved}");
}

#[test]
fn unreadable_formula_is_not_moved() {
    let mut mover = PasteFormulaMover::new().expect("engine");
    // A formula pasted back onto its own cell keeps its text without parsing.
    assert_eq!(
        mover.copy("=1+*2", at(0, 0), at(0, 0)).expect("same cell"),
        "=1+*2"
    );
    for formula in ["=1+*2", "=FOO(A1)"] {
        assert!(
            mover.copy(formula, at(0, 0), at(1, 0)).is_err(),
            "{formula}"
        );
    }
}

#[test]
fn cut_moves_only_references_into_the_cut_area() {
    let mut mover = PasteFormulaMover::new().expect("engine");
    // A1:B2 is cut and pasted at D5 (three rows down, three columns right).
    let area = CellRange {
        anchor: at(0, 0),
        head: at(1, 1),
    };
    let moved = mover
        .cut("=A1+B2+C3+$A$2", at(1, 1), at(4, 4), area)
        .expect("cut");
    assert_eq!(moved, "=D4+E5+C3+$D$5");
}

#[test]
fn other_cells_follow_a_cut_area() {
    let mut mover = PasteFormulaMover::new().expect("engine");
    let area = CellRange {
        anchor: at(0, 0),
        head: at(1, 0),
    };
    // F1 refers to A1 and A1:A2, both inside the cut area moved to C11.
    let moved = mover
        .follow_cut("=A1+SUM(A1:A2)+B1", at(0, 5), area, 10, 2)
        .expect("follow");
    assert_eq!(moved.as_deref(), Some("=C11+SUM(C11:C12)+B1"));
    // A range that only partly overlaps the area keeps its cells.
    let unchanged = mover
        .follow_cut("=sum(A1:A5)", at(0, 5), area, 10, 2)
        .expect("follow");
    assert_eq!(unchanged, None);
}
