use super::*;

#[test]
// Routes edits and saves to all three document windows by their real IDs.
fn three_document_host_keeps_each_edit_and_save_with_its_file() {
    let directory = crate::ui::tests::TestDirectory::new();
    let (mut harness, paths) = host_with_three_documents(&directory.0);
    let diagnostics = host_id_clash_texts(&harness);
    assert!(
        diagnostics.is_empty(),
        "window widgets have unique IDs: {diagnostics:?}"
    );
    let document_ids = host_document_ids(&harness, &paths);

    for ((document_id, path), value) in
        document_ids
            .into_iter()
            .zip(&paths)
            .zip(["North edited", "South edited", "West edited"])
    {
        type_and_save_in_host_window(&mut harness, document_id, value, path);
    }

    assert_eq!(host_document_ids(&harness, &paths), document_ids);
    close_host_window(&mut harness, document_ids[1]);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        harness.step();
        if !with_host(harness.state().id, |host| {
            host.app.workspace.documents.contains_key(&document_ids[1])
        }) {
            break;
        }
        assert!(Instant::now() < deadline, "the clean child window closes");
    }
    with_host(harness.state().id, |host| {
        assert_eq!(host.app.workspace.documents.len(), 2);
        assert!(!host.app.file_backings.contains_key(&document_ids[1]));
    });
}

/// Keeps an unfinished edit in its window when the user edits and saves another file.
#[test]
fn saving_another_window_preserves_the_first_draft_and_file() {
    let directory = crate::ui::tests::TestDirectory::new();
    let (mut harness, paths) = host_with_three_documents(&directory.0);
    let [first, second, _] = host_document_ids(&harness, &paths);
    let original = fs::read(&paths[0]).expect("read the first document before editing");

    click_host_cell(&mut harness, first);
    host_window_frame(
        &mut harness,
        first,
        [egui::Event::Text("Draft in north".into())],
    );
    host_window_frame(&mut harness, first, [egui::Event::WindowFocused(false)]);
    type_and_save_in_host_window(&mut harness, second, "Saved in south", &paths[1]);
    assert_eq!(fs::read(&paths[0]).unwrap(), original);

    // The returning window keeps its own text and can save it to its original file.
    host_window_frame(&mut harness, first, [egui::Event::WindowFocused(true)]);
    with_host(harness.state().id, |host| {
        let window = &host.app.window_states[&host.document_viewports[&first]];
        assert_eq!(window.edit.buffer, "Draft in north");
        assert_eq!(window.edit_document_id, Some(first));
    });
    let modifiers = crate::ui::scenario_tests::command_modifiers(&harness.ctx);
    host_window_frame(
        &mut harness,
        first,
        [
            egui::Event::ModifiersChanged(modifiers),
            crate::ui::scenario_tests::key_event(Key::S, modifiers),
        ],
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        harness.step();
        if fs::read_to_string(&paths[0])
            .unwrap()
            .contains("Draft in north")
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "saving the returning editor writes its draft"
        );
    }
    let south = fs::read_to_string(&paths[1]).unwrap();
    assert!(south.contains("Saved in south"));
    assert!(!south.contains("Draft in north"));
}

/// Shows save progress and its Cancel button only in the window doing the work.
#[test]
fn export_progress_stays_in_its_document_window() {
    let directory = crate::ui::tests::TestDirectory::new();
    let (mut harness, paths) = host_with_three_documents(&directory.0);
    let [owner, other, _] = host_document_ids(&harness, &paths);
    let message = with_host(harness.state().id, |host| {
        let revision = host.app.workspace.documents[&owner].revision;
        let token = host.app.next_job_token(owner, revision);
        host.app.pending_exports.insert(owner, token);
        Message::new("dialog-export-preparing").resolve(host.app.language)
    });

    // Paint each file after its layout settles, then edit and save in the free window.
    for document in [owner, other] {
        host_window_frame(&mut harness, document, []);
        let output = host_window_frame(&mut harness, document, []);
        let visible = output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text.contains(&message))
        });
        assert_eq!(
            visible,
            document == owner,
            "save progress in window {document}"
        );
    }
    type_and_save_in_host_window(&mut harness, other, "Independent save", &paths[1]);
    assert!(with_host(harness.state().id, |host| host
        .app
        .pending_exports
        .contains_key(&owner)));
}

