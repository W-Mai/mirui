use super::*;

#[test]
fn plain_visual_scene_has_no_geometry_preparation_demand() {
    assert!(!needs_geometry_preparation(&World::new()));
}

#[test]
fn offscreen_capture_keeps_geometry_preparation_enabled() {
    let mut world = World::new();
    let entity = world.spawn_empty();
    world.insert(entity, super::super::OffscreenRender::default());
    assert!(needs_geometry_preparation(&world));
}
