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
    ui_font_points: &mut f32,
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
            // The text size sits right of the font, as in a word processor's font bar.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = visual::SPACE_8;
                let size_width = 84.0_f32.min(width / 3.0);
                let font_width = (width - size_width - visual::SPACE_8).max(60.0);
                egui::ComboBox::from_id_salt("texcel_font_preset")
                    .selected_text(match font_preset {
                        FontPreset::Auto => text(language, "settings-font-auto"),
                        FontPreset::Default => text(language, "settings-font-default"),
                        FontPreset::NotoSansJp => text(language, "settings-font-noto"),
                    })
                    .width(font_width)
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
                ui_font_points_selector(ui, ui_font_points, size_width, language);
            });
            font_preview(ui, font_preview_details_open, language);
        });
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

/// Picks the interface text size in points from the offered list.
fn ui_font_points_selector(
    ui: &mut egui::Ui,
    ui_font_points: &mut f32,
    width: f32,
    language: Language,
) {
    let label = text(language, "settings-ui-font-size");
    let points_text = |points: f32| {
        Message::new("settings-ui-font-points")
            .with_text("points", format!("{points:.0}"))
            .resolve(language)
    };
    let response = egui::ComboBox::from_id_salt("texcel_ui_font_points")
        .selected_text(points_text(*ui_font_points))
        .width(width)
        .show_ui(ui, |ui| {
            for points in crate::settings::ui_font_point_choices() {
                ui.selectable_value(ui_font_points, points, points_text(points));
            }
        })
        .response;
    response.on_hover_text(label);
}
