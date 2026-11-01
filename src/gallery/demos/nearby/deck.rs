use crate::anim::{Tween, TweenConfig, ease};
use crate::prelude::*;
use crate::types::Transform;

pub(super) const FULL: Fixed = Fixed::from_ratio(13, 16);
const MINIATURE: Fixed = Fixed::from_ratio(36, 100);
const SLIDE: TweenConfig = TweenConfig::new(540, &ease::IN_OUT_CUBIC);
const PANEL: TweenConfig = TweenConfig::new(320, &ease::OUT_CUBIC);

#[model]
pub(super) struct Deck {
    #[observe]
    pub expansion: Fixed,
    #[observe]
    pub height: Fixed,
    #[observe]
    pub panel: Fixed,
    panel_target: Fixed,
    panel_tween: Tween,
    target: Fixed,
    tween: Tween,
}

impl Default for Deck {
    fn default() -> Self {
        Self {
            expansion: Fixed::ZERO,
            height: Fixed::ZERO,
            panel: Fixed::ZERO,
            panel_target: Fixed::ZERO,
            panel_tween: Tween::preset(Fixed::ZERO, Fixed::ZERO, PANEL),
            target: Fixed::ZERO,
            tween: Tween::preset(Fixed::ZERO, Fixed::ZERO, SLIDE),
        }
    }
}

#[model]
impl Deck {
    pub fn advance_panel(&mut self, open: bool, delta: u16) {
        let target = if open { Fixed::ONE } else { Fixed::ZERO };
        if target != self.panel_target {
            self.panel_target = target;
            self.panel_tween = Tween::preset(self.panel, target, PANEL);
        }
        self.panel_tween.tick(delta);
        let value = self.panel_tween.value();
        if value != self.panel {
            self.panel = value;
        }
    }
    pub fn set_height(&mut self, height: Fixed) {
        if self.height != height {
            self.height = height;
        }
    }
    pub fn advance(&mut self, expanded: bool, delta: u16, paused: bool) {
        if paused {
            return;
        }
        let target = if expanded { Fixed::ONE } else { Fixed::ZERO };
        if self.target != target {
            self.target = target;
            self.tween = Tween::preset(self.expansion, target, SLIDE);
        }
        self.tween.tick(delta);
        let value = self.tween.value();
        if self.expansion != value {
            self.expansion = value;
        }
    }
}

pub(super) fn catalog_share(expansion: Fixed) -> Fixed {
    FULL * expansion
}
pub(super) fn shelf_share(expansion: Fixed) -> Fixed {
    Fixed::ONE - catalog_share(expansion)
}
pub(super) fn transform(expansion: Fixed, height: Fixed) -> Transform {
    let scale = MINIATURE + (Fixed::ONE - MINIATURE) * expansion;
    Transform::translate(Fixed::ZERO, (scale - Fixed::ONE) * height / 2)
        .compose(&Transform::scale(scale, scale))
}
pub(super) fn shelf_transform(expansion: Fixed, height: Fixed) -> Transform {
    let scale = shelf_share(expansion);
    Transform::translate(Fixed::ZERO, (scale - Fixed::ONE) * height / 2)
        .compose(&Transform::scale(scale, scale))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reversing_the_deck_keeps_position_and_retained_viewport_shares() {
        let mut deck = Deck::default();
        deck.advance(true, 200, false);
        let middle = deck.expansion;
        assert!(middle > Fixed::ZERO && middle < Fixed::ONE);
        deck.advance(false, 0, false);
        assert_eq!(deck.expansion, middle);
        deck.advance(false, 540, true);
        assert_eq!(deck.expansion, middle);
        deck.advance(false, 540, false);
        assert_eq!(deck.expansion, Fixed::ZERO);
        for i in 0..=16 {
            let value = Fixed::from_ratio(i, 16);
            assert_eq!(shelf_share(value) + catalog_share(value), Fixed::ONE);
        }
    }

    #[test]
    fn room_panel_retargets_from_its_current_progress() {
        let mut deck = Deck::default();
        deck.advance_panel(true, 100);
        let progress = deck.panel;
        assert!(progress > Fixed::ZERO && progress < Fixed::ONE);
        deck.advance_panel(false, 0);
        assert_eq!(deck.panel, progress);
        deck.advance_panel(false, 100);
        let closing = deck.panel;
        deck.advance_panel(true, 0);
        assert_eq!(deck.panel, closing);
        deck.advance_panel(true, 320);
        assert_eq!(deck.panel, Fixed::ONE);
        deck.advance_panel(false, 320);
        assert_eq!(deck.panel, Fixed::ZERO);
    }
}
