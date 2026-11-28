use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) use self::persistence::{
    cleanup_saved, load_items, remove_recovery, restore_item, save_snapshot,
};

#[path = "storage/persistence.rs"]
mod persistence;

/// A single worker owns recovery files and serializes each source base only when it changes.
pub(crate) struct RecoveryWorker {
    requests: SyncSender<RecoveryRequest>,
    results: Receiver<RecoveryEvent>,
    stopped: AtomicBool,
    thread: Option<JoinHandle<()>>,
}

enum RecoveryRequest {
    Save(RecoverySnapshot),
    SaveAndCleanup {
        snapshot: RecoverySnapshot,
        saved_revision: u64,
        saved_generation: u64,
    },
    Cleanup {
        key: String,
        saved_revision: u64,
        saved_generation: u64,
    },
    Remove {
        key: String,
    },
    Restore {
        item: RecoveryItem,
        document_id: DocumentId,
        language: crate::i18n::Language,
    },
    Stop,
}

#[derive(Serialize, Deserialize)]
enum RecoveryBaseFile {
    Csv {
        #[serde(with = "base64_bytes")]
        bytes: Vec<u8>,
        encoding: CsvEncoding,
        delimiter: char,
    },
    Markdown {
        #[serde(with = "base64_bytes")]
        bytes: Vec<u8>,
    },
}

mod base64_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    /// Writes source bytes as compact base64 instead of a JSON number per byte.
    pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            bytes,
        ))
    }

    /// Reads bytes stored as compact base64 text.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, value)
            .map_err(serde::de::Error::custom)
    }
}

impl RecoveryWorker {
    /// Starts the recovery file worker and queues existing snapshots for startup review.
    pub fn new(root: PathBuf) -> Self {
        let (request_tx, request_rx) = mpsc::sync_channel(REQUEST_CAPACITY);
        let (result_tx, result_rx) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("texcel-recovery".to_owned())
            .spawn(move || recovery_loop(root, request_rx, result_tx))
            .ok();
        let stopped = thread.is_none();
        Self {
            requests: request_tx,
            results: result_rx,
            stopped: AtomicBool::new(stopped),
            thread,
        }
    }

    /// Queues one current recovery snapshot without waiting for file work.
    /// A full queue hands the snapshot back, boxed so the common `Ok` case stays small.
    pub fn save(&self, snapshot: RecoverySnapshot) -> Result<(), Box<RecoverySnapshot>> {
        match self.requests.try_send(RecoveryRequest::Save(snapshot)) {
            Ok(()) => Ok(()),
            Err(
                TrySendError::Full(RecoveryRequest::Save(snapshot))
                | TrySendError::Disconnected(RecoveryRequest::Save(snapshot)),
            ) => Err(Box::new(snapshot)),
            Err(_) => unreachable!("recovery save request was submitted"),
        }
    }

    /// Stores newer uncommitted input before removing the saved version.
    pub fn save_and_cleanup(
        &self,
        snapshot: RecoverySnapshot,
        saved_revision: u64,
        saved_generation: u64,
    ) -> Result<(), Box<RecoverySnapshot>> {
        match self.requests.try_send(RecoveryRequest::SaveAndCleanup {
            snapshot,
            saved_revision,
            saved_generation,
        }) {
            Ok(()) => Ok(()),
            Err(
                TrySendError::Full(RecoveryRequest::SaveAndCleanup { snapshot, .. })
                | TrySendError::Disconnected(RecoveryRequest::SaveAndCleanup { snapshot, .. }),
            ) => Err(Box::new(snapshot)),
            Err(_) => unreachable!("recovery flush request was submitted"),
        }
    }

    /// Removes only a recovery version already covered by a successful document save.
    pub fn cleanup(&self, key: String, saved_revision: u64, saved_generation: u64) -> bool {
        self.requests
            .try_send(RecoveryRequest::Cleanup {
                key,
                saved_revision,
                saved_generation,
            })
            .is_ok()
    }

    /// Removes a recovery record after the user explicitly discards it.
    pub fn remove(&self, key: String) -> bool {
        self.requests
            .try_send(RecoveryRequest::Remove { key })
            .is_ok()
    }

    /// Rebuilds one recovery item on the worker so parsing never runs in a frame.
    pub fn restore(
        &self,
        item: RecoveryItem,
        document_id: DocumentId,
        language: crate::i18n::Language,
    ) -> Result<(), Box<RecoveryItem>> {
        match self.requests.try_send(RecoveryRequest::Restore {
            item,
            document_id,
            language,
        }) {
            Ok(()) => Ok(()),
            Err(
                TrySendError::Full(RecoveryRequest::Restore { item, .. })
                | TrySendError::Disconnected(RecoveryRequest::Restore { item, .. }),
            ) => Err(Box::new(item)),
            Err(_) => unreachable!("recovery restore request was submitted"),
        }
    }

    /// Returns one worker result without blocking the frame.
    pub fn try_recv(&self) -> Result<RecoveryEvent, TryRecvError> {
        match self.results.try_recv() {
            Err(TryRecvError::Disconnected) => {
                self.stopped.store(true, Ordering::Release);
                Err(TryRecvError::Disconnected)
            }
            result => result,
        }
    }

    /// Reports when the worker has ended and can no longer accept maintenance requests.
    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }
}

