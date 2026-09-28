use super::composition::build_widgets;
use super::render::vector_mandala_view;
use super::scene::{EMBLEM_MIRX, decode_emblem, rebuild_frame};
use crate::prelude::*;
use crate::render::scene::{Scene, SceneOp};
use crate::ui::Children;
use crate::ui::IdMap;
use crate::ui::UiScope;
use crate::ui::view::ViewRegistry;

fn build_frame(cx: Fixed, cy: Fixed, petals: u8, spin_deg: Fixed) -> Scene {
    let emblem = decode_emblem();
    let mut scene = Scene::new();
    rebuild_frame(&mut scene, cx, cy, petals, spin_deg, &emblem);
    scene
}

#[test]
fn build_widgets_smoke() {
    let mut world = World::new();
    world.insert_resource(IdMap::new());
    let mut reg = ViewRegistry::with_builtins();
    reg.insert(vector_mandala_view());
    world.insert_resource(reg);
    let parent = WidgetBuilder::new(&mut world).id();
    let mut cx = UiScope::new(&mut world, parent);
    build_widgets(&mut cx);
    drop(cx);
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
fn frame_reuses_operation_capacity() {
    let emblem = decode_emblem();
    let mut scene = Scene::new();
    rebuild_frame(
        &mut scene,
        Fixed::from_int(240),
        Fixed::from_int(240),
        10,
        Fixed::ZERO,
        &emblem,
    );
    let capacity = scene.ops.capacity();
    rebuild_frame(
        &mut scene,
        Fixed::from_int(240),
        Fixed::from_int(240),
        10,
        Fixed::from_int(120),
        &emblem,
    );
    assert_eq!(scene.ops.capacity(), capacity);
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
    let reader = mirx::Reader::open(EMBLEM_MIRX).unwrap();
    let payload = reader
        .chunks()
        .find(|chunk| chunk.chunk_type() == mirx::ChunkType::VECTOR)
        .map(|chunk| chunk.payload())
        .unwrap();
    let scene = Scene::decode(payload).unwrap();
    assert_eq!(scene.ops.len(), 1);
    assert!(matches!(scene.ops[0], SceneOp::FillPath { .. }));
}
