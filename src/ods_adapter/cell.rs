//!  Converts one ODS cell's attributes and text into the shared cell model.

use super::cell_text::CellText;
use super::style::StyleSheet;
use super::*;

/// One `table:table-cell` or `table:covered-table-cell` being read.
pub(super) struct CellState {
    pub(super) attributes: BTreeMap<String, String>,
    pub(super) text: CellText,
}

/// Applies one text element inside a cell to its text builder.
pub(super) fn start_text_element(
    name: &str,
    attributes: &BTreeMap<String, String>,
    styles: &StyleSheet,
    text: &mut CellText,
) {
    match name {
        "text:p" | "text:h" => text.start_paragraph(),
        "text:s" => text.spaces(
            attributes
                .get("text:c")
                .and_then(|count| count.parse().ok())
                .unwrap_or(1),
        ),
        "text:tab" => text.tab(),
        "text:line-break" => text.line_break(),
        "text:span" => text.push_span(
            attributes
                .get("text:style-name")
                .map(|style| styles.text_props(style))
                .unwrap_or_default(),
        ),
        "text:a" => text.set_link(attributes.get("xlink:href").cloned()),
        _ => {}
    }
}

/// The cell to place (if it has content) and its merge span, plus a formula report.
pub(super) type ConvertedCell = (
    Option<(Option<XlsxCell>, Option<(usize, usize)>)>,
    Option<ConversionEntry>,
);

/// Turns a cell's typed value, text, and formula into the shared cell model.
pub(super) fn convert_cell(
    cell: CellState,
    style: XlsxStyle,
    sheet: Option<(usize, &str)>,
    address: CellAddress,
) -> ConvertedCell {
    let attributes = cell.attributes;
    let get = |key: &str| attributes.get(key).map(String::as_str);
    let text = cell.text.plain();
    let (mut value, hyperlink) = cell.text.into_value();
    let number = |key: &str| {
        get(key)
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite())
    };
    let typed = match get("office:value-type") {
        _ if get("calcext:value-type") == Some("error") => Some(XlsxValue::Error(text.clone())),
        Some("float" | "percentage" | "currency") => number("office:value").map(XlsxValue::Number),
        Some("date") => get("office:date-value")
            .and_then(values::date_serial)
            .map(XlsxValue::Number),
        Some("time") => get("office:time-value")
            .and_then(values::time_serial)
            .map(XlsxValue::Number),
        Some("boolean") => {
            get("office:boolean-value").map(|value| XlsxValue::Boolean(value == "true"))
        }
        Some("string") => {
            get("office:string-value").map(|value| XlsxValue::String(value.to_owned()))
        }
        _ => None,
    };
    if let Some(typed) = typed {
        value = typed;
    }
    let mut style = style;
    // Dates, times, percents, and currency lose their display format in Markdown.
    if matches!(
        get("office:value-type"),
        Some("date" | "time" | "percentage" | "currency")
    ) {
        style
            .number_format
            .get_or_insert_with(|| get("office:value-type").unwrap_or_default().to_owned());
    }
    let mut report = None;
    let formula = get("table:formula").and_then(|raw| {
        let converted = super::convert_open_formula(raw);
        if converted.is_none() {
            report = Some(formula_fallback(sheet, address, value != XlsxValue::Empty));
        }
        converted
    });
    // A formula's cached result is a plain value, never formatted runs.
    if formula.is_some()
        && let XlsxValue::RichText(_) = value
    {
        value = XlsxValue::String(text);
    }
    let span = |key: &str| {
        get(key)
            .and_then(|span| span.parse::<usize>().ok())
            .unwrap_or(1)
            .max(1)
    };
    let spans = (
        span("table:number-rows-spanned"),
        span("table:number-columns-spanned"),
    );
    let spans = (spans != (1, 1)).then_some(spans);
    let has_content = value != XlsxValue::Empty || formula.is_some() || hyperlink.is_some();
    let placed = has_content.then_some(XlsxCell {
        value,
        formula,
        style,
        hyperlink,
    });
    let converted = (placed.is_some() || spans.is_some()).then_some((placed, spans));
    (converted, report)
}

/// Reports a formula that has no A1 form and was replaced by its saved value.
fn formula_fallback(
    sheet: Option<(usize, &str)>,
    address: CellAddress,
    has_value: bool,
) -> ConversionEntry {
    let (class, reason, key) = if has_value {
        (
            ConversionClass::Converted,
            "An ODS formula that cannot be converted was replaced with its saved value.",
            "report-reason-ods-formula-value",
        )
    } else {
        (
            ConversionClass::Omitted,
            "An ODS formula that cannot be converted had no saved value and became empty.",
            "report-reason-ods-formula-empty",
        )
    };
    ConversionEntry::new(
        class,
        sheet.map(|(index, _)| index + 1),
        sheet.map(|(_, name)| name.to_owned()),
        Some(crate::xlsx_adapter::cell_reference(address)),
        reason,
    )
    .with_message(Message::new(key))
}
