#![allow(clippy::needless_update)]

extern crate alloc;

use alloc::vec::Vec;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::font::Font;
use crate::render::scene::resolver::SliceResolver;
use crate::render::scene::{ResourceRef, Scene, SceneOp};
use crate::render::texture::Texture;
use crate::types::{Point, Transform};
use crate::ui::dirty::Dirty;
use crate::ui::widgets::assets::IMG_THUMBS_UP;

pub struct VectorMandala {
    pub start_ms: u32,
    pub petals: u8,
}

impl Default for VectorMandala {
    fn default() -> Self {
        Self {
            start_ms: 0,
            petals: 10,
        }
    }
}

// sw rasterizer fills paths only under identity/translate; FillRect survives
// arbitrary rotation, so the rotating motif is built from rounded rects.
const PETAL_MOTIF: &[SceneOp] = scene! {
    rect -7 -96 14 40 7 120 178 232 255 255;
    rect -10 -64 20 44 10 86 142 214 255 255;
    rect -6 -36 12 30 6 58 96 168 255 255;
    rect -3 -118 6 22 3 255 226 138 255 255
};

const EMBLEM_MIRX: &[u8] = &[
    0x4d, 0x49, 0x52, 0x58, 0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00,
    0x8a, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1b, 0xb5, 0xf5, 0xa7, 0x03, 0x00, 0x01, 0x00,
    0x3c, 0x00, 0x00, 0x00, 0x4e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x01, 0x08, 0x00,
    0x89, 0x62, 0xb9, 0xc6, 0x03, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xee, 0xff, 0xff,
    0x03, 0x00, 0x0a, 0x00, 0x00, 0x00, 0xf6, 0xff, 0xff, 0x00, 0x0a, 0x00, 0x00, 0x00, 0x0a, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x12, 0x00, 0x00, 0x03, 0x00, 0xf6, 0xff, 0xff, 0x00, 0x0a,
    0x00, 0x00, 0x00, 0xf6, 0xff, 0xff, 0x00, 0xf6, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0xee,
    0xff, 0xff, 0x04, 0xff, 0xe2, 0x8a, 0xff, 0xff, 0x00, 0x00,
];

const RING_DOT: &[SceneOp] = scene! {
    rect -3 -3 6 6 3 255 255 255 255 200
};

const LAYERS: [(Fixed, u8); 3] = [
    (Fixed::from_f32(1.5), 235),
    (Fixed::from_f32(1.0), 255),
    (Fixed::from_f32(0.547), 220),
];

fn build_frame(cx: Fixed, cy: Fixed, petals: u8, spin_deg: Fixed) -> Scene {
    let n = petals.max(1) as i32;
    let center = Transform::translate(cx, cy);
    let step = Fixed::from_int(360) / Fixed::from_int(n);

    let mut s = Scene::new();

    s.group(center.compose(&Transform::rotate_deg(spin_deg)), |s| {
        for (li, (scale, opa)) in LAYERS.iter().enumerate() {
            let layer_phase = Fixed::from_int(li as i32 * 18);
            for i in 0..n {
                let angle = step * Fixed::from_int(i) + layer_phase;
                let petal = Transform::rotate_deg(angle).compose(&Transform::scale(*scale, *scale));
                s.group_alpha_multiply(petal, *opa, |s| {
                    s.extend_from_slice(PETAL_MOTIF);
                });
            }
        }
    });

    let ring_r = Fixed::from_int(150);
    let ring_count = (n * 2).max(2);
    let ring_step = Fixed::from_int(360) / Fixed::from_int(ring_count);
    s.group(
        center.compose(&Transform::rotate_deg(Fixed::ZERO - spin_deg)),
        |s| {
            for i in 0..ring_count {
                let a = ring_step * Fixed::from_int(i);
                let dx = Fixed::sin_deg(a) * ring_r;
                let dy = Fixed::ZERO - Fixed::cos_deg(a) * ring_r;
                s.group(Transform::translate(dx, dy), |s| {
                    s.extend_from_slice(RING_DOT);
                });
            }
        },
    );

    s.group(center, |s| {
        let parsed = mirx::parse_chunk(EMBLEM_MIRX).expect("baked EMBLEM_MIRX must parse");
        let payload = parsed
            .chunk_payload(EMBLEM_MIRX, mirx::chunk_type::VECTOR)
            .expect("baked EMBLEM_MIRX must contain a VECTOR chunk");
        let emblem = Scene::decode(payload).expect("baked EMBLEM_MIRX must decode");
        s.extend_from_slice(&emblem.ops);
    });

    push_thumbs_ring(&mut s, cx, cy, spin_deg);

    s
}

const THUMB_RING_COUNT: i32 = 6;
const THUMB_RING_RADIUS: i32 = 110;
const THUMB_SIZE: i32 = 28;

