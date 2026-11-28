use super::*;

/// Tells whether a frame drew a piece of text that contains `needle`.
fn frame_draws_text(output: &egui::FullOutput, needle: &str) -> bool {
    output.shapes.iter().any(|clipped| {
        matches!(&clipped.shape, egui::epaint::Shape::Text(shape)
            if shape.galley.job.text.contains(needle))
    })
}

#[test]
/// Shows the minimap in two document windows at once. The windows draw in turn, and a
/// single shared parse cache made each one restart the other's parse, so neither minimap
/// ever left its busy state.
fn two_windows_each_show_their_own_minimap() {
    let directory = crate::ui::tests::TestDirectory::new();
    let paths = [("alpha.md", "Alpha notes"), ("bravo.md", "Bravo notes")].map(|(name, note)| {
        let path = directory.0.join(name);
        fs::write(
            &path,
            format!("{note}\n\n| Name | Value |\n| --- | --- |\n| A | 1 |\n"),
        )
        .expect("write a Markdown fixture");
        fs::canonicalize(path).expect("canonicalize fixture")
    });
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
    let alpha = open_in_host(&mut harness, &paths[0]);
    let bravo = open_in_host(&mut harness, &paths[1]);
    with_host(harness.state().id, |host| {
        assert_eq!(
            host.root_document,
            Some(alpha),
            "alpha keeps the root window"
        );
        for document_id in [alpha, bravo] {
            let viewport = host.document_viewports[&document_id];
            host.app
                .with_window_state(viewport, Some(document_id), |app| app.minimap_open = true);
        }
    });

    // Draw the root (alpha) and bravo's window in turn until both minimaps show their text.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        harness.step();
        let alpha_ready = frame_draws_text(harness.output(), "Alpha notes");
        let bravo_output = host_window_frame(&mut harness, bravo, []);
        if alpha_ready && frame_draws_text(&bravo_output, "Bravo notes") {
            break;
        }
        assert!(Instant::now() < deadline, "both minimaps finish parsing");
        std::thread::sleep(Duration::from_millis(5));
    }
}
