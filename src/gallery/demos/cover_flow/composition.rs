use super::state::{Carousel, CarouselCard, CoverFlowBounds, CoverFlowLayoutItem};
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
    res!().insert(bounds);

    let card_colors = [
        Color::rgb(238, 122, 112),
        Color::rgb(242, 184, 64),
        Color::rgb(146, 204, 110),
        Color::rgb(90, 140, 220),
        Color::rgb(68, 204, 188),
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
            CoverFlowLayoutItem,
            ScrollOffset {
                x: Fixed::from_int(initial_offset),
                y: Fixed::ZERO,
            },
            ScrollConfig {
                direction: ScrollAxis::Horizontal,
                elastic: true,
                content_width: Fixed::from_int(content_width),
                content_height: Fixed::ZERO,
                transition: None,
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
                    border_radius: 24,
                    align: AlignItems::Center,
                    justify: JustifyContent::Center
                ) [
                    CarouselCard { index: item.0 },
                    CoverFlowLayoutItem,
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
