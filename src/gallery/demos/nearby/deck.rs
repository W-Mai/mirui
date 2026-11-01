use crate::anim::{Tween, TweenConfig, ease};
use crate::prelude::*;
use crate::types::Transform;

pub(super) const FULL: Fixed = Fixed::from_ratio(13, 16);
const MINIATURE: Fixed = Fixed::from_ratio(36, 100);
const SLIDE: TweenConfig = TweenConfig::new(540, &ease::IN_OUT_CUBIC);

#[model]
pub(super) struct Deck {
    #[observe]
    pub expansion: Fixed,
    #[observe]
    pub height: Fixed,
    target: Fixed,
    tween: Tween,
}

impl Default for Deck {
    fn default() -> Self {
        Self {
            expansion: Fixed::ZERO,
            height: Fixed::ZERO,
            target: Fixed::ZERO,
            tween: Tween::preset(Fixed::ZERO, Fixed::ZERO, SLIDE),
        }
    }
}

#[model]
impl Deck {
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
}
