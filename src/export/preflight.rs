use super::*;

use crate::i18n::Message;

/// Prepare output paths and record path-level compatibility notes.
pub(super) fn prepare_destinations(
    request: &ExportRequest,
    selected: &[ExportTable],
    entries: &mut Vec<ConversionEntry>,
) -> Vec<(PathBuf, Vec<ExportTable>)> {
    let mut destinations = Vec::new();

    let source_markdown_can_be_patched = request.options.format == ExportFormat::Markdown
        && request.options.scope == ExportScope::AllTables
        && matches!(
            request.options.formulas,
            FormulaOutput::Formula | FormulaOutput::LatestResult
        )
        && request.options.modifiers == ModifierOutput::Convert
        && request.options.markdown_header == MarkdownHeader::UseFirstRow;
    if !source_markdown_can_be_patched
        && let Some(ExportSource::Markdown(source)) = &request.source
        && markdown_has_non_table_content(source)
    {
        entries.push(
            ConversionEntry::new(
                ConversionClass::Omitted,
                None,
                None,
                None,
                "表外のMarkdown文章・見出し・リンク定義は、表形式の出力へ引き継がれません",
            )
            .with_message(Message::new("report-reason-markdown-outside-table")),
        );
    }

    if request.options.format.is_delimited()
        && let (Some(ExportSource::Markdown(_)), ExportScope::Selected(selected_id)) =
            (&request.source, request.options.scope)
    {
        for (index, table) in request
            .tables
            .iter()
            .enumerate()
            .filter(|(_, table)| table.id != selected_id)
        {
            entries.push(
                ConversionEntry::new(
                    ConversionClass::Omitted,
                    Some(index + 1),
                    Some(table.name.clone()),
                    None,
                    "Unselected Markdown tables are not included in this CSV file.",
                )
                .with_message(
                    Message::new("report-reason-unselected-tables")
                        .with_text_list("tables", vec![table.name.clone()]),
                ),
            );
        }
    }

    if let Some(ExportSource::Html(_)) = &request.source {
        entries.push(
            ConversionEntry::new(
                ConversionClass::Omitted,
                None,
                None,
                None,
                "HTML表の属性、CSS、表外の文章やスクリプトは別形式へ引き継がれません",
            )
            .with_message(Message::new("report-reason-html-structure-dropped")),
        );
    }

    if request.options.format.is_delimited() && selected.len() > 1 {
        let mut paths = BTreeSet::new();
        let duplicate_names = duplicate_table_names(selected);
        for table in selected {
            let requested_suffix = duplicate_names
                .contains(&table.id)
                .then_some(table.order + 1);
            let Some((path, collision_resolved)) = allocate_per_table_csv_path(
                &request.destination,
                table,
                requested_suffix,
                &mut paths,
            ) else {
                entries.push(
                    ConversionEntry::new(
                        ConversionClass::Unprocessable,
                        Some(table.order + 1),
                        Some(table.name.clone()),
                        None,
                        "表番号から一意なCSVファイル名を作れません",
                    )
                    .with_message(Message::new("report-reason-csv-name-unavailable")),
                );
                continue;
            };
            if let Some(suffix) = requested_suffix {
                entries.push(
                    ConversionEntry::new(
                        ConversionClass::Converted,
                        Some(table.order + 1),
                        Some(table.name.clone()),
                        None,
                        format!("同名表を区別するためCSVファイル名に表番号 {suffix} を付けました"),
                    )
                    .with_message(
                        Message::new("report-reason-csv-name-number")
                            .with_number("number", suffix as i64),
                    ),
                );
            }
            if collision_resolved {
                entries.push(
                    ConversionEntry::new(
                        ConversionClass::Converted,
                        Some(table.order + 1),
                        Some(table.name.clone()),
                        None,
                        "ほかの表から生成したCSV名との衝突を避ける番号を付けました",
                    )
                    .with_message(Message::new("report-reason-csv-name-collision")),
                );
            }
            destinations.push((path, vec![table.clone()]));
        }
    } else {
        destinations.push((request.destination.clone(), selected.to_vec()));
    }
    for (path, _) in &destinations {
        let extension_matches = request.options.format.matches_path(path);
        if !extension_matches {
            entries.push(
                ConversionEntry::new(
                    ConversionClass::Unprocessable,
                    None,
                    None,
                    None,
                    format!(
                        "出力形式に対応する拡張子 .{} を指定してください",
                        request.options.format.extension()
                    ),
                )
                .with_message(
                    Message::new("report-reason-export-extension")
                        .with_text("extension", request.options.format.extension()),
                ),
            );
        }
    }

    destinations
}

