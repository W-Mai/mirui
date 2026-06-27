use alloc::borrow::Cow;
use alloc::string::String;

use crate::core::i18n::Localized;
use crate::ecs::{Entity, World};
use crate::render::command::DrawCommand;
use crate::render::renderer::Renderer;
use crate::types::{Fixed, Point, Rect};
use crate::ui::view::{View, ViewCtx};

#[derive(Clone, Debug, crate::Component)]
pub enum Text {
    Owned(Cow<'static, str>),
    Localized(Localized),
}

impl Text {
    pub fn build(source: impl Into<Text>) -> TextBuilder {
        TextBuilder {
            text: source.into(),
            style: None,
        }
    }

    /// Unresolved `Localized` keys fall back to the key itself so render
    /// always has something to draw.
    pub fn resolve<'a>(&'a self, world: &World) -> Cow<'a, str> {
        match self {
            Text::Owned(c) => Cow::Borrowed(c.as_ref()),
            Text::Localized(loc) => Cow::Borrowed(loc.resolve_or_key(world)),
        }
    }

    pub fn is_localized(&self) -> bool {
        matches!(self, Text::Localized(_))
    }
}

impl From<&'static str> for Text {
    fn from(s: &'static str) -> Self {
        Text::Owned(Cow::Borrowed(s))
    }
}

impl From<String> for Text {
    fn from(s: String) -> Self {
        Text::Owned(Cow::Owned(s))
    }
}

impl From<Cow<'static, str>> for Text {
    fn from(c: Cow<'static, str>) -> Self {
        Text::Owned(c)
    }
}

impl From<Localized> for Text {
    fn from(loc: Localized) -> Self {
        Text::Localized(loc)
    }
}

pub struct TextBuilder {
    text: Text,
    style: Option<crate::ui::Style>,
}

impl TextBuilder {
    pub fn style(mut self, style: crate::ui::Style) -> Self {
        self.style = Some(style);
        self
    }

    pub fn spawn(self, world: &mut World) -> Entity {
        world.spawn(self)
    }
}

impl crate::ecs::IntoBundle for TextBuilder {
    fn spawn_into(self, world: &mut World, entity: Entity) {
        world.insert(entity, self.text);
        if let Some(style) = self.style {
            world.insert(entity, style);
        }
    }
}

fn text_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(text) = world.get::<Text>(entity) else {
        return;
    };
    let color = ctx.style.text_color.resolve_in(ctx.theme(world), ctx.state);
    let Some(font) = crate::render::font::resolve_or_default(world, &ctx.style.font_token) else {
        return;
    };
    let s = text.resolve(world);
    renderer.draw(
        &DrawCommand::Label {
            pos: Point {
                x: rect.x + Fixed::from_int(2),
                y: rect.y + Fixed::from_int(2),
            },
            transform: ctx.transform,
            text: &s,
            font: &font,
            color,
            opa: 255,
        },
        ctx.clip,
    );
}

pub fn view() -> View {
    View::new("Text", 80, text_render).with_filter::<Text>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_spawns_text_with_style() {
        let mut world = World::new();
        let e = Text::build("hi")
            .style(crate::ui::Style::default())
            .spawn(&mut world);
        let text = world.get::<Text>(e).unwrap();
        assert_eq!(text.resolve(&world), "hi");
        assert!(world.has::<crate::ui::Style>(e));
        assert!(world.has::<crate::ui::Widget>(e));
    }

    #[test]
    fn build_without_style_omits_it() {
        let mut world = World::new();
        let e = Text::build("hi").spawn(&mut world);
        assert!(world.has::<Text>(e));
        assert!(!world.has::<crate::ui::Style>(e));
    }

    #[test]
    fn static_str_is_borrowed() {
        let t: Text = "hi".into();
        match t {
            Text::Owned(Cow::Borrowed(s)) => assert_eq!(s, "hi"),
            _ => panic!("expected borrowed cow for &'static str"),
        }
    }

    #[test]
    fn string_is_owned() {
        let s: String = "hi".into();
        let t: Text = s.into();
        assert!(matches!(t, Text::Owned(Cow::Owned(_))));
    }

    #[test]
    fn from_localized_yields_localized() {
        let t: Text = Localized::new("welcome").into();
        assert!(t.is_localized());
    }
}
