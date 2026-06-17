use alloc::vec::Vec;

use crate::ecs::{Entity, World};
use crate::render::command::DrawCommand;
use crate::render::renderer::Renderer;
use crate::types::{Fixed, Point, Rect};
use crate::ui::view::{View, ViewCtx};

#[derive(crate::Component)]
pub struct Text(pub Vec<u8>);

impl Text {
    pub fn build(s: &str) -> TextBuilder {
        TextBuilder {
            text: Text(s.as_bytes().to_vec()),
            style: None,
        }
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
    renderer.draw(
        &DrawCommand::Label {
            pos: Point {
                x: rect.x + Fixed::from_int(2),
                y: rect.y + Fixed::from_int(2),
            },
            transform: ctx.transform,
            text: &text.0,
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
        assert_eq!(world.get::<Text>(e).unwrap().0, b"hi".to_vec());
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
}
