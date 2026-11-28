//! App-local Fluent messages and the supported language choices.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};

use eframe::egui;
use fluent_templates::{LanguageIdentifier, Loader, langid, static_loader};
use serde::{Deserialize, Serialize};

use crate::settings::LanguageChoice;

static_loader! {
    static LOCALES = {
        locales: "locales",
        fallback_language: "en-US",
        customise: |bundle| bundle.set_use_isolating(false),
    };
}

static ENGLISH: LanguageIdentifier = langid!("en-US");
static JAPANESE: LanguageIdentifier = langid!("ja-JP");

/// Names the resolved language used by one app instance.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Language {
    English,
    Japanese,
}

/// Names a shared source-data limit without storing translated text in errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceLimit {
    FileSize,
    CellCount,
    RowCount,
    ColumnCount,
    CsvOutputCellCount,
}

/// Returns the localized label for a source-data limit.
pub fn resource_limit_message(limit: ResourceLimit) -> Message {
    Message::new(match limit {
        ResourceLimit::FileSize => "status-resource-limit-file-size",
        ResourceLimit::CellCount => "status-resource-limit-cell-count",
        ResourceLimit::RowCount => "status-resource-limit-row-count",
        ResourceLimit::ColumnCount => "status-resource-limit-column-count",
        ResourceLimit::CsvOutputCellCount => "status-resource-limit-csv-output-cell-count",
    })
}

impl Language {
    /// Returns the Fluent language identifier for this language.
    fn identifier(self) -> &'static LanguageIdentifier {
        match self {
            Self::English => &ENGLISH,
            Self::Japanese => &JAPANESE,
        }
    }
}

/// Resolves an explicit choice or the current operating system language.
pub fn resolve_language(choice: LanguageChoice) -> Language {
    match choice {
        LanguageChoice::English => Language::English,
        LanguageChoice::Japanese => Language::Japanese,
        LanguageChoice::System => resolve_system_tag(sys_locale::get_locale().as_deref()),
    }
}

/// Uses Japanese only for Japanese system tags and falls back to English otherwise.
pub fn resolve_system_tag(tag: Option<&str>) -> Language {
    if tag
        .and_then(|tag| tag.split(['-', '_']).next())
        .is_some_and(|language| language.eq_ignore_ascii_case("ja"))
    {
        Language::Japanese
    } else {
        Language::English
    }
}

/// Stores a Fluent message key and its locale-neutral arguments.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Message {
    key: String,
    arguments: BTreeMap<String, MessageArgument>,
}

/// Turns a CSV adapter cause into a language-neutral message.
pub fn csv_error_message(error: crate::csv_adapter::CsvError) -> Message {
    use crate::csv_adapter::CsvError;
    match error {
        CsvError::EncodingChoiceRequired => Message::new("status-csv-encoding-choice-required"),
        CsvError::InvalidEncoding => Message::new("status-csv-encoding-invalid"),
        CsvError::UnrepresentableCharacter => Message::new("status-csv-character-unrepresentable"),
        CsvError::InvalidDelimiter => Message::new("status-csv-delimiter-invalid"),
        CsvError::UnclosedQuote => Message::new("status-csv-quote-unclosed"),
        CsvError::InvalidQuoteSyntax => Message::new("status-csv-quote-syntax-invalid"),
        CsvError::InvalidNewline => Message::new("status-csv-newline-invalid"),
        CsvError::CsvParse(detail) => {
            Message::new("status-csv-parse-failed").with_text("detail", detail)
        }
        CsvError::ParserMismatch => Message::new("status-csv-parser-mismatch"),
        CsvError::NoSafeDelimiterMarker => Message::new("status-csv-no-safe-marker"),
        CsvError::ResourceLimit(limit) => Message::new("status-csv-resource-limit")
            .with_message("limit", resource_limit_message(limit)),
        CsvError::OutputMismatch => Message::new("status-csv-output-mismatch"),
    }
}

