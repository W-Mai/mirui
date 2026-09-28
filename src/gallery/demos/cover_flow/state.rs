use crate::prelude::Fixed;

pub(super) const CARD_COUNT: i32 = 5;

#[crate::component]
pub struct CarouselCard {
    pub index: usize,
}

#[crate::component]
pub struct Carousel;

pub struct CoverFlowBounds {
    pub view_w: i32,
    pub view_h: i32,
    pub card_w: i32,
    pub card_h: i32,
    pub card_gap: i32,
    pub perspective: i32,
}

impl CoverFlowBounds {
    pub(super) fn for_view(view_w: u16, view_h: u16) -> Self {
        Self::for_px(view_w as i32, view_h as i32)
    }

    pub(super) fn for_px(vw: i32, vh: i32) -> Self {
        Self {
            view_w: vw,
            view_h: vh,
            card_w: vw * 7 / 32,
            card_h: vh / 2,
            card_gap: vw / 8,
            perspective: vw.max(vh) * 25 / 64,
        }
    }

    pub(super) fn content_width(&self) -> i32 {
        self.view_w + (self.card_w + self.card_gap) * (CARD_COUNT - 1)
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct LayoutCache {
    pub(super) view_w: i32,
    pub(super) view_h: i32,
    pub(super) offset: Fixed,
}
