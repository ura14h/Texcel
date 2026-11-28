use super::*;
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
// Verify that saved settings retain the app-wide preferences and nothing about single files.
fn settings_roundtrip_preserves_app_preferences() {
    let directory = TestDirectory::new();
    let path = directory.0.join("settings.json");
    let mut settings = SettingsFile::default();
    settings.app.language = LanguageChoice::Japanese;
    settings.app.font_preset = FontPreset::NotoSansJp;
    settings.app.ui_font_points = 16.0;
    settings.app.minimap_width = MinimapWidth::Wide;
    settings.recent_files.push(PathBuf::from("/tmp/data.csv"));
    save_settings(&path, settings).expect("save settings");
    let text = fs::read_to_string(&path).expect("read the saved settings");
    assert!(
        !text.contains("documents") && !text.contains("view_layout"),
        "no per-file view settings are written: {text}"
    );

    let loaded = load_settings(&path).expect("load settings");
    assert_eq!(loaded.app.language, LanguageChoice::Japanese);
    assert_eq!(loaded.app.font_preset, FontPreset::NotoSansJp);
    assert_eq!(loaded.app.ui_font_points, 16.0);
    assert_eq!(loaded.app.minimap_width, MinimapWidth::Wide);
    assert_eq!(loaded.recent_files, [PathBuf::from("/tmp/data.csv")]);
}

#[cfg(unix)]
#[test]
// Keeps non-UTF8 native paths valid in settings while ordinary paths stay plain strings.
fn recent_paths_roundtrip_non_utf8_os_bytes() {
    use std::os::unix::ffi::OsStringExt;

    let path = PathBuf::from(std::ffi::OsString::from_vec(
        b"/tmp/table-\xff.csv".to_vec(),
    ));
    let mut settings = SettingsFile::default();
    settings.recent_files.push(path.clone());

    let encoded = serde_json::to_vec(&settings).expect("native path settings encode");
    let decoded: SettingsFile =
        serde_json::from_slice(&encoded).expect("native path settings decode");
    assert_eq!(decoded.recent_files.as_slice(), std::slice::from_ref(&path));

    let plain: SettingsFile =
        serde_json::from_str(r#"{"schema_version":2,"app":{},"recent_files":["/tmp/plain.csv"]}"#)
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

    fs::write(&path, b"not-json").expect("write corrupt settings");
    assert!(
        load_settings(&path)
            .unwrap_err()
            .resolve(crate::i18n::Language::English)
            .contains("invalid")
    );
    assert_eq!(fs::read(&path).expect("corrupt settings kept"), b"not-json");

    // Version 1 kept per-file view settings, so it is not read.
    fs::write(&path, br#"{"schema_version":1,"app":{},"documents":[]}"#)
        .expect("write version 1 settings");
    assert!(
        load_settings(&path).is_err(),
        "version 1 settings are not read"
    );

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
// Verify that settings paths are normalized without filesystem access.
fn settings_path_normalization_is_lexical() {
    assert_eq!(
        normalize_settings_path(Path::new("/tmp/a/../b/./file.csv")),
        PathBuf::from("/tmp/b/file.csv")
    );
}

#[test]
/// Offers the requested point sizes plus the platform default, and converts points to egui's
/// logical units per platform.
fn ui_font_points_offer_the_platform_default_and_convert_per_platform() {
    let choices = ui_font_point_choices();
    for points in UI_FONT_POINT_CHOICES {
        assert!(choices.contains(&points));
    }
    assert!(choices.contains(&default_ui_font_points()));
    assert!(choices.windows(2).all(|pair| pair[0] < pair[1]));
    let expected = if cfg!(target_os = "macos") {
        12.0
    } else {
        16.0
    };
    assert_eq!(ui_font_points_to_logical(12.0), expected);
}

#[test]
/// Ignores the old pixel-based text size so it is not read as points.
fn old_pixel_text_size_is_not_read_as_points() {
    let preferences: AppPreferences =
        serde_json::from_str(r#"{"ui_font_size": 20.0}"#).expect("old settings still parse");
    assert_eq!(preferences.ui_font_points, default_ui_font_points());
}
