use super::*;

#[test]
// Checks that a layout with merges and column widths survives saving and loading.
fn layout_with_merges_roundtrips_through_serialization() {
    let mut layout = TableLayout::default();
    let merge = MergedRange {
        start: CellAddress { row: 1, column: 2 },
        end: CellAddress { row: 3, column: 4 },
    };
    layout.apply_value(LayoutTarget::Merge(merge), LayoutValue::Merge(true));
    layout.apply_value(
        LayoutTarget::Column(2),
        LayoutValue::Column(Some(ColumnLayout {
            width: Some(47.0),
            ..Default::default()
        })),
    );

    let encoded = serde_json::to_value(&layout).expect("layout serializes");
    assert!(encoded.get("merges").is_some());
    assert!(encoded.get("merge_geometry").is_none());
    let restored: TableLayout = serde_json::from_value(encoded).expect("saved layout loads");
    assert_eq!(restored.merges().collect::<Vec<_>>(), vec![merge]);
    assert_eq!(
        restored.column(2).and_then(|column| column.width),
        Some(47.0)
    );
}

#[test]
// Verify that input versions follow changed cells through edit, undo, and redo.
fn cell_input_versions_track_only_the_edited_cell_and_its_undo_history() {
    let mut document = Document::new(1, "cell-versions", &["Table"]);
    let first = CellAddress { row: 0, column: 0 };
    let second = CellAddress { row: 0, column: 1 };
    let initial = document.cell_input_revision(1, first);

    document
        .set_cell(1, first, Some(Cell::text("one")))
        .expect("first cell edit is stored");
    let edited = document.cell_input_revision(1, first);
    assert_ne!(edited, initial);

    document
        .set_cell(1, second, Some(Cell::text("other")))
        .expect("unrelated cell edit is stored");
    assert_eq!(document.cell_input_revision(1, first), edited);

    document
        .set_cell(1, first, Some(Cell::text("two")))
        .expect("second first-cell edit is stored");
    let edited_again = document.cell_input_revision(1, first);
    assert_ne!(edited_again, edited);

    document.undo().expect("cell edit undo succeeds");
    let undone = document.cell_input_revision(1, first);
    assert_ne!(undone, edited_again);
    assert_eq!(
        document.tables[0].cell(first).unwrap().input,
        CellInput::Text("one".to_owned())
    );
    document.redo().expect("cell edit redo succeeds");
    assert_ne!(document.cell_input_revision(1, first), undone);
    assert_eq!(
        document.tables[0].cell(first).unwrap().input,
        CellInput::Text("two".to_owned())
    );
}

#[test]
// Verify that a fresh imported formula result can be replaced and undone without changing formula text.
fn fresh_formula_result_replacement_is_typed_and_undoable_without_reformulating_text() {
    let mut document = Document::new(1, "replace-result", &["Table"]);
    let address = CellAddress { row: 0, column: 0 };
    document
        .set_cell(1, address, Some(Cell::formula("=1+1", None)))
        .expect("formula is stored");
    let calculation_revision = document.calculation_revision;
    assert!(document.apply_calculation_results(
        calculation_revision,
        [(
            1,
            address,
            CalcStatus::Fresh,
            Some(CalcValue::Text("=literal".to_owned()))
        )],
    ));

    assert_eq!(
        document.replace_formula_results(1, CellRange::single(address)),
        Ok(1)
    );
    assert_eq!(
        document.tables[0].cell(address).unwrap().input,
        CellInput::Text("'=literal".to_owned())
    );
    assert!(!document.tables[0].is_string_cell(address));
    assert_eq!(
        document.undo().expect("replacement is one undo command"),
        Some((1, address))
    );
    assert!(matches!(
        document.tables[0].cell(address).unwrap().input,
        CellInput::Formula(_)
    ));
    assert!(!document.tables[0].is_string_cell(address));
    document.redo().expect("redo restores the literal result");
    assert_eq!(
        document.tables[0].cell(address).unwrap().input,
        CellInput::Text("'=literal".to_owned())
    );
    assert!(!document.tables[0].is_string_cell(address));
}

