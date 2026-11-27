use super::*;
use egui_kittest::{Harness, kittest::State};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[path = "app_host_test/app_exit_test.rs"]
mod app_exit;
#[path = "app_host_test/multiple_documents_test.rs"]
mod multiple_documents;

/// Builds three real input files for the shared-window host.
fn host_with_three_documents(directory: &Path) -> (Harness<'static, TexcelAppHost>, [PathBuf; 3]) {
    let fixture_paths = ["north.md", "south.md", "west.md"].map(|name| directory.join(name));
    for (name, path) in ["North", "South", "West"].into_iter().zip(&fixture_paths) {
        fs::write(
            path,
            format!("| Name | Value |\n| --- | --- |\n| {name} | 1 |\n"),
        )
        .expect("write a Markdown fixture");
    }
    let paths = fixture_paths.map(|path| fs::canonicalize(path).expect("canonicalize fixture"));
    let open_paths = paths.clone();
    let settings_path = directory.join("settings.json");
    let mut harness = Harness::builder()
        .with_size([1000.0, 700.0])
        .allow_missing_glyphs()
        .build_eframe(move |creation| {
            creation.egui_ctx.set_embed_viewports(false);
            let mut app = TexcelApp::new_with_settings_path(creation, Some(settings_path));
            app.queue_open_paths(open_paths);
            TexcelAppHost::new(app, creation.egui_ctx.viewport_id())
        });
    wait_for_host_documents(&mut harness, &paths);
    (harness, paths)
}

/// Waits for all three files to be adopted by the normal load path.
fn wait_for_host_documents(harness: &mut Harness<'_, TexcelAppHost>, paths: &[PathBuf; 3]) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        harness.step();
        let state = with_host(harness.state().id, |host| {
            let paths_ready = paths.iter().all(|path| {
                host.app
                    .file_backings
                    .values()
                    .any(|backing| backing.path == *path)
            });
            (
                paths_ready
                    && host.app.pending_file_load_paths.is_empty()
                    && host.app.queued_open_paths.is_empty(),
                host.app
                    .file_backings
                    .values()
                    .map(|backing| backing.path.clone())
                    .collect::<Vec<_>>(),
                host.app
                    .pending_file_load_paths
                    .keys()
                    .copied()
                    .collect::<Vec<_>>(),
                host.app.queued_open_paths.clone(),
                host.app.status.clone(),
            )
        });
        if state.0 {
            return;
        }
        if Instant::now() >= deadline {
            panic!(
                "the three host documents load; paths={:?}, pending={:?}, queued={:?}, status={:?}",
                state.1, state.2, state.3, state.4
            );
        }
    }
}

/// Returns the loaded document IDs in the fixture path order.
fn host_document_ids(
    harness: &Harness<'_, TexcelAppHost>,
    paths: &[PathBuf; 3],
) -> [DocumentId; 3] {
    with_host(harness.state().id, |host| {
        std::array::from_fn(|index| {
            let path = &paths[index];
            host.app
                .file_backings
                .iter()
                .find_map(|(id, backing)| (backing.path == *path).then_some(*id))
                .expect("each fixture has an adopted document")
        })
    })
}

/// Finds egui's visible widget-ID diagnostics after multi-window layout.
fn host_id_clash_texts(harness: &Harness<'_, TexcelAppHost>) -> Vec<String> {
    harness
        .output()
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::epaint::Shape::Text(shape) if shape.galley.job.text.starts_with('🔥') => {
                Some(shape.galley.job.text.clone())
            }
            _ => None,
        })
        .collect()
}

/// Renders one document window after its real raw input passes through the host hook.
fn host_window_frame(
    harness: &mut Harness<'_, TexcelAppHost>,
    document_id: DocumentId,
    events: impl IntoIterator<Item = egui::Event>,
) -> egui::FullOutput {
    let host_id = harness.state().id;
    let viewport = with_host(host_id, |host| {
        *host
            .document_viewports
            .get(&document_id)
            .expect("the host assigned a window to the document")
    });
    let ctx = harness.ctx.clone();
    let mut raw = harness.input().clone();
    raw.viewport_id = viewport;
    let viewport_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0));
    let child_viewport = raw
        .viewports
        .entry(viewport)
        .or_insert_with(egui::ViewportInfo::default);
    child_viewport.parent = Some(egui::ViewportId::ROOT);
    child_viewport.focused = Some(true);
    child_viewport.inner_rect = Some(viewport_rect);
    child_viewport.outer_rect = Some(viewport_rect);
    raw.screen_rect = Some(viewport_rect);
    raw.events = events.into_iter().collect();
    eframe::App::raw_input_hook(harness.state_mut(), &ctx, &mut raw);
    let mut output = ctx.run_ui(raw, |ui| {
        draw_document_viewport(host_id, viewport, document_id, ui);
    });
    output.textures_delta.clear();
    output
}

