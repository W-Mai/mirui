use super::style::{BOARD_HEIGHT, BOARD_WIDTH, MUTED, color, mix};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::marble::{MarbleModel, THEMES};
use crate::gallery::play::paint::PlayPainter;
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::render::renderer::Renderer;
use crate::ui::view::ViewCtx;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

const SCENE_CARD_LEFT: i32 = 18;
const SCENE_CARD_TOP: i32 = 18;
const SCENE_CARD_STRIDE: i32 = 151;
const SCENE_CARD_WIDTH: i32 = 140;
const SCENE_CARD_HEIGHT: i32 = 163;

const fn scene_card_x(index: usize) -> i32 {
    SCENE_CARD_LEFT + index as i32 * SCENE_CARD_STRIDE
}

fn scene_card_rect(index: usize) -> Rect {
    Rect::new(
        scene_card_x(index),
        SCENE_CARD_TOP,
        SCENE_CARD_WIDTH,
        SCENE_CARD_HEIGHT,
    )
}

#[crate::component(bind(model))]
struct MarbleSceneBoard {
    model: MarbleModel,
}

fn paint_scene_cards(painter: &mut PlayPainter<'_, '_>, selected: usize) {
    painter.fill(
        Rect::new(10, 0, 460, 199),
        color(THEMES[selected].bg),
        Fixed::from_int(10),
    );
    for (index, theme) in THEMES.into_iter().enumerate() {
        let x = scene_card_x(index);
        let area = scene_card_rect(index);
        painter.fill(area, color(theme.panel), Fixed::from_int(8));
        painter.border(
            area,
            if index == selected {
                color(theme.accent)
            } else {
                mix(theme.bg, 0x90a679, Fixed::from_ratio(25, 100))
            },
            Fixed::ONE,
            Fixed::from_int(8),
        );
        painter.fill(
            Rect::new(x + 10, 29, 120, 72),
            color(theme.bg),
            Fixed::from_int(6),
        );
        for dot in 0..3 {
            painter.circle(
                Point::new(x + 37 + dot * 29, 57 + (dot % 2) * 17),
                Fixed::from_int(7),
                color(crate::gallery::play::marble::PALETTE[(index + dot as usize) % 5]),
            );
        }
        painter.fill(
            Rect::new(x + 14, 108, 18 + theme.name.len() as i32 * 4, 2),
            color(theme.accent),
            Fixed::ONE,
        );
        painter.fill(
            Rect::new(x + 14, 160, 12 + theme.subtitle.len() as i32 * 3, 2),
            MUTED,
            Fixed::ONE,
        );
    }
}

#[crate::view(
    component = MarbleSceneBoard,
    read(model),
    watch(model.scene()),
    name = "MarbleSceneBoard",
    priority = 60
)]
pub(super) fn scene_board_render(
    renderer: &mut dyn Renderer,
    model: &MarbleModel,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, BOARD_WIDTH, BOARD_HEIGHT);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    paint_scene_cards(&mut painter, model.scene);
}

#[compose(bind(model))]
pub(super) fn marble_scenes_page(model: MarbleModel) -> Entity {
    ui! {
        View (
            id: "marble_scene_board",
            width: BOARD_WIDTH,
            height: BOARD_HEIGHT,
            clip_children: true
        ) [
            MarbleSceneBoard {
                model: model.clone(),
            },
        ] {
            View (
                id: "marble_scene_card_0",
                position: Position::Absolute,
                left: scene_card_x(0),
                top: SCENE_CARD_TOP,
                width: SCENE_CARD_WIDTH,
                height: SCENE_CARD_HEIGHT
            ) [
                TouchAction::None,
            ] on Tap { model.reset_scene(0); }
            {
                Text (
                    "DAYDREAM",
                    id: "marble_scene_0_name",
                    position: Position::Absolute,
                    left: 14,
                    top: 100,
                    width: 112,
                    height: 18,
                    font_size: 8,
                    text_color: Color::rgb(225, 233, 214),
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "SOFT GREEN",
                    id: "marble_scene_0_sub",
                    position: Position::Absolute,
                    left: 14,
                    top: 121,
                    width: 112,
                    height: 16,
                    font_size: 7,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
            }
            View (
                id: "marble_scene_card_1",
                position: Position::Absolute,
                left: scene_card_x(1),
                top: SCENE_CARD_TOP,
                width: SCENE_CARD_WIDTH,
                height: SCENE_CARD_HEIGHT
            ) [
                TouchAction::None,
            ] on Tap { model.reset_scene(1); }
            {
                Text (
                    "AFTER HOURS",
                    id: "marble_scene_1_name",
                    position: Position::Absolute,
                    left: 14,
                    top: 100,
                    width: 112,
                    height: 18,
                    font_size: 8,
                    text_color: Color::rgb(225, 233, 214),
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "BLUE GREY",
                    id: "marble_scene_1_sub",
                    position: Position::Absolute,
                    left: 14,
                    top: 121,
                    width: 112,
                    height: 16,
                    font_size: 7,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
            }
            View (
                id: "marble_scene_card_2",
                position: Position::Absolute,
                left: scene_card_x(2),
                top: SCENE_CARD_TOP,
                width: SCENE_CARD_WIDTH,
                height: SCENE_CARD_HEIGHT
            ) [
                TouchAction::None,
            ] on Tap { model.reset_scene(2); }
            {
                Text (
                    "ZERO GRAVITY",
                    id: "marble_scene_2_name",
                    position: Position::Absolute,
                    left: 14,
                    top: 100,
                    width: 112,
                    height: 18,
                    font_size: 8,
                    text_color: Color::rgb(225, 233, 214),
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "COOL BLUE",
                    id: "marble_scene_2_sub",
                    position: Position::Absolute,
                    left: 14,
                    top: 121,
                    width: 112,
                    height: 16,
                    font_size: 7,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
            }
        }
    }
}
