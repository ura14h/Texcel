use super::*;
use egui_kittest::{
    Harness,
    kittest::{By, Queryable},
};

// Builds a keyboard event with the requested modifier state.
fn key_event(key: Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

// Sends one real input event through a complete app frame.
fn send_event(harness: &mut Harness<'_, TexcelApp>, event: egui::Event) {
    if let egui::Event::Key { modifiers, .. } = &event {
        harness.event(egui::Event::ModifiersChanged(*modifiers));
        harness.step();
    }
    harness.event(event);
    harness.step();
}

// Runs one real raw-input batch through the production adapter hook.
fn send_input_batch(
    harness: &mut Harness<'_, TexcelApp>,
    events: impl IntoIterator<Item = egui::Event>,
) {
    harness.input_mut().events.extend(events);
    let ctx = harness.ctx.clone();
    let mut raw = std::mem::take(harness.input_mut());
    eframe::App::raw_input_hook(harness.state_mut(), &ctx, &mut raw);
    *harness.input_mut() = raw;
    harness.step();
}

// Builds a grid harness with enough rows and columns for navigation gestures.
fn grid_harness(rows: usize, columns: usize) -> Harness<'static, TexcelApp> {
    grid_harness_with_step_dt(rows, columns, 0.25)
}

// Builds a grid harness with an explicit frame interval for timed pointer gestures.
fn grid_harness_with_step_dt(
    rows: usize,
    columns: usize,
    step_dt: f32,
) -> Harness<'static, TexcelApp> {
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .with_step_dt(step_dt)
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    install_test_markdown_grid(harness.state_mut(), rows, columns, "Grid interaction");
    harness.step();
    harness
}

// Verifies column edge drags change only the local width and row edges change nothing.
#[test]
fn header_boundary_drag_updates_local_dimensions_without_document_edits() {
    let mut harness = grid_harness(4, 4);
    let a1 = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("A1"))
        .next()
        .expect("A1 is visible")
        .rect();
    let (document_id, table_id, revision, column_size) = {
        let document = harness.state().workspace.active_document().unwrap();
        let table = document.active_table().unwrap();
        let view = document.view(table.id).unwrap();
        (
            document.id,
            table.id,
            document.revision,
            view.layout.column_width_pixels(0) as f32,
        )
    };
    let column_edge = Pos2::new(a1.right(), a1.top() - HEADER_HEIGHT / 2.0);
    send_event(&mut harness, egui::Event::PointerMoved(column_edge));
    send_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: column_edge,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    );
    let column_end = column_edge + Vec2::new(32.0, 0.0);
    send_event(&mut harness, egui::Event::PointerMoved(column_end));
    send_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: column_end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    let column = harness.state().workspace.documents[&document_id]
        .view(table_id)
        .unwrap()
        .layout
        .column(0)
        .expect("a resized column has a local layout value");
    assert!(column.width.unwrap_or_default() > ((column_size - 5.0) / 7.0) as f64);

    let a1 = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("A1"))
        .next()
        .expect("A1 remains visible")
        .rect();
    let row_edge = Pos2::new(a1.left() - ROW_HEADER_WIDTH / 2.0, a1.bottom());
    send_event(&mut harness, egui::Event::PointerMoved(row_edge));
    send_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: row_edge,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    );
    let row_end = row_edge + Vec2::new(0.0, 16.0);
    send_event(&mut harness, egui::Event::PointerMoved(row_end));
    send_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: row_end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    let document = &harness.state().workspace.documents[&document_id];
    // Row headers have no resize edge, so the drag leaves the document unchanged.
    assert_eq!(document.revision, revision);
}