/// Delivers a native child-window close through the production viewport callback.
fn close_host_window(
    harness: &mut Harness<'_, TexcelAppHost>,
    document_id: DocumentId,
) -> egui::FullOutput {
    let host_id = harness.state().id;
    let viewport = with_host(host_id, |host| host.document_viewports[&document_id]);
    let ctx = harness.ctx.clone();
    let mut raw = harness.input().clone();
    raw.viewport_id = viewport;
    let viewport_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0));
    let child_viewport = raw
        .viewports
        .entry(viewport)
        .or_insert_with(egui::ViewportInfo::default);
    child_viewport.parent = Some(egui::ViewportId::ROOT);
    child_viewport.focused = Some(true);
    child_viewport.inner_rect = Some(viewport_rect);
    child_viewport.outer_rect = Some(viewport_rect);
    child_viewport.events.push(egui::ViewportEvent::Close);
    raw.screen_rect = Some(viewport_rect);
    raw.events.clear();
    eframe::App::raw_input_hook(harness.state_mut(), &ctx, &mut raw);
    let mut output = ctx.run_ui(raw, |ui| {
        draw_document_viewport(host_id, viewport, document_id, ui);
    });
    output.textures_delta.clear();
    harness.step();
    output
}

/// Finds a rendered grid cell by its stable address metadata.
fn host_cell_center(
    output: &egui::FullOutput,
    table_token: u32,
    address: CellAddress,
) -> egui::Pos2 {
    let update = output
        .platform_output
        .accesskit_update
        .clone()
        .expect("the painted host frame publishes its accessibility tree");
    let tree = State::new(update);
    let node = find_host_grid_cell(tree.root(), table_token, address)
        .expect("the requested grid cell is visible");
    let bounds = node.raw_bounds().expect("the cell has visible bounds");
    // Node bounds are in points; only the root node carries the pixels-per-point transform.
    egui::pos2(
        (bounds.x0 + bounds.x1) as f32 / 2.0,
        (bounds.y0 + bounds.y1) as f32 / 2.0,
    )
}

/// Searches the painted accessibility tree for one grid address.
fn find_host_grid_cell<'a>(
    node: egui_kittest::kittest::AccessKitNode<'a>,
    table_token: u32,
    address: CellAddress,
) -> Option<egui_kittest::kittest::AccessKitNode<'a>> {
    if node.role() == egui::Role::GridCell
        && crate::ui::grid_accessibility::decode_grid_cell_node_id(node.locate().0)
            .is_some_and(|(token, actual, _)| token == table_token && actual == address)
    {
        return Some(node);
    }
    node.children()
        .find_map(|child| find_host_grid_cell(child, table_token, address))
}

/// Finds a named button in a rendered child window.
fn host_button_center(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
    let update = output
        .platform_output
        .accesskit_update
        .clone()
        .expect("the painted host frame publishes its accessibility tree");
    let tree = State::new(update);
    let node = find_host_button(tree.root(), label)
        .unwrap_or_else(|| panic!("the requested child button {label:?} is visible"));
    let bounds = node.raw_bounds().expect("the button has visible bounds");
    // Node bounds are in points; only the root node carries the pixels-per-point transform.
    egui::pos2(
        (bounds.x0 + bounds.x1) as f32 / 2.0,
        (bounds.y0 + bounds.y1) as f32 / 2.0,
    )
}

/// Searches a child window's accessible controls by their translated label.
fn find_host_button<'a>(
    node: egui_kittest::kittest::AccessKitNode<'a>,
    label: &str,
) -> Option<egui_kittest::kittest::AccessKitNode<'a>> {
    if node.role() == egui::Role::Button
        && node.label().is_some_and(|actual| actual.contains(label))
    {
        return Some(node);
    }
    node.children()
        .find_map(|child| find_host_button(child, label))
}

