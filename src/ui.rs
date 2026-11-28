//!  Implements the native screen, editing actions, and a grid that virtualizes both directions.
//!
//!  The screen keeps one edit buffer. The grid draws only visible cells, and committed
//!  content changes all go through the `Document` command path.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use eframe::egui::{
    self, Align2, Color32, FontFamily, FontId, Id, Key, Pos2, Rect, Sense, Stroke, Vec2,
};
use egui::text::{CCursor, CCursorRange};

use crate::calc::{
    CalculationChange, CalculationSnapshot, CalculationWorker, FormulaResultSnapshot,
};
use crate::clipboard::{
    ClipboardCell, CopyFormat, PasteOptions, create_html_clipboard, format_ranges_plain_text,
    format_ranges_plain_text_with_alignments, read_system_html_if_matching, write_system_clipboard,
    write_system_text,
};
use crate::conversion_report::{ImportReport, ImportReportState};
use crate::csv_adapter::{CsvCellChange, EncodingChoice};
use crate::document::{
    CalcStatus, Cell, CellAddress, CellDisplayMode, CellHyperlink, CellInput, CellRange,
    ColumnLayout, Document, DocumentId, EditMode, EditSession, FrozenPanes, InputAction, InputKey,
    LayoutTarget, LayoutValue, MAX_COLUMNS, MAX_ROWS, StructureAxis, StructureOperation, TableId,
    TableLayout, Workspace,
};
use crate::export::{
    ExportFormat, ExportOptions, ExportRequest, ExportScope, ExportSource, ExportTableMeta,
    FormulaOutput, MarkdownHeader, ModifierOutput, PreparedExport,
};
use crate::file_worker::{
    FileIdentity, FileOperation, FileResultKind, FileWorker, JobToken, LoadPreviewChunk,
    LoadPreviewRow, LoadProgressKind, LoadStage, SaveExpectation, SubmitError,
};
use crate::i18n::{Language, Message, context_message, context_text, text};
use crate::markdown_adapter::{MarkdownCellChange, MarkdownCellDisplay};
use crate::settings::{
    AppPreferences, FontPreset, LanguageChoice, SettingsEvent, SettingsFile, SettingsSubmitError,
    SettingsWorker, ThemeChoice, WrapMode, app_settings_path, normalize_recent_path,
    normalize_settings_path,
};
use crate::source::SourceDocument;
use crate::structure::{ClipboardPasteRequest, StructureRequest, StructureTableMeta};

use grid_theme::GridThemePalette;

mod about_dialog;
mod app_host;
mod appearance_dialog;
mod chrome;
mod footer;
mod formula_bar;
mod formula_functions;
mod grid_focus;
mod grid_theme;
#[cfg(any(test, target_os = "macos"))]
pub(crate) use about_dialog::APP_COPYRIGHT;
mod input_sequence;
mod menu_focus;
mod pending_checks;
mod pending_state;
mod status_notice;
mod text_parsing;
mod ui_constants;
mod visual;
mod window_state;

pub use app_host::TexcelAppHost;

/// Attaches the current shared menu to an additional Winit window on Windows.
#[cfg(target_os = "windows")]
pub(crate) fn attach_windows_native_menu(hwnd: isize) -> Option<isize> {
    app_host::attach_native_menu_to_window(hwnd)
}

use self::pending_state::*;
#[cfg(any(test, not(any(target_os = "macos", target_os = "windows"))))]
pub(super) use menu_focus::SharedEditMenuTarget;
pub(super) use menu_focus::{context_has_ime_event, name_box_id};
use text_parsing::*;

use self::ui_constants::*;