/// Keeps the formatting notice and the data-loss report in their own file windows.
#[test]
fn import_notice_stays_with_its_document() {
    let directory = crate::ui::tests::TestDirectory::new();
    let samples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("samples");
    let paths = [
        "interaction-workflow.md",
        "formatting-only.xlsx",
        "workbook.xlsx",
    ]
    .map(|name| {
        let path = directory.0.join(name);
        fs::copy(samples.join(name), &path).expect("copy the window's input file");
        path
    });
    let inputs = paths.clone();
    let settings = directory.0.join("settings.json");
    let mut harness = Harness::builder()
        .with_size([1000.0, 700.0])
        .build_eframe(move |creation| {
            creation.egui_ctx.set_embed_viewports(false);
            let mut app = TexcelApp::new_with_settings_path(creation, Some(settings));
            app.language = crate::i18n::Language::English;
            app.queue_open_paths(inputs);
            TexcelAppHost::new(app, creation.egui_ctx.viewport_id())
        });

    // Wait for the loader to give each file its own native viewport.
    let deadline = Instant::now() + Duration::from_secs(5);
    let [markdown, formatting, lossy] = loop {
        harness.step();
        let ids = with_host(harness.state().id, |host| {
            paths.each_ref().map(|path| {
                host.app.user_file_paths.iter().find_map(|(id, user_path)| {
                    (user_path == path && host.document_viewports.contains_key(id)).then_some(*id)
                })
            })
        });
        if let [Some(markdown), Some(formatting), Some(lossy)] = ids {
            break [markdown, formatting, lossy];
        }
        assert!(Instant::now() < deadline, "all three file windows open");
    };

    // Read the text painted in each window, including after switching back.
    for (document, expected) in [
        (formatting, true),
        (markdown, false),
        (lossy, false),
        (formatting, true),
    ] {
        let notice = with_host(harness.state().id, |host| {
            Message::new("status-import-formatting-only").resolve(host.app.language)
        });
        // The first child frame measures the footer. The next frame paints it.
        host_window_frame(&mut harness, document, []);
        let output = host_window_frame(&mut harness, document, []);
        let has_notice = output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text.contains(&notice))
        });
        let report_owner = with_host(harness.state().id, |host| {
            host.app.window_states[&host.document_viewports[&document]]
                .active_import_report
                .as_ref()
                .map(|(owner, _)| *owner)
        });
        assert_eq!(
            report_owner,
            (document == lossy).then_some(lossy),
            "report in window {document}"
        );
        assert_eq!(
            has_notice,
            expected,
            "notice in window {document}; window details: {:?}",
            with_host(harness.state().id, |host| host
                .app
                .window_states
                .values()
                .map(|window| (
                    window.document_id,
                    window
                        .import_notice
                        .as_ref()
                        .map(|(owner, notice)| (*owner, notice.resolve(host.app.language)))
                ))
                .collect::<Vec<_>>())
        );
    }
}

/// Sends a file path through the same drop event that the native window receives.
#[derive(Debug)]
struct DroppedCsv(PathBuf);

impl egui::DroppedFile for DroppedCsv {
    /// Returns the path the user dropped into the document window.
    fn path(&self) -> &Path {
        &self.0
    }

    /// Reads the same file if the desktop backend asks for its bytes.
    fn bytes(&self) -> Result<Vec<u8>, String> {
        fs::read(&self.0).map_err(|error| error.to_string())
    }
}