#[test]
// Verify that undo is shared across tables and a new edit clears the redo branch.
fn document_undo_redo_is_shared_across_tables_and_drops_redo_after_edit() {
    let mut document = Document::new(1, "two-sheets", &["A", "B"]);
    let first = CellAddress { row: 0, column: 0 };
    let second_table = document.tables[1].id;
    document
        .set_cell(1, first, Some(Cell::text("before")))
        .expect("first edit targets an existing table");
    document
        .set_cell(second_table, first, Some(Cell::text("other sheet")))
        .expect("second edit targets an existing table");
    assert_eq!(
        document.undo().expect("undo applies the prior command"),
        Some((second_table, first))
    );
    assert!(document.tables[1].cell(first).is_none());
    assert_eq!(
        document.redo().expect("redo applies the command"),
        Some((second_table, first))
    );
    assert_eq!(
        document.tables[1].cell(first).unwrap().input,
        CellInput::Text("other sheet".into())
    );
    assert!(
        document
            .undo()
            .expect("undo returns to the prior state")
            .is_some()
    );
    document
        .set_cell(1, first, Some(Cell::text("new branch")))
        .expect("new edit is accepted");
    assert!(
        document
            .redo()
            .expect("redo is available only for the current history branch")
            .is_none()
    );
}

#[test]
// Verify that undo and redo on empty history are safe.
fn undo_on_an_empty_history_is_safe_and_repeatable() {
    let mut document = Document::new(1, "sheet", &["A"]);
    assert_eq!(document.undo(), Ok(None));
    assert_eq!(document.undo(), Ok(None));
    assert_eq!(document.revision, 0);
}

#[test]
// Verify that saving an older snapshot does not mark later edits as saved.
fn saving_an_older_snapshot_keeps_later_edits_in_the_export_set() {
    let mut document = Document::new(1, "save-race", &["Sheet1"]);
    let first = CellAddress { row: 0, column: 0 };
    let second = CellAddress { row: 0, column: 1 };
    document
        .set_cell(1, first, Some(Cell::text("saved")))
        .expect("first edit is valid");
    let saved_revision = document.revision;
    document
        .set_cell(1, second, Some(Cell::text("later")))
        .expect("edit may continue while a save is running");

    document.mark_saved(saved_revision);
    assert!(document.is_dirty());
    assert_eq!(document.saved_revision, saved_revision);
    assert_eq!(
        document
            .changed_cell_snapshot()
            .into_iter()
            .map(|(_, address, cell)| (address, cell.unwrap().input.source_text()))
            .collect::<Vec<_>>(),
        vec![(first, "saved".to_owned()), (second, "later".to_owned())]
    );

    document.mark_saved(document.revision);
    assert!(!document.is_dirty());
    assert!(document.changed_cell_snapshot().is_empty());
}

#[test]
// Verify saved content follows undo positions without reusing a discarded branch.
fn saved_history_state_tracks_undo_redo_save_ack_and_new_branches() {
    let mut document = Document::new(1, "save-point", &["Sheet1"]);
    let address = CellAddress { row: 0, column: 0 };
    document
        .set_cell(1, address, Some(Cell::text("saved")))
        .expect("the first edit is valid");
    let saved_revision = document.revision;
    document.mark_saved(saved_revision);
    assert!(!document.is_dirty());

    document
        .set_cell(1, address, Some(Cell::text("pending save")))
        .expect("the next edit is valid");
    let pending_save_revision = document.revision;
    let pending_save_state = document.history_state_id();
    document.undo().expect("the pending edit can be undone");
    assert!(!document.is_dirty(), "undo returns to the saved state");

    document.mark_saved_at_state(pending_save_revision, pending_save_state);
    assert!(document.is_dirty(), "the late ACK saves its captured state");
    document.redo().expect("the saved edit can be redone");
    assert!(!document.is_dirty(), "redo returns to the late saved state");

    document.undo().expect("the saved edit can be undone again");
    document
        .set_cell(1, address, Some(Cell::text("new branch")))
        .expect("a new edit creates a different branch");
    assert!(document.is_dirty());
    document.undo().expect("the new branch can be undone");
    assert!(document.is_dirty(), "the old saved branch was discarded");
}