// Application screen state. `Workspace` owns document content, not the widgets.
pub struct TexcelApp {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    native_menu: Option<crate::native_menu::NativeMenu>,
    #[cfg(all(test, any(target_os = "macos", target_os = "windows")))]
    native_menu_events_for_test: Vec<crate::native_menu::NativeMenuEvent>,
    workspace: Workspace,
    window_states: BTreeMap<egui::ViewportId, window_state::WindowState>,
    current_viewport: Option<egui::ViewportId>,
    multi_window_mode: bool,
    /// Set by the window host, which then takes worker results and moves openings forward.
    host_result_dispatch: bool,
    app_exit_start_pending: bool,
    app_exit_transaction: bool,
    app_exit_ready: bool,
    edit: EditSession,
    edit_document_id: Option<DocumentId>,
    ime_composition: Option<ImeCompositionState>,
    ime_interrupt_pending: bool,
    /// True while this frame asks the platform to discard the current IME composition.
    ime_interrupt_this_frame: bool,
    ime_escape_cancelled_this_frame: bool,
    modal_ime_composition: Option<Id>,
    late_ime_editor: Option<Id>,
    /// The window whose raw input is being prepared, before egui starts its pass.
    raw_input_viewport: Option<egui::ViewportId>,
    startup_placeholder: Option<(DocumentId, u64)>,
    formula_references: FormulaReferenceState,
    pending_edit_input: Option<ordered_edit_input::PendingEditInput>,
    long_text_panel_open: bool,
    long_text_panel_dismissed_session: Option<u64>,
    theme: ThemeChoice,
    language_choice: LanguageChoice,
    language: Language,
    font_preset: FontPreset,
    installed_font_preset: FontPreset,
    ui_font_points: f32,
    /// This window's sheet zoom; the UI text size never changes it.
    sheet_zoom: SheetZoom,
    /// Width of one half-width grid monospace character at 100%, the column width unit.
    grid_character_width: f64,
    /// What to report when a user command's background job for a document succeeds.
    completion_notices: BTreeMap<DocumentId, (JobToken, Message)>,
    show_invisibles: bool,
    minimap_width: crate::settings::MinimapWidth,
    settings_window_open: bool,
    about_dialog_open: bool,
    /// App-wide: one Help window serves every document window.
    help_viewport_open: bool,
    help_blocks: help_window::HelpBlocks,
    help_outline: help_window::HelpOutline,
    /// Whether this window shows the Markdown minimap instead of the table list.
    minimap_open: bool,
    /// The table this window's minimap last brought into view.
    minimap_shown_table: Option<(DocumentId, TableId)>,
    /// The table whose tab this window's sheet strip last brought into view.
    sheet_tab_shown_table: Option<(DocumentId, TableId)>,
    minimap_caches: BTreeMap<DocumentId, minimap::MinimapCache>,
    settings_worker: Option<SettingsWorker>,
    settings_file: SettingsFile,
    settings_loaded: bool,
    settings_write_requested: bool,
    settings_save_generation: u64,
    settings_pending_save: Option<(u64, SettingsFile)>,
    settings_in_flight_generation: Option<u64>,
    settings_saved_generation: u64,
    settings_failed_generation: Option<u64>,
    settings_failure_notice_visible: bool,
    settings_close_attempt_generation: Option<u64>,
    settings_exit_authorized_generation: Option<u64>,
    close_after_settings_save: bool,
    app_preference_changes: AppPreferenceChanges,
    accesskit_table_tokens: BTreeMap<(DocumentId, u64), u32>,
    accesskit_tables_by_token: BTreeMap<u32, (DocumentId, u64)>,
    pending_accesskit_focus: Option<(DocumentId, u64, CellAddress)>,
    request_scroll_to_address: Option<(DocumentId, u64, CellAddress)>,
    accesskit_table_namespace: u32,
    next_accesskit_table_slot: u32,
    font_layout_generation: u64,
    last_pixels_per_point: f32,
    last_viewport_title: Option<String>,
    grid_focused: bool,
    text_dialog_open_previous_frame: bool,
    dialog_return_editor: Option<(DocumentId, u64, u64, crate::document::EditTarget)>,
    grid_keyboard_owner: Option<Id>,
    input_sequence_origin: Option<input_sequence::InputSequenceOrigin>,
    last_text_input_focus: Option<Id>,
    last_modal_text_input_focus: Option<Id>,
    active_modal_text_input_focus: Option<Id>,
    modal_menu_text_focus: Option<Id>,
    #[cfg(any(test, not(any(target_os = "macos", target_os = "windows"))))]
    shared_edit_menu_target: Option<SharedEditMenuTarget>,
    restore_modal_menu_focus_this_frame: bool,
    preserve_text_input_focus: bool,
    suppress_grid_keyboard: bool,
    axis_space_text_pending: Option<PendingAxisSpaceText>,
    request_editor_focus: bool,
    request_grid_focus: bool,
    request_scroll_to_selection: bool,
    status: Option<Message>,
    status_lifetime: Option<(Message, Instant)>,
    import_notice: Option<(DocumentId, Message)>,
    deferred_commands: Vec<commands::DeferredAppCommand>,
    deferred_text_commands: Vec<commands::DeferredTextCommand>,
    pending_close_document: Option<DocumentId>,
    close_after_save: BTreeSet<DocumentId>,
    close_after_save_edit: BTreeMap<DocumentId, EditSession>,
    save_conflict_dialog: Option<save_conflict::SaveConflictPrompt>,
    app_close_dialog_open: bool,
    app_close_save_tokens: BTreeMap<DocumentId, JobToken>,
    app_close_save_generation: u64,
    app_close_save_failed: bool,
    app_close_save_batch_active: bool,
    app_close_save_collecting_targets: bool,
    app_close_after_documents: bool,
    app_close_discard_pending: bool,
    app_close_waiting_recovery_removal: bool,
    app_close_recovery_removal_failed: bool,
    app_close_waiting_recovery_maintenance: bool,
    app_close_recovery_maintenance_failed: bool,
    app_close_after_recovery_maintenance: Option<bool>,
    app_close_discard_state: Option<AppCloseDiscardState>,
    app_close_discard_exit_ready: bool,
    #[cfg(test)]
    test_save_path_selection: Option<PathBuf>,
    #[cfg(test)]
    test_close_button_rects: BTreeMap<&'static str, egui::Rect>,
    #[cfg(test)]
    test_app_close_button_rects: BTreeMap<&'static str, egui::Rect>,
    recovery_worker: Option<crate::recovery::RecoveryWorker>,
    recovery_session_id: u128,
    recovery_keys: BTreeMap<DocumentId, String>,
    recovery_schedules: BTreeMap<DocumentId, RecoverySchedule>,
    pending_recovered_cell_edits: BTreeMap<DocumentId, crate::recovery::RecoveryCellEdit>,
    recovery_items: Vec<crate::recovery::RecoveryItem>,
    recovery_restore_pending: BTreeMap<String, DocumentId>,
    file_worker: FileWorker,
    queued_open_paths: VecDeque<PathBuf>,
    active_queued_open: Option<DocumentId>,
    load_request_viewports: BTreeMap<DocumentId, egui::ViewportId>,
    queued_open_failure: Option<Message>,
    recent_files: VecDeque<PathBuf>,
    recent_files_changed_before_load: bool,
    recent_save_paths: BTreeMap<DocumentId, (JobToken, PathBuf)>,
    file_backings: BTreeMap<DocumentId, FileBacking>,
    user_file_paths: BTreeMap<DocumentId, PathBuf>,
    import_origins: BTreeMap<DocumentId, PathBuf>,
    recovered_documents_needing_save: BTreeSet<DocumentId>,
    pending_file_load_paths: BTreeMap<DocumentId, PathBuf>,
    pending_document_focus: Option<DocumentId>,
    pending_csv_opens: BTreeMap<DocumentId, PendingCsvOpen>,
    pending_markdown_loads: BTreeMap<DocumentId, JobToken>,
    pending_html_loads: BTreeMap<DocumentId, JobToken>,
    pending_workbook_loads: BTreeMap<DocumentId, JobToken>,
    pending_load_previews: BTreeMap<DocumentId, LoadPreviewState>,
    pending_saves: BTreeMap<DocumentId, JobToken>,
    pending_save_history_states: BTreeMap<DocumentId, (JobToken, u64)>,
    pending_save_recovery_generations: BTreeMap<DocumentId, (JobToken, u64)>,
    cancelling_saves: BTreeMap<DocumentId, JobToken>,
    pending_recovery_flushes: BTreeMap<DocumentId, (crate::recovery::RecoverySnapshot, u64, u64)>,
    pending_recovery_cleanups: BTreeMap<String, (u64, u64)>,
    pending_recovery_removals: BTreeSet<String>,
    in_flight_recovery_cleanups: BTreeMap<String, (u64, u64)>,
    in_flight_recovery_maintenance: BTreeSet<String>,
    in_flight_recovery_removals: BTreeSet<String>,
    failed_recovery_maintenance: BTreeSet<String>,
    pending_report_saves: BTreeMap<DocumentId, JobToken>,
    import_reports: BTreeMap<DocumentId, ImportReportState>,
    active_import_report: Option<(DocumentId, Arc<ImportReport>)>,
    queued_import_reports: VecDeque<(DocumentId, Arc<ImportReport>)>,
    selected_import_group: usize,
    export_dialog: Option<ExportDialogState>,
    export_configs: BTreeMap<DocumentId, (ExportOptions, PathBuf)>,
    save_as_export_documents: BTreeSet<DocumentId>,
    pending_exports: BTreeMap<DocumentId, JobToken>,
    pending_export_writes: BTreeMap<DocumentId, JobToken>,
    cancelling_exports: BTreeMap<DocumentId, JobToken>,
    pending_export_report_saves: BTreeMap<DocumentId, JobToken>,
    active_export_confirmation: Option<ExportConfirmation>,
    pending_app_close_save_queue: VecDeque<DocumentId>,
    pending_app_close_save_targets: BTreeMap<DocumentId, (PathBuf, ExportOptions, bool)>,
    cell_link_dialog: Option<CellLinkDialog>,
    layout_dialog: Option<LayoutDialogState>,
    sheet_zoom_dialog: Option<sheet_zoom_dialog::SheetZoomDialogState>,
    search_dialog: Option<SearchDialogState>,
    paste_special_dialog: Option<PasteOptions>,
    copy_options_dialog: Option<CopyOptionsDialog>,
    clipboard_mark: Option<clipboard::ClipboardMark>,
    data_edit_dialog: Option<DataEditDialogState>,
    pending_searches: BTreeMap<DocumentId, PendingSearch>,
    pending_data_edits: BTreeMap<DocumentId, PendingDataEdit>,
    pending_structures: BTreeMap<DocumentId, JobToken>,
    pending_text_growth_sessions: BTreeMap<DocumentId, PendingTextGrowthEdit>,
    pending_growth_selection: Option<grid_events::PendingGrowthSelection>,
    pending_bulk_pastes: BTreeSet<DocumentId>,
    pending_table_management: BTreeSet<DocumentId>,
    pending_save_intents: BTreeMap<DocumentId, PendingSaveIntent>,
    pending_csv_promotion_cell_edits: BTreeMap<DocumentId, (TableId, CellAddress, String)>,
    pending_csv_promotion_cell_batches: BTreeMap<DocumentId, (TableId, Vec<(CellAddress, String)>)>,
    pending_markdown_link_edits: BTreeMap<DocumentId, (TableId, CellRange, Option<CellHyperlink>)>,
    fill_handle_drag: Option<FillHandleDrag>,
    grid_selection_drag: Option<GridSelectionDrag>,
    header_resize_drag: Option<HeaderResizeDrag>,
    grid_page_extent: Vec2,
    pending_history_restores: BTreeMap<DocumentId, JobToken>,
    retired_structure_roots: Vec<Vec<crate::document::Table>>,
    text_selection: Option<TextSelection>,
    name_box: String,
    grid_row_layouts: BTreeMap<(DocumentId, u64), GridRowLayout>,
    markdown_cell_displays: BTreeMap<MarkdownCellDisplayKey, CachedMarkdownCellDisplay>,
    pending_markdown_displays: BTreeMap<MarkdownCellDisplayKey, PendingMarkdownCellDisplay>,
    pending_auto_fits: BTreeMap<DocumentId, autofit::PendingAutoFit>,
    /// When each still-opening document began, for the opening progress window.
    opening_since: BTreeMap<DocumentId, Instant>,
    /// Loaded documents whose tables still need their first column fit.
    opening_fits: BTreeMap<DocumentId, opening_fit::OpeningFit>,
    deferred_auto_fits: BTreeMap<DocumentId, autofit::DeferredAutoFit>,
    calculation_worker: CalculationWorker,
    pending_calculations: BTreeMap<DocumentId, JobToken>,
    pending_initial_calculations: BTreeMap<DocumentId, (JobToken, Arc<CalculationSnapshot>)>,
    calculation_initial_snapshots: BTreeMap<DocumentId, (JobToken, Arc<CalculationSnapshot>)>,
    submitted_calculation_changes: BTreeMap<DocumentId, (JobToken, Vec<(TableId, CellAddress)>)>,
    calculation_snapshots: BTreeMap<DocumentId, (u64, FormulaResultSnapshot)>,
    calculation_retries: BTreeSet<DocumentId>,
    calculation_retry_available: BTreeSet<DocumentId>,
    deferred_file_results: Vec<crate::file_worker::FileResult>,
    last_load_progress_frame: Option<u64>,
    next_job_id: u64,
}

