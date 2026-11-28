use super::*;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[test]
// Let final-result-only clients load a file without reading preview messages.
fn default_worker_load_does_not_wait_for_progress_polling() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock is after the epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("texcel-no-progress-{unique}.csv"));
    fs::write(
        &path,
        (0..300)
            .map(|row| format!("{row},value\n"))
            .collect::<String>(),
    )
    .expect("write CSV rows");
    let worker = FileWorker::new();
    let token = JobToken {
        document_id: 31,
        revision: 0,
        job_id: 1,
    };
    worker
        .load_csv(token, path.clone(), EncodingChoice::Auto, Some(','))
        .expect("enqueue CSV load");

    let deadline = Instant::now() + Duration::from_secs(2);
    let result = loop {
        if let Some(result) = worker.poll().into_iter().next() {
            break result;
        }
        assert!(Instant::now() < deadline, "load waited for preview polling");
        thread::sleep(Duration::from_millis(2));
    };

    assert!(matches!(result.kind, FileResultKind::LoadedCsv { .. }));
    fs::remove_file(path).expect("remove CSV fixture");
}

#[test]
// Keep ordinary chunks near their targets while preserving a single oversized row.
fn preview_chunks_keep_row_boundaries_and_large_cells() {
    let (sender, receiver) = mpsc::sync_channel(8);
    let cancelled = AtomicBool::new(false);
    let shutdown = AtomicBool::new(false);
    let token = JobToken {
        document_id: 3,
        revision: 0,
        job_id: 7,
    };
    let queue = ProgressQueue::new(sender.clone(), WorkerWaker::default());
    let mut progress = LoadProgressSender::new(
        token,
        FileOperation::LoadCsv,
        Path::new("input.csv"),
        Some(&queue),
        &cancelled,
        &shutdown,
    );
    for row in 0..260 {
        assert!(progress.push_row(
            0,
            "CSV",
            row,
            vec![
                LoadPreviewCell {
                    column: 0,
                    text: row.to_string(),
                },
                LoadPreviewCell {
                    column: 1,
                    text: "value".to_owned(),
                },
            ],
        ));
    }
    assert!(progress.push_row(
        0,
        "CSV",
        260,
        vec![LoadPreviewCell {
            column: 0,
            text: "x".repeat(70 * 1024),
        }],
    ));
    assert!(progress.finish());

    let chunks = receiver
        .try_iter()
        .filter_map(|item| match item.kind {
            LoadProgressKind::Chunk(chunk) => Some(chunk),
            LoadProgressKind::Stage(_) => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(chunks.len(), 4);
    assert_eq!(
        chunks
            .iter()
            .map(|chunk| chunk.rows.len())
            .collect::<Vec<_>>(),
        [128, 128, 4, 1]
    );
    let oversized = chunks.last().unwrap();
    assert_eq!(oversized.rows[0].cells[0].text.len(), 70 * 1024);
}

#[test]
// Stop a worker retry when cancellation arrives while the progress queue is full.
fn progress_retry_stops_after_cancellation() {
    let (sender, _receiver) = mpsc::sync_channel(1);
    let cancelled = Arc::new(AtomicBool::new(false));
    let shutdown = Arc::new(AtomicBool::new(false));
    let token = JobToken {
        document_id: 8,
        revision: 1,
        job_id: 9,
    };
    sender
        .try_send(LoadProgress {
            token,
            operation: FileOperation::LoadCsv,
            path: PathBuf::from("input.csv"),
            kind: LoadProgressKind::Stage(LoadStage::ReadingSource),
        })
        .expect("the test fills the progress queue");
    let worker_sender = ProgressQueue::new(sender.clone(), WorkerWaker::default());
    let worker_cancelled = Arc::clone(&cancelled);
    let worker_shutdown = Arc::clone(&shutdown);
    let worker = thread::spawn(move || {
        let mut progress = LoadProgressSender::new(
            token,
            FileOperation::LoadCsv,
            Path::new("input.csv"),
            Some(&worker_sender),
            &worker_cancelled,
            &worker_shutdown,
        );
        for row in 0..128 {
            if !progress.push_row(
                0,
                "CSV",
                row,
                vec![LoadPreviewCell {
                    column: 0,
                    text: "x".to_owned(),
                }],
            ) {
                return false;
            }
        }
        true
    });
    thread::sleep(Duration::from_millis(10));
    cancelled.store(true, Ordering::Release);
    assert!(!worker.join().expect("progress worker exits promptly"));
}
