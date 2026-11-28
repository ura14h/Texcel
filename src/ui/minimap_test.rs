use super::scenario_tests::{ScenarioDirectory, queue_open_and_wait, scenario_harness};
use super::*;
use egui_kittest::kittest::{By, Queryable};

#[test]
/// Shows a Markdown file's minimap from the table list button and switches tables from it.
fn markdown_file_minimap_switches_tables() {
    let directory = ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    harness.state_mut().language = crate::i18n::Language::English;
    crate::i18n::set_context_language(&harness.ctx, crate::i18n::Language::English);
    queue_open_and_wait(
        &mut harness,
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("samples")
            .join("tables.md"),
    );
    let document_id = harness.state().workspace.active_document.unwrap();
    assert!(harness.state().minimap_source(document_id).is_some());
    let list = crate::i18n::context_text(&harness.ctx, "dialog-table-list");
    harness
        .query_all(By::new().role(egui::Role::Button).label(&list))
        .next()
        .expect("the table list button is visible")
        .click();
    harness.step();
    assert!(harness.state().minimap_open);

    // Wait for the background parse, then pick the second table from the minimap.
    let second = harness.state().workspace.documents[&document_id].tables[1].clone();
    for _ in 0..200 {
        harness.step();
        if harness
            .query_all(By::new().role(egui::Role::Button).label(&second.name))
            .next()
            .is_some()
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let _ = harness.render().map(|image| {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("kittest-renders");
        let _ = std::fs::create_dir_all(&directory);
        image.save(directory.join("minimap.png"))
    });
    harness
        .query_all(By::new().role(egui::Role::Button).label(&second.name))
        .next()
        .expect("the minimap names the second table")
        .click();
    harness.run_steps(2);
    assert_eq!(
        harness.state().workspace.documents[&document_id].active_table,
        second.id
    );

    // Tables are boxes with their names; their cells are not drawn.
    assert!(
        harness
            .query_all(By::new().label("user_id"))
            .next()
            .is_none(),
        "the minimap does not draw table cells"
    );
}

#[test]
/// Keeps the minimap beside the sheet, with its text against its left edge, while inserted
/// columns widen a table.
fn minimap_stays_left_aligned_as_columns_are_inserted() {
    let directory = ScenarioDirectory::new();
    let path = directory.0.join("tables.md");
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("samples")
            .join("tables.md"),
        &path,
    )
    .expect("copy the sample");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    queue_open_and_wait(&mut harness, path);
    harness.state_mut().minimap_open = true;
    let heading_left = |harness: &mut egui_kittest::Harness<'_, TexcelApp>| {
        for _ in 0..100 {
            harness.step();
            std::thread::sleep(std::time::Duration::from_millis(3));
        }
        harness
            .query_all(By::new().label_contains("この文章はセル編集で変化しない"))
            .next()
            .expect("the minimap shows the paragraph")
            .rect()
            .left()
    };
    let before = heading_left(&mut harness);
    super::scenario_tests::click_cell(&mut harness, "B2");
    for _ in 0..12 {
        harness
            .state_mut()
            .request_structure_action(StructureAction::InsertColumns);
        for _ in 0..20 {
            harness.step();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    let after = heading_left(&mut harness);
    let _ = harness.render().map(|image| {
        image.save(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("kittest-renders")
                .join("minimap-inserted-columns.png"),
        )
    });
    assert!(
        (after - before).abs() < 0.5,
        "the paragraph stays at the left edge: {before} -> {after}"
    );
    // The sheet ends where the minimap starts, so the minimap is not drawn under it.
    let sheet_right = harness
        .query_all(By::new().role(egui::Role::GridCell))
        .map(|cell| cell.rect().right())
        .fold(f32::MIN, f32::max);
    assert!(
        sheet_right <= after,
        "the sheet ({sheet_right}) stays left of the minimap ({after})"
    );
}

#[test]
/// Dragging over the minimap's text selects nothing.
fn minimap_text_cannot_be_selected() {
    let directory = ScenarioDirectory::new();
    let path = directory.0.join("tables.md");
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("samples")
            .join("tables.md"),
        &path,
    )
    .expect("copy the sample");
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    queue_open_and_wait(&mut harness, path);
    harness.state_mut().minimap_open = true;
    let paragraph = "この文章はセル編集で変化しない";
    let mut rect = None;
    for _ in 0..200 {
        harness.step();
        rect = harness
            .query_all(By::new().label_contains(paragraph))
            .next()
            .map(|node| node.rect());
        if rect.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let rect = rect.expect("the minimap shows the paragraph");
    let (start, end) = (rect.left_center(), rect.right_center());
    super::scenario_tests::send_batch(
        &mut harness,
        [
            egui::Event::PointerMoved(start),
            egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    super::scenario_tests::send_batch(&mut harness, [egui::Event::PointerMoved(end)]);
    super::scenario_tests::send_batch(
        &mut harness,
        [egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    super::scenario_tests::send_batch(&mut harness, [egui::Event::Copy]);
    let selected = harness
        .output()
        .platform_output
        .commands
        .iter()
        .any(|command| {
            matches!(command, egui::OutputCommand::TextSelectionSettled(text)
                if text.contains("この文章"))
        });
    assert!(!selected, "the minimap paragraph is not selected");
}

/// Opens a Markdown file, shows its minimap, and steps until the background parse is drawn.
fn open_with_minimap(path: PathBuf) -> egui_kittest::Harness<'static, TexcelApp> {
    let directory = ScenarioDirectory::new();
    let mut harness = scenario_harness(directory.0.join("settings.json"));
    queue_open_and_wait(&mut harness, path);
    harness.state_mut().minimap_open = true;
    for _ in 0..60 {
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(3));
    }
    harness
}

/// Returns where the minimap drew the box named `name`; sheet tabs with the same name sit
/// at the bottom left, outside the minimap panel on the right.
fn minimap_box(harness: &egui_kittest::Harness<'_, TexcelApp>, name: &str) -> Option<Rect> {
    harness
        .query_all(By::new().role(egui::Role::Button).label(name))
        .map(|node| node.rect())
        .find(|rect| rect.left() >= 600.0)
}

#[test]
/// Names every table box of a file that starts with a byte-order mark. The minimap's own
/// parse used to read the mark as text, lose the first table, and shift the names.
fn minimap_names_every_table_of_a_file_with_a_byte_order_mark() {
    let directory = ScenarioDirectory::new();
    let path = directory.0.join("bom-tables.md");
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("samples")
            .join("bom-tables.md"),
        &path,
    )
    .expect("copy the sample");
    let harness = open_with_minimap(path);
    let document_id = harness.state().workspace.active_document.unwrap();
    let names = harness.state().workspace.documents[&document_id]
        .tables
        .iter()
        .map(|table| table.name.clone())
        .collect::<Vec<_>>();
    assert_eq!(names.len(), 3);
    let boxes = names
        .iter()
        .map(|name| minimap_box(&harness, name).unwrap_or_else(|| panic!("a box for {name}")))
        .collect::<Vec<_>>();
    assert!(
        boxes.windows(2).all(|pair| pair[0].top() < pair[1].top()),
        "the boxes follow the tables' order in the file: {boxes:?}"
    );
}
