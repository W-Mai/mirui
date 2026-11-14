use crate::prelude::{Color, ColorToken};
use crate::ui::Theme;

pub const ACCENT: ColorToken = ColorToken::custom("accent");

#[crate::component]
pub(super) struct FormSlider;
#[crate::component]
pub(super) struct FormProgress;

#[derive(Default)]
#[crate::model]
pub(super) struct ThemeCycle {
    index: u8,
}

pub fn dark_with_accent() -> Theme {
    crate::gallery::demos::theme_swap::dark_with_accent().with(ACCENT, Color::rgb(255, 200, 60))
}

pub fn light_with_accent() -> Theme {
    crate::gallery::demos::theme_swap::light_with_accent().with(ACCENT, Color::rgb(220, 60, 90))
}

pub fn custom_theme() -> Theme {
    crate::gallery::demos::theme_swap::custom_theme().with(ACCENT, Color::rgb(140, 200, 220))
}

#[crate::model]
impl ThemeCycle {
    fn advance(&mut self) {
        self.index = (self.index + 1) % 3;
    }

    fn current_theme(&self) -> Theme {
        match self.index {
            0 => dark_with_accent(),
            1 => light_with_accent(),
            _ => custom_theme(),
        }
    }
}

#[mirui_macros::timer(every = 3_000)]
pub(super) fn cycle(theme_cycle: model!(ThemeCycle), commands: commands!()) {
    theme_cycle.advance();
    commands.set_theme(theme_cycle.current_theme());
}
