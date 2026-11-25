use super::*;

/// Converts a worker submission error into a status message.
pub(super) fn submit_error_message(error: SubmitError) -> Message {
    match error {
        SubmitError::QueueFull => Message::new("status-submit-queue-full"),
        SubmitError::Stopped => Message::new("status-submit-stopped"),
    }
}

/// Keeps the selected file name beside its localized load cause.
pub(super) fn file_open_error(path: &std::path::Path, cause: Message) -> Message {
    let message = Message::new("status-file-open-error").with_message("cause", cause);
    if let Some(name) = path.file_name() {
        message.with_text("path", name.to_string_lossy())
    } else {
        message.with_message("path", Message::new("dialog-file"))
    }
}

/// Chooses the natural Save As format for a document.
pub(super) fn default_save_format(document: &crate::document::Document) -> ExportFormat {
    match document.source.as_deref() {
        Some(SourceDocument::Csv(_)) => ExportFormat::Csv,
        Some(SourceDocument::Markdown(_)) | None => ExportFormat::Markdown,
    }
}

/// Returns the editor input only when it differs from the stored cell.
pub(super) fn changed_editor_input(input: CellInput, existing: Option<&Cell>) -> Option<CellInput> {
    let unchanged = existing.map_or(input == CellInput::Empty, |cell| cell.input == input);
    (!unchanged).then_some(input)
}

/// Retry coalesced work after an old failed batch, but stop on a same-version deterministic error.
pub(super) fn should_schedule_after_calculation_ack(
    current_revision: u64,
    completed_revision: u64,
    has_coalesced_changes: bool,
    worker_failed: bool,
) -> bool {
    has_coalesced_changes && (!worker_failed || current_revision > completed_revision)
}

/// Builds a stable widget ID for one edit session.
pub(super) fn editor_id(document_id: DocumentId, session_id: u64) -> Id {
    Id::unique(("shared_cell_editor", document_id, session_id))
}

/// Maps an egui key event to an editing command.
pub(super) fn editing_key_from_event(event: &egui::Event) -> Option<(InputKey, bool)> {
    let egui::Event::Key {
        key,
        pressed: true,
        modifiers,
        ..
    } = event
    else {
        return None;
    };
    let key = match key {
        Key::Enter if modifiers.alt => InputKey::AltEnter,
        Key::Enter => InputKey::Enter,
        Key::Tab => InputKey::Tab,
        Key::Escape => InputKey::Escape,
        Key::F2 if !modifiers.command && !modifiers.ctrl && !modifiers.alt => InputKey::F2,
        Key::ArrowUp if !modifiers.command && !modifiers.ctrl && !modifiers.alt => {
            InputKey::ArrowUp
        }
        Key::ArrowDown if !modifiers.command && !modifiers.ctrl && !modifiers.alt => {
            InputKey::ArrowDown
        }
        Key::ArrowLeft if !modifiers.command && !modifiers.ctrl && !modifiers.alt => {
            InputKey::ArrowLeft
        }
        Key::ArrowRight if !modifiers.command && !modifiers.ctrl && !modifiers.alt => {
            InputKey::ArrowRight
        }
        _ => return None,
    };
    Some((key, modifiers.shift))
}

/// Converts a character index to a UTF-8 byte offset.
pub(super) fn byte_index_for_char(text: &str, character_index: usize) -> usize {
    text.char_indices()
        .nth(character_index)
        .map_or(text.len(), |(byte_index, _)| byte_index)
}

/// Converts a UTF-8 byte position into an editor character offset.
pub(super) fn local_char_offset(text: &str, byte_offset: usize) -> usize {
    text.get(..byte_offset.min(text.len()))
        .map_or(0, |prefix| prefix.chars().count())
}

/// Formats a cell address for display.
pub(super) fn cell_coordinate(address: CellAddress) -> String {
    format!("{}{}", column_name(address.column), address.row + 1)
}

