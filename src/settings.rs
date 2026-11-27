//!  App-local appearance and view preferences, stored independently from document edits.

use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::thread::{self, JoinHandle};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[path = "settings/document_preferences.rs"]
mod document_preferences;
pub use self::document_preferences::{DocumentKey, DocumentPreferences, TablePreferences};

#[path = "settings/path_serde.rs"]
mod path_serde;
use self::path_serde::recent_paths_serde;

const SETTINGS_VERSION: u32 = 1;
const MAX_SETTINGS_BYTES: u64 = 16 * 1024 * 1024;
const MAX_DOCUMENT_ENTRIES: usize = 128;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FontPreset {
    #[default]
    Auto,
    Default,
    NotoSansJp,
}

/// Names the saved system, English, or Japanese language preference.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LanguageChoice {
    #[default]
    System,
    English,
    Japanese,
}

/// Chooses whether a table wraps long cell text; this is a view setting only.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WrapMode {
    On,
    #[default]
    Off,
}

/// Width of the Markdown minimap panel beside the sheet.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MinimapWidth {
    Wide,
    #[default]
    Medium,
    Narrow,
}

impl MinimapWidth {
    /// Returns the panel width in logical points.
    pub fn points(self) -> f32 {
        match self {
            Self::Wide => 360.0,
            Self::Medium => 260.0,
            Self::Narrow => 180.0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct AppPreferences {
    #[serde(default)]
    pub language: LanguageChoice,
    pub theme: ThemeChoice,
    pub font_preset: FontPreset,
    /// Text size in typographic points for everything but the sheet: menus, bars, tabs,
    /// status, dialogs. `ui_font_points_to_logical` turns it into egui's logical points.
    pub ui_font_points: f32,
    pub show_invisibles: bool,
    pub minimap_width: MinimapWidth,
}

/// The smallest UI text size the settings accept, in points.
pub const MIN_UI_FONT_POINTS: f32 = 6.0;
/// The largest UI text size the settings accept, in points.
pub const MAX_UI_FONT_POINTS: f32 = 24.0;
/// The text sizes offered in Settings, in points.
pub const UI_FONT_POINT_CHOICES: [f32; 12] = [
    6.0, 8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0,
];

/// Follows the platform's standard UI text size: 13 pt on macOS, 9 pt on Windows, 11 pt
/// elsewhere.
pub fn default_ui_font_points() -> f32 {
    if cfg!(target_os = "macos") {
        13.0
    } else if cfg!(target_os = "windows") {
        9.0
    } else {
        11.0
    }
}

/// Lists the offered sizes, adding the platform default (13 pt on macOS) when it is missing.
pub fn ui_font_point_choices() -> Vec<f32> {
    let mut choices = UI_FONT_POINT_CHOICES.to_vec();
    let default = default_ui_font_points();
    if !choices.contains(&default) {
        choices.push(default);
        choices.sort_by(f32::total_cmp);
    }
    choices
}

/// Converts points to egui's logical points. macOS already counts one logical point per
/// typographic point; Windows and Linux lay out at 96 logical points per inch. The OS display
/// scale is applied by egui on top, so the size follows DPI and scaling changes by itself.
pub fn ui_font_points_to_logical(points: f32) -> f32 {
    if cfg!(target_os = "macos") {
        points
    } else {
        points * 96.0 / 72.0
    }
}

impl Default for AppPreferences {
    /// Builds the default settings value.
    fn default() -> Self {
        Self {
            language: LanguageChoice::System,
            theme: ThemeChoice::System,
            font_preset: FontPreset::Auto,
            ui_font_points: default_ui_font_points(),
            show_invisibles: false,
            minimap_width: MinimapWidth::default(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SettingsFile {
    pub schema_version: u32,
    pub app: AppPreferences,
    #[serde(default)]
    pub documents: Vec<Arc<DocumentPreferences>>,
    /// Paths opened or fully saved most recently, newest first.
    #[serde(default, with = "recent_paths_serde")]
    pub recent_files: Vec<PathBuf>,
}

impl Default for SettingsFile {
    /// Builds the default settings value.
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_VERSION,
            app: AppPreferences::default(),
            documents: Vec::new(),
            recent_files: Vec::new(),
        }
    }
}

impl SettingsFile {
    /// Returns the shared preferences record for a document key.
    pub fn document_shared(&self, key: &DocumentKey) -> Option<&Arc<DocumentPreferences>> {
        self.documents.iter().find(|entry| &entry.key == key)
    }

    /// Adds or replaces preferences for one document.
    pub fn upsert_document(&mut self, preferences: DocumentPreferences) {
        // Retain one content version per path. A new saved identity replaces the old entry
        // instead of growing an unbounded per-path history.
        self.documents
            .retain(|existing| existing.key.path != preferences.key.path);
        self.documents.push(Arc::new(preferences));
        if self.documents.len() > MAX_DOCUMENT_ENTRIES {
            self.documents.remove(0);
        }
    }
}

#[derive(Debug)]
pub enum SettingsEvent {
    Loaded(Result<SettingsFile, crate::i18n::Message>),
    Saved {
        generation: u64,
        result: Result<SettingsSaveOutcome, crate::i18n::Message>,
    },
}

#[derive(Debug)]
pub struct SettingsSaveOutcome {
    pub persisted: SettingsFile,
    pub dropped_documents: usize,
}

#[derive(Debug)]
pub enum SettingsSubmitError {
    QueueFull(SettingsFile),
    Disconnected(SettingsFile),
}

enum SettingsRequest {
    Save { generation: u64, file: SettingsFile },
}

/// Background loader/writer for appdata JSON. The UI only transfers small preference snapshots.
pub struct SettingsWorker {
    requests: SyncSender<SettingsRequest>,
    results: Receiver<SettingsEvent>,
    _thread: JoinHandle<()>,
}

impl SettingsWorker {
    /// Starts the settings worker for a settings file.
    pub fn new(path: PathBuf) -> io::Result<Self> {
        let (request_tx, request_rx) = mpsc::sync_channel(1);
        let (result_tx, result_rx) = mpsc::channel();
        let worker_path = path.clone();
        let worker = thread::Builder::new()
            .name("texcel-settings".to_owned())
            .spawn(move || {
                let loaded = load_settings(&worker_path);
                let _ = result_tx.send(SettingsEvent::Loaded(loaded));
                while let Ok(request) = request_rx.recv() {
                    match request {
                        SettingsRequest::Save { generation, file } => {
                            let result = save_settings(&worker_path, file);
                            let _ = result_tx.send(SettingsEvent::Saved { generation, result });
                        }
                    }
                }
            })?;
        Ok(Self {
            requests: request_tx,
            results: result_rx,
            _thread: worker,
        })
    }

    /// Returns ownership of the newest snapshot when the single-slot queue is busy.
    pub fn try_save(&self, generation: u64, file: SettingsFile) -> Result<(), SettingsSubmitError> {
        match self
            .requests
            .try_send(SettingsRequest::Save { generation, file })
        {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(SettingsRequest::Save { file, .. })) => {
                Err(SettingsSubmitError::QueueFull(file))
            }
            Err(TrySendError::Disconnected(SettingsRequest::Save { file, .. })) => {
                Err(SettingsSubmitError::Disconnected(file))
            }
        }
    }

    /// Returns a settings event already received by the UI.
    pub fn try_recv(&self) -> Result<SettingsEvent, TryRecvError> {
        self.results.try_recv()
    }
}

/// Resolves the conventional per-user app-data location without touching the filesystem.
pub fn app_settings_path() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    let root = std::env::var_os("HOME")
        .map(PathBuf::from)?
        .join("Library")
        .join("Application Support");

    #[cfg(target_os = "windows")]
    let root = std::env::var_os("APPDATA").map(PathBuf::from)?;

    #[cfg(all(unix, not(target_os = "macos")))]
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;

    Some(root.join("Texcel").join("settings.json"))
}

/// Normalizes lexical path components without following symlinks or doing filesystem I/O.
pub fn normalize_settings_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

/// Removes harmless current-directory components but keeps symlink-sensitive parents.
pub fn normalize_recent_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        if !matches!(component, Component::CurDir) {
            normalized.push(component.as_os_str());
        }
    }
    normalized
}

/// Reads settings from disk and applies defaults for missing fields.
fn load_settings(path: &Path) -> Result<SettingsFile, crate::i18n::Message> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(SettingsFile::default()),
        Err(error) => return Err(settings_load_error(error)),
    };
    let length = file.metadata().map_err(settings_load_error)?.len();
    if length > MAX_SETTINGS_BYTES {
        return Err(crate::i18n::Message::new("status-settings-size-limit"));
    }
    let mut bytes = Vec::with_capacity(length as usize);
    (&mut file)
        .take(MAX_SETTINGS_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(settings_load_error)?;
    if bytes.len() as u64 > MAX_SETTINGS_BYTES {
        return Err(crate::i18n::Message::new("status-settings-size-limit"));
    }
    let settings: SettingsFile = serde_json::from_slice(&bytes).map_err(|error| {
        crate::i18n::Message::new("status-settings-file-invalid").with_text("detail", error)
    })?;
    validate_settings(&settings)?;
    Ok(settings)
}

