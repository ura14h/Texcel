use super::*;

/// Lets users follow the operating system or choose either supported language.
pub(super) fn language_selector(
    ui: &mut egui::Ui,
    choice: &mut LanguageChoice,
    language: Language,
) {
    let selected = match choice {
        LanguageChoice::System => crate::i18n::Message::new("settings-language-system")
            .with_text(
                "language",
                text(
                    language,
                    match language {
                        Language::English => "settings-language-english",
                        Language::Japanese => "settings-language-japanese",
                    },
                ),
            )
            .resolve(language),
        LanguageChoice::English => text(language, "settings-language-english"),
        LanguageChoice::Japanese => text(language, "settings-language-japanese"),
    };
    egui::ComboBox::from_id_salt("texcel_settings_language")
        .selected_text(selected)
        .width(control_width(ui))
        .show_ui(ui, |ui| {
            ui.selectable_value(
                choice,
                LanguageChoice::System,
                text(language, "settings-language-system-name"),
            );
            ui.selectable_value(
                choice,
                LanguageChoice::English,
                text(language, "settings-language-english"),
            );
            ui.selectable_value(
                choice,
                LanguageChoice::Japanese,
                text(language, "settings-language-japanese"),
            );
        })
        .response
        .on_hover_text(text(language, "settings-language-help"));
}

/// Keeps the active language and saved language choice together.
pub(super) struct LanguageControls<'a> {
    pub(super) current: Language,
    pub(super) selected: &'a mut LanguageChoice,
}

/// Draws app-wide appearance controls in their own sections.
pub(super) fn draw_appearance_page(
    ui: &mut egui::Ui,
    language_controls: LanguageControls<'_>,
    font_preset: &mut FontPreset,
    font_preview_details_open: &mut bool,
    theme: &mut ThemeChoice,
    ui_font_size: &mut f32,
    show_invisibles: &mut bool,
) {
    let LanguageControls {
        current: language,
        selected: language_choice,
    } = language_controls;
    settings_page_header(
        ui,
        &text(language, "settings-page-appearance"),
        &text(language, "settings-page-appearance-help"),
    );
    settings_section(ui, &text(language, "settings-theme-section"));
    settings_row(ui, &text(language, "settings-theme"), |ui| {
        theme_selector(ui, theme, language)
    });

    ui.add_space(visual::SPACE_4);
    settings_section(ui, &text(language, "settings-language-section"));
    settings_row(ui, &text(language, "settings-language"), |ui| {
        language_selector(ui, language_choice, language);
    });

    ui.add_space(visual::SPACE_4);
    settings_section(ui, &text(language, "settings-font-section"));
    settings_row(ui, &text(language, "settings-font"), |ui| {
        ui.vertical(|ui| {
            let width = control_width(ui);
            ui.set_min_width(width);
            ui.set_max_width(width);
            egui::ComboBox::from_id_salt("texcel_font_preset")
                .selected_text(match font_preset {
                    FontPreset::Auto => text(language, "settings-font-auto"),
                    FontPreset::Default => text(language, "settings-font-default"),
                    FontPreset::NotoSansJp => text(language, "settings-font-noto"),
                })
                .width(control_width(ui))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        font_preset,
                        FontPreset::Auto,
                        text(language, "settings-font-auto"),
                    );
                    ui.selectable_value(
                        font_preset,
                        FontPreset::Default,
                        text(language, "settings-font-default"),
                    );
                    ui.selectable_value(
                        font_preset,
                        FontPreset::NotoSansJp,
                        text(language, "settings-font-noto"),
                    );
                });
            font_preview(ui, font_preview_details_open, language);
        });
    });
    settings_row(ui, &text(language, "settings-ui-font-size"), |ui| {
        ui_font_size_slider(ui, ui_font_size);
    });

    ui.add_space(visual::SPACE_4);
    settings_section(ui, &text(language, "settings-edit-display-section"));
    settings_row(
        ui,
        &text(language, "settings-show-invisibles-label"),
        |ui| {
            ui.checkbox(show_invisibles, text(language, "settings-show-invisibles"));
        },
    );
}

/// Picks the UI text size in whole points, applying it only when a drag ends.
///
/// Applying it while dragging would resize this dialog under the pointer.
fn ui_font_size_slider(ui: &mut egui::Ui, ui_font_size: &mut f32) {
    let row_width = control_width(ui);
    let draft_id = Id::unique("texcel_ui_font_size_draft");
    let mut draft = ui
        .data(|data| data.get_temp::<f32>(draft_id))
        .unwrap_or(*ui_font_size);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = visual::SPACE_8;
        let value_width = 52.0;
        let slider_width = (row_width - value_width - visual::SPACE_8).max(60.0);
        ui.spacing_mut().slider_width = slider_width;
        let response = ui.add_sized(
            [slider_width, CHROME_CONTROL_HEIGHT],
            egui::Slider::new(
                &mut draft,
                crate::settings::MIN_UI_FONT_SIZE..=crate::settings::MAX_UI_FONT_SIZE,
            )
            .step_by(1.0)
            .show_value(false),
        );
        if response.dragged() {
            ui.data_mut(|data| data.insert_temp(draft_id, draft));
        } else {
            ui.data_mut(|data| data.remove::<f32>(draft_id));
            *ui_font_size = draft;
        }
        ui.add_sized(
            [value_width, CHROME_CONTROL_HEIGHT],
            egui::Label::new(egui::RichText::new(format!("{draft:.0} px")).monospace()),
        );
    });
}
