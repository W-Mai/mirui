use super::binding::ROW_H;
use super::composition::build_widgets;
use crate::ecs::SystemScheduler;
use crate::input::event::scroll::ScrollOffset;
use crate::prelude::*;
use crate::ui::Children;
use crate::ui::widgets::Text;

#[test]
fn build_widgets_smoke() {
    let mut app = App::headless(320, 320);
    app.with_default_widgets();
    let parent = app.spawn_root().id();
    app.compose(parent, build_widgets());
    crate::ui::render_system::try_update_layout(
        &mut app.world,
        parent,
        &crate::types::Viewport::new(320, 320, Fixed::ONE),
    )
    .unwrap();
    let mut world = app.world;
    assert!(
        world
            .get::<Children>(parent)
            .is_some_and(|c| !c.0.is_empty()),
    );
    let list = world.find_by_id("lazy_list_view").unwrap();
    let row = world.get::<Children>(list).unwrap().0[0];
    assert!(!world.has::<Text>(row));
    let label = world.get::<Children>(row).unwrap().0[0];
    let paragraph = world.get::<Text>(label).unwrap().paragraph().clone();
    let mut scheduler = SystemScheduler::new();
    scheduler.add(crate::ui::widgets::lazy_list::lazy_list_system::system());
    world.get_mut::<ScrollOffset>(list).unwrap().y = Fixed::from_int(42 * ROW_H);
    scheduler.run_all(&mut world).unwrap();
    crate::core::reactive::flush_signal_dirty(&mut world);
    let row = *world
        .get::<Children>(list)
        .unwrap()
        .0
        .iter()
        .find(|&&row| {
            world.get::<Children>(row).is_some_and(|children| {
                world
                    .get::<Text>(children.0[0])
                    .unwrap()
                    .resolve(world.resource())
                    == "Row 42"
            })
        })
        .unwrap();
    let label = world.get::<Children>(row).unwrap().0[0];
    assert_eq!(
        world.get::<Text>(label).unwrap().resolve(world.resource()),
        "Row 42"
    );
    assert_eq!(world.get::<Text>(label).unwrap().paragraph(), &paragraph);
    assert_eq!(
        world.get::<Style>(row).unwrap().bg_color,
        Some(ColorToken::Surface.into())
    );
}

#[test]
fn incremental_scroll_preserves_the_stationary_list_frame() {
    use crate::surface::{FramebufferAccess, framebuf::FramebufSurface};

    for (scale, radius) in [(1, 0), (1, 12), (2, 0), (2, 12)] {
        let backend = FramebufSurface::with_scale(
            320 * scale,
            320 * scale,
            Fixed::from_int(scale as i32),
            |_, _| {},
        );
        let mut app = App::new(backend);
        app.with_default_widgets();
        let root = app.spawn_root().id();
        app.compose(root, build_widgets());
        app.render_dirty().unwrap();
        let list = app.world.find_by_id("lazy_list_view").unwrap();
        let style = app.world.get_mut::<Style>(list).unwrap();
        style.bg_color = Some(ColorToken::SurfaceVariant.into());
        style.border_color = Some(ColorToken::Outline.into());
        style.border_radius = Fixed::from_int(radius);
        style.border_width = Fixed::from_int(if radius == 0 { 0 } else { 1 });
        app.world.invalidate(list);
        app.render_dirty().unwrap();
        for position in [
            12.0, 24.0, 8.0, 0.0, -12.0, -24.0, -40.0, -20.0, 0.0, 38.0, 76.0, 76.25, 76.5, 77.0,
            0.0,
        ] {
            let next = Fixed::from_f32(position);
            app.world.get_mut::<ScrollOffset>(list).unwrap().y = next;
            app.world.invalidate_visual(list);
            app.render_dirty().unwrap();
            assert!(
                !crate::ui::dirty::has_visible_dirty::<crate::ui::dirty::Dirty>(&app.world, root),
                "scrolling must consume offscreen row dirtiness"
            );
            let incremental = app
                .backend
                .framebuffer()
                .read_pixels()
                .unwrap()
                .bytes()
                .to_vec();
            app.render().unwrap();
            let full = app.backend.framebuffer();
            let mismatch = incremental
                .iter()
                .zip(full.read_pixels().unwrap().bytes())
                .position(|(a, b)| a != b);
            assert_eq!(
                mismatch, None,
                "scale={scale}, offset={position}, differing byte={mismatch:?}"
            );
        }
    }
}
