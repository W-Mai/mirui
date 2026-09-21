extern crate alloc;

use crate::core::i18n::{I18n, Locale, Translation};
use crate::prelude::*;
use crate::render::font::{Font, FontManager};
use crate::t;
use crate::ui::widgets::{Button, ParagraphStyle, Text};

const UI_FONT: &[u8] = include_bytes!("assets/misans_ui.mirx");
const TOKEN_CJK: FontToken = FontToken::Custom("misans24");

const TRANSLATIONS: &[Translation] = &[
    (Locale::EnUs, "welcome", "Welcome to mirui"),
    (Locale::EnUs, "greeting", "Hello, friend"),
    (Locale::EnUs, "goodbye", "See you soon"),
    (Locale::EnUs, "toggle", "Switch language"),
    (Locale::ZhCn, "welcome", "欢迎使用 mirui"),
    (Locale::ZhCn, "greeting", "你好,朋友"),
    (Locale::ZhCn, "goodbye", "回头见"),
    (Locale::ZhCn, "toggle", "切换语言"),
];

fn register_font(world: &mut World) {
    let Some(mgr) = world.resource::<FontManager>() else {
        return;
    };
    let font = Font::from_mirx("MiSans UI", 14, UI_FONT, &mirx::reader::PayloadLimits::HOST)
        .expect("UI font");
    mgr.add_static(TOKEN_CJK.cache_key(), font);
}

#[compose]
pub fn build_widgets() {
    register_font(cx.world_mut());
    cx.world_mut()
        .insert_resource(I18n::new(Locale::EnUs).with_translations(TRANSLATIONS));

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
                if let Some(i18n) = ctx.world.resource::<I18n>() {
                    let next = if i18n.locale() == Locale::EnUs {
                        Locale::ZhCn
                    } else {
                        Locale::EnUs
                    };
                    i18n.set_locale(next);
                }
                ctx.world.invalidate(ctx.entity);
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

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(480, 320);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::UiScope;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        world.insert_resource(crate::render::font::default_font_manager());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        drop(cx);
        let column = world
            .get::<Children>(parent)
            .and_then(|c| c.0.first().copied())
            .expect("Column spawned under parent");
        let column_children = world
            .get::<Children>(column)
            .expect("Column has children")
            .0
            .len();
        assert_eq!(column_children, 4, "3 labels + 1 toggle button");
        assert!(world.resource::<I18n>().is_some(), "I18n resource inserted");
    }
}
