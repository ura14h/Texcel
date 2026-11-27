use chrono::{NaiveDate, NaiveDateTime};

/// Seconds in one day, the unit of spreadsheet date serials.
const SECONDS_PER_DAY: f64 = 86_400.0;

/// Converts an ODF `office:date-value` into the 1900-system serial the app calculates with.
pub(super) fn date_serial(value: &str) -> Option<f64> {
    let epoch = NaiveDate::from_ymd_opt(1899, 12, 30)?.and_hms_opt(0, 0, 0)?;
    // Time zones are rare here and do not change the calendar day the user typed.
    let value = value.trim();
    let local = value
        .split_once(['Z', '+'])
        .map_or(value, |(local, _)| local);
    let moment = NaiveDateTime::parse_from_str(local, "%Y-%m-%dT%H:%M:%S%.f")
        .ok()
        .or_else(|| {
            NaiveDate::parse_from_str(local, "%Y-%m-%d")
                .ok()
                .and_then(|date| date.and_hms_opt(0, 0, 0))
        })?;
    let seconds = (moment - epoch).num_milliseconds() as f64 / 1000.0;
    Some(seconds / SECONDS_PER_DAY).filter(|serial| serial.is_finite())
}

/// Converts an ODF `office:time-value` duration such as `PT10H30M00S` into a fraction of days.
pub(super) fn time_serial(value: &str) -> Option<f64> {
    let value = value.trim();
    let (negative, value) = match value.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, value),
    };
    let body = value.strip_prefix('P')?;
    let (date_part, time_part) = body.split_once('T').unwrap_or((body, ""));
    let mut seconds = 0.0;
    seconds += duration_fields(date_part, &[('D', SECONDS_PER_DAY)])?;
    seconds += duration_fields(time_part, &[('H', 3600.0), ('M', 60.0), ('S', 1.0)])?;
    let days = seconds / SECONDS_PER_DAY;
    Some(if negative { -days } else { days }).filter(|days| days.is_finite())
}

/// Sums `<number><unit>` fields of an ISO 8601 duration part, in the given unit order.
fn duration_fields(part: &str, units: &[(char, f64)]) -> Option<f64> {
    let mut total = 0.0;
    let mut rest = part;
    let mut next_unit = 0;
    while !rest.is_empty() {
        let end = rest.find(|character: char| character.is_ascii_alphabetic())?;
        let unit = rest[end..].chars().next()?;
        let number = rest[..end].parse::<f64>().ok()?;
        let position = units[next_unit..]
            .iter()
            .position(|(candidate, _)| *candidate == unit)?;
        total += number * units[next_unit + position].1;
        next_unit += position + 1;
        rest = &rest[end + unit.len_utf8()..];
    }
    Some(total)
}