/// Turns a Markdown adapter cause into a language-neutral message.
pub fn markdown_error_message(error: crate::markdown_adapter::MarkdownError) -> Message {
    use crate::markdown_adapter::MarkdownError;
    match error {
        MarkdownError::InvalidUtf8 => Message::new("status-markdown-utf8-invalid"),
        MarkdownError::ResourceLimit(limit) => Message::new("status-markdown-resource-limit")
            .with_message("limit", resource_limit_message(limit)),
        MarkdownError::OutputMismatch => Message::new("status-markdown-output-mismatch"),
        MarkdownError::LastTable => Message::new("status-markdown-last-table-required"),
        MarkdownError::CellNotFound { table, row, column } => {
            Message::new("status-markdown-cell-not-found")
                .with_number("table", table as i64 + 1)
                .with_number("row", row as i64 + 1)
                .with_number("column", column as i64 + 1)
        }
    }
}

/// Turns an HTML adapter cause into a language-neutral message.
pub fn html_error_message(error: crate::html_adapter::HtmlError) -> Message {
    use crate::html_adapter::HtmlError;
    match error {
        HtmlError::InvalidUtf8 => Message::new("status-html-utf8-invalid"),
        HtmlError::UnsupportedEncoding(encoding) => {
            Message::new("status-html-encoding-unsupported").with_text("encoding", encoding)
        }
        HtmlError::ResourceLimit => Message::new("status-html-resource-limit"),
        HtmlError::NoTables => Message::new("status-html-table-required"),
        HtmlError::OutputMismatch => Message::new("status-html-output-mismatch"),
    }
}

/// Keeps an XLSX adapter message typed for the current app language.
pub fn xlsx_error_message(error: crate::xlsx_adapter::XlsxError) -> Message {
    error.0
}

/// Translates an XLSX package scan failure while preserving its part and limit details.
pub fn xlsx_scan_error_message(error: crate::conversion_report::XlsxScanError) -> Message {
    use crate::conversion_report::XlsxScanError;
    match error {
        XlsxScanError::InvalidArchive(detail) => {
            Message::new("status-xlsx-archive-invalid").with_text("detail", detail)
        }
        XlsxScanError::InvalidPackage(message) => message,
        XlsxScanError::InvalidXml { part, reason } => Message::new("status-xlsx-xml-invalid")
            .with_text("part", part)
            .with_text("detail", reason),
        XlsxScanError::LimitExceeded { limit, maximum } => {
            Message::new("status-xlsx-package-limit")
                .with_message("limit_name", limit.message())
                .with_number("limit", maximum as i64)
        }
    }
}

/// Turns a document edit cause into a language-neutral message.
pub fn document_error_message(error: crate::document::DocumentError) -> Message {
    use crate::document::DocumentError;
    match error {
        DocumentError::MissingTable(table_id) => {
            Message::new("status-table-id-unavailable").with_number("table_id", table_id as i64)
        }
        DocumentError::NeedsWorkerHistoryRestore => {
            Message::new("status-history-worker-restore-required")
        }
        DocumentError::StalePreparedStructure => Message::new("status-structure-stale"),
        DocumentError::OutOfBounds(address) => Message::new("status-cell-outside-table")
            .with_text("cell", crate::xlsx_adapter::cell_reference(address)),
        DocumentError::DuplicateCell(address) => Message::new("status-cell-duplicate-change")
            .with_text("cell", crate::xlsx_adapter::cell_reference(address)),
        DocumentError::ResourceLimit(limit) => resource_limit_message(limit),
        DocumentError::TextSourceValidationRequired => {
            Message::new("status-text-source-validation-required")
        }
        DocumentError::FormatRangeTooLarge(count) => {
            Message::new("status-format-range-limit").with_number("count", count as i64)
        }
    }
}

/// Keeps values typed until the active language formats the message.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
enum MessageArgument {
    Text(String),
    TextList(Vec<String>),
    Number(i64),
    Nested(Box<Message>),
}

