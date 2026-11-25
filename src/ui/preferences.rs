use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum TablePreferenceField {
    DisplayMode,
    WrapMode,
    ViewLayout,
}

impl TablePreferenceField {
    /// Identifies preferences whose saved coordinates depend on the table structure.
    pub(super) fn is_coordinate_based(self) -> bool {
        matches!(self, Self::WrapMode | Self::ViewLayout)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AppPreferenceField {
    Language,
    Theme,
    FontPreset,
    UiFontSize,
    ShowInvisibles,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct AppPreferenceChanges {
    pub(super) language: bool,
    pub(super) theme: bool,
    pub(super) font_preset: bool,
    pub(super) ui_font_size: bool,
    pub(super) show_invisibles: bool,
}

// Build the saved preference key from a path and file identity.
pub(super) fn document_key(path: &std::path::Path, identity: &FileIdentity) -> DocumentKey {
    DocumentKey {
        path: normalize_recent_path(path),
        size: identity.size,
        sha256: identity.sha256,
    }
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

// Capture per-table view settings for later restoration.
pub(super) fn snapshot_table_preferences(document: &Document) -> Vec<TablePreferences> {
    document
        .tables
        .iter()
        .enumerate()
        .map(|(table_index, table)| {
            let view = document.view(table.id);
            TablePreferences {
                table_index,
                table_name: table.name.clone(),
                wrap_mode: view.map_or(WrapMode::Off, |view| {
                    if view.wrap_text {
                        WrapMode::On
                    } else {
                        WrapMode::Off
                    }
                }),
                view_layout: view.map_or_else(
                    || Arc::new(TableLayout::default()),
                    |view| Arc::clone(&view.layout),
                ),
            }
        })
        .collect()
}

// Copies coordinate-based preferences only from the saved identity's matching table.
pub(super) fn preserve_saved_coordinate_preferences(
    current: &mut [TablePreferences],
    saved: Option<&DocumentPreferences>,
) {
    let mut current_name_counts = BTreeMap::<String, usize>::new();
    for table in current.iter() {
        *current_name_counts
            .entry(table.table_name.clone())
            .or_default() += 1;
    }
    for table in current.iter_mut() {
        // Prefer the same position and name; a table moved by an unsaved edit matches by a unique name.
        let saved_table = saved.and_then(|saved| {
            let same_place = saved.tables.iter().find(|candidate| {
                candidate.table_index == table.table_index
                    && candidate.table_name == table.table_name
            });
            let mut same_name = saved
                .tables
                .iter()
                .filter(|candidate| candidate.table_name == table.table_name);
            let unique_name = current_name_counts.get(&table.table_name) == Some(&1);
            same_place.or_else(|| {
                let first = same_name.next();
                (unique_name && same_name.next().is_none())
                    .then_some(first)
                    .flatten()
            })
        });
        match saved_table {
            Some(saved_table) => {
                table.wrap_mode = saved_table.wrap_mode;
                table.view_layout = Arc::clone(&saved_table.view_layout);
            }
            None => {
                table.wrap_mode = WrapMode::Off;
                table.view_layout = Arc::new(TableLayout::default());
            }
        }
    }
}