/// Formats a selection as a spreadsheet name.
pub(super) fn selection_name(selection: CellRange) -> String {
    let (start, end) = selection.normalized();
    if start == end {
        cell_coordinate(start)
    } else {
        format!("{}:{}", cell_coordinate(start), cell_coordinate(end))
    }
}

/// Parses the name box text into a cell range.
pub(super) fn parse_name_box_range(
    value: &str,
    row_count: usize,
    column_count: usize,
) -> Option<CellRange> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Some((start, end)) = value.split_once(':') {
        let start = start.trim();
        let end = end.trim();
        if let (Some(start), Some(end)) = (parse_cell_address(start), parse_cell_address(end)) {
            if start.row < row_count
                && end.row < row_count
                && start.column < column_count
                && end.column < column_count
            {
                return Some(CellRange {
                    anchor: start,
                    head: end,
                });
            }
            return None;
        }
        if let (Some(start), Some(end)) = (parse_absolute_column(start), parse_absolute_column(end))
            && start < column_count
            && end < column_count
            && row_count > 0
        {
            return Some(CellRange {
                anchor: CellAddress {
                    row: 0,
                    column: start,
                },
                head: CellAddress {
                    row: row_count - 1,
                    column: end,
                },
            });
        }
        if let (Some(start), Some(end)) = (parse_absolute_row(start), parse_absolute_row(end))
            && start < row_count
            && end < row_count
            && column_count > 0
        {
            return Some(CellRange {
                anchor: CellAddress {
                    row: start,
                    column: 0,
                },
                head: CellAddress {
                    row: end,
                    column: column_count - 1,
                },
            });
        }
        return None;
    }
    let address = parse_cell_address(value)?;
    (address.row < row_count && address.column < column_count).then_some(CellRange::single(address))
}

/// Parses one A1-style cell address, with an optional `$` before the column and the row.
pub(super) fn parse_cell_address(value: &str) -> Option<CellAddress> {
    let value = value.trim();
    let boundary = value
        .char_indices()
        .skip(1)
        .find_map(|(index, character)| {
            (character.is_ascii_digit() || character == '$').then_some(index)
        })?;
    let column = parse_absolute_column(&value[..boundary])?;
    let row = parse_absolute_row(&value[boundary..])?;
    Some(CellAddress { row, column })
}

/// Parses a column label that may carry one leading `$`, as in `$C`.
fn parse_absolute_column(value: &str) -> Option<usize> {
    parse_column_index(value.strip_prefix('$').unwrap_or(value))
}

/// Parses a row number that may carry one leading `$`, as in `$3`.
fn parse_absolute_row(value: &str) -> Option<usize> {
    let digits = value.strip_prefix('$').unwrap_or(value);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    parse_row_index(digits)
}

/// Parses a column label into a zero-based index.
pub(super) fn parse_column_index(value: &str) -> Option<usize> {
    if value.is_empty()
        || !value
            .chars()
            .all(|character| character.is_ascii_alphabetic())
    {
        return None;
    }
    let mut column = 0_usize;
    for character in value.chars() {
        column = column
            .checked_mul(26)?
            .checked_add((character.to_ascii_uppercase() as u8 - b'A' + 1) as usize)?;
    }
    let index = column.checked_sub(1)?;
    (index < MAX_COLUMNS).then_some(index)
}

/// Parses a row number into a zero-based index.
pub(super) fn parse_row_index(value: &str) -> Option<usize> {
    let row = value.trim().parse::<usize>().ok()?.checked_sub(1)?;
    (row < MAX_ROWS).then_some(row)
}

/// Formats a zero-based column index as a spreadsheet label.
pub(super) fn column_name(index: usize) -> String {
    let mut value = index + 1;
    let mut letters = Vec::new();
    while value > 0 {
        value -= 1;
        letters.push((b'A' + (value % 26) as u8) as char);
        value /= 26;
    }
    letters.iter().rev().collect()
}
