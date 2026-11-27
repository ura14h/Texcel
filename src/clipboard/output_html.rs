use super::*;

/// Creates formatted HTML with a plain-text fallback and validated Texcel data.
pub fn create_html_clipboard(
    internal_ranges: &[Vec<Vec<ClipboardCell>>],
    visible_ranges: &[Vec<Vec<ClipboardCell>>],
    origins: &[CellAddress],
    text: &str,
) -> Result<String, ClipboardError> {
    validate_ranges(internal_ranges)?;
    validate_ranges(visible_ranges)?;
    let payload = serde_json::to_vec(&InternalPayload {
        version: 1,
        owner_id: next_owner_id(),
        text: text.to_owned(),
        ranges: internal_ranges.to_vec(),
        origins: origins
            .iter()
            .map(|origin| [origin.row, origin.column])
            .collect(),
    })
    .map_err(|error| {
        ClipboardError(
            crate::i18n::Message::new("status-clipboard-payload-encode-failed")
                .with_text("detail", error),
        )
    })?;
    if payload.len() > MAX_CLIPBOARD_BYTES {
        return Err(ClipboardError(crate::i18n::Message::new(
            "status-clipboard-copy-limit",
        )));
    }
    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, payload);
    let mut html = String::with_capacity(encoded.len() + text.len() + 256);
    html.push_str("<html><body>");
    html.push_str(PAYLOAD_PREFIX);
    html.push_str(&encoded);
    html.push_str(PAYLOAD_SUFFIX);
    html.push_str("<table>");
    for (range_index, range) in visible_ranges.iter().enumerate() {
        for row in range {
            html.push_str("<tr>");
            for cell in row {
                html.push_str("<td");
                let style = html_cell_style(cell);
                if !style.is_empty() {
                    html.push_str(" style=\"");
                    html.push_str(&escape_html_attribute(&style));
                    html.push('"');
                }
                html.push('>');
                if let Some(runs) = &cell.rich_text {
                    for run in runs {
                        let run_style = run
                            .font_color
                            .as_deref()
                            .and_then(css_color)
                            .map(|color| format!("color:{color}"))
                            .into_iter()
                            .chain(
                                run.font_size
                                    .filter(|size| size.is_finite() && *size > 0.0)
                                    .map(|size| format!("font-size:{size}pt")),
                            )
                            .collect::<Vec<_>>()
                            .join(";");
                        if !run_style.is_empty() {
                            html.push_str("<span style=\"");
                            html.push_str(&escape_html_attribute(&run_style));
                            html.push_str("\">");
                        }
                        if let Some(link) = &run.hyperlink {
                            let target = if link.location {
                                format!("#{}", link.target)
                            } else {
                                link.target.clone()
                            };
                            html.push_str("<a href=\"");
                            html.push_str(&escape_html_attribute(&target));
                            html.push('"');
                            if let Some(tooltip) = &link.tooltip {
                                html.push_str(" title=\"");
                                html.push_str(&escape_html_attribute(tooltip));
                                html.push('"');
                            }
                            html.push('>');
                        }
                        let mut tags = Vec::new();
                        if run.bold {
                            tags.push("b");
                        }
                        if run.italic {
                            tags.push("i");
                        }
                        if run.strike {
                            tags.push("s");
                        }
                        if run.code {
                            tags.push("code");
                        }
                        for tag in &tags {
                            html.push('<');
                            html.push_str(tag);
                            html.push('>');
                        }
                        html.push_str(&escape_html_text(&run.text).replace('\n', "<br>"));
                        for tag in tags.into_iter().rev() {
                            html.push_str("</");
                            html.push_str(tag);
                            html.push('>');
                        }
                        if run.hyperlink.is_some() {
                            html.push_str("</a>");
                        }
                        if !run_style.is_empty() {
                            html.push_str("</span>");
                        }
                    }
                } else {
                    html.push_str(&escape_html_text(&cell.plain_text()).replace('\n', "<br>"));
                }
                html.push_str("</td>");
            }
            html.push_str("</tr>");
        }
        if range_index + 1 < visible_ranges.len() {
            html.push_str("<tr><td><br></td></tr>");
        }
    }
    html.push_str("</table></body></html>");
    if html.len() > MAX_CLIPBOARD_BYTES {
        return Err(ClipboardError(crate::i18n::Message::new(
            "status-clipboard-copy-limit",
        )));
    }
    Ok(html)
}