// Verifies double-click auto-fit measures the live sparse axis on the worker.
#[test]
fn header_double_click_auto_fits_without_editing_the_document() {
    let mut harness = grid_harness_with_step_dt(3, 3, 0.05);
    let (document_id, table_id, before_revision) = {
        let document = harness.state_mut().workspace.active_document_mut().unwrap();
        let table_id = document.active_table().unwrap().id;
        document
            .set_cell(
                table_id,
                CellAddress { row: 1, column: 0 },
                Some(Cell::text("a sparse column with a long value")),
            )
            .expect("the long value is stored");
        (document.id, table_id, document.revision)
    };
    harness.step();
    let a1 = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("A1"))
        .next()
        .expect("A1 is visible")
        .rect();
    let edge = Pos2::new(a1.right() - 1.0, a1.top() - HEADER_HEIGHT / 2.0);
    for _ in 0..2 {
        send_event(&mut harness, egui::Event::PointerMoved(edge));
        send_event(
            &mut harness,
            egui::Event::PointerButton {
                pos: edge,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        );
        send_event(
            &mut harness,
            egui::Event::PointerButton {
                pos: edge,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        );
    }
    assert!(
        harness.ctx.input(|input| input
            .pointer
            .button_double_clicked(egui::PointerButton::Primary)),
        "the pointer batch is recognized as a primary double-click"
    );
    assert!(
        harness.state().pending_auto_fits.contains_key(&document_id),
        "the header double-click starts an auto-fit request"
    );
    let before_width = 72.0;
    for _ in 0..80 {
        harness.step();
        if harness.state().workspace.documents[&document_id]
            .view(table_id)
            .unwrap()
            .layout
            .column(0)
            .is_some()
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let document = &harness.state().workspace.documents[&document_id];
    let fitted = document
        .view(table_id)
        .unwrap()
        .layout
        .column(0)
        .unwrap_or_else(|| {
            panic!(
                "worker auto-fit result missing; pending={:?}, status={:?}",
                harness.state().pending_auto_fits.keys().collect::<Vec<_>>(),
                harness.state().status
            )
        });
    assert!(fitted.width.unwrap_or_default() > (before_width - 5.0) / 7.0);
    assert_eq!(document.revision, before_revision);
}

/// Keeps a header's resize strip from selecting the adjacent column.
#[test]
fn header_resize_strip_has_priority_over_adjacent_header_selection() {
    let mut harness = grid_harness(4, 4);
    let b1 = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("B1"))
        .next()
        .expect("B1 is visible")
        .rect();
    let point = Pos2::new(b1.left() + 1.0, b1.top() - HEADER_HEIGHT * 0.5);
    send_event(&mut harness, egui::Event::PointerMoved(point));
    send_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    );
    send_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    let document = harness.state().workspace.active_document().unwrap();
    let view = document.view(document.active_table().unwrap().id).unwrap();
    assert_eq!(view.selection, CellRange::single(CellAddress::default()));
}

/// Keeps the frozen header resize strip above a horizontally scrolled column.
#[test]
fn frozen_header_boundary_resize_owns_the_scroll_pane_hit_strip() {
    let mut harness = grid_harness(4, 40);
    let (document_id, table_id, start_width) = {
        let document = harness.state_mut().workspace.active_document_mut().unwrap();
        let table_id = document.active_table().unwrap().id;
        let start_width = {
            let view = document.view(table_id).unwrap();
            view.layout.column_width_pixels(0) as f32
        };
        let view = document.view_mut(table_id).unwrap();
        view.set_layout_value(
            LayoutTarget::Freeze,
            LayoutValue::Freeze(FrozenPanes {
                columns: 1,
                ..FrozenPanes::default()
            }),
        );
        view.scroll.x = 160.0;
        (document.id, table_id, start_width)
    };
    harness.step();
    let a1 = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("A1"))
        .next()
        .expect("frozen A1 remains visible")
        .rect();
    let point = Pos2::new(a1.right() + 2.0, a1.top() - HEADER_HEIGHT * 0.5);
    send_event(&mut harness, egui::Event::PointerMoved(point));
    send_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    );
    let moved = point + Vec2::new(20.0, 0.0);
    send_event(&mut harness, egui::Event::PointerMoved(moved));
    send_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: moved,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    let document = &harness.state().workspace.documents[&document_id];
    let view = document.view(table_id).unwrap();
    assert!(
        view.layout.column_width_pixels(0) as f32 > start_width,
        "the shared fixed and scroll pane strip resizes frozen column A"
    );
    assert_eq!(view.selection, CellRange::single(CellAddress::default()));
}

