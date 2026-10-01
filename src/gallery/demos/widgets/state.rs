use crate::prelude::{Color, ColorToken};
use crate::ui::Theme;

pub const ACCENT: ColorToken = ColorToken::custom("accent");

#[crate::component]
pub(super) struct FormSlider;
#[crate::component]
pub(super) struct FormProgress;

#[derive(Default)]
#[cfg(feature = "std")]
#[crate::model]
pub(super) struct ThemeCycle {
    index: u8,
}

pub fn dark_with_accent() -> Theme {
    Theme::dark().with(ACCENT, Color::rgb(255, 200, 60))
}

pub fn light_with_accent() -> Theme {
    Theme::light().with(ACCENT, Color::rgb(220, 60, 90))
}

pub fn custom_theme() -> Theme {
    Theme::dark().with_many([
        (ColorToken::Primary, Color::rgb(255, 105, 180)),
        (ColorToken::OnPrimary, Color::rgb(20, 20, 30)),
        (ColorToken::Success, Color::rgb(255, 200, 60)),
        (ColorToken::Surface, Color::rgb(38, 28, 50)),
        (ColorToken::SurfaceVariant, Color::rgb(70, 50, 90)),
        (ColorToken::OnSurface, Color::rgb(245, 235, 255)),
        (ColorToken::OnSurfaceVariant, Color::rgb(180, 150, 200)),
        (ACCENT, Color::rgb(140, 200, 220)),
    ])
}

#[cfg(feature = "std")]
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

#[cfg(feature = "std")]
#[mirui_macros::timer(every = 3_000)]
pub(super) fn cycle(theme_cycle: model!(ThemeCycle), commands: commands!()) {
    theme_cycle.advance();
    commands.set_theme(theme_cycle.current_theme());
}
