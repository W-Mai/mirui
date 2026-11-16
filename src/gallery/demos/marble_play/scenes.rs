use super::style::{BOARD_HEIGHT, BOARD_WIDTH, MUTED, TEXT, color, mix};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::marble::{MarbleModel, PALETTE, THEMES};
use crate::gallery::play::paint::PlayPainter;
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

const SCENE_CARD_LEFT: i32 = 18;
const SCENE_CARD_TOP: i32 = 24;
const SCENE_CARD_STRIDE: i32 = 151;
const SCENE_CARD_WIDTH: i32 = 140;
const SCENE_CARD_HEIGHT: i32 = 216;
/// The small board drawn at the top of each card.
const PREVIEW_INSET: i32 = 10;
const PREVIEW_HEIGHT: i32 = 128;
/// Card-relative top of the scene's name and of its gravity line.
const NAME_TOP: i32 = 148;
const SUBTITLE_TOP: i32 = 176;

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

#[crate::component]
struct MarbleSceneBoard {
    model: model!(MarbleModel),
}

fn paint_scene_cards(painter: &mut PlayPainter<'_, '_>, selected: usize) {
    painter.fill(
        Rect::new(10, 0, 460, BOARD_HEIGHT),
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
        let top = SCENE_CARD_TOP + PREVIEW_INSET;
        painter.fill(
            Rect::new(
                x + PREVIEW_INSET,
                top,
                SCENE_CARD_WIDTH - 2 * PREVIEW_INSET,
                PREVIEW_HEIGHT,
            ),
            color(theme.bg),
            Fixed::from_int(6),
        );
        for dot in 0..3 {
            painter.circle(
                Point::new(x + 37 + dot * 33, top + 42 + (dot % 2) * 34),
                Fixed::from_int(10),
                color(PALETTE[(index + dot as usize) % 5]),
            );
        }
        painter.line(
            Point::new(x + 22, top + PREVIEW_HEIGHT - 12),
            Point::new(x + SCENE_CARD_WIDTH - 22, top + PREVIEW_HEIGHT - 12),
            mix(theme.bg, theme.accent, Fixed::from_ratio(30, 100)),
            Fixed::from_int(2),
        );
        painter.fill(
            Rect::new(x + 14, SCENE_CARD_TOP + NAME_TOP + 18, 24, 2),
            color(theme.accent),
            Fixed::ONE,
        );
    }
}

#[crate::view(
    component = MarbleSceneBoard,
    name = "MarbleSceneBoard",
    priority = 60
)]
pub(super) fn scene_board_render(model: model!(component.model, watch(scene)), paint: paint!()) {
    paint.set_background_handled(true);
    let transform = fit_logical_canvas(paint.rect(), paint.transform(), BOARD_WIDTH, BOARD_HEIGHT);
    let mut painter = PlayPainter::new(paint, transform);
    paint_scene_cards(&mut painter, model.scene);
}

#[compose]
pub(super) fn marble_scenes_page(model: model!(MarbleModel)) -> Entity {
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
            ] {
                Text (
                    THEMES[0].name,
                    id: "marble_scene_0_name",
                    position: Position::Absolute,
                    left: 14,
                    top: NAME_TOP,
                    width: 112,
                    height: 14,
                    font_size: 10,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    THEMES[0].subtitle,
                    id: "marble_scene_0_sub",
                    position: Position::Absolute,
                    left: 14,
                    top: SUBTITLE_TOP,
                    width: 112,
                    height: 13,
                    font_size: 10,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
            } on Tap { model.reset_scene(0); }
            View (
                id: "marble_scene_card_1",
                position: Position::Absolute,
                left: scene_card_x(1),
                top: SCENE_CARD_TOP,
                width: SCENE_CARD_WIDTH,
                height: SCENE_CARD_HEIGHT
            ) [
                TouchAction::None,
            ] {
                Text (
                    THEMES[1].name,
                    id: "marble_scene_1_name",
                    position: Position::Absolute,
                    left: 14,
                    top: NAME_TOP,
                    width: 112,
                    height: 14,
                    font_size: 10,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    THEMES[1].subtitle,
                    id: "marble_scene_1_sub",
                    position: Position::Absolute,
                    left: 14,
                    top: SUBTITLE_TOP,
                    width: 112,
                    height: 13,
                    font_size: 10,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
            } on Tap { model.reset_scene(1); }
            View (
                id: "marble_scene_card_2",
                position: Position::Absolute,
                left: scene_card_x(2),
                top: SCENE_CARD_TOP,
                width: SCENE_CARD_WIDTH,
                height: SCENE_CARD_HEIGHT
            ) [
                TouchAction::None,
            ] {
                Text (
                    THEMES[2].name,
                    id: "marble_scene_2_name",
                    position: Position::Absolute,
                    left: 14,
                    top: NAME_TOP,
                    width: 112,
                    height: 14,
                    font_size: 10,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    THEMES[2].subtitle,
                    id: "marble_scene_2_sub",
                    position: Position::Absolute,
                    left: 14,
                    top: SUBTITLE_TOP,
                    width: 112,
                    height: 13,
                    font_size: 10,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
            } on Tap { model.reset_scene(2); }
        }
    }
}
