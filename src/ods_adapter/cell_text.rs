use super::style::TextProps;
use super::*;

/// Collects one cell's paragraphs as formatted runs, following ODF white-space rules.
#[derive(Debug, Default)]
pub(super) struct CellText {
    runs: Vec<XlsxTextRun>,
    paragraphs: usize,
    /// Collapsed white space waiting for the next visible character.
    pending_space: bool,
    /// True right after a paragraph or line start, where leading spaces are dropped.
    at_line_start: bool,
    spans: Vec<TextProps>,
    link: Option<String>,
}

impl CellText {
    /// Starts a `text:p`; paragraphs after the first become line breaks in the cell.
    pub(super) fn start_paragraph(&mut self) {
        if self.paragraphs > 0 {
            self.push_raw("\n");
        }
        self.paragraphs += 1;
        self.pending_space = false;
        self.at_line_start = true;
    }

    /// Ends a `text:p`, dropping trailing collapsed white space.
    pub(super) fn end_paragraph(&mut self) {
        self.pending_space = false;
    }

    /// Adds character data, collapsing runs of white space to one space.
    pub(super) fn text(&mut self, text: &str) {
        let mut visible = String::with_capacity(text.len());
        for character in text.chars() {
            if matches!(character, ' ' | '\t' | '\n' | '\r') {
                self.pending_space |= !self.at_line_start;
                continue;
            }
            if self.pending_space {
                visible.push(' ');
                self.pending_space = false;
            }
            visible.push(character);
            self.at_line_start = false;
        }
        if !visible.is_empty() {
            self.push_raw(&visible);
        }
    }

    /// Adds `count` spaces from `text:s`, keeping any collapsed space before them.
    pub(super) fn spaces(&mut self, count: usize) {
        self.flush_space();
        self.push_raw(&" ".repeat(count.clamp(1, 1024)));
        self.at_line_start = false;
    }

    /// Adds a tab from `text:tab`.
    pub(super) fn tab(&mut self) {
        self.flush_space();
        self.push_raw("\t");
        self.at_line_start = false;
    }

    /// Adds a line break from `text:line-break`.
    pub(super) fn line_break(&mut self) {
        self.pending_space = false;
        self.push_raw("\n");
        self.at_line_start = true;
    }

    /// Applies a `text:span` style on top of the enclosing span.
    pub(super) fn push_span(&mut self, mut props: TextProps) {
        if let Some(outer) = self.spans.last() {
            props.bold = props.bold.or(outer.bold);
            props.italic = props.italic.or(outer.italic);
            props.strike = props.strike.or(outer.strike);
        }
        self.spans.push(props);
    }

    /// Leaves the innermost `text:span`.
    pub(super) fn pop_span(&mut self) {
        self.spans.pop();
    }

    /// Enters or leaves a `text:a` link.
    pub(super) fn set_link(&mut self, target: Option<String>) {
        self.link = target;
    }

    /// Emits a collapsed space before explicit spacing elements.
    fn flush_space(&mut self) {
        if self.pending_space {
            self.pending_space = false;
            self.push_raw(" ");
        }
    }

    /// Appends text to the last run when its formatting matches, or starts a new run.
    fn push_raw(&mut self, text: &str) {
        let props = self.spans.last().cloned().unwrap_or_default();
        let run = XlsxTextRun {
            text: String::new(),
            bold: props.bold.unwrap_or(false),
            italic: props.italic.unwrap_or(false),
            strike: props.strike.unwrap_or(false),
            code: false,
            hyperlink: self.link.as_ref().map(|target| CellHyperlink {
                target: target.clone(),
                tooltip: None,
                location: target.starts_with('#'),
            }),
            font_name: None,
            font_size: None,
            font_color: None,
        };
        match self.runs.last_mut() {
            Some(last)
                if last.bold == run.bold
                    && last.italic == run.italic
                    && last.strike == run.strike
                    && last.hyperlink == run.hyperlink =>
            {
                last.text.push_str(text);
            }
            _ => self.runs.push(XlsxTextRun {
                text: text.to_owned(),
                ..run
            }),
        }
    }

    /// The cell's plain text.
    pub(super) fn plain(&self) -> String {
        self.runs.iter().map(|run| run.text.as_str()).collect()
    }

    /// Turns the runs into a value: plain text when nothing varies, one cell link, or rich text.
    pub(super) fn into_value(self) -> (XlsxValue, Option<CellHyperlink>) {
        let text = self.plain();
        if text.is_empty() {
            return (XlsxValue::Empty, None);
        }
        let plain = |run: &XlsxTextRun| !run.bold && !run.italic && !run.strike;
        let first_link = self.runs.first().and_then(|run| run.hyperlink.clone());
        if self.runs.iter().all(plain) && self.runs.iter().all(|run| run.hyperlink == first_link) {
            return (XlsxValue::String(text), first_link);
        }
        (XlsxValue::RichText(self.runs), None)
    }
}
