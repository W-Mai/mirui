use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;

use crate::core::i18n::Localized;
use crate::ecs::{Entity, World};
use crate::render::command::DrawCommand;
use crate::render::renderer::Renderer;
use crate::types::{Fixed, Point, Rect};
use crate::ui::view::{View, ViewCtx};

#[derive(Clone, Debug, crate::Component)]
pub enum Text {
    Owned(Vec<u8>),
    Localized(Localized),
}

impl Text {
    pub fn build(source: impl Into<Text>) -> TextBuilder {
        TextBuilder {
            text: source.into(),
            style: None,
        }
    }

    /// Resolve to a byte slice. `Localized` variants look up the active
    /// `I18n` resource; unresolved keys fall back to the key's own bytes
    /// so layout / render still see *something* deterministic.
    pub fn bytes<'a>(&'a self, world: &World) -> Cow<'a, [u8]> {
        match self {
            Text::Owned(v) => Cow::Borrowed(v.as_slice()),
            Text::Localized(loc) => Cow::Borrowed(loc.resolve_or_key(world).as_bytes()),
        }
    }

    pub fn is_localized(&self) -> bool {
        matches!(self, Text::Localized(_))
    }
}

impl From<&str> for Text {
    fn from(s: &str) -> Self {
        Text::Owned(s.as_bytes().to_vec())
    }
}

impl From<String> for Text {
    fn from(s: String) -> Self {
        Text::Owned(s.into_bytes())
    }
}

impl From<Vec<u8>> for Text {
    fn from(v: Vec<u8>) -> Self {
        Text::Owned(v)
    }
}

impl From<&[u8]> for Text {
    fn from(v: &[u8]) -> Self {
        Text::Owned(v.to_vec())
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
    let bytes = text.bytes(world);
    renderer.draw(
        &DrawCommand::Label {
            pos: Point {
                x: rect.x + Fixed::from_int(2),
                y: rect.y + Fixed::from_int(2),
            },
            transform: ctx.transform,
            text: &bytes,
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
        assert_eq!(&*text.bytes(&world), b"hi");
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
    fn from_str_yields_owned() {
        let t: Text = "hi".into();
        assert!(matches!(t, Text::Owned(_)));
        assert!(!t.is_localized());
    }

    #[test]
    fn from_localized_yields_localized() {
        let t: Text = Localized::new("welcome").into();
        assert!(t.is_localized());
    }
}