impl Message {
    /// Returns the stable Fluent key for language-neutral checks.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Creates a message with no arguments.
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            arguments: BTreeMap::new(),
        }
    }

    /// Adds one external text detail for Fluent to show with the message.
    pub fn with_text(mut self, name: &'static str, value: impl std::fmt::Display) -> Self {
        self.arguments
            .insert(name.to_owned(), MessageArgument::Text(value.to_string()));
        self
    }

    /// Adds raw item names that should use the active language's list separator.
    pub fn with_text_list(mut self, name: &'static str, values: Vec<String>) -> Self {
        self.arguments
            .insert(name.to_owned(), MessageArgument::TextList(values));
        self
    }

    /// Adds one integer so Fluent can choose language-specific number forms.
    pub fn with_number(mut self, name: &'static str, value: i64) -> Self {
        self.arguments
            .insert(name.to_owned(), MessageArgument::Number(value));
        self
    }

    /// Adds a nested message that is resolved in the same active language.
    pub fn with_message(mut self, name: &'static str, value: Message) -> Self {
        self.arguments
            .insert(name.to_owned(), MessageArgument::Nested(Box::new(value)));
        self
    }

    /// Resolves this message using the app's current language.
    pub fn resolve(&self, language: Language) -> String {
        if self.arguments.is_empty() {
            return LOCALES.lookup(language.identifier(), &self.key);
        }
        let arguments = self
            .arguments
            .iter()
            .map(|(name, value)| {
                let value = match value {
                    MessageArgument::Text(value) => {
                        fluent_templates::fluent_bundle::FluentValue::String(Cow::Owned(
                            value.clone(),
                        ))
                    }
                    MessageArgument::TextList(values) => {
                        let separator = LOCALES.lookup(language.identifier(), "list-separator");
                        fluent_templates::fluent_bundle::FluentValue::String(Cow::Owned(
                            values.join(&separator),
                        ))
                    }
                    MessageArgument::Number(value) => {
                        fluent_templates::fluent_bundle::FluentValue::from(
                            fluent_templates::fluent_bundle::types::FluentNumber::from(*value),
                        )
                    }
                    MessageArgument::Nested(value) => {
                        fluent_templates::fluent_bundle::FluentValue::String(Cow::Owned(
                            value.resolve(language),
                        ))
                    }
                };
                (Cow::Owned(name.clone()), value)
            })
            .collect::<HashMap<_, _>>();
        LOCALES.lookup_with_args(language.identifier(), &self.key, &arguments)
    }

    /// Resolves one generated label using the app's shared Fluent bundle settings.
    pub fn resolve_generated_name(&self, language: Language) -> String {
        self.resolve(language)
    }
}

/// Provides an English fallback for error APIs that require a display string.
impl std::fmt::Display for Message {
    /// Formats a fallback string for non-UI error APIs.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.resolve(Language::English))
    }
}

impl std::error::Error for Message {}

/// Keeps calculation queue errors independent from the active language.
pub fn calculation_submit_message(error: crate::calc::CalculationSubmitError) -> Message {
    Message::new(match error {
        crate::calc::CalculationSubmitError::QueueFull => "status-calculation-queue-full",
        crate::calc::CalculationSubmitError::Stopped => "status-calculation-worker-stopped",
    })
}

/// Looks up one Fluent message without formatting arguments.
pub fn text(language: Language, key: &str) -> String {
    LOCALES.lookup(language.identifier(), key)
}

/// Tells whether a native window title is the Help window's title in either language.
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) fn is_help_window_title(title: &str) -> bool {
    [Language::English, Language::Japanese]
        .into_iter()
        .any(|language| text(language, "help-window-title") == title)
}

/// Stores one app's language in its own egui context for helper widgets.
pub fn set_context_language(ctx: &egui::Context, language: Language) {
    let id = egui::Id::NULL.with("texcel_language");
    ctx.data_mut(|data| data.insert_temp(id, language));
}

/// Looks up UI text using the language installed in this app's egui context.
pub fn context_text(ctx: &egui::Context, key: &str) -> String {
    text(context_language(ctx), key)
}

/// Resolves one retained message using this app context's current language.
pub fn context_message(ctx: &egui::Context, message: &Message) -> String {
    message.resolve(context_language(ctx))
}

/// Reads the language attached to one egui app context.
pub fn context_language(ctx: &egui::Context) -> Language {
    let id = egui::Id::NULL.with("texcel_language");
    let language = ctx.data(|data| data.get_temp::<Language>(id));
    language.unwrap_or(Language::English)
}

#[cfg(test)]
#[path = "i18n_test.rs"]
mod tests;
