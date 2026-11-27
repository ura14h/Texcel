use super::*;

impl FileWorker {
    /// Requests an import-only HTML conversion into the Markdown editing model.
    pub fn import_html_with_language(
        &self,
        token: JobToken,
        path: PathBuf,
        language: crate::i18n::Language,
    ) -> Result<(), SubmitError> {
        self.submit(
            token,
            WorkerMessage::ImportHtml {
                token,
                path,
                language,
                cancelled: Arc::new(AtomicBool::new(false)),
            },
        )
    }

    // Requests CSV loading and parsing asynchronously.
    pub fn load_csv(
        &self,
        token: JobToken,
        path: PathBuf,
        encoding: EncodingChoice,
        delimiter: Option<char>,
    ) -> Result<(), SubmitError> {
        self.submit(
            token,
            WorkerMessage::LoadCsv {
                token,
                path,
                request: CsvLoadRequest {
                    encoding,
                    delimiter,
                    settings_only: false,
                },
                cancelled: Arc::new(AtomicBool::new(false)),
            },
        )
    }

    /// Analyzes CSV import settings before any document is created.
    pub fn load_csv_options(
        &self,
        token: JobToken,
        path: PathBuf,
        encoding: EncodingChoice,
    ) -> Result<(), SubmitError> {
        self.submit(
            token,
            WorkerMessage::LoadCsv {
                token,
                path,
                request: CsvLoadRequest {
                    encoding,
                    delimiter: None,
                    settings_only: true,
                },
                cancelled: Arc::new(AtomicBool::new(false)),
            },
        )
    }

    /// Requests Markdown loading with the language used for generated table names.
    pub fn load_markdown_with_language(
        &self,
        token: JobToken,
        path: PathBuf,
        language: crate::i18n::Language,
    ) -> Result<(), SubmitError> {
        self.submit(
            token,
            WorkerMessage::LoadMarkdown {
                token,
                path,
                language,
                cancelled: Arc::new(AtomicBool::new(false)),
            },
        )
    }

    /// Requests an import-only workbook conversion into the Markdown editing model.
    pub fn import_workbook_with_language(
        &self,
        token: JobToken,
        path: PathBuf,
        format: WorkbookFormat,
        language: crate::i18n::Language,
    ) -> Result<(), SubmitError> {
        self.submit(
            token,
            WorkerMessage::ImportWorkbook {
                token,
                path,
                format,
                language,
                cancelled: Arc::new(AtomicBool::new(false)),
            },
        )
    }
}

#[cfg(test)]
impl FileWorker {
    // Requests Markdown loading and GFM table parsing asynchronously.
    pub fn load_markdown(&self, token: JobToken, path: PathBuf) -> Result<(), SubmitError> {
        self.load_markdown_with_language(token, path, crate::i18n::Language::English)
    }
}
