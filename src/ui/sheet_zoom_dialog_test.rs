use super::*;

// Opens on the matching preset, or on the custom value for any other zoom.
#[test]
fn dialog_starts_on_the_current_zoom() {
    let preset = SheetZoomDialogState::new(0.75);
    assert_eq!(preset.preset, Some(75));
    assert_eq!(preset.zoom(), 0.75);
    let custom = SheetZoomDialogState::new(1.37);
    assert_eq!(custom.preset, None);
    assert_eq!(custom.custom_percent, 137);
    assert_eq!(custom.zoom(), 1.37);
}

// A custom value outside 10% to 400% is pulled back into the range.
#[test]
fn custom_zoom_stays_in_range() {
    let mut dialog = SheetZoomDialogState::new(1.0);
    dialog.preset = None;
    dialog.custom_percent = 900;
    assert_eq!(dialog.zoom(), MAX_SHEET_ZOOM);
    dialog.custom_percent = 1;
    assert_eq!(dialog.zoom(), MIN_SHEET_ZOOM);
}
