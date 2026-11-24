extern crate std;
use super::*;
use crate::render::sw::SwRenderer;
use crate::render::texture::{ColorFormat, Texture};
use crate::types::{Color, Dimension, Viewport};
use crate::ui::layout::{LayoutStyle, Position};
use crate::ui::{Children, Parent, Style, Widget};

fn spawn_widget(world: &mut World, parent: Option<Entity>, style: Style) -> Entity {
    let e = world.spawn_empty();
    world.insert(e, Widget);
    world.insert(e, style);
    if let Some(p) = parent {
        world.insert(e, Parent(p));
        if let Some(children) = world.get_mut::<Children>(p) {
            children.0.push(e);
        } else {
            world.insert(p, Children(std::vec![e]));
        }
    }
    e
}

fn make_world() -> World {
    {
        let mut app = crate::app::App::headless(64, 64);
        app.with_default_widgets();
        app.world
    }
}

fn vp() -> Viewport {
    Viewport::new(64, 64, Fixed::ONE)
}

#[test]
fn child_scroll_and_clip_do_not_change_leaf_paint() {
    let mut world = make_world();
    let root = spawn_widget(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(17, 31, 49).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(64)),
                height: Dimension::Px(Fixed::from_int(64)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let leaf = spawn_widget(
        &mut world,
        Some(root),
        Style {
            bg_color: Some(Color::rgb(201, 101, 51).into()),
            layout: LayoutStyle {
                position: Position::Absolute,
                left: Dimension::Px(Fixed::from_int(7)),
                top: Dimension::Px(Fixed::from_int(9)),
                width: Dimension::Px(Fixed::from_int(16)),
                height: Dimension::Px(Fixed::from_int(20)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let draw = |world: &World| {
        let mut bytes = std::vec![0; 64 * 64 * 4];
        let texture = Texture::new(&mut bytes, 64, 64, ColorFormat::RGBA8888).unwrap();
        let mut renderer = SwRenderer::new(texture);
        super::render(world, root, &vp(), &mut renderer).unwrap();
        drop(renderer);
        bytes
    };
    let expected = draw(&world);
    world.get_mut::<Style>(leaf).unwrap().clip_children = true;
    world.insert(
        leaf,
        crate::input::event::scroll::ScrollOffset {
            x: Fixed::from_int(100),
            y: Fixed::from_int(-100),
        },
    );
    assert_eq!(draw(&world), expected);
}

#[test]
fn clip_children_clips_oversize_inner_rect() {
    // 8-px-wide mask with clip_children=true holding a 64-px-wide inner
    // child: the right 56 px must show only root bg, not the child.
    let mut world = make_world();
    let root = spawn_widget(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(0, 0, 0).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(64)),
                height: Dimension::Px(Fixed::from_int(64)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let mask = spawn_widget(
        &mut world,
        Some(root),
        Style {
            clip_children: true,
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(8)),
                height: Dimension::Px(Fixed::from_int(64)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    spawn_widget(
        &mut world,
        Some(mask),
        Style {
            bg_color: Some(Color::rgb(255, 0, 0).into()),
            layout: LayoutStyle {
                position: Position::Absolute,
                left: Dimension::Px(Fixed::from_int(0)),
                top: Dimension::Px(Fixed::from_int(0)),
                width: Dimension::Px(Fixed::from_int(64)),
                height: Dimension::Px(Fixed::from_int(64)),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let mut buf = std::vec![0u8; 64 * 64 * 4];
    let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = SwRenderer::new(tex);
    let viewport = vp();
    super::render(&world, root, &viewport, &mut renderer).unwrap();

    for x in 0..8 {
        let c = renderer.target.read_pixels().unwrap().get_pixel(x, 32);
        assert_eq!(
            c.r, 255,
            "x={x} should be red (inside mask), got rgb({}, {}, {})",
            c.r, c.g, c.b
        );
    }
    for x in 8..64 {
        let c = renderer.target.read_pixels().unwrap().get_pixel(x, 32);
        assert_eq!(
            c.r, 0,
            "x={x} should be black (clipped), got rgb({}, {}, {})",
            c.r, c.g, c.b
        );
    }
}

#[test]
fn no_clip_children_lets_inner_overflow() {
    // Default (clip_children=false): inner paints across full 64 px.
    let mut world = make_world();
    let root = spawn_widget(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(0, 0, 0).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(64)),
                height: Dimension::Px(Fixed::from_int(64)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let mask = spawn_widget(
        &mut world,
        Some(root),
        Style {
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(8)),
                height: Dimension::Px(Fixed::from_int(64)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    spawn_widget(
        &mut world,
        Some(mask),
        Style {
            bg_color: Some(Color::rgb(255, 0, 0).into()),
            layout: LayoutStyle {
                position: Position::Absolute,
                left: Dimension::Px(Fixed::from_int(0)),
                top: Dimension::Px(Fixed::from_int(0)),
                width: Dimension::Px(Fixed::from_int(64)),
                height: Dimension::Px(Fixed::from_int(64)),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let mut buf = std::vec![0u8; 64 * 64 * 4];
    let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = SwRenderer::new(tex);
    let viewport = vp();
    super::render(&world, root, &viewport, &mut renderer).unwrap();

    for x in 0..64 {
        let c = renderer.target.read_pixels().unwrap().get_pixel(x, 32);
        assert_eq!(c.r, 255, "x={x} should be red (no clip), got r={}", c.r);
    }
}

#[test]
fn clip_children_zero_width_hides_inner() {
    // Repro for the slider-at-ratio=0 bug: a clip_children mask with
    // width=0 should hide the inner entirely, not leak any pixel.
    let mut world = make_world();
    let root = spawn_widget(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(0, 0, 0).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(64)),
                height: Dimension::Px(Fixed::from_int(64)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let mask = spawn_widget(
        &mut world,
        Some(root),
        Style {
            clip_children: true,
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::ZERO),
                height: Dimension::Px(Fixed::from_int(64)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    spawn_widget(
        &mut world,
        Some(mask),
        Style {
            bg_color: Some(Color::rgb(255, 0, 0).into()),
            layout: LayoutStyle {
                position: Position::Absolute,
                left: Dimension::Px(Fixed::from_int(0)),
                top: Dimension::Px(Fixed::from_int(0)),
                width: Dimension::Px(Fixed::from_int(64)),
                height: Dimension::Px(Fixed::from_int(64)),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let mut buf = std::vec![0u8; 64 * 64 * 4];
    let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = SwRenderer::new(tex);
    let viewport = vp();
    super::render(&world, root, &viewport, &mut renderer).unwrap();

    for x in 0..64 {
        let c = renderer.target.read_pixels().unwrap().get_pixel(x, 32);
        assert_eq!(
            c.r, 0,
            "x={x} leaked red ({}), mask width=0 must hide it",
            c.r
        );
    }
}

#[test]
fn dirty_region_uses_2d_transform_bbox() {
    use crate::types::Transform;
    use crate::ui::dirty::Dirty;
    use crate::ui::widgets::transform::WidgetTransform;

    let mut world = make_world();
    let root = spawn_widget(
        &mut world,
        None,
        Style {
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(64)),
                height: Dimension::Px(Fixed::from_int(64)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let target = spawn_widget(
        &mut world,
        Some(root),
        Style {
            bg_color: Some(crate::types::Color::rgb(255, 255, 255).into()),
            layout: LayoutStyle {
                position: Position::Absolute,
                left: Dimension::Px(Fixed::from_int(20)),
                top: Dimension::Px(Fixed::from_int(20)),
                width: Dimension::Px(Fixed::from_int(10)),
                height: Dimension::Px(Fixed::from_int(10)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let viewport = vp();
    // Settle initial Style registration before measuring transform-only damage.
    collect_dirty_region(&mut world, root, &viewport)
        .unwrap()
        .expect("initial layout damage");
    seed_prev_rects(&mut world, root, &viewport);
    assert!(!world.has::<Dirty>(root));
    world.insert(
        target,
        WidgetTransform(Transform::scale(Fixed::from_int(2), Fixed::from_int(2))),
    );
    world.insert(target, Dirty);

    let dirty = collect_dirty_region(&mut world, root, &viewport)
        .unwrap()
        .expect("dirty region");

    assert_eq!(dirty.x, Fixed::from_int(15));
    assert_eq!(dirty.y, Fixed::from_int(15));
    assert_eq!(dirty.w, Fixed::from_int(20));
    assert_eq!(dirty.h, Fixed::from_int(20));
}

#[test]
fn seeded_prev_rect_uses_2d_transform_bbox() {
    use crate::types::Transform;
    use crate::ui::dirty::Dirty;
    use crate::ui::widgets::transform::WidgetTransform;

    let mut world = make_world();
    let root = spawn_widget(
        &mut world,
        None,
        Style {
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(64)),
                height: Dimension::Px(Fixed::from_int(64)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let target = spawn_widget(
        &mut world,
        Some(root),
        Style {
            bg_color: Some(crate::types::Color::rgb(255, 255, 255).into()),
            layout: LayoutStyle {
                position: Position::Absolute,
                left: Dimension::Px(Fixed::from_int(20)),
                top: Dimension::Px(Fixed::from_int(20)),
                width: Dimension::Px(Fixed::from_int(10)),
                height: Dimension::Px(Fixed::from_int(10)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let viewport = vp();
    // Settle the initial frame before seeding a transformed presentation.
    collect_dirty_region(&mut world, root, &viewport)
        .unwrap()
        .expect("initial layout damage");
    assert!(!world.has::<Dirty>(root));
    world.insert(
        target,
        WidgetTransform(Transform::scale(Fixed::from_int(2), Fixed::from_int(2))),
    );
    seed_prev_rects(&mut world, root, &viewport);

    world.insert(target, WidgetTransform(Transform::IDENTITY));
    world.insert(target, Dirty);
    let dirty = collect_dirty_region(&mut world, root, &viewport)
        .unwrap()
        .expect("dirty region");

    assert_eq!(dirty.x, Fixed::from_int(15));
    assert_eq!(dirty.y, Fixed::from_int(15));
    assert_eq!(dirty.w, Fixed::from_int(20));
    assert_eq!(dirty.h, Fixed::from_int(20));
}

#[test]
fn projective_dirty_and_previous_bounds_include_affine_ancestors() {
    use crate::types::{Transform, Transform3D};
    use crate::ui::dirty::{Dirty, PaintedCoverage};
    use crate::ui::widgets::transform::WidgetTransform;
    use crate::ui::widgets::transform_3d::WidgetTransform3D;

    let mut world = make_world();
    let root = spawn_widget(
        &mut world,
        None,
        Style {
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(64)),
                height: Dimension::Px(Fixed::from_int(64)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let target = spawn_widget(
        &mut world,
        Some(root),
        Style {
            layout: LayoutStyle {
                position: Position::Absolute,
                left: Dimension::Px(Fixed::from_int(18)),
                top: Dimension::Px(Fixed::from_int(20)),
                width: Dimension::Px(Fixed::from_int(16)),
                height: Dimension::Px(Fixed::from_int(12)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(
        root,
        WidgetTransform(Transform::translate(Fixed::from_int(7), Fixed::from_int(3))),
    );
    world.insert(
        target,
        WidgetTransform3D(Transform3D::rotate_y_perspective(
            Fixed::from_int(18),
            Fixed::from_int(320),
        )),
    );
    let viewport = vp();
    try_update_layout(&mut world, root, &viewport).unwrap();
    collect_dirty_region(&mut world, root, &viewport).unwrap();

    let root_rect = world.get::<super::super::ComputedRect>(root).unwrap().0;
    let target_rect = world.get::<super::super::ComputedRect>(target).unwrap().0;
    let (root_affine, root_projective) = render_transforms(
        Transform::IDENTITY,
        Transform3D::IDENTITY,
        &world,
        root,
        root_rect,
    );
    let (target_affine, target_projective) =
        render_transforms(root_affine, root_projective, &world, target, target_rect);
    let expected = visual_bounds(
        &world,
        target,
        target_rect,
        target_affine,
        target_projective,
        viewport.scale(),
    );
    let (x0, y0, x1, y1) = expected.pixel_bounds();
    let expected = Rect::new(x0, y0, x1 - x0, y1 - y0);

    seed_prev_rects(&mut world, root, &viewport);
    assert_eq!(
        world.get::<PaintedCoverage>(target).unwrap().0.outer,
        expected
    );

    world.insert(target, Dirty);
    assert_eq!(
        collect_dirty_region(&mut world, root, &viewport).unwrap(),
        Some(expected)
    );
}

#[test]
fn background_blur_expands_visual_dirty_bounds() {
    use crate::ui::widgets::BackgroundBlur;

    let mut world = make_world();
    let entity = world.spawn_empty();
    world.insert(entity, BackgroundBlur::new(4).with_spread(2));
    let bounds = visual_bounds(
        &world,
        entity,
        Rect::new(20, 24, 10, 8),
        Transform::IDENTITY,
        Transform3D::IDENTITY,
        Fixed::ONE,
    );

    assert_eq!(bounds, Rect::new(14, 18, 22, 20));
}

#[test]
fn style_border_expands_transformed_visual_dirty_bounds() {
    use crate::types::Color;

    let mut world = make_world();
    let entity = world.spawn_empty();
    world.insert(
        entity,
        Style {
            border_color: Some(Color::rgb(255, 255, 255).into()),
            border_width: Fixed::from_int(2),
            ..Default::default()
        },
    );
    let bounds = visual_bounds(
        &world,
        entity,
        Rect::new(20, 24, 10, 8),
        Transform::scale(Fixed::from_int(2), Fixed::from_int(2)),
        Transform3D::IDENTITY,
        Fixed::ONE,
    );

    assert_eq!(bounds, Rect::new(38, 46, 24, 20));
}

#[test]
fn dirty_render_clears_pixels_from_previous_2d_transform() {
    use crate::types::Transform;
    use crate::ui::dirty::Dirty;
    use crate::ui::widgets::transform::WidgetTransform;

    let mut world = make_world();
    let root = spawn_widget(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(0, 0, 0).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(64)),
                height: Dimension::Px(Fixed::from_int(64)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let target = spawn_widget(
        &mut world,
        Some(root),
        Style {
            bg_color: Some(Color::rgb(0, 0, 255).into()),
            layout: LayoutStyle {
                position: Position::Absolute,
                left: Dimension::Px(Fixed::from_int(20)),
                top: Dimension::Px(Fixed::from_int(20)),
                width: Dimension::Px(Fixed::from_int(10)),
                height: Dimension::Px(Fixed::from_int(10)),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let viewport = vp();
    world.insert(
        target,
        WidgetTransform(Transform::scale(Fixed::from_int(2), Fixed::from_int(2))),
    );
    let mut buf = std::vec![0u8; 64 * 64 * 4];
    {
        let tex =
            Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        super::render(&world, root, &viewport, &mut renderer).unwrap();
        assert_eq!(
            renderer.target.read_pixels().unwrap().get_pixel(16, 16).b,
            255
        );
    }
    seed_prev_rects(&mut world, root, &viewport);

    world.insert(
        target,
        WidgetTransform(Transform::scale(
            Fixed::ONE / Fixed::from_int(2),
            Fixed::ONE / Fixed::from_int(2),
        )),
    );
    world.insert(target, Dirty);
    let dirty = collect_dirty_region(&mut world, root, &viewport)
        .unwrap()
        .expect("dirty region");
    {
        let tex =
            Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        super::render_region(&world, root, &viewport, &dirty, &mut renderer).unwrap();
        assert_eq!(
            renderer.target.read_pixels().unwrap().get_pixel(16, 16).b,
            0
        );
        assert_eq!(
            renderer.target.read_pixels().unwrap().get_pixel(25, 25).b,
            255
        );
    }
}

#[test]
fn dirty_region_covers_rotated_2d_transform_pixels() {
    use crate::types::Transform;
    use crate::ui::dirty::Dirty;
    use crate::ui::widgets::transform::WidgetTransform;

    let mut world = make_world();
    let root = spawn_widget(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(0, 0, 0).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(64)),
                height: Dimension::Px(Fixed::from_int(64)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let target = spawn_widget(
        &mut world,
        Some(root),
        Style {
            bg_color: Some(Color::rgb(0, 0, 255).into()),
            layout: LayoutStyle {
                position: Position::Absolute,
                left: Dimension::Px(Fixed::from_int(22)),
                top: Dimension::Px(Fixed::from_int(22)),
                width: Dimension::Px(Fixed::from_int(20)),
                height: Dimension::Px(Fixed::from_int(12)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(
        target,
        WidgetTransform(Transform::rotate_deg(Fixed::from_int(45))),
    );
    world.insert(target, Dirty);

    let viewport = vp();
    let mut full = std::vec![0u8; 64 * 64 * 4];
    {
        let tex =
            Texture::new(&mut full, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        super::render(&world, root, &viewport, &mut renderer).unwrap();
    }

    let dirty = collect_dirty_region(&mut world, root, &viewport)
        .unwrap()
        .expect("dirty region");
    let mut region = std::vec![0u8; 64 * 64 * 4];
    {
        let tex =
            Texture::new(&mut region, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        super::render_region(&world, root, &viewport, &dirty, &mut renderer).unwrap();
    }

    let full_tex =
        Texture::new(&mut full, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
    let region_tex =
        Texture::new(&mut region, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
    for y in 0..64 {
        for x in 0..64 {
            let full_blue = full_tex.read_pixels().unwrap().get_pixel(x, y).b == 255;
            let region_blue = region_tex.read_pixels().unwrap().get_pixel(x, y).b == 255;
            assert_eq!(
                region_blue, full_blue,
                "dirty render mismatch at ({x},{y}), dirty={:?}",
                dirty,
            );
        }
    }
}