/// Carry source import warnings into the output report.
pub(super) fn merge_source_report_entries(
    request: &ExportRequest,
    entries: &mut Vec<ConversionEntry>,
) {
    if let Some(import_report) = &request.source_report {
        for entry in &import_report.entries {
            if entry.class == ConversionClass::Retained {
                continue;
            }
            if let (ExportScope::Selected(id), Some(order)) =
                (request.options.scope, entry.sheet_order)
                && request
                    .tables
                    .iter()
                    .position(|table| table.id == id)
                    .is_some_and(|index| order != index + 1)
            {
                continue;
            }
            let reason = if entry.class == ConversionClass::Omitted {
                format!(
                    "読み込み時にモデルへ保持されていない情報です: {}",
                    entry.reason
                )
            } else {
                format!("読み込み時に変換された情報です: {}", entry.reason)
            };
            let source_detail = entry.message.clone().unwrap_or_else(|| {
                Message::new("report-reason-raw-detail").with_text("detail", &entry.reason)
            });
            let reason_message = Message::new(if entry.class == ConversionClass::Omitted {
                "report-reason-import-omitted"
            } else {
                "report-reason-import-converted"
            })
            .with_message("detail", source_detail);
            entries.push(
                ConversionEntry::new(
                    if entry.class == ConversionClass::Unprocessable {
                        ConversionClass::Omitted
                    } else {
                        entry.class
                    },
                    entry.sheet_order,
                    entry.sheet_name.clone(),
                    entry.range.clone(),
                    reason,
                )
                .with_message(reason_message),
            );
        }
    }
}

/// Build output bytes and check each destination before writing.
pub(super) fn build_export_artifacts(
    context: &ExportBuildContext<'_, '_>,
    destinations: &[(PathBuf, Vec<ExportTable>)],
    entries: &mut Vec<ConversionEntry>,
    cancelled: Option<&AtomicBool>,
) -> Result<Vec<ExportArtifact>, crate::i18n::Message> {
    let request = context.request;
    let mut artifacts = Vec::new();
    for (path, tables) in destinations {
        check_cancelled(cancelled)?;
        if request
            .source_path
            .as_deref()
            .is_some_and(|source| same_normalized_path(path, source))
        {
            entries.push(
                ConversionEntry::new(
                    ConversionClass::Unprocessable,
                    None,
                    None,
                    None,
                    "現在開いている入力ファイルと同じ保存先は指定できません",
                )
                .with_message(Message::new("report-reason-same-source-path")),
            );
        }
        if destinations.len() > 1 && path.exists() {
            entries.push(
                ConversionEntry::new(
                    ConversionClass::Unprocessable,
                    None,
                    None,
                    None,
                    format!("複数CSVの出力先が既に存在します: {}", path.display()),
                )
                .with_message(
                    Message::new("report-reason-output-exists").with_text("path", path.display()),
                ),
            );
        }
        let Some(table) = tables.first() else {
            continue;
        };
        match build_artifact(context, tables, entries) {
            Ok(bytes) => {
                let expected_identity = if path.exists() && destinations.len() == 1 {
                    match crate::file_worker::FileIdentity::read(path) {
                        Ok(identity) => Some(identity),
                        Err(error) => {
                            entries.push(
                                ConversionEntry::new(
                                    ConversionClass::Unprocessable,
                                    Some(table.order + 1),
                                    Some(table.name.clone()),
                                    None,
                                    format!("保存先を安全に検査できません: {error}"),
                                )
                                .with_message(
                                    Message::new("report-reason-file-check-failed")
                                        .with_text("detail", error),
                                ),
                            );
                            None
                        }
                    }
                } else {
                    None
                };
                artifacts.push(ExportArtifact {
                    path: path.clone(),
                    bytes,
                    replace_existing: expected_identity.is_some(),
                    expected_identity,
                });
            }
            Err(message) => entries.push(
                ConversionEntry::new(
                    ConversionClass::Unprocessable,
                    Some(table.order + 1),
                    Some(table.name.clone()),
                    None,
                    message.resolve(crate::i18n::Language::English),
                )
                .with_message(
                    Message::new("report-reason-export-failed").with_message("detail", message),
                ),
            ),
        }
    }

    Ok(artifacts)
}