impl Drop for RecoveryWorker {
    /// Stops the worker after its current bounded operation finishes.
    fn drop(&mut self) {
        let _ = self.requests.send(RecoveryRequest::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl RecoverySnapshot {
    /// Captures sparse changes while sharing the immutable source root with the worker.
    pub fn from_document(
        document: &Document,
        key: String,
        original_path: Option<PathBuf>,
        original_identity: Option<FileIdentity>,
        cell_edit: Option<RecoveryCellEdit>,
        generation: u64,
    ) -> Option<Self> {
        let text_overlay = document.text_overlay_snapshot();
        let base = match document.source.as_deref() {
            Some(SourceDocument::Csv(source)) => RecoveryBase::Csv(Arc::clone(source)),
            Some(SourceDocument::Markdown(source)) => RecoveryBase::Markdown(Arc::clone(source)),
            // Every editable document has a text source; a source-less one has nothing to restore.
            None => return None,
        };
        let tables = document
            .tables
            .iter()
            .map(|table| {
                let view = document.view(table.id);
                RecoveryTableInfo {
                    id: table.id,
                    name: table.name.clone(),
                    rows: table.row_count(),
                    columns: table.column_count(),
                    string_cells: table.string_cells_shared().iter().copied().collect(),
                    header_row: view.and_then(|view| view.header_row),
                    selection: view
                        .map_or(crate::document::CellRange::default(), |view| view.selection),
                    scroll: view.map_or(crate::document::ScrollPosition::default(), |view| {
                        view.scroll
                    }),
                }
            })
            .collect();
        Some(Self {
            key,
            document_id: document.id,
            name: document.name.clone(),
            original_path,
            original_identity,
            requires_initial_save: false,
            revision: document.revision,
            saved_revision: document.saved_revision,
            generation,
            structure_revision: document.structure_revision,
            base,
            cells: document.changed_cell_snapshot(),
            tables,
            text_overlay_revision: text_overlay.revision,
            text_overlay_cells: text_overlay.cells,
            cell_edit,
        })
    }
}

impl RecoveryBase {
    /// Builds the serializable immutable base on the recovery worker.
    fn into_file(self) -> RecoveryBaseFile {
        match self {
            Self::Csv(source) => RecoveryBaseFile::Csv {
                bytes: source.source_bytes().to_vec(),
                encoding: source.encoding,
                delimiter: source.delimiter,
            },
            Self::Markdown(source) => RecoveryBaseFile::Markdown {
                bytes: source.source_bytes().to_vec(),
            },
        }
    }
}

// Loads saved recovery data and handles requests until the worker closes.
fn recovery_loop(
    root: PathBuf,
    requests: Receiver<RecoveryRequest>,
    results: Sender<RecoveryEvent>,
) {
    let loaded = load_items(&root);
    let _ = results.send(RecoveryEvent::Loaded(loaded));
    let mut save_cutoffs = BTreeMap::<String, (u64, u64)>::new();
    while let Ok(request) = requests.recv() {
        let event = match request {
            RecoveryRequest::Save(snapshot) => {
                let key = snapshot.key.clone();
                let revision = snapshot.revision;
                let generation = snapshot.generation;
                let result = if save_cutoffs
                    .get(&key)
                    .is_some_and(|cutoff| revision <= cutoff.0 && generation <= cutoff.1)
                {
                    Ok(())
                } else {
                    save_snapshot(&root, snapshot)
                };
                Some(RecoveryEvent::Saved {
                    key,
                    revision,
                    generation,
                    result,
                })
            }
            RecoveryRequest::SaveAndCleanup {
                snapshot,
                saved_revision,
                saved_generation,
            } => {
                let key = snapshot.key.clone();
                let revision = snapshot.revision;
                let generation = snapshot.generation;
                update_save_cutoff(
                    &mut save_cutoffs,
                    key.clone(),
                    saved_revision,
                    saved_generation,
                );
                let superseded = save_cutoffs
                    .get(&key)
                    .is_some_and(|cutoff| revision <= cutoff.0 && generation <= cutoff.1);
                let result = if superseded {
                    cleanup_saved(
                        &root,
                        &key,
                        saved_revision.max(save_cutoffs[&key].0),
                        saved_generation.max(save_cutoffs[&key].1),
                    )
                } else {
                    save_snapshot(&root, snapshot)
                        .and_then(|()| cleanup_saved(&root, &key, saved_revision, saved_generation))
                };
                Some(RecoveryEvent::Saved {
                    key,
                    revision,
                    generation,
                    result,
                })
            }
            RecoveryRequest::Cleanup {
                key,
                saved_revision,
                saved_generation,
            } => {
                update_save_cutoff(
                    &mut save_cutoffs,
                    key.clone(),
                    saved_revision,
                    saved_generation,
                );
                Some(RecoveryEvent::Removed {
                    key: key.clone(),
                    result: cleanup_saved(&root, &key, saved_revision, saved_generation),
                })
            }
            RecoveryRequest::Remove { key } => Some(RecoveryEvent::Removed {
                key: key.clone(),
                result: remove_recovery(&root, &key),
            }),
            RecoveryRequest::Restore {
                item,
                document_id,
                language,
            } => {
                let key = item.record.key.clone();
                let result = restore_item(item, document_id, language).map(Box::new);
                Some(RecoveryEvent::Restored { key, result })
            }
            RecoveryRequest::Stop => break,
        };
        if let Some(event) = event
            && results.send(event).is_err()
        {
            break;
        }
    }
}

/// Keeps the latest successful save version for this document key.
fn update_save_cutoff(
    cutoffs: &mut BTreeMap<String, (u64, u64)>,
    key: String,
    revision: u64,
    generation: u64,
) {
    cutoffs
        .entry(key)
        .and_modify(|cutoff| {
            cutoff.0 = cutoff.0.max(revision);
            cutoff.1 = cutoff.1.max(generation);
        })
        .or_insert((revision, generation));
}
