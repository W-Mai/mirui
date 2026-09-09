//! SDF zoom demo — one word continuously scaling up and down from a
//! single SDF atlas. This is what SDF buys over a bitmap: the same
//! source resamples to any size, staying smooth at every frame, so a
//! growing label never pixelates or re-bakes.
//!
//! A `ZoomText` component holds the current pixel size; an `animate!`
//! tween drives it and marks the entity dirty each frame. The custom
//! view resolves the SDF font and lays out the word at the animated size.

extern crate alloc;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::prelude::*;
#[cfg(test)]
use crate::render::command::DrawCommand;
use crate::render::font::{FontManager, FontStack, ResolvedFontStack, mirx as mirx_font};
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

pub fn register_font(world: &mut World) {
    if let Some(mgr) = world.resource::<FontManager>() {
        let font = mirx_font::font_from_mirx(
            "MiSans-SDF-zoom",
            SDF_ATLAS,
            &mirx::reader::PayloadLimits::HOST,
        )
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
    let Some(resource) = world.resource::<crate::text::layout::TextLayoutResource>() else {
        return;
    };
    let face_limit = resource.borrow().limits().fallback_faces;
    let stack = FontStack::new(ZOOM_TOKEN);
    let Ok(Some(fonts)) =
        ResolvedFontStack::resolve(world, &stack, Some(z.size.max(1)), face_limit)
    else {
        return;
    };
    let font = fonts.primary();
    let paragraph = crate::ui::widgets::ParagraphStyle {
        wrap: crate::ui::widgets::TextWrap::NoWrap,
        max_lines: Some(1),
        ..crate::ui::widgets::ParagraphStyle::default()
    };
    let inset = Fixed::from_int(8);
    let width = (rect.w - inset * Fixed::from_int(2)).max(Fixed::ZERO);
    let request = paragraph.layout_request(z.text, font.metrics(font.size), Some(width));
    let owner = u64::from(entity.id) | (u64::from(entity.generation) << 32);
    let font_fingerprint = fonts.layout_fingerprint(None, paragraph.shaping);
    let Ok(handle) = fonts.with_typefaces(None, paragraph.shaping, |typefaces| {
        resource
            .borrow_mut()
            .layout_cached(owner, font_fingerprint, request, typefaces)
    }) else {
        return;
    };
    let cache = resource.borrow();
    let Some(layout) = cache.get(handle) else {
        return;
    };
    crate::ui::widgets::text::draw_text_layout(
        renderer,
        &layout,
        |font_id| (font_id == font.face_id()).then_some(font),
        crate::ui::widgets::text::TextPaint::new(
            Point {
                x: rect.x + inset,
                y: rect.y + inset,
            },
            ctx.transform,
            ctx.clip,
            Color::rgb(255, 200, 120),
        ),
    );
}

pub fn zoom_view() -> View {
    View::new("ZoomText", 60, zoom_render)
}

#[compose]
pub fn build_widgets() {
    //~focus-start
    let label = ui! {
        View (
            grow: 1.0,
            width: 480,
            height: 320
        ) [
            ZoomText { size: 16, text: "SDF" },
        ]
    };
    //~focus-end

    cx.world_mut().insert(label, Dirty);
    cx.world_mut().insert(
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
    app.with_widget(zoom_view());
    register_font(&mut app.world);
    app.add_system(ZoomSize::system());
    app.add_plugin(StdInstantClockPlugin);
    app.compose(parent, build_widgets);
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;
    use crate::render::renderer::Renderer;
    use crate::types::Transform;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::Style;
    use crate::ui::UiScope;
    use crate::ui::theme::{Theme, WidgetState};
    use crate::ui::view::ViewRegistry;

    #[derive(Default)]
    struct RecordingRenderer {
        glyph_counts: Vec<usize>,
    }

    impl Renderer for RecordingRenderer {
        fn draw(&mut self, command: &DrawCommand, _clip: &Rect) {
            if let DrawCommand::GlyphRun { glyphs, .. } = command {
                self.glyph_counts.push(glyphs.len());
            }
        }

        fn flush(&mut self) {}
    }

    #[test]
    fn zoom_text_uses_positioned_glyph_layout() {
        let mut world = World::new();
        world.insert_resource(Theme::default());
        world.insert_resource(crate::render::font::default_font_manager());
        world.insert_resource(crate::text::layout::TextLayoutResource::new(
            crate::text::TextLayoutLimits::EMBEDDED,
        ));
        register_font(&mut world);
        let entity = world.spawn_empty();
        world.insert(
            entity,
            ZoomText {
                size: 32,
                text: "SDF",
            },
        );
        let style = Style::default();
        let rect = Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: Fixed::from_int(160),
            h: Fixed::from_int(80),
        };
        let mut ctx = ViewCtx {
            style: &style,
            transform: Transform::IDENTITY,
            quad: None,
            clip: &rect,
            bg_handled: false,
            state: WidgetState::Enabled,
        };
        let mut renderer = RecordingRenderer::default();

        zoom_render(&mut renderer, &world, entity, &rect, &mut ctx);

        assert_eq!(renderer.glyph_counts, vec![3]);
    }

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
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        drop(cx);
        let label = world.get::<Children>(parent).unwrap().0[0];
        assert_eq!(world.get::<ZoomText>(label).map(|z| z.size), Some(16));
    }
}
