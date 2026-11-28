use super::*;

impl FileWorker {
    // Creates a worker without preview traffic for callers that only poll final results.
    pub fn new() -> Self {
        Self::new_inner(false)
    }

    /// Creates a worker that also publishes bounded read-only load previews.
    pub fn new_with_load_progress() -> Self {
        Self::new_inner(true)
    }

    // Starts the worker with optional preview delivery.
    fn new_inner(with_load_progress: bool) -> Self {
        let (sender, receiver) = mpsc::sync_channel(JOB_QUEUE_CAPACITY);
        let (result_sender, results) = mpsc::sync_channel(RESULT_QUEUE_CAPACITY);
        let waker = WorkerWaker::default();
        let (progress_sender, load_progress) = if with_load_progress {
            let (sender, receiver) = mpsc::sync_channel(1);
            (
                Some(ProgressQueue::new(sender, waker.clone())),
                Some(receiver),
            )
        } else {
            (None, None)
        };
        let worker_waker = waker.clone();
        let cancellation = Arc::new(Mutex::new(HashMap::new()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_shutdown = Arc::clone(&shutdown);
        let thread = thread::Builder::new()
            .name("texcel-file-worker".to_owned())
            .spawn(move || {
                worker_loop(
                    receiver,
                    result_sender,
                    progress_sender,
                    worker_shutdown,
                    worker_waker,
                );
            })
            .ok();
        let stopped = thread.is_none();
        Self {
            sender,
            results,
            load_progress,
            cancellation,
            shutdown,
            stopped: AtomicBool::new(stopped),
            thread,
            waker,
        }
    }

    /// Sets how the worker wakes the UI after handing over a result or a load preview.
    pub fn set_waker(&self, wake: impl Fn() + Send + Sync + 'static) {
        let _ = self.waker.0.set(Box::new(wake));
    }

    // Cancels a running or queued job. A save is not canceled after its final replacement.
    pub fn cancel(&self, token: JobToken) -> bool {
        let Ok(jobs) = self.cancellation.lock() else {
            return false;
        };
        let Some(cancelled) = jobs.get(&token) else {
            return false;
        };
        cancelled.store(true, Ordering::Release);
        true
    }

    // Collects all received results without stopping the UI frame.
    pub fn poll(&self) -> Vec<FileResult> {
        let mut results = Vec::new();
        loop {
            match self.results.try_recv() {
                Ok(mut result) => {
                    let cancelled = self
                        .cancellation
                        .lock()
                        .ok()
                        .and_then(|mut jobs| jobs.remove(&result.token))
                        .is_some_and(|flag| flag.load(Ordering::Acquire));
                    if cancelled
                        && matches!(
                            result.operation,
                            FileOperation::LoadCsv
                                | FileOperation::LoadMarkdown
                                | FileOperation::LoadHtml
                                | FileOperation::LoadWorkbook
                        )
                    {
                        result.kind = FileResultKind::Cancelled;
                    }
                    results.push(result);
                }
                Err(TryRecvError::Empty) => return results,
                Err(TryRecvError::Disconnected) => {
                    self.stopped.store(true, Ordering::Release);
                    return results;
                }
            }
        }
    }

    /// Collects a bounded number of progress messages without completing their jobs.
    pub fn poll_load_progress(&self) -> Vec<LoadProgress> {
        let Some(receiver) = &self.load_progress else {
            return Vec::new();
        };
        let mut progress = Vec::new();
        for _ in 0..4 {
            match receiver.try_recv() {
                Ok(message) => progress.push(message),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        progress
    }

    /// Reports that the worker ended and can no longer answer queued requests.
    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }

    // Queue a file job and return its token for result tracking.
    pub(super) fn submit(
        &self,
        token: JobToken,
        mut message: WorkerMessage,
    ) -> Result<(), SubmitError> {
        let cancelled = match &mut message {
            WorkerMessage::LoadCsv { cancelled, .. }
            | WorkerMessage::LoadMarkdown { cancelled, .. }
            | WorkerMessage::ImportHtml { cancelled, .. }
            | WorkerMessage::ImportWorkbook { cancelled, .. }
            | WorkerMessage::SaveConvertedCsv { cancelled, .. }
            | WorkerMessage::SaveReport { cancelled, .. }
            | WorkerMessage::PrepareExport { cancelled, .. }
            | WorkerMessage::Search { cancelled, .. }
            | WorkerMessage::DataEdit { cancelled, .. }
            | WorkerMessage::PrepareStructure { cancelled, .. }
            | WorkerMessage::AutoFit { cancelled, .. }
            | WorkerMessage::PrepareMarkdownCellDisplay { cancelled, .. }
            | WorkerMessage::PrepareTableStructure { cancelled, .. }
            | WorkerMessage::RestoreStructureHistory { cancelled, .. }
            | WorkerMessage::Export { cancelled, .. }
            | WorkerMessage::ExportReport { cancelled, .. }
            | WorkerMessage::SaveCsv { cancelled, .. }
            | WorkerMessage::SaveMarkdown { cancelled, .. } => Arc::clone(cancelled),
            WorkerMessage::DiscardStructureTables { .. } => unreachable!(),
        };
        let Ok(mut jobs) = self.cancellation.lock() else {
            return Err(SubmitError::Stopped);
        };
        jobs.insert(token, cancelled);
        drop(jobs);
        match self.sender.try_send(message) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => {
                if let Ok(mut jobs) = self.cancellation.lock() {
                    jobs.remove(&token);
                }
                Err(SubmitError::QueueFull)
            }
            Err(TrySendError::Disconnected(_)) => {
                if let Ok(mut jobs) = self.cancellation.lock() {
                    jobs.remove(&token);
                }
                Err(SubmitError::Stopped)
            }
        }
    }
}

impl Default for FileWorker {
    // Create a file worker with its standard worker threads.
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for FileWorker {
    // Cancel pending file jobs and stop the worker threads.
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Ok(jobs) = self.cancellation.lock() {
            for cancelled in jobs.values() {
                cancelled.store(true, Ordering::Release);
            }
        }
        // The worker waits without blocking the UI thread. Dropping the sender closes its wait loop.
        if let Some(thread) = self.thread.take() {
            drop(thread);
        }
    }
}
