use crate::prelude::*;
use crate::surface::FramebufferAccess;
use crate::types::Transform;
use crate::ui::property::{apply_to_world, prop};
use crate::ui::{OffscreenGeneration, OffscreenRender};

#[crate::compose]
fn scene() {
    ui! {
        View (id: "moving", width: 100, height: 70, clip_children: true) {
            View (id: "cached", width: 80, height: 50) [
                OffscreenRender::default(),
            ] {
                View (id: "inner", width: 30, height: 20) [
                    OffscreenRender::default(),
                ] {
                    View (id: "content", width: 30, height: 20, bg_color: Color::rgb(180, 40, 30))
                }
            }
        }
    };
}

fn generation(world: &crate::ecs::World, entity: Entity) -> u32 {
    world
        .get::<OffscreenGeneration>(entity)
        .map_or(0, |value| value.0)
}

#[test]
fn ancestor_motion_reuses_local_content_and_internal_motion_invalidates_only_its_container() {
    let mut app = App::headless(160, 100);
    app.with_default_widgets().with_default_systems();
    app.with_offscreen_pool_budget(64 * 1024);
    let root = app.spawn_root().id();
    app.compose(root, scene());
    app.render().unwrap();
    let moving = app.world.find_by_id("moving").unwrap();
    let cached = app.world.find_by_id("cached").unwrap();
    let inner = app.world.find_by_id("inner").unwrap();
    let content = app.world.find_by_id("content").unwrap();
    let outer_generation = generation(&app.world, cached);
    let inner_generation = generation(&app.world, inner);
    for offset in [5, 17, -9, 0] {
        apply_to_world::<prop::Transform>(
            &mut app.world,
            moving,
            Transform::translate(Fixed::from_int(offset), Fixed::ZERO),
        );
        app.dirty_regions().unwrap();
        app.dirty_regions().unwrap();
        assert_eq!(
            generation(&app.world, cached),
            outer_generation,
            "ancestor motion must not repaint cached content"
        );
        assert_eq!(generation(&app.world, inner), inner_generation);
        app.render_dirty().unwrap();
        let dirty = app.backend.framebuffer().buf.as_slice().to_vec();
        app.render().unwrap();
        assert!(dirty == app.backend.framebuffer().buf.as_slice());
    }
    apply_to_world::<prop::Transform>(
        &mut app.world,
        inner,
        Transform::translate(Fixed::from_int(9), Fixed::ZERO),
    );
    app.dirty_regions().unwrap();
    assert_eq!(generation(&app.world, cached), outer_generation + 1);
    assert_eq!(generation(&app.world, inner), inner_generation);
    apply_to_world::<prop::Transform>(
        &mut app.world,
        moving,
        Transform::translate(Fixed::from_int(13), Fixed::ZERO),
    );
    apply_to_world::<prop::BackgroundColor>(
        &mut app.world,
        content,
        Color::rgb(20, 190, 60).into(),
    );
    app.render_dirty().unwrap();
    assert!(generation(&app.world, cached) > outer_generation + 1);
    assert!(generation(&app.world, inner) > inner_generation);
    let dirty = app.backend.framebuffer().buf.as_slice().to_vec();
    app.render().unwrap();
    assert!(dirty == app.backend.framebuffer().buf.as_slice());
}

#[test]
fn transform_damage_reuses_prepared_storage() {
    let mut app = App::headless(160, 100);
    app.with_default_widgets().with_default_systems();
    app.with_offscreen_pool_budget(64 * 1024);
    let root = app.spawn_root().id();
    app.compose(root, scene());
    app.render().unwrap();
    let moving = app.world.find_by_id("moving").unwrap();
    let cycle = |app: &mut App<_>| {
        for offset in [5, -9, 0] {
            apply_to_world::<prop::Transform>(
                &mut app.world,
                moving,
                Transform::translate(Fixed::from_int(offset), Fixed::ZERO),
            );
            app.dirty_regions().unwrap();
            app.render_dirty().unwrap();
        }
    };
    cycle(&mut app);
    assert_eq!(
        crate::test_support::tracked_allocations(|| cycle(&mut app)),
        0
    );
}