mod app_adapter;
mod app_close;
mod app_helpers;
mod app_misc;
mod app_preferences;
mod app_settings;
mod app_startup;
mod app_window;
mod autofit;
mod calculation;
mod calculation_results;
mod cell_edit;
mod cell_links;
mod cell_paint;
mod clipboard;
mod close_transactions;
mod commands;
mod completion_notice;
mod copy_options_dialog;
mod data_edit_actions;
mod data_edit_dialog;
mod dialog_state;
mod display_mode_bar;
mod document_lifecycle;
mod document_names;
mod edit_text;
mod export;
mod export_dialogs;
mod export_report_dialogs;
mod file_dialogs;
mod file_open;
mod file_result_analysis;
mod file_result_export;
mod file_result_failure;
mod file_result_load;
mod file_result_save;
mod file_result_structure;
mod file_results;
mod format_actions;
mod formula_reference_actions;
mod formula_reference_input;
mod formula_references;
mod grid;
mod grid_accessibility;
mod grid_context;
mod grid_events;
mod grid_geometry;
mod grid_layout;
mod grid_scale;
mod grid_segment;
mod grid_selection;
mod help_window;
mod ime;
mod keyboard;
mod keyboard_navigation;
mod layout_dialog;
mod load_preview;
mod markdown_alignment;
mod markdown_cell_layout;
mod markdown_display;
mod markdown_format;
mod markdown_text_layout;
mod markdown_view;
mod minimap;
mod open_queue;
mod opening_fit;
pub(super) mod opening_progress;
mod ordered_edit_input;
mod paste_special_dialog;
mod preferences;
mod recovery_maintenance;
mod recovery_ui;
mod report_rows;
mod retired_tables;
mod row_selection;
mod save;
mod save_conflict;
mod search_actions;
mod search_dialog;
mod selection;
mod sheet_zoom_dialog;
mod structure_ops;
mod table_management;
mod top;
mod virtual_grid;

