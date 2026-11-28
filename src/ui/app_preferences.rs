use super::*;

impl TexcelApp {
    /// Captures the app-wide preferences and recent files for saving. Nothing about a single
    /// file is kept: Markdown and CSV cannot carry view settings, so a file always opens the
    /// same way.
    pub(super) fn snapshot_settings(&self) -> SettingsFile {
        SettingsFile {
            schema_version: self.settings_file.schema_version,
            app: self.current_app_preferences(),
            recent_files: self.recent_files.iter().cloned().collect(),
        }
    }

    // Build application preferences from the active UI state.
    pub(super) fn current_app_preferences(&self) -> AppPreferences {
        AppPreferences {
            language: self.language_choice,
            theme: self.theme,
            font_preset: self.font_preset,
            ui_font_points: self.ui_font_points,
            show_invisibles: self.show_invisibles,
            minimap_width: self.minimap_width,
        }
    }
}
