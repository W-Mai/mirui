use crate::anim::{BOUNCY, PlayMode, SMOOTH, Spring, Tween, ease};
use crate::prelude::*;
use crate::ui::widgets::icon::Icon;

mirui_macros::animate!(IconScale, |world, entity, value| {
    if let Some(icon) = world.get_mut::<Icon>(entity) {
        icon.scale = value;
    }
    world.invalidate(entity);
});

pub(super) fn beat() -> IconScale {
    IconScale(
        Tween::new(
            Fixed::ONE,
            Fixed::from_f32(1.4),
            600,
            ease::ease_in_out_cubic,
            PlayMode::PingPong,
        )
        .into(),
    )
}

pub(super) fn breathe() -> IconScale {
    IconScale(
        Spring::preset(Fixed::ONE, Fixed::from_f32(1.25), SMOOTH)
            .repeat()
            .into(),
    )
}

pub(super) fn bounce() -> IconScale {
    IconScale(
        Spring::preset(Fixed::from_f32(0.8), Fixed::from_f32(1.2), BOUNCY)
            .repeat()
            .into(),
    )
}
