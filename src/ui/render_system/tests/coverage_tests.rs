use crate::prelude::*;
use crate::surface::FramebufferAccess;
use crate::types::Viewport;

#[test]
fn retained_descendants_and_subtree_rendering_respect_hidden_ancestors() {
    let mut app = App::headless(160, 120);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.compose(root, outline_scene());
    app.render().unwrap();
    let container = app.world.find_by_id("container").unwrap();
    let outline = app.world.find_by_id("outline").unwrap();
    app.world.insert(container, crate::ui::Hidden);
    let viewport = Viewport::new(160, 120, Fixed::ONE);
    super::seed_prev_rects(&mut app.world, root, &viewport);
    assert!(
        app.world
            .get::<crate::ui::dirty::PaintedCoverage>(outline)
            .unwrap()
            .0
            .is_empty()
    );

    let mut bytes = alloc::vec![0; 160 * 120 * 4];
    {
        let texture = crate::render::texture::Texture::new(
            &mut bytes,
            160,
            120,
            crate::render::texture::ColorFormat::RGBA8888,
        )
        .unwrap();
        let mut renderer = crate::render::sw::SwRenderer::new(texture);
        super::render_region(
            &app.world,
            outline,
            &viewport,
            &Rect::new(0, 0, 160, 120),
            &mut renderer,
        )
        .unwrap();
    }
    assert!(bytes.iter().all(|&byte| byte == 0));
}

#[test]
fn visibility_cycles_reuse_storage_and_retire_only_successful_coverage() {
    let mut app = App::headless(160, 120);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.compose(root, outline_scene());
    let container = app.world.find_by_id("container").unwrap();
    app.render().unwrap();
    let cycle = |app: &mut App<_>| {
        for visible in [false, true] {
            crate::ui::property::apply_to_world::<crate::ui::property::prop::Visible>(
                &mut app.world,
                container,
                visible,
            );
            app.dirty_regions().unwrap();
            app.render_dirty().unwrap();
        }
    };
    cycle(&mut app);
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for _ in 0..16 {
                cycle(&mut app);
            }
        }),
        0
    );
}

#[crate::compose]
fn outline_scene() {
    ui! {
        View (id: "host", grow: 1.0) {
            View (id: "container", grow: 1.0) {
                View (
                    id: "outline",
                    position: Position::Absolute,
                    left: 10,
                    top: 10,
                    width: 120,
                    height: 70,
                    border_color: Color::rgb(40, 200, 110),
                    border_width: 2
                )
            }
        }
    };
}

#[test]
fn hiding_and_retirement_preserve_border_gaps_and_erase_visible_pixels() {
    for operation in 0..4 {
        let mut app = App::headless(160, 120);
        app.with_default_widgets();
        let root = app.spawn_root().id();
        app.compose(root, outline_scene());
        let container = app.world.find_by_id("container").unwrap();
        let host = app.world.find_by_id("host").unwrap();
        let outline = app.world.find_by_id("outline").unwrap();
        let branches = [alloc::vec![container]];
        if operation == 0 {
            crate::ui::branch::prepare(&mut app.world, host, &branches);
            crate::ui::branch::select(&mut app.world, &branches, None, Some(0));
        }
        app.render().unwrap();
        match operation {
            0 => crate::ui::branch::select(&mut app.world, &branches, Some(0), None),
            1 => {
                crate::ui::property::apply_to_world::<crate::ui::property::prop::Visible>(
                    &mut app.world,
                    container,
                    false,
                );
            }
            2 => crate::ui::retirement::despawn_subtree(&mut app.world, container),
            _ => {
                app.world.insert(container, crate::ui::Hidden);
                app.world.invalidate(host);
            }
        }
        for _ in 0..2 {
            let plan = app.dirty_regions().unwrap();
            assert_eq!(
                plan.rects.len(),
                4,
                "operation={operation}: {:?}",
                plan.rects
            );
            assert_eq!(
                plan.rects
                    .iter()
                    .map(|r| r.w.to_int() * r.h.to_int())
                    .sum::<i32>(),
                744,
                "operation={operation}: {:?}",
                plan.rects
            );
        }
        app.render_dirty().unwrap();
        let partial = app
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec();
        app.render().unwrap();
        assert!(
            partial == app.backend.framebuffer().read_pixels().unwrap().bytes(),
            "operation={operation}"
        );
        if operation != 2 {
            assert!(
                app.world
                    .get::<crate::ui::dirty::PaintedCoverage>(outline)
                    .unwrap()
                    .0
                    .is_empty()
            );
        }
    }
}
