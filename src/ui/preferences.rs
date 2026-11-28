use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AppPreferenceField {
    Language,
    Theme,
    FontPreset,
    UiFontSize,
    ShowInvisibles,
    MinimapWidth,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct AppPreferenceChanges {
    pub(super) language: bool,
    pub(super) theme: bool,
    pub(super) font_preset: bool,
    pub(super) ui_font_points: bool,
    pub(super) show_invisibles: bool,
    pub(super) minimap_width: bool,
}

/// Returns the path spelling chosen by the user for one loaded document.
pub(super) fn user_file_path<'a>(
    app: &'a TexcelApp,
    document_id: DocumentId,
    backing: &'a FileBacking,
) -> &'a std::path::Path {
    app.user_file_paths
        .get(&document_id)
        .map_or(backing.path.as_path(), std::path::PathBuf::as_path)
}
