//! SDF zoom demo — one word continuously scaling up and down from a
//! single SDF atlas. This is what SDF buys over a bitmap: the same
//! source resamples to any size, staying smooth at every frame, so a
//! growing label never pixelates or re-bakes.
//!
//! A `ZoomText` component holds the current pixel size; an `animate!`
//! tween drives it and marks the entity dirty each frame. The custom
//! view resolves the SDF font, rebuilds it at the animated size, and
//! emits one `Label` — `blit_sdf_glyph` resamples the atlas to it.

extern crate alloc;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::prelude::*;
use crate::render::command::DrawCommand;
use crate::render::font::{Font, FontManager, sdf};
use crate::render::renderer::Renderer;
use crate::ui::dirty::Dirty;
use crate::ui::view::{View, ViewCtx};

const SDF_ATLAS: &[u8] = include_bytes!("assets/misans_sdf_24.mirx");
const ZOOM_TOKEN: FontToken = FontToken::Custom("sdf_zoom");

#[derive(crate::Component)]
pub struct ZoomText {
    pub size: u16,
    pub text: &'static str,
}

mirui_macros::animate!(ZoomSize, |world, entity, value| {
    if let Some(z) = world.get_mut::<ZoomText>(entity) {
        z.size = value.to_int().clamp(1, 200) as u16;
    }
    world.insert(entity, crate::ui::dirty::Dirty);
});

fn font_payload(atlas: &'static [u8]) -> &'static [u8] {
    mirx::parse_chunk(atlas)
        .expect("bundled atlas parses")
        .chunk_payload(atlas, mirx::chunk_type::FONT)
        .expect("FONT chunk present")
}

pub fn register_font(world: &mut World) {
    if let Some(mgr) = world.resource::<FontManager>() {
        let font = sdf::font_from_mirx_chunk("MiSans-SDF-zoom", font_payload(SDF_ATLAS))
            .expect("zoom atlas");
        mgr.add_static(ZOOM_TOKEN.cache_key(), font);
    }
}

fn zoom_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(z) = world.get::<ZoomText>(entity) else {
        return;
    };
    let Some(mgr) = world.resource::<FontManager>() else {
        return;
    };
    // Rebuild the resolved SDF font at the animated size — Font is the
    // size descriptor, so a clone with a new `size` renders the same
    // atlas at that target. Clone is an Rc bump on the provider.
    let mut font: Font = (*mgr.resolve(&ZOOM_TOKEN.cache_key())).clone();
    font.size = z.size.max(1);
    renderer.draw(
        &DrawCommand::Label {
            pos: Point {
                x: rect.x + Fixed::from_int(8),
                y: rect.y + Fixed::from_int(8),
            },
            transform: ctx.transform,
            text: z.text,
            font: &font,
            color: Color::rgb(255, 200, 120),
            opa: 255,
        },
        ctx.clip,
    );
}

pub fn zoom_view() -> View {
    View::new("ZoomText", 60, zoom_render)
}

pub fn build_widgets(world: &mut World, parent: Entity) {
    //~focus-start
    let label = ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            grow: 1.0,
            width: 480,
            height: 320
        ) [
            ZoomText { size: 16, text: "SDF" },
        ]
    };
    //~focus-end

    world.insert(label, Dirty);
    world.insert(
        label,
        ZoomSize(
            crate::anim::Tween::new(
                Fixed::from_int(16),
                Fixed::from_int(120),
                1800,
                crate::anim::ease::ease_in_out_cubic,
                crate::anim::PlayMode::PingPong,
            )
            .into(),
        ),
    );
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::ecs;
    app.with_widget(zoom_view());
    register_font(&mut app.world);
    app.add_system(ecs::System::new(
        "zoom_size",
        ecs::run_order::ANIMATION,
        ZoomSize::system(),
    ));
    app.add_plugin(StdInstantClockPlugin);
    build_widgets(&mut app.world, parent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::view::ViewRegistry;

    #[test]
    fn build_widgets_inserts_zoom_label() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let mut reg = ViewRegistry::with_builtins();
        reg.insert(zoom_view());
        world.insert_resource(reg);
        world.insert_resource(crate::render::font::default_font_manager());
        register_font(&mut world);
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);
        let label = world.get::<Children>(parent).unwrap().0[0];
        assert_eq!(world.get::<ZoomText>(label).map(|z| z.size), Some(16));
    }
}