use self::app_helpers::*;
use self::app_misc::*;
use self::cell_paint::*;
use self::dialog_state::*;
use self::edit_text::*;
use self::formula_references::*;
use self::grid_accessibility::*;
use self::grid_context::*;
use self::grid_geometry::*;
use self::grid_layout::*;
use self::grid_scale::*;
use self::grid_segment::*;
use self::grid_selection::*;
use self::markdown_alignment::*;
use self::markdown_cell_layout::*;
use self::markdown_display::*;
use self::markdown_text_layout::*;
use self::preferences::*;
use self::row_selection::*;
use self::virtual_grid::*;

pub(crate) use self::cell_paint::{CellTextStyle, HorizontalAlignment};
pub(crate) use self::edit_text::cell_view_text as auto_fit_cell_view_text;
pub(crate) use self::markdown_cell_layout::markdown_cell_layout_job as auto_fit_markdown_cell_layout_job;
pub(crate) use self::markdown_text_layout::CellTextFormat;
pub(crate) use self::markdown_text_layout::apply_grid_bold as auto_fit_apply_grid_bold;
pub(crate) use self::markdown_text_layout::apply_grid_monospace as auto_fit_apply_grid_monospace;
pub(crate) use self::markdown_text_layout::cell_layout_job as auto_fit_cell_layout_job;

