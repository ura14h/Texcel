use super::*;
mod pages;
mod widgets;

use pages::*;
pub(in crate::ui) use widgets::settings_row;
use widgets::*;

const SETTINGS_MAX_WIDTH: f32 = 680.0;
const SETTINGS_CONTROL_WIDTH: f32 = 360.0;

impl TexcelApp {
    /// Shows the app preferences as one page with a fixed close row.
    pub(super) fn draw_settings_dialog(&mut self, ctx: &egui::Context) {
        if !self.settings_window_open {
            return;
        }
        let before_font = self.font_preset;
        let before_font_size = self.ui_font_size;
        let before_theme = self.theme;
        let before_language = self.language_choice;
        let before_invisibles = self.show_invisibles;
        let mut next_font_size = self.ui_font_size;
        let language = self.language;
        let content = ctx.content_rect();
        let max_width = (content.width() - 56.0).clamp(180.0, SETTINGS_MAX_WIDTH);
        // Leave room for the centered modal frame in shorter native windows.
        let max_height = (content.height() - visual::DIALOG_MARGIN * 2.0).max(180.0);
        let mut close_requested = false;
        let modal_id = Id::unique("texcel_appearance_settings");
        let close_key =
            self.dialog_key_pressed_in_modal(ctx, modal_id, Key::Escape, self.edit.composing);

        egui::Modal::new(modal_id).show(ctx, |ui| {
            // Modal frame and inner inset add forty points beyond this content width.
            ui.set_min_width((max_width.min(480.0) - 40.0).max(120.0));
            ui.set_max_width((max_width - 40.0).max(120.0));
            ui.set_min_height(max_height.min(320.0));
            ui.set_max_height(max_height);
            visual::dialog_surface(ui, |ui| {
                visual::prepare_dialog_body(ui);
                visual::dialog_title(ui, &text(language, "settings-title"));
                visual::group_gap(ui);
                let footer_height = visual::BUTTON_HEIGHT + visual::SPACE_8;
                let body_height = (ui.available_height() - footer_height).max(80.0);
                ui.spacing_mut().scroll.floating = false;
                ui.spacing_mut().scroll.bar_width = 6.0;
                egui::ScrollArea::vertical()
                    .id_salt("texcel_settings_body")
                    .scroll_bar_visibility(
                        egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                    )
                    .max_height(body_height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        draw_appearance_page(
                            ui,
                            LanguageControls {
                                current: language,
                                selected: &mut self.language_choice,
                            },
                            &mut self.font_preset,
                            &mut self.font_preview_details_open,
                            &mut self.theme,
                            &mut next_font_size,
                            &mut self.show_invisibles,
                        );
                    });
                ui.separator();
                visual::dialog_footer(ui, |ui| {
                    let close = ui.add_sized(
                        [112.0, visual::BUTTON_HEIGHT],
                        egui::Button::new(text(language, "action-close")),
                    );
                    close_requested |= close.clicked();
                });
            });
        });

        close_requested |= close_key;
        self.settings_window_open = !close_requested;
        if (next_font_size - before_font_size).abs() > 0.001 {
            self.set_ui_font_size(ctx, next_font_size);
        }
        if self.font_preset != before_font {
            self.mark_app_preference_change(AppPreferenceField::FontPreset);
        }
        if self.theme != before_theme {
            self.mark_app_preference_change(AppPreferenceField::Theme);
        }
        if self.language_choice != before_language {
            self.language = crate::i18n::resolve_language(self.language_choice);
            self.mark_app_preference_change(AppPreferenceField::Language);
            ctx.request_repaint();
        }
        if self.show_invisibles != before_invisibles {
            self.mark_app_preference_change(AppPreferenceField::ShowInvisibles);
        }
        self.apply_appearance(ctx);
        let other_appearance_changed = self.font_preset != before_font
            || self.theme != before_theme
            || self.language_choice != before_language
            || self.show_invisibles != before_invisibles;
        if other_appearance_changed && (next_font_size - before_font_size).abs() <= 0.001 {
            self.invalidate_shared_window_appearance(ctx);
        }
    }
}