/// Rejects invalid setting values before they are saved.
fn validate_settings(settings: &SettingsFile) -> Result<(), crate::i18n::Message> {
    if settings.schema_version != SETTINGS_VERSION {
        return Err(
            crate::i18n::Message::new("status-settings-version-unsupported")
                .with_number("version", settings.schema_version as i64),
        );
    }
    if !(MIN_UI_FONT_POINTS..=MAX_UI_FONT_POINTS).contains(&settings.app.ui_font_points) {
        return Err(crate::i18n::Message::new(
            "status-settings-ui-font-size-out-of-range",
        ));
    }
    let mut keys = BTreeSet::new();
    for document in &settings.documents {
        if !keys.insert(&document.key) {
            return Err(crate::i18n::Message::new(
                "status-settings-document-duplicate",
            ));
        }
        let mut table_indices = BTreeSet::new();
        for table in &document.tables {
            if !table_indices.insert(table.table_index) {
                return Err(crate::i18n::Message::new("status-settings-table-duplicate"));
            }
        }
    }
    Ok(())
}

/// Writes a new settings snapshot without losing newer edits.
fn save_settings(
    path: &Path,
    settings: SettingsFile,
) -> Result<SettingsSaveOutcome, crate::i18n::Message> {
    validate_settings(&settings)?;
    let parent = path
        .parent()
        .ok_or_else(|| crate::i18n::Message::new("status-settings-directory-missing"))?;
    let (persisted, dropped_documents, bytes) = bounded_settings_serialization(settings)?;
    if bytes.len() as u64 > MAX_SETTINGS_BYTES {
        return Err(crate::i18n::Message::new("status-settings-size-limit"));
    }
    fs::create_dir_all(parent).map_err(settings_save_error)?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp_path = parent.join(format!(
        ".texcel-settings-{}-{nonce}.tmp",
        std::process::id()
    ));
    let mut temp = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp_path)
        .map_err(settings_save_error)?;
    let result = (|| {
        temp.write_all(&bytes).map_err(settings_save_error)?;
        temp.sync_all().map_err(settings_save_error)?;
        drop(temp);
        crate::file_worker::replace_file_atomic(&temp_path, path).map_err(settings_save_error)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result?;
    Ok(SettingsSaveOutcome {
        persisted,
        dropped_documents,
    })
}

/// Serializes settings within the configured size limit.
fn bounded_settings_serialization(
    settings: SettingsFile,
) -> Result<(SettingsFile, usize, Vec<u8>), crate::i18n::Message> {
    let base = settings.clone();
    let original_documents = settings.documents;
    let mut low = 0;
    let mut high = original_documents.len();
    let mut fitting = None;
    while low <= high {
        let dropped = low + (high - low) / 2;
        let candidate = SettingsFile {
            documents: original_documents[dropped..].to_vec(),
            ..base.clone()
        };
        let bytes = serde_json::to_vec_pretty(&candidate).map_err(|error| {
            crate::i18n::Message::new("status-settings-save-failed").with_text("detail", error)
        })?;
        if bytes.len() as u64 <= MAX_SETTINGS_BYTES {
            fitting = Some((candidate, dropped, bytes));
            if dropped == 0 {
                break;
            }
            high = dropped - 1;
        } else {
            low = dropped.saturating_add(1);
        }
    }
    fitting.ok_or_else(|| crate::i18n::Message::new("status-settings-size-failure"))
}

// Keeps operating system save details under a translated main message.
fn settings_save_error(error: io::Error) -> crate::i18n::Message {
    crate::i18n::Message::new("status-settings-save-failed")
        .with_message("detail", crate::file_worker::file_error_message(error))
}

/// Keeps operating system details translated under the settings load status.
fn settings_load_error(error: io::Error) -> crate::i18n::Message {
    crate::i18n::Message::new("status-settings-load-failed")
        .with_message("detail", crate::file_worker::file_error_message(error))
}

#[cfg(test)]
impl SettingsFile {
    /// Returns preferences stored for a document key.
    pub fn document(&self, key: &DocumentKey) -> Option<&DocumentPreferences> {
        self.documents
            .iter()
            .find(|entry| &entry.key == key)
            .map(Arc::as_ref)
    }
}

#[cfg(test)]
#[path = "settings_test.rs"]
pub(crate) mod tests;