#[cfg(test)]
#[path = "ui_test.rs"]
mod tests;
#[cfg(test)]
pub(super) use tests::install_test_markdown_grid;

#[cfg(test)]
#[path = "ui_settings_test.rs"]
mod settings_tests;

#[cfg(test)]
#[path = "ui_accessibility_test.rs"]
mod accessibility_tests;

#[cfg(test)]
#[path = "ui_csv_format_test.rs"]
mod csv_format_tests;

#[cfg(test)]
#[path = "ui/startup_test.rs"]
mod startup_tests;

#[cfg(test)]
#[path = "ui/grid_interaction_test.rs"]
mod grid_interaction_tests;

#[cfg(test)]
#[path = "ui/table_alignment_action_test.rs"]
mod table_alignment_action_tests;

#[cfg(test)]
#[path = "ui/formula_reference_interaction_test.rs"]
mod formula_reference_interaction_tests;

#[cfg(test)]
#[path = "ui/search_data_enter_test.rs"]
mod search_data_enter_tests;

#[cfg(test)]
#[path = "ui/scenario_test.rs"]
mod scenario_tests;

#[cfg(test)]
#[path = "ui/grid_modes_test.rs"]
mod grid_modes_tests;

#[cfg(test)]
#[path = "ui/sheet_selection_test.rs"]
mod sheet_selection_tests;