#[test]
// Verify that an explicit zero-width column has no extent.
fn explicit_zero_width_column_has_zero_extent() {
    let mut layout = TableLayout::default();
    layout.apply_value(
        LayoutTarget::Column(0),
        LayoutValue::Column(Some(ColumnLayout {
            width: Some(0.0),
            ..Default::default()
        })),
    );
    assert_eq!(layout.column_width_pixels(0), 0.0);
    assert_eq!(layout.prefix_column_width_pixels(1), 0.0);
}

#[test]
// Verify that a sized column holds that many half-width characters plus the cell inset, and
// that column positions agree with the widths for any character width.
fn sized_columns_follow_the_character_width_and_positions_add_up() {
    let mut layout = TableLayout::default();
    for (column, width) in [(1, Some(10.0)), (3, Some(0.0)), (4, Some(2.5))] {
        layout.apply_value(
            LayoutTarget::Column(column),
            LayoutValue::Column(Some(ColumnLayout {
                width,
                ..Default::default()
            })),
        );
    }
    for character_width in [7.0, 8.4] {
        assert_eq!(
            layout.column_width_pixels_with(1, character_width),
            10.0 * character_width + crate::document::COLUMN_TEXT_PADDING_PIXELS
        );
        assert_eq!(
            layout.column_width_pixels_with(0, character_width),
            crate::document::DEFAULT_COLUMN_WIDTH_PIXELS
        );
        for columns in 0..7 {
            let sum = (0..columns)
                .map(|column| layout.column_width_pixels_with(column, character_width))
                .sum::<f64>();
            let prefix = layout.prefix_column_width_pixels_with(columns, character_width);
            assert!(
                (prefix - sum).abs() < 1e-9,
                "{columns} columns at {character_width}"
            );
        }
    }
    // Clearing a width brings the column back to the default.
    layout.apply_value(LayoutTarget::Column(1), LayoutValue::Column(None));
    assert_eq!(
        layout.prefix_column_width_pixels_with(2, 8.4),
        2.0 * crate::document::DEFAULT_COLUMN_WIDTH_PIXELS
    );
    assert_eq!(
        crate::document::column_width_units_for_pixels(10.0 * 8.4 + 12.0, 8.4),
        10.0
    );
}

#[test]
// Verify that merge row lookup skips unrelated ranges after a tall merged cell.
fn merge_row_blocks_skip_unrelated_ranges_beyond_a_tall_merge() {
    let mut layout = TableLayout::default();
    layout.apply_value(
        LayoutTarget::Merge(MergedRange {
            start: CellAddress { row: 0, column: 0 },
            end: CellAddress {
                row: 40_000,
                column: 0,
            },
        }),
        LayoutValue::Merge(true),
    );
    for row in 1..=20_000 {
        layout.apply_value(
            LayoutTarget::Merge(MergedRange {
                start: CellAddress { row, column: 100 },
                end: CellAddress { row, column: 101 },
            }),
            LayoutValue::Merge(true),
        );
    }
    let viewport = MergedRange {
        start: CellAddress {
            row: 30_000,
            column: 0,
        },
        end: CellAddress {
            row: 30_050,
            column: 1,
        },
    };
    for _ in 0..400 {
        assert_eq!(layout.merges_intersecting(viewport).count(), 1);
    }
}

#[test]
// Verify that active IME composition blocks commit, cancel, and cell navigation.
fn ime_composition_blocks_commit_cancel_and_cell_navigation() {
    let mut edit = EditSession::default();
    edit.begin(
        1,
        CellAddress::default(),
        CellInput::Text("元".into()),
        EditTarget::Cell,
    );
    edit.set_composing(true);
    for key in [InputKey::Enter, InputKey::Escape, InputKey::ArrowRight] {
        assert_eq!(edit.key_action(key, false), InputAction::PropagateToIme);
    }
    edit.set_composing(false);
    assert_eq!(
        edit.key_action(InputKey::Enter, false),
        InputAction::CommitAndMoveDown
    );
    assert_eq!(
        edit.key_action(InputKey::Escape, false),
        InputAction::Cancel
    );
}

#[test]
// Verify that switching editors preserves the same uncommitted text buffer.
fn switching_editor_keeps_the_same_uncommitted_buffer() {
    let mut edit = EditSession::default();
    edit.begin(
        7,
        CellAddress { row: 3, column: 2 },
        CellInput::Text("source".into()),
        EditTarget::Cell,
    );
    edit.buffer = "編集中".into();
    edit.switch_target(EditTarget::FormulaBar);
    assert_eq!(edit.buffer, "編集中");
    assert_eq!(
        edit.completed_input(),
        Some(CellInput::Text("編集中".into()))
    );
}

