use super::egui::{Context, FontId, RichText, TextStyle, Ui, Vec2};

pub(super) const DIALOG_TITLE_SIZE: f32 = 18.0;
pub(super) const PAGE_TITLE_SIZE: f32 = 16.0;
pub(super) const SECTION_TITLE_SIZE: f32 = 14.0;
pub(super) const CAPTION_SIZE: f32 = 12.0;
pub(super) const BODY_SIZE: f32 = 14.0;
pub(super) const SPACE_4: f32 = 4.0;
pub(super) const SPACE_8: f32 = 8.0;
pub(super) const SPACE_16: f32 = 16.0;
pub(super) const DIALOG_MARGIN: f32 = 20.0;
const DIALOG_FRAME_INSET: i8 = (DIALOG_MARGIN - 6.0) as i8;
pub(super) const BUTTON_HEIGHT: f32 = 32.0;
/// One context menu row, close to the native menu row height.
const CONTEXT_MENU_ROW_HEIGHT: f32 = 24.0;
/// The text inset on both sides of a context menu row.
const CONTEXT_MENU_SIDE_INSET: f32 = 14.0;

/// Gives app widgets shared text roles and spacing at every zoom level.
pub(super) fn configure_text_roles(ctx: &Context) {
    for theme in [super::egui::Theme::Light, super::egui::Theme::Dark] {
        ctx.style_mut_of(theme, |style| {
            style
                .text_styles
                .insert(TextStyle::Body, FontId::proportional(BODY_SIZE));
            style
                .text_styles
                .insert(TextStyle::Button, FontId::proportional(BODY_SIZE));
            style
                .text_styles
                .insert(TextStyle::Heading, FontId::proportional(PAGE_TITLE_SIZE));
            style
                .text_styles
                .insert(TextStyle::Small, FontId::proportional(CAPTION_SIZE));
            style
                .text_styles
                .insert(TextStyle::Monospace, FontId::monospace(BODY_SIZE));
            style.spacing.item_spacing = Vec2::splat(SPACE_8);
            style.spacing.button_padding = Vec2::new(12.0, 6.0);
            style.spacing.interact_size.y = BUTTON_HEIGHT;
            style.spacing.icon_spacing = SPACE_8;
            // Scroll areas stop at their ends. egui holds a rubber-band overscroll while it
            // thinks a finger rests on the trackpad, which looked frozen before snapping back.
            style.spacing.scroll.kinetic.rubber_band = false;
        });
    }
}

/// Gives a context menu native-like rows: compact height, no gaps, wider side insets.
pub(super) fn prepare_context_menu(ui: &mut Ui) {
    ui.spacing_mut().item_spacing.y = 0.0;
    ui.spacing_mut().button_padding = Vec2::new(CONTEXT_MENU_SIDE_INSET, 2.0);
    ui.spacing_mut().interact_size.y = CONTEXT_MENU_ROW_HEIGHT;
}

/// Shows a dialog's main title with the shared title role.
pub(super) fn dialog_title(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).size(DIALOG_TITLE_SIZE).strong());
}

/// Gives a dialog body the same left-aligned spacing as the settings page.
pub(super) fn prepare_dialog_body(ui: &mut Ui) {
    ui.spacing_mut().item_spacing = Vec2::splat(SPACE_8);
    ui.spacing_mut().button_padding = Vec2::new(12.0, 6.0);
    ui.spacing_mut().interact_size.y = BUTTON_HEIGHT;
    ui.spacing_mut().icon_spacing = SPACE_8;
}

/// Adds the shared inset inside egui's six-point modal frame.
pub(super) fn dialog_surface<R>(
    ui: &mut Ui,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> super::egui::InnerResponse<R> {
    super::egui::Frame::new()
        .inner_margin(super::egui::Margin::same(DIALOG_FRAME_INSET))
        .show(ui, add_contents)
}

/// Reserves one compact row for actions and keeps the group at the right edge.
pub(super) fn dialog_footer<R>(
    ui: &mut Ui,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> super::egui::InnerResponse<R> {
    let size = Vec2::new(ui.available_width(), BUTTON_HEIGHT);
    ui.allocate_ui_with_layout(
        size,
        super::egui::Layout::right_to_left(super::egui::Align::Center),
        add_contents,
    )
}

/// Shows a fixed-size dialog action and marks its primary choice.
pub(super) fn dialog_button(ui: &mut Ui, label: String, width: f32, primary: bool) -> bool {
    let button = super::egui::Button::new(label);
    let button = if primary {
        button.fill(ui.visuals().selection.bg_fill)
    } else {
        button
    };
    ui.add_sized([width, BUTTON_HEIGHT], button).clicked()
}

/// Adds a small group break without inventing a decorative divider.
pub(super) fn group_gap(ui: &mut Ui) {
    ui.add_space(SPACE_16);
}

/// Lets only the top modal handle one shortcut while a popup is closed.
pub(super) fn dialog_key_pressed_in_modal(
    ctx: &Context,
    modal_id: super::egui::Id,
    key: super::egui::Key,
    composing: bool,
) -> bool {
    if !modal_can_handle_key(ctx, modal_id, composing) {
        return false;
    }
    ctx.input_mut(|input| input.consume_key(super::egui::Modifiers::NONE, key))
}

/// Lets a focused modal editor handle its platform command shortcut.
pub(super) fn dialog_command_key_pressed_in_modal(
    ctx: &Context,
    modal_id: super::egui::Id,
    key: super::egui::Key,
    composing: bool,
) -> bool {
    if !modal_can_handle_key(ctx, modal_id, composing)
        || !ctx.input(|input| input.modifiers.command)
    {
        return false;
    }
    ctx.input_mut(|input| input.consume_key(input.modifiers, key))
}

/// Confirms that a modal editor is the one receiving keys this frame.
fn modal_can_handle_key(ctx: &Context, modal_id: super::egui::Id, composing: bool) -> bool {
    let expected = super::egui::LayerId::new(super::egui::Order::Foreground, modal_id);
    let topmost = ctx.memory(|memory| {
        memory
            .top_modal_layer()
            .is_none_or(|layer| layer == expected)
    });
    !composing && topmost && !ctx.any_popup_open() && !has_ime_event(ctx)
}

/// Keeps dialog commands out of frames owned by an IME.
fn has_ime_event(ctx: &Context) -> bool {
    ctx.input(|input| {
        input
            .events
            .iter()
            .any(|event| matches!(event, super::egui::Event::Ime(_)))
    })
}

/// Shows a settings page title with the shared page role.
pub(super) fn page_title(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).size(PAGE_TITLE_SIZE).strong());
}

/// Shows one related group label and returns it for accessible control names.
pub(super) fn section_title(ui: &mut Ui, title: &str) -> super::egui::Response {
    ui.label(RichText::new(title).size(SECTION_TITLE_SIZE).strong())
}

/// Shows a short supporting note without shrinking it below the Japanese-safe size.
pub(super) fn caption(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .size(CAPTION_SIZE)
            .color(ui.visuals().weak_text_color()),
    );
}
