use super::catalog::{TOKEN_CJK, TRANSLATIONS, register_font};
use crate::core::i18n::{I18n, Locale};
use crate::prelude::*;
use crate::render::font::FontManager;
use crate::t;
use crate::ui::widgets::{Button, ParagraphStyle, Text};

#[compose]
pub fn build_widgets() {
    res!(optional FontManager).with(register_font);
    let i18n = I18n::new(Locale::EnUs).with_translations(TRANSLATIONS);
    let locale = i18n.locale_signal();
    res!().insert(i18n);

    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(20),
            justify: JustifyContent::Center,
            align: AlignItems::Center,
            row_gap: 10
        ) {
            Text (
                t!("welcome"),
                width: Dimension::percent(100),
                max_width: 300,
                height: 48,
                font: TOKEN_CJK,
                font_size: 22,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label()
            )
            Text (
                t!("greeting"),
                width: Dimension::percent(100),
                max_width: 300,
                height: 34,
                font: TOKEN_CJK,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label()
            )
            Text (
                t!("goodbye"),
                width: Dimension::percent(100),
                max_width: 300,
                height: 34,
                font: TOKEN_CJK,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label()
            )
            Button (
                height: 44,
                width: Dimension::percent(100),
                max_width: 240,
                normal_color: ColorToken::Primary,
                pressed_color: ColorToken::Secondary,
                text_color: ColorToken::OnPrimary,
                border_radius: 12,
                font: TOKEN_CJK
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
    app.compose(parent, build_widgets);
}