#[test]
// Verify that a new edit session gets a fresh widget ID and retains empty cell types.
fn editing_session_uses_a_new_widget_generation_and_preserves_empty_types() {
    let mut edit = EditSession::default();
    edit.begin(
        1,
        CellAddress::default(),
        CellInput::Text("before".into()),
        EditTarget::Cell,
    );
    let first_session = edit.session_id;
    edit.reset();
    edit.begin(
        1,
        CellAddress { row: 0, column: 1 },
        CellInput::Empty,
        EditTarget::Cell,
    );
    assert_ne!(edit.session_id, first_session);
    assert_eq!(edit.completed_input(), Some(CellInput::Empty));

    let explicit_empty = CellInput::Text(String::new()).edited_source(String::new());
    assert_eq!(explicit_empty, CellInput::Text(String::new()));
    assert_eq!(
        CellInput::Empty.edited_source(String::new()),
        CellInput::Empty
    );
    assert_eq!(
        CellInput::Formula("=1".to_owned()).edited_source("abc".to_owned()),
        CellInput::Text("abc".to_owned())
    );
}

#[test]
// Verify that clearing a range is one sparse undo command that empties the cell values.
fn clearing_a_range_is_one_sparse_command_and_empties_values() {
    let mut document = Document::new(1, "sheet", &["A"]);
    let first = CellAddress { row: 1, column: 1 };
    let second = CellAddress { row: 2, column: 2 };
    let outside = CellAddress { row: 3, column: 3 };
    for (address, text) in [(first, "one"), (second, "two"), (outside, "keep")] {
        document
            .set_cell(
                1,
                address,
                Some(Cell {
                    input: CellInput::Text(text.to_owned()),
                    imported_result: None,
                    calculation: None,
                }),
            )
            .expect("cell is inside the table");
    }
    let mut original = document.tables[0].cell(first).unwrap().clone();
    original.imported_result = Some(CalcValue::Text("old result".to_owned()));
    original.calculation = Some(CalcResult {
        revision: document.calculation_revision,
        status: CalcStatus::Imported,
        value: Some(CalcValue::Text("old result".to_owned())),
    });
    document
        .set_cell(1, first, Some(original.clone()))
        .expect("cell attributes are set");
    assert!(
        document
            .clear_range(
                1,
                CellRange {
                    anchor: first,
                    head: second
                }
            )
            .expect("range is cleared")
    );
    for address in [first, second] {
        let cell = document.tables[0]
            .cell(address)
            .expect("cleared cell stays in the sparse table");
        assert_eq!(cell.input, CellInput::Empty);
        assert!(cell.imported_result.is_none());
        assert!(cell.calculation.is_none());
    }
    assert_eq!(
        document.tables[0]
            .cell(outside)
            .map(|cell| cell.input.clone()),
        Some(CellInput::Text("keep".into()))
    );
    assert_eq!(
        document.undo().expect("one range command undoes"),
        Some((1, first))
    );
    assert_eq!(document.tables[0].cell(first), Some(&original));
    assert_eq!(
        document.tables[0].cell(second).unwrap().input,
        CellInput::Text("two".into())
    );
}

#[test]
// Verify that Alt+Enter inserts an edit action without committing the cell.
fn alt_enter_is_an_edit_action_and_does_not_mean_commit() {
    let mut edit = EditSession::default();
    edit.begin(
        1,
        CellAddress::default(),
        CellInput::Empty,
        EditTarget::Cell,
    );
    assert_eq!(
        edit.key_action(InputKey::AltEnter, false),
        InputAction::InsertLineBreak
    );
    assert_eq!(
        edit.key_action(InputKey::Enter, false),
        InputAction::CommitAndMoveDown
    );
}