/// Waits until startup worker requests have been handled before another test request starts.
fn wait_for_worker_requests(harness: &mut Harness<'_, TexcelApp>, document_id: DocumentId) {
    for _ in 0..160 {
        let app = harness.state();
        let worker_is_idle = !app.pending_initial_calculations.contains_key(&document_id)
            && !app.pending_calculations.contains_key(&document_id)
            && !app.calculation_retries.contains(&document_id)
            && app.pending_markdown_displays.is_empty()
            && !app.pending_auto_fits.contains_key(&document_id)
            && !app.deferred_auto_fits.contains_key(&document_id);
        if worker_is_idle {
            return;
        }
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let app = harness.state();
    assert!(
        !app.pending_initial_calculations.contains_key(&document_id)
            && !app.pending_calculations.contains_key(&document_id)
            && !app.calculation_retries.contains(&document_id)
            && app.pending_markdown_displays.is_empty()
            && !app.pending_auto_fits.contains_key(&document_id)
            && !app.deferred_auto_fits.contains_key(&document_id),
        "worker requests settle before the next request: status={:?}, markdown={}, auto_fit={}, deferred_auto_fit={}",
        app.status,
        app.pending_markdown_displays.len(),
        app.pending_auto_fits.contains_key(&document_id),
        app.deferred_auto_fits.contains_key(&document_id),
    );
}

/// Measures a fresh formula result after its calculation root is published.
#[test]
fn formula_autofit_waits_for_the_visible_calculation_root() {
    let directory = crate::ui::tests::TestDirectory::new();
    let path = directory.0.join("autofit.md");
    std::fs::write(
        &path,
        "| Source | Result |\n| --- | --- |\n| A long calculated value that must fit | =A2 |\n",
    )
    .expect("the Markdown source is written");
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    let (document_id, token) = {
        let app = harness.state_mut();
        let document_id = app
            .start_path_load_for_queue(path)
            .unwrap_or_else(|error| {
                panic!("the Markdown load is admitted after startup work settles: {error:?}")
            })
            .expect("the Markdown load starts");
        (document_id, app.pending_markdown_loads[&document_id])
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let result = loop {
        let app = harness.state_mut();
        let _ = app.file_worker.poll_load_progress();
        if let Some(result) = app
            .file_worker
            .poll()
            .into_iter()
            .find(|result| result.token == token)
        {
            break result;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the Markdown load worker completes"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    };
    assert!(!harness.state_mut().dispatch_file_result(result));
    let table_id = harness.state().workspace.documents[&document_id]
        .active_table()
        .unwrap()
        .id;
    assert!(
        harness
            .state()
            .pending_calculations
            .contains_key(&document_id)
    );
    let ctx = harness.ctx.clone();
    harness
        .state_mut()
        .request_column_auto_fit(&ctx, document_id, table_id, 1);
    assert!(
        harness
            .state()
            .deferred_auto_fits
            .contains_key(&document_id)
    );

    for _ in 0..120 {
        harness.step();
        let document = &harness.state().workspace.documents[&document_id];
        let fresh_formula_root = harness
            .state()
            .calculation_snapshots
            .get(&document_id)
            .filter(|(revision, _)| *revision == document.calculation_revision)
            .and_then(|(_, root)| root.get(table_id, CellAddress { row: 1, column: 1 }))
            .is_some_and(|(status, value)| status == CalcStatus::Fresh && value.is_some());
        if !harness
            .state()
            .deferred_auto_fits
            .contains_key(&document_id)
            && !harness.state().pending_auto_fits.contains_key(&document_id)
            && document.view(table_id).unwrap().layout.column(1).is_some()
            && fresh_formula_root
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let document = &harness.state().workspace.documents[&document_id];
    let width = document
        .view(table_id)
        .unwrap()
        .layout
        .column_width_pixels(1);
    let calculation = document.calculation_result_for(table_id, CellAddress { row: 1, column: 1 });
    assert!(
        width > 180.0,
        "the fit uses the visible formula result: width={width}, calculation={calculation:?}, cells={:?}, root={:?}",
        document.tables[0].stored_cells().collect::<Vec<_>>(),
        harness
            .state()
            .calculation_snapshots
            .get(&document_id)
            .map(|(_, root)| (
                root.get(table_id, CellAddress { row: 1, column: 1 }),
                root.entries()
            ))
    );
}

/// Fits every column of the table in one background measurement and leaves later columns alone.
#[test]
fn autofit_all_columns_sets_each_table_column_width() {
    let mut harness = grid_harness(3, 3);
    let (document_id, table_id) = {
        let document = harness.state().workspace.active_document().unwrap();
        (document.id, document.active_table().unwrap().id)
    };
    wait_for_worker_requests(&mut harness, document_id);
    let ctx = harness.ctx.clone();
    harness
        .state_mut()
        .apply_layout_action(LayoutAction::AutoFitAllColumns, &ctx);
    assert!(harness.state().pending_auto_fits.contains_key(&document_id));
    wait_for_worker_requests(&mut harness, document_id);
    let layout = &harness.state().workspace.documents[&document_id]
        .view(table_id)
        .unwrap()
        .layout;
    for column in 0..3 {
        let width = layout.column(column).and_then(|column| column.width);
        assert!(
            width.is_some_and(|width| width < DEFAULT_COLUMN_WIDTH_UNITS),
            "column {column} fits its short values: {width:?}"
        );
    }
    assert_eq!(layout.column(3).and_then(|column| column.width), None);
}

/// Drops a deferred measurement when the user changes the same local dimension.
#[test]
fn deferred_autofit_does_not_overwrite_a_manual_layout_change() {
    let mut harness = grid_harness(3, 3);
    let (document_id, table_id) = {
        let document = harness.state().workspace.active_document().unwrap();
        (document.id, document.active_table().unwrap().id)
    };
    let ctx = harness.ctx.clone();
    {
        let app = harness.state_mut();
        let revision = app.workspace.documents[&document_id].revision;
        let token = app.next_job_token(document_id, revision);
        app.pending_calculations.insert(document_id, token);
        app.request_column_auto_fit(&ctx, document_id, table_id, 0);
    }
    assert!(
        harness
            .state()
            .deferred_auto_fits
            .contains_key(&document_id)
    );
    harness
        .state_mut()
        .workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .view_mut(table_id)
        .unwrap()
        .set_layout_value(
            LayoutTarget::Column(0),
            LayoutValue::Column(Some(ColumnLayout { width: Some(20.0) })),
        );
    harness
        .state_mut()
        .pending_calculations
        .remove(&document_id);
    harness.state_mut().start_ready_auto_fits(&ctx);
    let app = harness.state();
    assert!(!app.deferred_auto_fits.contains_key(&document_id));
    assert!(!app.pending_auto_fits.contains_key(&document_id));
    assert_eq!(
        app.workspace.documents[&document_id]
            .view(table_id)
            .unwrap()
            .layout
            .column(0)
            .and_then(|column| column.width),
        Some(20.0)
    );
}

/// Removes a deferred measurement when its calculation request is canceled.
#[test]
fn canceled_calculation_discards_deferred_autofit() {
    let mut harness = grid_harness(3, 3);
    let (document_id, table_id) = {
        let document = harness.state().workspace.active_document().unwrap();
        (document.id, document.active_table().unwrap().id)
    };
    let ctx = harness.ctx.clone();
    {
        let app = harness.state_mut();
        app.clear_calculation_jobs_for_reset(document_id);
        let revision = app.workspace.documents[&document_id].revision;
        let token = app.next_job_token(document_id, revision);
        app.pending_calculations.insert(document_id, token);
        app.request_column_auto_fit(&ctx, document_id, table_id, 0);
        assert!(app.deferred_auto_fits.contains_key(&document_id));
        app.cancel_calculation(document_id);
        assert!(!app.deferred_auto_fits.contains_key(&document_id));
    }
}

/// Clears active and deferred measurements when the shared worker has stopped.
#[test]
fn stopped_file_worker_ends_all_auto_fit_requests() {
    let mut harness = grid_harness(3, 3);
    let (document_id, table_id) = {
        let document = harness.state().workspace.active_document().unwrap();
        (document.id, document.active_table().unwrap().id)
    };
    wait_for_worker_requests(&mut harness, document_id);
    let ctx = harness.ctx.clone();
    harness
        .state_mut()
        .request_column_auto_fit(&ctx, document_id, table_id, 0);
    assert!(harness.state().pending_auto_fits.contains_key(&document_id));

    let revision = harness.state().workspace.documents[&document_id].revision;
    harness.state_mut().pending_calculations.insert(
        document_id,
        JobToken {
            document_id,
            revision,
            job_id: u64::MAX,
        },
    );
    harness
        .state_mut()
        .request_column_auto_fit(&ctx, document_id, table_id, 1);
    assert!(
        harness
            .state()
            .deferred_auto_fits
            .contains_key(&document_id)
    );
    harness.state_mut().file_worker = crate::file_worker::test_support::stopped_worker();
    harness.state_mut().process_file_results();
    harness
        .state_mut()
        .pending_calculations
        .remove(&document_id);
    assert!(harness.state().deferred_auto_fits.is_empty());
    assert!(harness.state().pending_auto_fits.is_empty());
    assert_eq!(
        harness.state().status.as_ref().map(Message::key),
        Some("status-file-worker-stopped")
    );
}

// Verifies that range cycling and editing keep the active cell inside the rectangle.
#[test]
fn range_navigation_and_f2_enter_preserve_the_primary_rectangle() {
    let mut harness = grid_harness(6, 2);
    harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("A1"))
        .next()
        .expect("the grid exposes a cell")
        .focus();
    harness.step();

    let mut shift = egui::Modifiers::NONE;
    shift.shift = true;
    send_event(&mut harness, key_event(Key::ArrowDown, shift));
    send_event(&mut harness, key_event(Key::ArrowDown, shift));
    let document = harness.state().workspace.active_document().unwrap();
    let table_id = document.active_table().unwrap().id;
    let view = document.view(table_id).unwrap();
    let range = CellRange {
        anchor: CellAddress { row: 0, column: 0 },
        head: CellAddress { row: 2, column: 0 },
    };
    assert_eq!(view.selection, range);
    assert_eq!(view.active_cell(), range.anchor);

    send_event(&mut harness, key_event(Key::Enter, egui::Modifiers::NONE));
    let view = harness
        .state()
        .workspace
        .active_document()
        .unwrap()
        .view(table_id)
        .unwrap();
    assert_eq!(view.selection, range);
    assert_eq!(view.active_cell(), CellAddress { row: 1, column: 0 });

    send_event(&mut harness, key_event(Key::F2, egui::Modifiers::NONE));
    assert!(harness.state().edit.is_editing());
    send_event(&mut harness, key_event(Key::Enter, egui::Modifiers::NONE));
    let view = harness
        .state()
        .workspace
        .active_document()
        .unwrap()
        .view(table_id)
        .unwrap();
    assert!(!harness.state().edit.is_editing());
    assert_eq!(view.selection, range);
    assert_eq!(view.active_cell(), CellAddress { row: 2, column: 0 });
}

// Verifies that Page navigation uses the drawn body's exact pixel extent.
#[test]
fn page_navigation_uses_viewport_prefix_geometry_and_shift_keeps_active_cell() {
    let mut harness = grid_harness(120, 2);
    harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("A1"))
        .next()
        .expect("the grid exposes a cell")
        .focus();
    harness.step();

    let expected = {
        let app = harness.state();
        let document = app.workspace.active_document().unwrap();
        let table = document.active_table().unwrap();
        app.grid_row_layouts[&(document.id, table.id)].row_at_position(app.grid_page_extent.y)
    };
    send_event(
        &mut harness,
        key_event(Key::PageDown, egui::Modifiers::NONE),
    );
    let document = harness.state().workspace.active_document().unwrap();
    let table_id = document.active_table().unwrap().id;
    let view = document.view(table_id).unwrap();
    assert_eq!(view.active_cell().row, expected);
    let active_before_shift = view.active_cell();
    let mut shift = egui::Modifiers::NONE;
    shift.shift = true;
    send_event(&mut harness, key_event(Key::PageDown, shift));
    harness.step();
    harness.step();
    harness.step();
    let document = harness.state().workspace.active_document().unwrap();
    let view = document.view(table_id).unwrap();
    assert_eq!(view.active_cell(), active_before_shift);
    assert_eq!(view.selection.anchor, active_before_shift);
    assert!(view.selection.head.row > active_before_shift.row);
    let layout = view.layout.as_ref();
    let row_layout = &harness.state().grid_row_layouts[&(document.id, table_id)];
    let frozen_height = row_layout.prefix_height(layout.frozen().rows);
    let endpoint_top = row_layout.prefix_height(view.selection.head.row);
    let endpoint_bottom = endpoint_top + row_layout.row_height(view.selection.head.row);
    let scrollable_body_top = frozen_height + view.scroll.y;
    let scrollable_body_bottom = scrollable_body_top + harness.state().grid_page_extent.y;
    assert!(endpoint_top >= scrollable_body_top);
    assert!(endpoint_bottom <= scrollable_body_bottom);

    let first_extended_head = view.selection.head;
    send_event(&mut harness, key_event(Key::PageDown, shift));
    harness.step();
    harness.step();
    harness.step();
    let view = harness
        .state()
        .workspace
        .active_document()
        .unwrap()
        .view(table_id)
        .unwrap();
    assert!(view.selection.head.row > first_extended_head.row);
    assert_eq!(view.active_cell(), active_before_shift);
    assert!(harness.state().grid_focused);
    let focus_owner = harness
        .state()
        .grid_keyboard_owner
        .expect("the visible selection head owns grid keyboard input");
    assert_eq!(
        harness.ctx.memory(|memory| memory.focused()),
        Some(focus_owner)
    );

    let range_after_scroll = view.selection;
    send_event(&mut harness, key_event(Key::Enter, egui::Modifiers::NONE));
    let view = harness
        .state()
        .workspace
        .active_document()
        .unwrap()
        .view(table_id)
        .unwrap();
    assert_eq!(view.selection, range_after_scroll);
    assert_eq!(view.active_cell().row, active_before_shift.row + 1);
}

// Verifies startup and dialog close return keyboard focus to the active grid cell.
#[test]
fn startup_and_search_close_restore_grid_keyboard_focus() {
    let mut harness = grid_harness(4, 2);
    let document_id = harness.state().workspace.active_document().unwrap().id;
    assert!(harness.state().grid_focused);
    let initial_focus = harness.ctx.memory(|memory| memory.focused());
    assert!(initial_focus.is_some());

    harness.state_mut().start_search_dialog();
    harness.step();
    let search_field = Id::unique(egui::IdSalt::new(("search_query", document_id)));
    assert_eq!(
        harness.ctx.memory(|memory| memory.focused()),
        Some(search_field)
    );

    // The harness tree omits egui's separate dialog viewport, so close its state between frames.
    harness.state_mut().search_dialog = None;
    harness.step();
    assert!(harness.state().search_dialog.is_none());
    harness.step();
    assert!(harness.state().grid_focused);
    assert_ne!(
        harness.ctx.memory(|memory| memory.focused()),
        Some(search_field)
    );
}

// Verify that startup grid text can be entered and committed without a pointer click.
#[test]
fn startup_grid_accepts_text_and_enter_without_a_click() {
    let mut harness = Harness::builder()
        .with_size([900.0, 620.0])
        .build_eframe(|creation| TexcelApp::new_with_settings_path(creation, None));
    harness.step();

    let (document_id, table_id) = {
        let document = harness
            .state()
            .workspace
            .active_document()
            .expect("the startup document is active");
        (
            document.id,
            document
                .active_table()
                .expect("the startup table is active")
                .id,
        )
    };
    assert!(harness.state().grid_focused);
    assert_eq!(
        harness.state().workspace.documents[&document_id]
            .view(table_id)
            .unwrap()
            .active_cell(),
        CellAddress::default()
    );
    let original_revision = harness.state().workspace.documents[&document_id].revision;

    harness.event(egui::Event::Text("typed at launch".to_owned()));
    harness.step();
    assert!(harness.state().edit.is_editing());
    assert_eq!(harness.state().edit.buffer, "typed at launch");
    send_event(&mut harness, key_event(Key::Enter, egui::Modifiers::NONE));

    let document = &harness.state().workspace.documents[&document_id];
    assert!(!harness.state().edit.is_editing());
    assert!(document.revision > original_revision);
    assert_eq!(
        document.tables[0]
            .cell(CellAddress::default())
            .map(|cell| &cell.input),
        Some(&CellInput::Text("typed at launch".to_owned()))
    );
}

// Verifies whole-axis shortcuts and Backspace use the shared clear command.
#[test]
fn axis_shortcuts_keep_active_cell_and_backspace_clears_selected_cells() {
    let mut harness = grid_harness(3, 2);
    harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("A1"))
        .next()
        .expect("A1 is visible")
        .focus();
    harness.step();

    let mut control = egui::Modifiers::NONE;
    control.ctrl = true;
    send_event(&mut harness, key_event(Key::Space, control));
    let table_id = harness
        .state()
        .workspace
        .active_document()
        .unwrap()
        .active_table()
        .unwrap()
        .id;
    let view = harness
        .state()
        .workspace
        .active_document()
        .unwrap()
        .view(table_id)
        .unwrap();
    assert_eq!(
        view.selection.normalized(),
        (
            CellAddress::default(),
            CellAddress {
                row: MAX_ROWS - 1,
                column: 0,
            }
        )
    );
    assert_eq!(view.active_cell(), CellAddress::default());

    let mut shift = egui::Modifiers::NONE;
    shift.shift = true;
    send_event(&mut harness, key_event(Key::Space, shift));
    let view = harness
        .state()
        .workspace
        .active_document()
        .unwrap()
        .view(table_id)
        .unwrap();
    assert_eq!(
        view.selection.normalized(),
        (
            CellAddress::default(),
            CellAddress {
                row: 0,
                column: MAX_COLUMNS - 1,
            }
        )
    );
    send_event(
        &mut harness,
        key_event(Key::Backspace, egui::Modifiers::NONE),
    );
    let table = harness
        .state()
        .workspace
        .active_document()
        .unwrap()
        .active_table()
        .unwrap();
    assert_eq!(
        table.cell(CellAddress::default()).unwrap().input,
        CellInput::Empty
    );
    assert_eq!(
        table.cell(CellAddress { row: 0, column: 1 }).unwrap().input,
        CellInput::Empty
    );
    assert!(table.cell(CellAddress { row: 1, column: 0 }).is_some());

    let mut command = egui::Modifiers::NONE;
    command.command = true;
    command.ctrl = !harness.ctx.os().is_mac();
    command.mac_cmd = harness.ctx.os().is_mac();
    send_event(&mut harness, key_event(Key::Z, command));
    let table = harness
        .state()
        .workspace
        .active_document()
        .unwrap()
        .active_table()
        .unwrap();
    assert!(matches!(
        table.cell(CellAddress::default()).unwrap().input,
        CellInput::Text(_)
    ));
    assert!(matches!(
        table.cell(CellAddress { row: 0, column: 1 }).unwrap().input,
        CellInput::Text(_)
    ));
}

// Verifies repeated Cmd/Ctrl+Arrow keys leave each sparse data region.
#[test]
fn command_arrow_advances_from_region_edges_to_next_region_and_sheet_edge() {
    let mut harness = grid_harness(8, 1);
    let (document_id, table_id) = {
        let document = harness.state().workspace.active_document().unwrap();
        (document.id, document.active_table().unwrap().id)
    };
    let empty_rows = [2, 3, 5, 6, 7]
        .into_iter()
        .map(|row| (CellAddress { row, column: 0 }, None))
        .collect();
    harness
        .state_mut()
        .workspace
        .documents
        .get_mut(&document_id)
        .unwrap()
        .set_cells(table_id, empty_rows)
        .expect("the fixture clears rows outside the two data regions");
    harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("A1"))
        .next()
        .expect("A1 is visible")
        .focus();
    harness.step();
    let mut command = egui::Modifiers::NONE;
    command.command = true;
    command.ctrl = !harness.ctx.os().is_mac();
    command.mac_cmd = harness.ctx.os().is_mac();

    for expected_row in [1, 4, MAX_ROWS - 1] {
        send_event(&mut harness, key_event(Key::ArrowDown, command));
        let document = harness.state().workspace.active_document().unwrap();
        assert_eq!(
            document.view(table_id).unwrap().active_cell(),
            CellAddress {
                row: expected_row,
                column: 0,
            }
        );
    }
    for expected_row in [4, 1, 0] {
        send_event(&mut harness, key_event(Key::ArrowUp, command));
        let document = harness.state().workspace.active_document().unwrap();
        assert_eq!(
            document.view(table_id).unwrap().active_cell(),
            CellAddress {
                row: expected_row,
                column: 0,
            }
        );
    }
}

// Verifies the Space shortcut consumes only its own text event.
#[test]
fn shift_space_does_not_replace_the_row_selection_with_text() {
    let mut harness = grid_harness(3, 2);
    harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("A1"))
        .next()
        .expect("A1 is visible")
        .focus();
    harness.step();

    let mut shift = egui::Modifiers::NONE;
    shift.shift = true;
    send_input_batch(
        &mut harness,
        [
            key_event(Key::Space, shift),
            egui::Event::Text(" ".to_owned()),
            egui::Event::ModifiersChanged(egui::Modifiers::NONE),
        ],
    );

    let document = harness.state().workspace.active_document().unwrap();
    let table = document.active_table().unwrap();
    let view = document.view(table.id).unwrap();
    assert_eq!(
        view.selection.normalized(),
        (
            CellAddress::default(),
            CellAddress {
                row: 0,
                column: MAX_COLUMNS - 1,
            }
        )
    );
    assert!(!harness.state().edit.is_editing());
    assert_eq!(
        table.cell(CellAddress::default()).unwrap().input,
        CellInput::Text("1:1".to_owned())
    );

    send_event(&mut harness, key_event(Key::Enter, egui::Modifiers::NONE));
    assert_eq!(
        harness
            .state()
            .workspace
            .active_document()
            .unwrap()
            .active_table()
            .unwrap()
            .cell(CellAddress::default())
            .unwrap()
            .input,
        CellInput::Text("1:1".to_owned())
    );

    let mut shift = egui::Modifiers::NONE;
    shift.shift = true;
    send_input_batch(
        &mut harness,
        [
            egui::Event::ModifiersChanged(shift),
            key_event(Key::Space, shift),
            egui::Event::Text(" ".to_owned()),
            egui::Event::Text("x".to_owned()),
        ],
    );
    assert_eq!(harness.state().edit.buffer, "x");
    send_event(&mut harness, key_event(Key::Escape, egui::Modifiers::NONE));
    assert_eq!(
        harness
            .state()
            .workspace
            .active_document()
            .unwrap()
            .active_table()
            .unwrap()
            .cell(CellAddress { row: 0, column: 1 })
            .unwrap()
            .input,
        CellInput::Text("1:2".to_owned())
    );
}

