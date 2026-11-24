use super::catalog::{TOKEN_CJK, TRANSLATIONS, register_font};
use crate::core::i18n::{I18n, Locale};
use crate::gallery::sample_style::{centered_label, centered_sample, primary_action};
use crate::prelude::*;
use crate::render::font::FontManager;
use crate::t;
use crate::ui::widgets::{Button, Text};

#[compose]
pub fn build_widgets() {
    res!(FontManager).with(|manager| register_font(&manager));
    let i18n = I18n::new(Locale::EnUs).with_translations(TRANSLATIONS);
    let locale = i18n.locale_signal();
    res!().insert(i18n);

    //~focus-start
    ui! {
        Column (
            style: centered_sample(),
            row_gap: 8,
            bg_color: ColorToken::Surface
        ) {
            Text (
                t!("welcome"),
                style: centered_label(),
                max_width: 300,
                height: 48,
                font: TOKEN_CJK,
                font_size: 26,
                text_color: ColorToken::OnSurface
            )
            Text (
                t!("greeting"),
                style: centered_label(),
                max_width: 300,
                height: 30,
                font: TOKEN_CJK,
                font_size: 16,
                text_color: ColorToken::OnSurfaceVariant
            )
            Text (
                t!("goodbye"),
                style: centered_label(),
                max_width: 300,
                height: 30,
                font: TOKEN_CJK,
                font_size: 16,
                text_color: ColorToken::OnSurfaceVariant
            )
            Button (
                style: primary_action(),
                width: Dimension::percent(100),
                max_width: 240,
                font: TOKEN_CJK,
                font_size: 16
            ) [
                Text::label(t!("toggle")),
            ] on Tap {
                let next = if locale.get_untracked() == Locale::EnUs {
                    Locale::ZhCn
                } else {
                    Locale::EnUs
                };
                locale.set(next);
            }
        }
    };
    //~focus-end
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    crate::gallery::showcase_theme::install_sample(app);
    app.compose(parent, build_widgets());
}
