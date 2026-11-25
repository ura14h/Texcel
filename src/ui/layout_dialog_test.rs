use super::*;

// Shows widths with at most two decimals and drops trailing zeros.
#[test]
fn column_width_text_has_at_most_two_decimals() {
    assert_eq!(format_column_width(13.857_142), "13.86");
    assert_eq!(format_column_width(8.43), "8.43");
    assert_eq!(format_column_width(10.5), "10.5");
    assert_eq!(format_column_width(20.0), "20");
    assert_eq!(format_column_width(0.004), "0");
}

// Stores an entered width rounded to the two decimals the dialog offers.
#[test]
fn entered_column_width_is_rounded_to_two_decimals() {
    assert_eq!(round_column_width(12.345_6), 12.35);
    assert_eq!(round_column_width(7.0), 7.0);
}