/// Sends a pointer click with press and release on their own frames.
fn click_host_cell(harness: &mut Harness<'_, TexcelAppHost>, document_id: DocumentId) {
    let output = host_window_frame(harness, document_id, []);
    let table_token = with_host(harness.state().id, |host| {
        let table = host.app.workspace.documents[&document_id]
            .active_table()
            .expect("the document has an active table");
        host.app
            .accesskit_table_tokens
            .iter()
            .find_map(|((id, table_id), token)| {
                (*id == document_id && *table_id == table.id).then_some(*token)
            })
            .expect("the rendered document has an accessibility table token")
    });
    let point = host_cell_center(&output, table_token, CellAddress { row: 1, column: 0 });
    host_window_frame(harness, document_id, [egui::Event::WindowFocused(true)]);
    host_window_frame(harness, document_id, [egui::Event::PointerMoved(point)]);
    host_window_frame(
        harness,
        document_id,
        [egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    host_window_frame(
        harness,
        document_id,
        [egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    let selected = with_host(harness.state().id, |host| {
        host.app
            .workspace
            .documents
            .get(&document_id)
            .and_then(|document| {
                let table = document.active_table()?;
                document.view(table.id).map(|view| view.active_cell())
            })
    });
    assert_eq!(
        selected,
        Some(CellAddress { row: 1, column: 0 }),
        "the child window selects A2 after its production pointer sequence at {point:?}"
    );
}

/// Clicks a translated button through pointer input in its owning viewport.
fn click_host_button(
    harness: &mut Harness<'_, TexcelAppHost>,
    document_id: DocumentId,
    label: &str,
) {
    let output = host_window_frame(harness, document_id, []);
    let point = host_button_center(&output, label);
    host_window_frame(harness, document_id, [egui::Event::PointerMoved(point)]);
    host_window_frame(
        harness,
        document_id,
        [egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    host_window_frame(
        harness,
        document_id,
        [egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
}

/// Sends Quit through the native-menu action queue on desktop platforms.
fn request_host_app_quit(harness: &mut Harness<'_, TexcelAppHost>, document_id: DocumentId) {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let host_id = harness.state().id;
        let viewport = with_host(host_id, |host| host.document_viewports[&document_id]);
        let context = harness.ctx.clone();
        let target = with_host(host_id, |host| {
            host.app
                .with_window_state(viewport, Some(document_id), |app| {
                    app.native_action_target(&context, false)
                })
        });
        with_host(host_id, |host| {
            host.app.native_menu_events_for_test = vec![crate::native_menu::NativeMenuEvent {
                id: "app.quit".to_owned(),
                recent_path: None,
                target,
            }];
        });
        host_window_frame(harness, document_id, []);
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let file_label = crate::i18n::context_text(&harness.ctx, "menu-file");
        click_host_button(harness, document_id, &file_label);
        let quit_label = crate::i18n::context_text(&harness.ctx, "menu-file-quit");
        click_host_button(harness, document_id, &quit_label);
    }
}

/// Saves text entered in a child document and waits for its file result.
fn type_and_save_in_host_window(
    harness: &mut Harness<'_, TexcelAppHost>,
    document_id: DocumentId,
    text: &str,
    path: &Path,
) {
    click_host_cell(harness, document_id);
    host_window_frame(harness, document_id, [egui::Event::Text(text.to_owned())]);
    host_window_frame(
        harness,
        document_id,
        [egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    host_window_frame(
        harness,
        document_id,
        [egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    click_host_cell(harness, document_id);
    let mut command_modifiers = egui::Modifiers::COMMAND;
    if harness.ctx.os().is_mac() {
        command_modifiers.mac_cmd = true;
    } else {
        command_modifiers.ctrl = true;
    }
    host_window_frame(
        harness,
        document_id,
        [
            egui::Event::ModifiersChanged(command_modifiers),
            egui::Event::Key {
                key: egui::Key::S,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: command_modifiers,
            },
        ],
    );
    host_window_frame(
        harness,
        document_id,
        [
            egui::Event::Key {
                key: egui::Key::S,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: command_modifiers,
            },
            egui::Event::ModifiersChanged(egui::Modifiers::NONE),
        ],
    );

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        harness.step();
        let state = with_host(harness.state().id, |host| {
            let document = host.app.workspace.documents.get(&document_id);
            (
                document.is_some_and(|document| document.revision == document.saved_revision)
                    && !host.app.pending_saves.contains_key(&document_id),
                document.map(|document| (document.revision, document.saved_revision)),
                host.app.edit_document_id,
                host.app.edit.is_editing(),
                host.app.status.clone(),
                host.app.pending_saves.contains_key(&document_id),
            )
        });
        if state.0 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the child document save completes; revision={:?}, edit={:?}/{}, status={:?}, pending={}",
            state.1,
            state.2,
            state.3,
            state.4,
            state.5
        );
    }
    let bytes = fs::read(path).expect("the original file remains readable");
    assert!(
        bytes
            .windows(text.len())
            .any(|window| window == text.as_bytes()),
        "the owning file contains the typed value"
    );
}

#[test]
/// Asks the OS to bring a newly opened document window forward, so the shared menu bar
/// acts on it even when a file was dropped from another app that stays in front.
fn newly_opened_document_window_is_brought_forward() {
    let directory = crate::ui::tests::TestDirectory::new();
    let (harness, _) = host_with_three_documents(&directory.0);
    with_host(harness.state().id, |host| {
        host.app.pending_document_focus = None;
        let document_id = host
            .app
            .workspace
            .add_document("Dropped import", &["Sheet"]);
        host.sync_document_viewports();
        assert_eq!(host.app.pending_document_focus, Some(document_id));
        assert_eq!(
            host.document_viewports.get(&document_id),
            Some(&egui::ViewportId::from_hash_of((
                "texcel-document",
                document_id
            )))
        );
    });
}