#[cfg(test)]
#[path = "ui/source_limit_test.rs"]
mod source_limit_tests;

#[cfg(test)]
#[path = "ui/markdown_format_test.rs"]
mod markdown_format_tests;

#[cfg(test)]
#[path = "ui/footer_test.rs"]
mod footer_tests;

#[cfg(test)]
#[path = "ui/window_state_test.rs"]
mod window_state_tests;

#[cfg(test)]
#[path = "ui/grid_ime_test.rs"]
mod grid_ime_tests;

#[cfg(test)]
#[path = "ui/fill_handle_test.rs"]
mod fill_handle_tests;

#[cfg(test)]
#[path = "ui/save_extent_test.rs"]
mod save_extent_tests;

#[cfg(test)]
#[path = "ui/header_extent_test.rs"]
mod header_extent_tests;

#[cfg(test)]
#[path = "ui/formula_structure_test.rs"]
mod formula_structure_tests;

#[cfg(test)]
#[path = "ui/header_selection_test.rs"]
mod header_selection_tests;

#[cfg(test)]
#[path = "ui/freeze_view_test.rs"]
mod freeze_view_tests;
#[cfg(test)]
#[path = "ui/minimap_test.rs"]
mod minimap_tests;
#[cfg(test)]
#[path = "ui/opening_fit_test.rs"]
mod opening_fit_tests;
#[cfg(test)]
#[path = "ui/wrap_height_test.rs"]
mod wrap_height_tests;