#[test]
/// Keeps a CSV encoding choice in the window that received the file drop.
fn csv_encoding_prompt_allows_editing_and_saving_in_another_window() {
    let directory = crate::ui::tests::TestDirectory::new();
    let (mut harness, paths) = host_with_three_documents(&directory.0);
    let [origin, other, _] = host_document_ids(&harness, &paths);
    let csv = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("samples/cp932-inventory.csv");

    // Drop an actual CP932 CSV into the first window, then wait for the encoding choice.
    harness
        .input_mut()
        .dropped_files
        .push(std::sync::Arc::new(DroppedCsv(csv)));
    host_window_frame(&mut harness, origin, []);
    harness.input_mut().dropped_files.clear();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        harness.step();
        if with_host(harness.state().id, |host| {
            host.app
                .pending_csv_opens
                .values()
                .any(|pending| pending.prompt.is_some())
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the real file needs encoding confirmation"
        );
    }

    let dialog_title = context_text(&harness.ctx, "dialog-csv-settings-title");
    loop {
        let output = host_window_frame(&mut harness, origin, []);
        if output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text.contains(&dialog_title))
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the CSV choice is painted in the window that received the drop"
        );
    }
    let other_output = host_window_frame(&mut harness, other, []);
    assert!(
        !other_output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text.contains(&dialog_title))
        }),
        "the CSV choice is not painted over another document window"
    );

    // The second window must still accept input and write its own file.
    with_host(harness.state().id, |host| {
        for (id, blocked) in [(origin, true), (other, false)] {
            let viewport = host.document_viewports[&id];
            host.app.with_window_state(viewport, Some(id), |app| {
                assert_eq!(
                    app.modal_input_blocked(),
                    blocked,
                    "input gate in window {id}"
                );
            });
        }
    });
    type_and_save_in_host_window(
        &mut harness,
        other,
        "edited with CSV prompt open",
        &paths[1],
    );
    assert!(with_host(harness.state().id, |host| {
        host.app
            .pending_csv_opens
            .values()
            .any(|pending| pending.prompt.is_some())
    }));
}

#[test]
/// Keeps a child window's IME preedit when the root window draws between its frames. Raw
/// input prepared right after the root's pass read the root's focus, so the child's preedit
/// was cancelled and redrawn each time.
fn root_pass_between_child_frames_keeps_the_preedit() {
    let directory = crate::ui::tests::TestDirectory::new();
    let (mut harness, paths) = host_with_three_documents(&directory.0);
    let root_document = with_host(harness.state().id, |host| host.root_document);
    let child = host_document_ids(&harness, &paths)
        .into_iter()
        .find(|id| Some(*id) != root_document)
        .expect("a document has a window of its own");
    click_host_cell(&mut harness, child);
    let mut preedit = String::new();
    for _ in 0..3 {
        preedit.push('あ');
        let shows_preedit = |output: &egui::FullOutput| {
            output.shapes.iter().any(|clipped| {
                matches!(&clipped.shape, egui::epaint::Shape::Text(text)
                    if text.galley.job.text.contains(&preedit))
            })
        };
        let output = host_window_frame(
            &mut harness,
            child,
            [egui::Event::Ime(egui::ImeEvent::Preedit {
                text: preedit.clone(),
                active_range_chars: None,
            })],
        );
        assert!(shows_preedit(&output));
        // The root window draws, then the child draws again without new input.
        harness.step();
        let output = host_window_frame(&mut harness, child, []);
        assert!(
            shows_preedit(&output),
            "the preedit {preedit:?} stays after the root drew"
        );
    }
}