#[test]
// Verify that selection movement stays in sheet bounds and keeps its anchor.
fn selection_movement_clamps_to_sheet_and_preserves_range_anchor() {
    let mut view = TableViewState::default();
    assert_eq!(
        view.move_selection(2, 2, 3, 3, false),
        CellAddress { row: 2, column: 2 }
    );
    assert_eq!(
        view.move_selection(-1, 0, 3, 3, true),
        CellAddress { row: 1, column: 2 }
    );
    assert_eq!(view.selection.anchor, CellAddress { row: 2, column: 2 });
    assert_eq!(view.selection.head, CellAddress { row: 1, column: 2 });
    assert_eq!(
        view.move_selection(8, 8, 3, 3, false),
        CellAddress { row: 2, column: 2 }
    );
    assert_eq!(
        view.selection,
        CellRange::single(CellAddress { row: 2, column: 2 })
    );
}

#[test]
// Verify that toggling a disjoint selection does not select the gaps.
fn disjoint_selection_toggles_without_expanding_over_unselected_cells() {
    let first = CellRange::single(CellAddress { row: 0, column: 0 });
    let second = CellRange::single(CellAddress { row: 2, column: 3 });
    let overlapping = CellRange {
        anchor: CellAddress { row: 0, column: 0 },
        head: CellAddress { row: 1, column: 1 },
    };
    let mut view = TableViewState::default();
    view.set_selection(first);
    view.toggle_disjoint_selection(second)
        .expect("separate range");
    assert_eq!(
        view.selected_ranges().collect::<Vec<_>>(),
        vec![second, first]
    );
    assert!(view.toggle_disjoint_selection(overlapping).is_err());
    view.toggle_disjoint_selection(first)
        .expect("remove previous range");
    assert_eq!(view.selection, second);
    assert!(view.additional_selections.is_empty());
}

#[test]
// Applies full-sheet edits to sparse cells and restores them with one undo step.
fn full_sheet_clear_only_visits_stored_cells() {
    let mut document = Document::new(84, "sparse sheet", &["Sheet1"]);
    let first = CellAddress::default();
    let last = CellAddress {
        row: MAX_ROWS - 1,
        column: MAX_COLUMNS - 1,
    };
    let row_last = CellAddress {
        row: 0,
        column: MAX_COLUMNS - 1,
    };
    let column_last = CellAddress {
        row: MAX_ROWS - 1,
        column: 0,
    };
    for (address, value) in [
        (first, "first"),
        (row_last, "row last"),
        (column_last, "column last"),
        (last, "last"),
    ] {
        document
            .set_cell(1, address, Some(Cell::text(value)))
            .expect("sparse endpoint is within the sheet");
    }
    let all = CellRange {
        anchor: first,
        head: last,
    };
    let table = &document.tables[0];
    let before = table
        .stored_cells()
        .map(|(address, cell)| (address, cell.clone()))
        .collect::<Vec<_>>();

    let history_before = document.history.len();
    assert!(
        document
            .clear_range(1, all)
            .expect("full-sheet clear is sparse")
    );
    assert_eq!(document.history.len(), history_before + 1);
    assert_eq!(document.tables[0].stored_cells().count(), before.len());
    assert!(before.iter().all(|(address, _)| {
        document.tables[0]
            .cell(*address)
            .is_none_or(|cell| cell.input == CellInput::Empty)
    }));
    document
        .undo()
        .expect("one undo restores the cleared cells");
    assert!(
        before
            .iter()
            .all(|(address, cell)| { document.tables[0].cell(*address) == Some(cell) })
    );
}

