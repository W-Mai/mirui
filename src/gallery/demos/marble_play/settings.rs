use super::style::{BOARD_HEIGHT, BOARD_WIDTH, TEXT, color, mix};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::marble::{MarbleModel, Theme};
use crate::gallery::play::paint::PlayPainter;
use crate::prelude::*;
use crate::render::renderer::Renderer;
use crate::types::Fixed64;
use crate::ui::view::ViewCtx;
use crate::ui::widgets::{ParagraphStyle, Slider, Switch, Text, TextAlign};

#[crate::component(bind(model))]
struct MarbleSettingsPanel {
    model: MarbleModel,
}

fn paint_settings(painter: &mut PlayPainter<'_, '_>, theme: Theme) {
    painter.fill(
        Rect::new(10, 0, 460, 199),
        color(theme.bg),
        Fixed::from_int(10),
    );
    painter.fill(
        Rect::new(24, 18, 432, 163),
        color(theme.panel),
        Fixed::from_int(9),
    );
    for y in [63, 113, 151] {
        painter.line(
            Point::new(42, y),
            Point::new(422, y),
            mix(theme.panel, 0xa1b887, Fixed::from_ratio(18, 100)),
            Fixed::ONE,
        );
    }
}

#[crate::view(
    component = MarbleSettingsPanel,
    read(model),
    watch(model.scene()),
    name = "MarbleSettingsPanel",
    priority = 60
)]
pub(super) fn settings_panel_render(
    renderer: &mut dyn Renderer,
    model: &MarbleModel,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, BOARD_WIDTH, BOARD_HEIGHT);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    paint_settings(&mut painter, model.theme());
}

#[compose(bind(model))]
pub(super) fn marble_settings_page(model: MarbleModel) -> Entity {
    ui! {
        View (
            id: "marble_settings_panel",
            width: BOARD_WIDTH,
            height: BOARD_HEIGHT,
            clip_children: true
        ) [
            MarbleSettingsPanel {
                model: model.clone(),
            },
        ] {
            Text (
                "GRAVITY",
                id: "marble_setting_gravity",
                position: Position::Absolute,
                left: 42,
                top: 15,
                width: 100,
                height: 16,
                font_size: 8,
                text_color: TEXT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                text: ${ format_args!("TEMPO · {} BPM", model.bpm()) },
                text_capacity: 16,
                id: "marble_setting_bpm",
                position: Position::Absolute,
                left: 42,
                top: 65,
                width: 180,
                height: 16,
                font_size: 8,
                text_color: TEXT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "TRAILS · 6 POINTS PER MARBLE",
                id: "marble_setting_trails",
                position: Position::Absolute,
                left: 42,
                top: 122,
                width: 260,
                height: 18,
                font_size: 8,
                text_color: TEXT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "FEEDBACK · RINGS / PARTICLES",
                id: "marble_setting_feedback",
                position: Position::Absolute,
                left: 42,
                top: 157,
                width: 260,
                height: 18,
                font_size: 8,
                text_color: TEXT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Slider (
                id: "marble_setting_gravity_control",
                position: Position::Absolute,
                left: 42,
                top: 29,
                width: 380,
                height: 28,
                min: Fixed::ZERO,
                max: Fixed::from_ratio(16, 10),
                value: ${ model.gravity().to_fixed() },
                track_color: Color::rgb(69, 87, 70),
                fill_color: Color::rgb(217, 248, 138),
                thumb_color: Color::rgb(225, 233, 214)
            ) on ValueChanged { model.set_gravity(Fixed64::from_fixed(*new)); }
            Slider (
                id: "marble_setting_bpm_control",
                position: Position::Absolute,
                left: 42,
                top: 79,
                width: 380,
                height: 28,
                min: Fixed::from_int(55),
                max: Fixed::from_int(160),
                value: ${ Fixed::from_int(i32::from(model.bpm())) },
                track_color: Color::rgb(69, 87, 70),
                fill_color: Color::rgb(198, 176, 239),
                thumb_color: Color::rgb(225, 233, 214)
            ) on ValueChanged { model.set_bpm(Fixed64::from_fixed(*new)); }
            Switch (
                id: "marble_setting_trails_control",
                position: Position::Absolute,
                left: 370,
                top: 121,
                width: 54,
                height: 24,
                on: ${ model.trails() },
                on_color: Color::rgb(217, 248, 138),
                off_color: Color::rgb(69, 87, 70),
                thumb_color: Color::rgb(23, 34, 28)
            ) on Toggled {
                if model.trails() != *now {
                    model.toggle_trails();
                }
            }
            Switch (
                id: "marble_setting_feedback_control",
                position: Position::Absolute,
                left: 370,
                top: 156,
                width: 54,
                height: 24,
                on: ${ model.feedback() },
                on_color: Color::rgb(217, 248, 138),
                off_color: Color::rgb(69, 87, 70),
                thumb_color: Color::rgb(23, 34, 28)
            ) on Toggled {
                if model.feedback() != *now {
                    model.toggle_feedback();
                }
            }
        }
    }
}