/// Opens one file through the host's queue and waits until its document exists.
fn open_in_host(harness: &mut Harness<'_, TexcelAppHost>, path: &Path) -> DocumentId {
    with_host(harness.state().id, |host| {
        host.app.queue_open_paths([path.to_path_buf()]);
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        harness.step();
        let loaded = with_host(harness.state().id, |host| {
            host.app
                .file_backings
                .iter()
                .find_map(|(id, backing)| (backing.path == path).then_some(*id))
        });
        if let Some(id) = loaded {
            harness.step();
            return id;
        }
        assert!(Instant::now() < deadline, "{} opens", path.display());
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
/// Opens README.md and then the CP932 sample, answering its encoding prompt. README.md
/// replaces the start-up document in the root window, the prompt appears there, and the CSV
/// gets a window of its own that the root registers. The root used to hide once README.md
/// moved to a window of its own, and a hidden root never created the CSV's window.
fn readme_then_cp932_opens_the_csv_in_a_window_of_its_own() {
    let directory = crate::ui::tests::TestDirectory::new();
    let samples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("samples");
    let readme = fs::canonicalize(samples.join("README.md")).expect("the sample exists");
    let cp932 = fs::canonicalize(samples.join("cp932-inventory.csv")).expect("the sample exists");
    let settings_path = directory.0.join("settings.json");
    let mut harness = Harness::builder()
        .with_size([1000.0, 700.0])
        .allow_missing_glyphs()
        .build_eframe(move |creation| {
            creation.egui_ctx.set_embed_viewports(false);
            let app = TexcelApp::new_with_settings_path(creation, Some(settings_path));
            TexcelAppHost::new(app, creation.egui_ctx.viewport_id())
        });
    harness.step();
    let readme_id = open_in_host(&mut harness, &readme);
    let root = with_host(harness.state().id, |host| {
        assert_eq!(
            host.root_document,
            Some(readme_id),
            "README.md is in the root window"
        );
        host.root_viewport
    });

    // The encoding prompt appears in the visible root window.
    with_host(harness.state().id, |host| {
        host.app.queue_open_paths([cp932.clone()])
    });
    let title = crate::i18n::context_text(&harness.ctx, "dialog-csv-settings-title");
    let deadline = Instant::now() + Duration::from_secs(5);
    let csv_id = loop {
        harness.step();
        let prompt_shown = harness.output().shapes.iter().any(|clipped| {
            matches!(&clipped.shape, egui::epaint::Shape::Text(text)
                if text.galley.job.text == title)
        });
        let pending = with_host(harness.state().id, |host| {
            host.app.pending_csv_opens.keys().next().copied()
        });
        if prompt_shown && let Some(id) = pending {
            assert_eq!(
                with_host(harness.state().id, |host| host.app.load_request_viewports
                    [&id]),
                root
            );
            break id;
        }
        assert!(
            Instant::now() < deadline,
            "the CSV prompt appears in the root window"
        );
        std::thread::sleep(Duration::from_millis(5));
    };

    // Choosing Shift_JIS loads the CSV into a new window the root draws.
    with_host(harness.state().id, |host| {
        host.app.with_window_state(root, Some(readme_id), |app| {
            let pending = app
                .pending_csv_opens
                .get_mut(&csv_id)
                .expect("the load waits");
            pending.encoding = crate::csv_adapter::EncodingChoice::ShiftJis;
            pending.delimiter = Some(',');
            app.submit_csv_load(csv_id).expect("the load is submitted");
        });
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        harness.step();
        let (csv_window, readme_open) = with_host(harness.state().id, |host| {
            (
                host.child_viewports()
                    .into_iter()
                    .find(|(id, _, _)| *id == csv_id)
                    .map(|(_, viewport, _)| viewport),
                host.app.workspace.documents.contains_key(&readme_id),
            )
        });
        if let Some(viewport) = csv_window {
            assert_ne!(viewport, root);
            assert!(readme_open, "README.md stays open");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the CSV gets a window of its own"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
/// Moves another open document into the root window when the root's own document closes,
/// so the root keeps drawing the other windows instead of hiding.
fn root_takes_over_another_document_when_its_own_closes() {
    let directory = crate::ui::tests::TestDirectory::new();
    let (mut harness, paths) = host_with_three_documents(&directory.0);
    let ids = host_document_ids(&harness, &paths);
    let (root, first) = with_host(harness.state().id, |host| {
        (
            host.root_viewport,
            host.root_document.expect("the root shows a document"),
        )
    });
    with_host(harness.state().id, |host| {
        host.app
            .with_window_state(root, Some(first), |app| app.request_close_document(first));
    });
    harness.run_steps(3);
    with_host(harness.state().id, |host| {
        let adopted = host.root_document.expect("the root took over a document");
        assert_ne!(adopted, first);
        assert!(ids.contains(&adopted));
        assert_eq!(host.document_viewports[&adopted], root);
        assert_eq!(
            host.child_viewports().len(),
            1,
            "one other document keeps its window"
        );
    });
}