// Verifies an actual secondary click opens the cell context menu.
#[test]
fn cell_context_menu_opens_for_the_clicked_cell() {
    let mut harness = grid_harness(3, 3);
    let (document_id, table_id) = {
        let document = harness.state().workspace.active_document().unwrap();
        (document.id, document.active_table().unwrap().id)
    };
    let popup_ids = harness
        .state()
        .accesskit_table_tokens
        .get(&(document_id, table_id))
        .copied()
        .into_iter()
        .flat_map(|token| {
            (0..4).filter_map(move |segment| {
                grid_accessibility::grid_accesskit_cell_id(
                    token,
                    CellAddress { row: 1, column: 1 },
                    segment,
                )
                .map(|widget| widget.with("popup"))
            })
        })
        .collect::<Vec<_>>();
    harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("B2"))
        .next()
        .expect("B2 is visible")
        .click_secondary();
    harness.step();

    assert!(
        popup_ids
            .iter()
            .any(|id| egui::Popup::is_id_open(&harness.ctx, *id))
    );
}

// Verifies focus loss ends a range drag even when no pointer release arrives.
#[test]
fn window_focus_loss_cancels_a_range_drag() {
    let mut harness = grid_harness(4, 4);
    let first = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("B2"))
        .next()
        .expect("B2 is visible")
        .rect()
        .center();
    let first_target = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("C3"))
        .next()
        .expect("C3 is visible")
        .rect()
        .center();
    let lost_release_target = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("D4"))
        .next()
        .expect("D4 is visible")
        .rect()
        .center();

    send_event(&mut harness, egui::Event::PointerMoved(first));
    send_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: first,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    );
    send_event(&mut harness, egui::Event::PointerMoved(first_target));
    assert!(harness.state().grid_selection_drag.is_some());
    let selection_at_focus_loss = harness
        .state()
        .workspace
        .active_document()
        .unwrap()
        .view(
            harness
                .state()
                .workspace
                .active_document()
                .unwrap()
                .active_table()
                .unwrap()
                .id,
        )
        .unwrap()
        .selection;

    send_event(&mut harness, egui::Event::WindowFocused(false));
    assert!(harness.state().grid_selection_drag.is_none());
    send_event(&mut harness, egui::Event::PointerMoved(lost_release_target));

    let document = harness.state().workspace.active_document().unwrap();
    let view = document.view(document.active_table().unwrap().id).unwrap();
    assert_eq!(view.selection, selection_at_focus_loss);
}

