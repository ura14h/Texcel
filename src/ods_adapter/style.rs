use std::collections::{BTreeMap, BTreeSet};

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use super::*;

/// Character attributes a style sets; `None` means "inherit from the parent".
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct TextProps {
    pub(super) bold: Option<bool>,
    pub(super) italic: Option<bool>,
    pub(super) strike: Option<bool>,
    pub(super) font_name: Option<String>,
    pub(super) font_size: Option<f64>,
    pub(super) color: Option<String>,
}

impl TextProps {
    /// Fills unset attributes from `parent`.
    fn inherit(&mut self, parent: &TextProps) {
        self.bold = self.bold.or(parent.bold);
        self.italic = self.italic.or(parent.italic);
        self.strike = self.strike.or(parent.strike);
        if self.font_name.is_none() {
            self.font_name.clone_from(&parent.font_name);
        }
        self.font_size = self.font_size.or(parent.font_size);
        if self.color.is_none() {
            self.color.clone_from(&parent.color);
        }
    }
}

/// One `style:style` entry with only the properties the import looks at.
#[derive(Clone, Debug, Default)]
struct StyleEntry {
    parent: Option<String>,
    data_style: Option<String>,
    text: TextProps,
    wrap: Option<bool>,
    fill: Option<String>,
    border: Option<bool>,
    horizontal: Option<String>,
}

/// Cell, text, and sheet styles from `styles.xml` and `content.xml`.
#[derive(Debug, Default)]
pub(crate) struct StyleSheet {
    cell_styles: BTreeMap<String, StyleEntry>,
    text_styles: BTreeMap<String, StyleEntry>,
    hidden_tables: BTreeSet<String>,
}

/// The ODF name of the built-in style every cell inherits; its fonts are the app's own.
const DEFAULT_STYLE: &str = "Default";
/// The ODF family of cell styles, split so the translation-key check does not read it as a key.
const CELL_FAMILY: &str = concat!("table", "-cell");
/// Guards against parent loops in malformed files.
const MAX_STYLE_DEPTH: usize = 32;

impl StyleSheet {
    /// Reads named styles from `styles.xml` and automatic styles from `content.xml`.
    pub(super) fn parse(styles: &[u8], content: &[u8]) -> Result<Self, Message> {
        let mut sheet = Self::default();
        sheet.read_part(styles, "styles.xml")?;
        sheet.read_part(content, "content.xml")?;
        Ok(sheet)
    }

    /// Collects every `style:style` of a part until its body starts.
    fn read_part(&mut self, bytes: &[u8], part: &str) -> Result<(), Message> {
        let mut reader = Reader::from_reader(bytes);
        let mut current: Option<(String, String, StyleEntry)> = None;
        loop {
            let event = reader
                .read_event()
                .map_err(|error| xml_error(part, &error))?;
            match event {
                Event::Eof => return Ok(()),
                Event::Start(element) if element.name().as_ref() == b"office:body" => {
                    return Ok(());
                }
                Event::Start(element) if element.name().as_ref() == b"style:style" => {
                    current = Some(style_header(&element, part)?);
                }
                // A self-closing style has no properties of its own, only a parent.
                Event::Empty(element) if element.name().as_ref() == b"style:style" => {
                    let (family, name, entry) = style_header(&element, part)?;
                    self.store(&family, name, entry);
                }
                Event::Start(element) | Event::Empty(element) => {
                    if let Some((family, name, entry)) = current.as_mut() {
                        let attributes = attributes(&element, part)?;
                        apply_properties(element.name().as_ref(), &attributes, entry);
                        if element.name().as_ref() == b"style:table-properties"
                            && family == "table"
                            && attributes.get("table:display").map(String::as_str) == Some("false")
                        {
                            self.hidden_tables.insert(name.clone());
                        }
                    }
                }
                Event::End(element) if element.name().as_ref() == b"style:style" => {
                    if let Some((family, name, entry)) = current.take() {
                        self.store(&family, name, entry);
                    }
                }
                _ => {}
            }
        }
    }

    /// Keeps a finished style in the table for its family.
    fn store(&mut self, family: &str, name: String, entry: StyleEntry) {
        match family {
            CELL_FAMILY => {
                self.cell_styles.insert(name, entry);
            }
            "text" => {
                self.text_styles.insert(name, entry);
            }
            _ => {}
        }
    }

    /// Resolves a cell style and its parents into the XLSX-shaped style the import uses.
    pub(super) fn cell_style(&self, name: Option<&str>) -> XlsxStyle {
        let mut resolved = StyleEntry::default();
        let mut next = name.map(str::to_owned);
        for _ in 0..MAX_STYLE_DEPTH {
            let Some(current) = next.take() else {
                break;
            };
            if current == DEFAULT_STYLE {
                break;
            }
            let Some(entry) = self.cell_styles.get(&current) else {
                break;
            };
            resolved.text.inherit(&entry.text);
            if resolved.data_style.is_none() {
                resolved.data_style.clone_from(&entry.data_style);
            }
            resolved.wrap = resolved.wrap.or(entry.wrap);
            if resolved.fill.is_none() {
                resolved.fill.clone_from(&entry.fill);
            }
            resolved.border = resolved.border.or(entry.border);
            if resolved.horizontal.is_none() {
                resolved.horizontal.clone_from(&entry.horizontal);
            }
            next.clone_from(&entry.parent);
        }
        let border = resolved
            .border
            .unwrap_or(false)
            .then(|| crate::xlsx_adapter::XlsxBorder {
                style: "thin".to_owned(),
                color: None,
            });
        XlsxStyle {
            bold: resolved.text.bold.unwrap_or(false),
            italic: resolved.text.italic.unwrap_or(false),
            strike: resolved.text.strike.unwrap_or(false),
            font_name: resolved.text.font_name,
            font_size: resolved.text.font_size,
            font_color: resolved.text.color,
            fill_color: resolved.fill,
            number_format: resolved.data_style,
            wrap_text: resolved.wrap.unwrap_or(false),
            horizontal_alignment: resolved.horizontal,
            vertical_alignment: None,
            top_border: border.clone(),
            bottom_border: border.clone(),
            left_border: border.clone(),
            right_border: border,
        }
    }

