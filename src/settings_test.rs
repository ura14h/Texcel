use super::*;
use crate::document::TableLayout;
use std::time::{SystemTime, UNIX_EPOCH};

/// Creates a settings worker whose request and result channels are closed.
pub(crate) fn disconnected_for_test() -> SettingsWorker {
    let (requests, request_receiver) = mpsc::sync_channel(1);
    drop(request_receiver);
    let (result_sender, results) = mpsc::channel();
    drop(result_sender);
    let worker = thread::spawn(|| {});
    SettingsWorker {
        requests,
        results,
        _thread: worker,
    }
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    // Create settings storage for an isolated test path.
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is valid")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("texcel-settings-test-{nonce}"));
        fs::create_dir_all(&path).expect("create test directory");
        Self(path)
    }
}

impl Drop for TestDirectory {
    // Remove temporary settings test files.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
// Keeps operating system details out of localized settings error messages.
fn settings_io_errors_use_translated_file_causes() {
    let message = settings_load_error(io::Error::new(
        io::ErrorKind::PermissionDenied,
        "/private/secret/settings.json: os error 13",
    ));
    let english = message.resolve(crate::i18n::Language::English);
    let japanese = message.resolve(crate::i18n::Language::Japanese);

    assert!(english.contains("Settings could not be loaded"));
    assert!(english.contains("Access was denied"));
    assert!(japanese.contains("設定を読み込めません"));
    assert!(japanese.contains("アクセスが拒否"));
    assert!(!english.contains("/private/secret"));
    assert!(!japanese.contains("/private/secret"));
    assert!(!english.contains("os error"));
    assert!(!japanese.contains("os error"));
}

#[test]
// Verify that saved settings retain application and document preferences.
fn settings_roundtrip_preserves_app_and_document_preferences() {
    let directory = TestDirectory::new();
    let path = directory.0.join("settings.json");
    let key = DocumentKey {
        path: PathBuf::from("/tmp/data.csv"),
        size: 3,
        sha256: [7; 32],
    };
    let preferences = DocumentPreferences {
        key: key.clone(),
        display_mode: crate::document::DocumentDisplayMode::Source,
        tables: vec![TablePreferences {
            table_index: 0,
            table_name: "Sheet A".to_owned(),
            wrap_mode: WrapMode::On,
            view_layout: Arc::new(TableLayout::default()),
        }],
    };
    let mut settings = SettingsFile::default();
    settings.app.language = LanguageChoice::Japanese;
    settings.app.font_preset = FontPreset::NotoSansJp;
    settings.app.ui_font_size = 16.0;
    settings.upsert_document(preferences);
    save_settings(&path, settings).expect("save settings");

    let loaded = load_settings(&path).expect("load settings");
    assert_eq!(loaded.app.language, LanguageChoice::Japanese);
    assert_eq!(loaded.app.font_preset, FontPreset::NotoSansJp);
    assert_eq!(loaded.app.ui_font_size, 16.0);
    let saved_document = loaded.document(&key).expect("matching identity");
    assert_eq!(
        saved_document.display_mode,
        crate::document::DocumentDisplayMode::Source
    );
    let table = &saved_document.tables[0];
    assert_eq!(table.table_index, 0);
    assert_eq!(table.table_name, "Sheet A");
    assert_eq!(table.wrap_mode, WrapMode::On);
}

#[cfg(unix)]
#[test]
// Keeps non-UTF8 native paths valid in settings while ordinary paths stay plain strings.
fn recent_and_document_paths_roundtrip_non_utf8_os_bytes() {
    use std::os::unix::ffi::OsStringExt;

    let path = PathBuf::from(std::ffi::OsString::from_vec(
        b"/tmp/table-\xff.csv".to_vec(),
    ));
    let mut settings = SettingsFile::default();
    settings.recent_files.push(path.clone());
    settings.documents.push(Arc::new(DocumentPreferences {
        key: DocumentKey {
            path: path.clone(),
            size: 0,
            sha256: [0; 32],
        },
        display_mode: crate::document::DocumentDisplayMode::Live,
        tables: Vec::new(),
    }));

    let encoded = serde_json::to_vec(&settings).expect("native path settings encode");
    let decoded: SettingsFile =
        serde_json::from_slice(&encoded).expect("native path settings decode");
    assert_eq!(decoded.recent_files.as_slice(), std::slice::from_ref(&path));
    assert_eq!(decoded.documents[0].key.path, path);

    let plain: SettingsFile =
        serde_json::from_str(r#"{"schema_version":1,"app":{},"recent_files":["/tmp/plain.csv"]}"#)
            .expect("UTF-8 recent paths are stored as plain strings");
    assert_eq!(plain.recent_files, [PathBuf::from("/tmp/plain.csv")]);
}

#[test]
// Verify that invalid settings files do not alter application source data.
fn missing_corrupt_and_unknown_version_settings_never_replace_source_data() {
    let directory = TestDirectory::new();
    let path = directory.0.join("settings.json");
    let missing = load_settings(&path).expect("missing settings are defaults");
    assert_eq!(missing.schema_version, SETTINGS_VERSION);
    assert_eq!(missing.app.theme, ThemeChoice::System);
    assert_eq!(missing.app.font_preset, FontPreset::Auto);
    assert!(missing.documents.is_empty());

    fs::write(&path, b"not-json").expect("write corrupt settings");
    assert!(
        load_settings(&path)
            .unwrap_err()
            .resolve(crate::i18n::Language::English)
            .contains("invalid")
    );
    assert_eq!(fs::read(&path).expect("corrupt settings kept"), b"not-json");

    fs::write(&path, br#"{"schema_version":99,"app":{},"documents":[]}"#)
        .expect("write unknown schema");
    assert!(
        load_settings(&path)
            .unwrap_err()
            .resolve(crate::i18n::Language::English)
            .contains("not supported")
    );
}

#[test]
// Verify that document settings match both the path and file content identity.
fn document_preferences_require_exact_path_and_content_identity() {
    let key = DocumentKey {
        path: PathBuf::from("/tmp/data.csv"),
        size: 3,
        sha256: [1; 32],
    };
    let settings = SettingsFile {
        documents: vec![Arc::new(DocumentPreferences {
            key: key.clone(),
            display_mode: crate::document::DocumentDisplayMode::Live,
            tables: Vec::new(),
        })],
        ..SettingsFile::default()
    };
    assert!(settings.document(&key).is_some());
    assert!(
        settings
            .document(&DocumentKey {
                sha256: [2; 32],
                ..key.clone()
            })
            .is_none()
    );
    assert!(
        settings
            .document(&DocumentKey {
                path: PathBuf::from("/tmp/other.csv"),
                ..key
            })
            .is_none()
    );
}

#[test]
// Verify that saving at a path replaces its prior content identity.
fn updating_a_saved_path_replaces_its_previous_content_identity() {
    let path = PathBuf::from("/tmp/data.csv");
    let old_key = DocumentKey {
        path: path.clone(),
        size: 3,
        sha256: [1; 32],
    };
    let new_key = DocumentKey {
        sha256: [2; 32],
        ..old_key.clone()
    };
    let mut settings = SettingsFile::default();
    settings.upsert_document(DocumentPreferences {
        key: old_key.clone(),
        display_mode: crate::document::DocumentDisplayMode::Live,
        tables: Vec::new(),
    });
    settings.upsert_document(DocumentPreferences {
        key: new_key.clone(),
        display_mode: crate::document::DocumentDisplayMode::Live,
        tables: Vec::new(),
    });

    assert_eq!(settings.documents.len(), 1);
    assert!(settings.document(&old_key).is_none());
    assert!(settings.document(&new_key).is_some());
}

#[test]
// Verify that a failed atomic settings save leaves the existing file unchanged.
fn failed_atomic_settings_save_leaves_existing_target_intact() {
    let directory = TestDirectory::new();
    let target = directory.0.join("target-directory");
    fs::create_dir(&target).expect("create target directory");
    let settings = SettingsFile::default();
    assert!(save_settings(&target, settings).is_err());
    assert!(target.is_dir());
}

#[test]
// Verify that oversized history is trimmed so application settings can still save.
fn oversized_document_history_is_trimmed_so_app_settings_still_save() {
    let directory = TestDirectory::new();
    let path = directory.0.join("settings.json");
    let settings = SettingsFile {
        app: AppPreferences {
            theme: ThemeChoice::Dark,
            ..AppPreferences::default()
        },
        documents: vec![Arc::new(DocumentPreferences {
            key: DocumentKey {
                path: PathBuf::from("/tmp/large.csv"),
                size: 0,
                sha256: [0; 32],
            },
            display_mode: crate::document::DocumentDisplayMode::Live,
            tables: vec![TablePreferences {
                table_index: 0,
                table_name: "x".repeat(MAX_SETTINGS_BYTES as usize + 1),
                wrap_mode: WrapMode::Off,
                view_layout: Arc::new(TableLayout::default()),
            }],
        })],
        ..SettingsFile::default()
    };

    let outcome = save_settings(&path, settings).expect("save bounded settings");
    assert_eq!(outcome.dropped_documents, 1);
    assert_eq!(outcome.persisted.app.theme, ThemeChoice::Dark);
    assert!(outcome.persisted.documents.is_empty());
    let loaded = load_settings(&path).expect("read bounded settings");
    assert_eq!(loaded.app.theme, ThemeChoice::Dark);
    assert!(loaded.documents.is_empty());
}

#[test]
// Verify that settings paths are normalized without filesystem access.
fn settings_path_normalization_is_lexical() {
    assert_eq!(
        normalize_settings_path(Path::new("/tmp/a/../b/./file.csv")),
        PathBuf::from("/tmp/b/file.csv")
    );
}
