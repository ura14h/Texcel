use super::*;

/// Gives each page a clear title and a short explanation of its scope.
pub(super) fn settings_page_header(ui: &mut egui::Ui, title: &str, description: &str) {
    visual::page_title(ui, title);
    visual::caption(ui, description);
    ui.add_space(visual::SPACE_4);
}

/// Names a setting group and leaves a short gap before its rows.
pub(super) fn settings_section(ui: &mut egui::Ui, title: &str) {
    visual::section_title(ui, title);
}

/// Aligns setting labels and lets controls define each row's real height.
///
/// Option dialogs use the same rows so every dialog places labels and controls alike.
pub(in crate::ui) fn settings_row(
    ui: &mut egui::Ui,
    label: &str,
    add_control: impl FnOnce(&mut egui::Ui),
) {
    let available = ui.available_width();
    let narrow = available < 440.0;
    let color = ui.visuals().text_color();
    if narrow {
        ui.vertical(|ui| {
            ui.add(settings_label(label, color));
            ui.scope(|ui| {
                ui.set_width(ui.available_width());
                configure_setting_control(ui);
                add_control(ui);
            });
        });
    } else {
        let gap = 12.0_f32.min((available - 160.0).max(0.0));
        let label_width = 180.0_f32.min((available - gap - 1.0).max(1.0));
        let control_width =
            (available - label_width - gap).clamp(1.0, super::SETTINGS_CONTROL_WIDTH);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            let (label_rect, response) = ui.allocate_exact_size(
                egui::vec2(label_width, visual::BUTTON_HEIGHT),
                egui::Sense::hover(),
            );
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Label, ui.is_enabled(), label)
            });
            ui.painter().text(
                label_rect.left_center(),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(visual::BODY_SIZE),
                color,
            );
            ui.allocate_ui_with_layout(
                egui::vec2(control_width, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_min_width(control_width);
                    ui.set_max_width(control_width);
                    configure_setting_control(ui);
                    add_control(ui);
                },
            );
        });
    }
}

/// Height of one radio choice in a list, shorter than a button row.
const CHOICE_HEIGHT: f32 = 24.0;

/// Packs a list of radio choices closer together, keeping the first one level with its label.
pub(in crate::ui) fn compact_choices(ui: &mut egui::Ui) {
    ui.spacing_mut().interact_size.y = CHOICE_HEIGHT;
    ui.spacing_mut().item_spacing.y = 2.0;
    ui.add_space((visual::BUTTON_HEIGHT - CHOICE_HEIGHT) / 2.0);
}

/// Builds a left-aligned settings label that may wrap at the available width.
fn settings_label(label: &str, color: Color32) -> egui::Label {
    egui::Label::new(egui::RichText::new(label).color(color))
        .halign(egui::Align::Min)
        .wrap()
}

/// Uses shared control spacing while allowing vertical content to grow naturally.
fn configure_setting_control(ui: &mut egui::Ui) {
    ui.spacing_mut().button_padding = Vec2::new(12.0, 6.0);
    ui.spacing_mut().interact_size.y = visual::BUTTON_HEIGHT;
    ui.spacing_mut().icon_spacing = visual::SPACE_8;
}

/// Shows the three theme choices as one compact segmented control.
pub(super) fn theme_selector(ui: &mut egui::Ui, theme: &mut ThemeChoice, language: Language) {
    let content_width = (control_width(ui) - 6.0).max(1.0);
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(egui::Margin::same(3))
        .show(ui, |ui| {
            ui.set_min_width(content_width);
            ui.set_max_width(content_width);
            ui.spacing_mut().item_spacing.x = 1.0;
            ui.spacing_mut().button_padding.x = 4.0;
            let width = ((ui.available_width() - 2.0) / 3.0).max(0.0);
            ui.horizontal(|ui| {
                for (choice, label, help) in [
                    (
                        ThemeChoice::System,
                        text(language, "settings-theme-system"),
                        text(language, "settings-theme-system-help"),
                    ),
                    (
                        ThemeChoice::Light,
                        text(language, "settings-theme-light"),
                        text(language, "settings-theme-light-help"),
                    ),
                    (
                        ThemeChoice::Dark,
                        text(language, "settings-theme-dark"),
                        text(language, "settings-theme-dark-help"),
                    ),
                ] {
                    let response = ui.add_sized(
                        [width, CHROME_CONTROL_HEIGHT],
                        egui::Button::selectable(*theme == choice, label).corner_radius(5.0),
                    );
                    if response.clicked() {
                        *theme = choice;
                    }
                    response.on_hover_text(help);
                }
            });
        });
}

/// Keeps each font sample on its own bounded row.
pub(super) fn font_preview(ui: &mut egui::Ui, language: Language) {
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            let examples = [
                (
                    text(language, "settings-preview-regular"),
                    grid_font_family(false),
                ),
                (
                    text(language, "settings-preview-bold"),
                    grid_font_family(true),
                ),
                (
                    text(language, "settings-preview-mono"),
                    grid_mono_font_family(false),
                ),
                (
                    text(language, "settings-preview-mono-bold"),
                    grid_mono_font_family(true),
                ),
            ];
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = visual::SPACE_4;
                let sample_width = ((ui.available_width() - visual::SPACE_8) / 2.0).max(1.0);
                for pair in examples.chunks(2) {
                    ui.horizontal(|ui| {
                        for (label, family) in pair {
                            ui.allocate_ui_with_layout(
                                egui::vec2(sample_width, CHROME_CONTROL_HEIGHT),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    ui.horizontal_centered(|ui| {
                                        ui.label(label);
                                        ui.label(
                                            egui::RichText::new("Aあ12").font(FontId::new(
                                                visual::BODY_SIZE,
                                                family.clone(),
                                            )),
                                        );
                                    });
                                },
                            );
                        }
                    });
                }
            });
        });
}

/// Keeps menu controls within the same short width as their setting rows.
pub(super) fn control_width(ui: &egui::Ui) -> f32 {
    ui.available_width().min(SETTINGS_CONTROL_WIDTH)
}