// Verifies a real cell drag selects its endpoint and an outside start cannot be clamped in.
#[test]
fn pointer_drag_selects_cells_and_rejects_an_outside_start() {
    let mut harness = grid_harness(4, 4);
    let first = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("A1"))
        .next()
        .expect("A1 is visible")
        .rect()
        .center();
    let target = harness
        .query_all(By::new().role(egui::Role::GridCell).label_contains("C3"))
        .next()
        .expect("C3 is visible")
        .rect()
        .center();
    let initial = harness
        .state()
        .workspace
        .active_document()
        .unwrap()
        .view(
            harness
                .state()
                .workspace
                .active_document()
                .unwrap()
                .active_table()
                .unwrap()
                .id,
        )
        .unwrap()
        .selection;

    send_event(&mut harness, egui::Event::PointerMoved(first));
    send_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: first,
            button: egui::PointerButton::Secondary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    );
    send_event(&mut harness, egui::Event::PointerMoved(target));
    send_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Secondary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    let document = harness.state().workspace.active_document().unwrap();
    assert_eq!(
        document
            .view(document.active_table().unwrap().id)
            .unwrap()
            .selection,
        initial
    );

    send_event(&mut harness, egui::Event::PointerMoved(first));
    send_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: first,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    );
    send_event(&mut harness, egui::Event::PointerGone);
    assert!(harness.state().grid_selection_drag.is_none());

    let outside = Pos2::new(16.0, target.y);
    send_event(&mut harness, egui::Event::PointerMoved(outside));
    send_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: outside,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    );
    send_event(&mut harness, egui::Event::PointerMoved(target));
    send_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    let document = harness.state().workspace.active_document().unwrap();
    let view = document.view(document.active_table().unwrap().id).unwrap();
    assert_eq!(view.selection, initial);

    send_event(&mut harness, egui::Event::PointerMoved(first));
    send_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: first,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    );
    send_event(&mut harness, egui::Event::PointerMoved(target));
    send_event(
        &mut harness,
        egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    let document = harness.state().workspace.active_document().unwrap();
    let view = document.view(document.active_table().unwrap().id).unwrap();
    assert_eq!(
        view.selection,
        CellRange {
            anchor: CellAddress { row: 0, column: 0 },
            head: CellAddress { row: 2, column: 2 },
        }
    );
    assert_eq!(view.active_cell(), CellAddress { row: 0, column: 0 });
}
