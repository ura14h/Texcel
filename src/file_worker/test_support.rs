use super::*;

/// Builds a stopped worker so UI cleanup follows the real terminal-result path.
pub(crate) fn stopped_worker() -> FileWorker {
    let (sender, _receiver) = mpsc::sync_channel(1);
    let (_result_sender, results) = mpsc::sync_channel(1);
    FileWorker {
        sender,
        results,
        load_progress: None,
        cancellation: Arc::new(Mutex::new(HashMap::new())),
        shutdown: Arc::new(AtomicBool::new(true)),
        stopped: AtomicBool::new(true),
        thread: None,
        waker: WorkerWaker::default(),
    }
}