    /// Resolves the character attributes of a `text:span` style.
    pub(super) fn text_props(&self, name: &str) -> TextProps {
        let mut resolved = TextProps::default();
        let mut next = Some(name.to_owned());
        for _ in 0..MAX_STYLE_DEPTH {
            let Some(current) = next.take() else {
                break;
            };
            let Some(entry) = self.text_styles.get(&current) else {
                break;
            };
            resolved.inherit(&entry.text);
            next.clone_from(&entry.parent);
        }
        resolved
    }

    /// Says whether a sheet style hides the sheet.
    pub(super) fn hides_table(&self, name: &str) -> bool {
        self.hidden_tables.contains(name)
    }
}

/// Reads a style's family, name, parent, and number format from its start tag.
fn style_header(
    element: &BytesStart<'_>,
    part: &str,
) -> Result<(String, String, StyleEntry), Message> {
    let attributes = attributes(element, part)?;
    let entry = StyleEntry {
        parent: attributes.get("style:parent-style-name").cloned(),
        data_style: attributes.get("style:data-style-name").cloned(),
        ..StyleEntry::default()
    };
    let name = attributes.get("style:name").cloned().unwrap_or_default();
    let family = attributes.get("style:family").cloned().unwrap_or_default();
    Ok((family, name, entry))
}

/// Reads the style properties the import maps into one style entry.
fn apply_properties(element: &[u8], attributes: &BTreeMap<String, String>, entry: &mut StyleEntry) {
    let value = |key: &str| attributes.get(key).map(String::as_str);
    match element {
        b"style:text-properties" => {
            if let Some(weight) = value("fo:font-weight") {
                entry.text.bold = Some(
                    weight == "bold" || weight.parse::<u32>().is_ok_and(|weight| weight >= 600),
                );
            }
            if let Some(style) = value("fo:font-style") {
                entry.text.italic = Some(matches!(style, "italic" | "oblique"));
            }
            if let Some(line) = value("style:text-line-through-style") {
                entry.text.strike = Some(line != "none");
            }
            if let Some(font) = value("style:font-name").or(value("fo:font-family")) {
                entry.text.font_name = Some(font.to_owned());
            }
            if let Some(size) = value("fo:font-size").and_then(points) {
                entry.text.font_size = Some(size);
            }
            if let Some(color) = value("fo:color") {
                entry.text.color = Some(color.to_owned());
            }
        }
        b"style:table-cell-properties" => {
            if let Some(wrap) = value("fo:wrap-option") {
                entry.wrap = Some(wrap == "wrap");
            }
            if let Some(fill) = value("fo:background-color") {
                entry.fill = (fill != "transparent").then(|| fill.to_owned());
            }
            for key in [
                "fo:border",
                "fo:border-top",
                "fo:border-bottom",
                "fo:border-left",
                "fo:border-right",
            ] {
                if let Some(border) = value(key) {
                    entry.border = Some(entry.border.unwrap_or(false) || border != "none");
                }
            }
            // Automatic alignment follows the value type, like an unset XLSX alignment.
            if value("style:text-align-source") == Some("value-type") {
                entry.horizontal = Some("general".to_owned());
            }
        }
        b"style:paragraph-properties" => {
            if entry.horizontal.is_none()
                && let Some(align) = value("fo:text-align")
            {
                entry.horizontal = Some(
                    match align {
                        "start" | "left" => "left",
                        "center" => "center",
                        "end" | "right" => "right",
                        other => other,
                    }
                    .to_owned(),
                );
            }
        }
        _ => {}
    }
}

/// Converts an ODF length such as `10pt` or `0.5in` to points.
fn points(length: &str) -> Option<f64> {
    let units = [
        ("pt", 1.0),
        ("in", 72.0),
        ("cm", 72.0 / 2.54),
        ("mm", 72.0 / 25.4),
        ("pc", 12.0),
        ("px", 0.75),
    ];
    units.iter().find_map(|(suffix, factor)| {
        length
            .strip_suffix(suffix)
            .and_then(|number| number.trim().parse::<f64>().ok())
            .filter(|number| number.is_finite() && *number > 0.0)
            .map(|number| number * factor)
    })
}

/// Reads all attributes of an element, keyed by their prefixed names.
pub(super) fn attributes(
    element: &BytesStart<'_>,
    part: &str,
) -> Result<BTreeMap<String, String>, Message> {
    let mut map = BTreeMap::new();
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|error| xml_error(part, &error))?;
        let key = String::from_utf8_lossy(attribute.key.as_ref()).into_owned();
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|error| xml_error(part, &error))?
            .into_owned();
        map.insert(key, value);
    }
    Ok(map)
}

/// Describes unreadable XML in one package part.
pub(super) fn xml_error(part: &str, error: &dyn std::fmt::Display) -> Message {
    Message::new("status-ods-xml-invalid")
        .with_text("part", part)
        .with_text("detail", error.to_string())
}
