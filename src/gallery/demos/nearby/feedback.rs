use super::motion::timing;
use crate::anim::{PlayMode, Tween, ease};
use crate::prelude::*;

pub struct Feedback {
    press: Tween,
    activity: [Tween; 3],
    last_ms: u64,
    pub press_sequence: u64,
}

impl Default for Feedback {
    fn default() -> Self {
        Self {
            press: Tween::ease_to(Fixed::ZERO, Fixed::ZERO, timing::PRESS_FADE_MS),
            activity: core::array::from_fn(|index| {
                let mut tween = Tween::new(
                    Fixed::ZERO,
                    Fixed::ONE,
                    timing::ACTIVITY_MS,
                    &ease::LINEAR,
                    PlayMode::PingPong,
                );
                tween.elapsed_ms = index as u16 * timing::ACTIVITY_STAGGER_MS;
                tween
            }),
            last_ms: 0,
            press_sequence: 0,
        }
    }
}

impl Feedback {
    pub fn press(&mut self, now: u64) {
        self.press_sequence = self.press_sequence.wrapping_add(1);
        self.last_ms = now;
        self.press = Tween::ease_to(Fixed::ONE, Fixed::ZERO, timing::PRESS_FADE_MS);
    }

    pub fn sample(
        &mut self,
        now: u64,
        loading: bool,
        theme: &crate::ui::theme::Theme,
    ) -> (Color, [Color; 3]) {
        let delta = now.saturating_sub(self.last_ms).min(u64::from(u16::MAX)) as u16;
        self.last_ms = now;
        self.press.tick(delta);
        let press = self.press.value().to_f32();
        let stamp = theme.resolve(if press > 0.0 {
            ColorToken::Primary
        } else {
            ColorToken::Custom("sky")
        });
        let accent = theme.resolve(ColorToken::Primary);
        let activity = core::array::from_fn(|index| {
            self.activity[index].tick(delta);
            if loading {
                Color::rgba(
                    accent.r,
                    accent.g,
                    accent.b,
                    (48.0 + self.activity[index].value().to_f32() * 207.0) as u8,
                )
            } else {
                theme.resolve(ColorToken::Surface)
            }
        });
        (stamp, activity)
    }
}