// Skewed (non-axis-aligned) quad on every thumb forces every
// backend's quad blit path — this ring exists for backend coverage,
// not visual design.
fn push_thumbs_ring(s: &mut Scene, cx: Fixed, cy: Fixed, spin_deg: Fixed) {
    let step = Fixed::from_int(360) / Fixed::from_int(THUMB_RING_COUNT);
    for i in 0..THUMB_RING_COUNT {
        let a = step * Fixed::from_int(i) + spin_deg;
        let dx = Fixed::sin_deg(a) * Fixed::from_int(THUMB_RING_RADIUS);
        let dy = Fixed::ZERO - Fixed::cos_deg(a) * Fixed::from_int(THUMB_RING_RADIUS);
        let opa = 140u8 + (i as u8 * 20);
        let half = Fixed::from_int(THUMB_SIZE) / Fixed::from_int(2);
        let skew = Fixed::from_int(THUMB_SIZE) / Fixed::from_int(4);
        let p0 = Point::new(cx + dx - half, cy + dy - half);
        let p1 = Point::new(cx + dx + half + skew, cy + dy - half);
        let p2 = Point::new(cx + dx + half, cy + dy + half);
        let p3 = Point::new(cx + dx - half - skew, cy + dy + half);

        s.group_alpha_multiply(Transform::IDENTITY, opa, |s| {
            s.push(SceneOp::Blit {
                texture: ResourceRef::Token("thumbs_up".into()),
                pos: Point::ZERO,
                size: Point::new(Fixed::from_int(THUMB_SIZE), Fixed::from_int(THUMB_SIZE)),
                transform: Transform::IDENTITY,
                quad: Some([p0, p1, p2, p3]),
                opa: 255,
                radius: Fixed::ZERO,
                composite: CompositeMode::SourceOver,
            });
        });
    }
}

fn vector_mandala_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(state) = world.get::<VectorMandala>(entity) else {
        return;
    };
    let now_ms = world
        .resource::<MonoClock>()
        .map(|c| c.now_ms())
        .unwrap_or(0);
    let elapsed_ms = now_ms.wrapping_sub(state.start_ms) as i32;
    let spin_deg = Fixed::from_int((elapsed_ms * 360 / 16000) % 360);

    let cx = rect.x + rect.w / Fixed::from_int(2);
    let cy = rect.y + rect.h / Fixed::from_int(2);

    let scene = build_frame(cx, cy, state.petals, spin_deg);

    let fonts: [(&str, &Font); 0] = [];
    let textures: [(&str, &Texture); 1] = [("thumbs_up", &IMG_THUMBS_UP)];
    let resolver = SliceResolver::new(&fonts, &textures);
    let _ = scene.replay(renderer, ctx.clip, &resolver);
}

pub fn vector_mandala_view() -> View {
    View::new("VectorMandala", 60, vector_mandala_render).with_filter::<VectorMandala>()
}

#[mirui_macros::system(order = ANIMATION)]
pub fn vector_mandala_anim_system(world: &mut World) {
    let mut buf = Vec::new();
    world.query::<VectorMandala>().collect_into(&mut buf);
    for e in buf {
        world.insert(e, Dirty);
    }
}

pub fn build_widgets(world: &mut World, parent: Entity) {
    let now_ms = world
        .resource::<MonoClock>()
        .map(|c| c.now_ms())
        .unwrap_or(0);

    ui! {
        :(
            parent: parent
            world: world
        :)

        VectorMandala (
            start_ms: now_ms,
            petals: 10,
            grow: 1.0
        )
    };
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_plugin(StdInstantClockPlugin);
    app.with_widget(vector_mandala_view());
    app.add_system(vector_mandala_anim_system::system());
    build_widgets(&mut app.world, parent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::view::ViewRegistry;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let mut reg = ViewRegistry::with_builtins();
        reg.insert(vector_mandala_view());
        world.insert_resource(reg);
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }

    #[test]
    fn frame_ops_non_empty_and_group_balanced() {
        let scene = build_frame(
            Fixed::from_int(240),
            Fixed::from_int(240),
            10,
            Fixed::from_int(30),
        );
        assert!(!scene.ops.is_empty());
        let mut depth = 0i32;
        for op in &scene.ops {
            match op {
                SceneOp::GroupBegin { .. } => depth += 1,
                SceneOp::GroupEnd => {
                    depth -= 1;
                    assert!(depth >= 0, "GroupEnd without matching GroupBegin");
                }
                _ => {}
            }
        }
        assert_eq!(depth, 0, "groups must be balanced");
    }

    #[test]
    fn frame_has_two_level_nesting() {
        let scene = build_frame(Fixed::ZERO, Fixed::ZERO, 6, Fixed::ZERO);
        let mut depth = 0i32;
        let mut max_depth = 0i32;
        for op in &scene.ops {
            match op {
                SceneOp::GroupBegin { .. } => {
                    depth += 1;
                    max_depth = max_depth.max(depth);
                }
                SceneOp::GroupEnd => depth -= 1,
                _ => {}
            }
        }
        assert_eq!(max_depth, 2, "outer spin group wrapping per-petal groups");
    }

    #[test]
    fn frame_roundtrips_through_codec() {
        let scene = build_frame(Fixed::ZERO, Fixed::ZERO, 4, Fixed::ZERO);
        let bytes = scene.encode().unwrap();
        let back = Scene::decode(&bytes).unwrap();
        assert_eq!(back.ops, scene.ops);
    }

    #[test]
    fn baked_emblem_mirx_decodes_to_a_filled_path() {
        let parsed = mirx::parse_chunk(EMBLEM_MIRX).unwrap();
        let payload = parsed
            .chunk_payload(EMBLEM_MIRX, mirx::chunk_type::VECTOR)
            .unwrap();
        let scene = Scene::decode(payload).unwrap();
        assert_eq!(scene.ops.len(), 1);
        assert!(matches!(scene.ops[0], SceneOp::FillPath { .. }));
    }
}