#[test]
// Verify that sparse axis navigation jumps directly to contiguous data edges.
fn sparse_axis_index_jumps_to_contiguous_region_edges() {
    let mut table = Table::new(1, "index");
    for row in [0, 1, 4] {
        table.set_loaded_cell(
            CellAddress { row, column: 2 },
            Cell::text(format!("r{row}")),
        );
    }
    for column in [0, 1, 4] {
        table.set_loaded_cell(
            CellAddress { row: 7, column },
            Cell::text(format!("c{column}")),
        );
    }
    assert_eq!(
        table.data_region_edge(CellAddress { row: 0, column: 2 }, 1, 0),
        CellAddress { row: 1, column: 2 }
    );
    assert_eq!(
        table.data_region_edge(CellAddress { row: 1, column: 2 }, 1, 0),
        CellAddress { row: 4, column: 2 },
        "movement from a run end reaches the next occupied run start"
    );
    assert_eq!(
        table.data_region_edge(CellAddress { row: 4, column: 2 }, 1, 0),
        CellAddress {
            row: MAX_ROWS - 1,
            column: 2,
        },
        "movement from the last run reaches the sheet edge"
    );
    assert_eq!(
        table.data_region_edge(CellAddress { row: 4, column: 2 }, -1, 0),
        CellAddress { row: 1, column: 2 },
        "movement above a run reaches the previous run end"
    );
    assert_eq!(
        table.data_region_edge(CellAddress { row: 2, column: 2 }, 1, 0),
        CellAddress { row: 4, column: 2 }
    );
    assert_eq!(
        table.data_region_edge(CellAddress { row: 7, column: 0 }, 0, 1),
        CellAddress { row: 7, column: 1 }
    );
    assert_eq!(
        table.data_region_edge(CellAddress { row: 7, column: 1 }, 0, 1),
        CellAddress { row: 7, column: 4 },
        "movement at the end of a horizontal run reaches the next run start"
    );
    assert_eq!(
        table.data_region_edge(CellAddress { row: 7, column: 4 }, 0, -1),
        CellAddress { row: 7, column: 1 },
        "movement left of a run reaches the previous run end"
    );
    assert_eq!(
        table.data_region_edge(CellAddress { row: 7, column: 2 }, 0, 1),
        CellAddress { row: 7, column: 4 }
    );
    assert_eq!(
        table.last_used_cell(),
        Some(CellAddress { row: 7, column: 4 })
    );
}

#[test]
// Verify the used-range corner combines the farthest row and column independently.
fn last_used_cell_uses_independent_row_and_column_edges() {
    let mut table = Table::new(1, "sparse extent");
    table.set_loaded_cell(CellAddress { row: 1, column: 5 }, Cell::text("right"));
    table.set_loaded_cell(CellAddress { row: 7, column: 0 }, Cell::text("bottom"));

    assert_eq!(
        table.last_used_cell(),
        Some(CellAddress { row: 7, column: 5 })
    );
}

#[test]
// Keeps a far sheet edit sparse and undoable without changing source dimensions.
fn sparse_sheet_edit_outside_source_extent_uses_document_history() {
    let mut document = Document::new(1, "sparse sheet", &["A"]);
    document.tables[0].set_dimensions(3, 4);
    let far = CellAddress {
        row: MAX_ROWS - 1,
        column: MAX_COLUMNS - 1,
    };

    document
        .set_cell(1, far, Some(Cell::text("far value")))
        .expect("valid sheet cells remain editable beyond source dimensions");
    assert_eq!(document.tables[0].row_count(), 3);
    assert_eq!(document.tables[0].column_count(), 4);
    assert_eq!(document.tables[0].last_used_cell(), Some(far));
    assert_eq!(document.tables[0].extent_end(), far);
    assert_eq!(document.tables[0].cells.len(), 1);

    document.undo().expect("far cell edit can be undone");
    assert!(document.tables[0].cell(far).is_none());
    assert_eq!(document.tables[0].last_used_cell(), None);
    assert_eq!(
        document.tables[0].extent_end(),
        CellAddress { row: 2, column: 3 }
    );
    document.redo().expect("far cell edit can be redone");
    assert_eq!(
        document.tables[0].cell(far).map(|cell| &cell.input),
        Some(&CellInput::Text("far value".to_owned()))
    );
    assert!(
        document
            .clear_range(1, CellRange::single(far))
            .expect("far sparse cell can be cleared")
    );
    assert_eq!(
        document.tables[0].cell(far).map(|cell| &cell.input),
        Some(&CellInput::Empty)
    );
    assert_eq!(document.tables[0].last_used_cell(), None);
    document.undo().expect("far sparse clear is undoable");
    assert_eq!(
        document.tables[0].cell(far).map(|cell| &cell.input),
        Some(&CellInput::Text("far value".to_owned()))
    );
}

#[test]
// Verify that out-of-bounds cell edits fail without changing history.
fn cell_command_rejects_out_of_bounds_without_mutating_history() {
    let mut document = Document::new(1, "sheet", &["A"]);
    let result = document.set_cell(
        1,
        CellAddress {
            row: MAX_ROWS,
            column: 0,
        },
        Some(Cell::text("outside")),
    );
    assert_eq!(
        result,
        Err(DocumentError::OutOfBounds(CellAddress {
            row: MAX_ROWS,
            column: 0,
        }))
    );
    assert!(!document.can_undo());
    assert_eq!(document.revision, 0);
}

