use crate::prelude::Fixed;
use crate::prelude::draw::*;
use crate::render::scene::{ResourceRef, Scene, SceneOp};
use crate::scene;
use crate::types::{Point, Transform};

// sw rasterizer fills paths only under identity/translate; FillRect survives
// arbitrary rotation, so the rotating motif is built from rounded rects.
const PETAL_MOTIF: &[SceneOp] = scene! {
    rect -7 -96 14 40 7 120 178 232 255 255;
    rect -10 -64 20 44 10 86 142 214 255 255;
    rect -6 -36 12 30 6 58 96 168 255 255;
    rect -3 -118 6 22 3 255 226 138 255 255
};

pub(super) const EMBLEM_MIRX: &[u8] = &[
    0x4d, 0x49, 0x52, 0x58, 0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00,
    0xa6, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0xe9, 0x28, 0xe2, 0x03, 0x00, 0x01, 0x00,
    0x3c, 0x00, 0x00, 0x00, 0x6a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x01, 0x08, 0x00,
    0xc1, 0xb3, 0x2a, 0xb2, 0x03, 0x00, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x12, 0x00, 0x00, 0x00, 0xf4, 0xff, 0xff, 0x02, 0x00, 0x0a, 0x00, 0x00, 0x00, 0xf6,
    0xff, 0xff, 0x00, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x0a, 0x00, 0x00, 0x00,
    0x0a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0a, 0x00, 0x00, 0x02, 0x00, 0xf6, 0xff, 0xff,
    0x00, 0x0a, 0x00, 0x00, 0x00, 0xf6, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0xf6, 0xff,
    0xff, 0x00, 0xf6, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0xee, 0xff, 0xff, 0x04, 0x00, 0xff,
    0xe2, 0x8a, 0xff, 0xff, 0x00, 0x00,
];

const RING_DOT: &[SceneOp] = scene! {
    rect -3 -3 6 6 3 255 255 255 255 200
};

const LAYERS: [(Fixed, u8); 3] = [
    (Fixed::from_f32(1.5), 235),
    (Fixed::from_f32(1.0), 255),
    (Fixed::from_f32(0.547), 220),
];

pub(super) fn decode_emblem() -> Scene {
    let reader = mirx::Reader::open(EMBLEM_MIRX).expect("baked EMBLEM_MIRX must parse");
    let payload = reader
        .chunks()
        .find(|chunk| chunk.chunk_type() == mirx::ChunkType::VECTOR)
        .map(|chunk| chunk.payload())
        .expect("baked EMBLEM_MIRX must contain a VECTOR chunk");
    Scene::decode(payload).expect("baked EMBLEM_MIRX must decode")
}

pub(super) fn rebuild_frame(
    s: &mut Scene,
    cx: Fixed,
    cy: Fixed,
    petals: u8,
    spin_deg: Fixed,
    emblem: &Scene,
) {
    let n = petals.max(1) as i32;
    let center = Transform::translate(cx, cy);
    let step = Fixed::from_int(360) / Fixed::from_int(n);

    s.ops.clear();

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
        s.extend_from_slice(&emblem.ops);
    });

    push_thumbs_ring(s, cx, cy, spin_deg);
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
