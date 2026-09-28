use super::state::{Carousel, CarouselCard, CoverFlowBounds};
use crate::input::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use crate::prelude::*;
use crate::ui::widgets::Image;

#[compose]
pub fn build_widgets(view_w: u16, view_h: u16) {
    let bounds = CoverFlowBounds::for_view(view_w, view_h);
    let vw = bounds.view_w;
    let vh = bounds.view_h;
    let card_w = bounds.card_w;
    let card_h = bounds.card_h;
    let content_width = bounds.content_width();
    let initial_offset = (content_width - vw) / 2 - card_w / 4;
    cx.world_mut().insert_resource(bounds);

    let card_colors = [
        Color::rgb(255, 107, 107),
        Color::rgb(255, 206, 84),
        Color::rgb(136, 216, 176),
        Color::rgb(118, 209, 244),
        Color::rgb(178, 148, 255),
    ];

    let card_colors_ref = &card_colors;
    //~focus-start
    ui! {
        View (
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: vw,
            height: vh,
            bg_color: ColorToken::Surface
        ) [
            Carousel,
            ScrollOffset {
                x: Fixed::from_int(initial_offset),
                y: Fixed::ZERO,
            },
            ScrollConfig {
                direction: ScrollAxis::Horizontal,
                elastic: true,
                content_width: Fixed::from_int(content_width),
                content_height: Fixed::ZERO,
            },
        ] {
            walk card_colors_ref.iter().enumerate() with item {
                View (
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: card_w,
                    height: card_h,
                    bg_color: *item.1,
                    border_radius: 8,
                    border_color: ColorToken::Outline,
                    border_width: 5,
                    align: AlignItems::Center,
                    justify: JustifyContent::Center
                ) [
                    CarouselCard { index: item.0 },
                ] {
                    if item.0 % 2 == 1 {
                        View (
                            width: 64,
                            height: 64,
                            image: Image::new("thumbs_up")
                        )
                    }
                }
            }
        }
    };
    //~focus-end
}