#[test]
// Verify that a stale calculation result is not shown as current.
fn stale_formula_result_is_not_displayed_as_current() {
    let formula = Cell::formula("=A1+1", Some(CalcValue::Number(42.0)));
    assert_eq!(formula.display_text(0), "42");
    assert_eq!(formula.display_text(1), "=A1+1");
}

#[test]
// Verify that a failed old edit is combined with a newer cell value before retry.
fn failed_old_change_merges_with_newer_value_before_resubmit() {
    let mut document = Document::new(8, "coalesced", &["Sheet1"]);
    let address = CellAddress { row: 0, column: 0 };
    let other_address = CellAddress { row: 0, column: 1 };
    document
        .set_cell(1, address, Some(Cell::number(1.0)))
        .expect("first revision is valid");
    let first = document.calculation_cell_snapshot();
    let first_keys = first
        .iter()
        .map(|(table_id, address, _)| (*table_id, *address))
        .collect::<Vec<_>>();
    document.mark_calculation_submitted(&first_keys);

    document
        .set_cell(1, address, Some(Cell::number(2.0)))
        .expect("newer revision is valid");
    document
        .set_cell(1, other_address, Some(Cell::number(3.0)))
        .expect("independent cell edit is valid");
    assert!(!document.apply_calculation_results(
        1,
        [(
            1,
            address,
            CalcStatus::Fresh,
            Some(CalcValue::Number(100.0)),
        )]
    ));
    document.requeue_calculation_changes(&first_keys);

    let merged = document.calculation_cell_snapshot();
    assert_eq!(merged.len(), 2);
    assert_eq!(
        merged
            .iter()
            .find(|(_, current_address, _)| *current_address == address)
            .unwrap()
            .2
            .as_ref()
            .unwrap()
            .input,
        CellInput::Number(2.0)
    );
    assert_eq!(
        merged
            .iter()
            .find(|(_, current_address, _)| *current_address == other_address)
            .unwrap()
            .2
            .as_ref()
            .unwrap()
            .input,
        CellInput::Number(3.0)
    );
}

#[test]
// Checks that the fill handle continues an even sequence and copies anything else.
fn auto_fill_continues_sequences_and_copies_other_values() {
    let fill = |cells: &[(usize, CellInput)], target_rows: usize| {
        let mut table = Table::new(1, "fill");
        for (row, input) in cells {
            table.set_loaded_cell(
                CellAddress {
                    row: *row,
                    column: 0,
                },
                Cell {
                    input: input.clone(),
                    imported_result: None,
                    calculation: None,
                },
            );
        }
        let source = CellRange {
            anchor: CellAddress::default(),
            head: CellAddress {
                row: cells.len() - 1,
                column: 0,
            },
        };
        let target = CellRange {
            anchor: CellAddress::default(),
            head: CellAddress {
                row: target_rows - 1,
                column: 0,
            },
        };
        table
            .apply_fill(
                source,
                target,
                FillMode::Auto,
                &std::collections::BTreeMap::new(),
            )
            .expect("the fill applies");
        (0..target_rows)
            .map(|row| {
                table
                    .cell(CellAddress { row, column: 0 })
                    .map(|cell| cell.input.source_text())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
    };
    let numbers = [(0, CellInput::Number(1.0)), (1, CellInput::Number(3.0))];
    assert_eq!(fill(&numbers, 4), ["1", "3", "5", "7"]);
    let items = [
        (0, CellInput::Text("Item1".to_owned())),
        (1, CellInput::Text("Item2".to_owned())),
    ];
    assert_eq!(fill(&items, 3), ["Item1", "Item2", "Item3"]);
    assert_eq!(fill(&[(0, CellInput::Number(5.0))], 3), ["5", "5", "5"]);
    let words = [
        (0, CellInput::Text("a".to_owned())),
        (1, CellInput::Text("b".to_owned())),
    ];
    assert_eq!(fill(&words, 4), ["a", "b", "a", "b"]);
}
