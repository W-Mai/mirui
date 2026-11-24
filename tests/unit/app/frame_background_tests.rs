use super::*;
use crate::render::texture::ColorFormat;
use crate::surface::framebuf::FramebufSurface;
use crate::types::{Color, Fixed};

type TestSurface = FramebufSurface<fn(&[u8], PhysicalRect)>;

#[crate::compose]
fn layers() {
    crate::ui! {
        View (grow: 1.0) {
            View (
                id: "a",
                position: crate::ui::layout::Position::Absolute,
                left: 1,
                top: 2,
                width: 15,
                height: 9,
                bg_color: Color::rgba(210, 40, 80, 128),
                border_radius: 3
            )
            View (
                id: "b",
                position: crate::ui::layout::Position::Absolute,
                left: 8,
                top: 5,
                width: 13,
                height: 11,
                bg_color: Color::rgba(20, 190, 100, 160),
                border_radius: 4
            )
        }
    };
}

fn pixels(app: &mut App<TestSurface>) -> Vec<u8> {
    app.backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec()
}

#[test]
fn frame_write_boundary_preserves_background_sampling() {
    let mut app = App::new(FramebufSurface::with_scale_and_format(
        41,
        31,
        Fixed::ONE,
        ColorFormat::RGBA8888,
        (|_, _| {}) as fn(&[u8], PhysicalRect),
    ));
    app.with_default_widgets();
    let root = app.spawn_root().bg_color(Color::rgb(30, 50, 100)).id();
    app.compose(root, layers());
    let a = app.world.find_by_id("a").unwrap();
    app.world
        .insert(a, crate::ui::widgets::BackgroundBlur::new(2));
    app.world.invalidate(a);
    app.render().unwrap();
    let first = pixels(&mut app);
    app.render().unwrap();
    assert_eq!(pixels(&mut app), first);
    app.world.invalidate(a);
    app.render_dirty().unwrap();
    let dirty = pixels(&mut app);
    app.render().unwrap();
    assert_eq!(pixels(&mut app), dirty);
}

#[test]
fn retained_frames_reconstruct_transparency_after_queries_moves_hiding_and_deletion() {
    for scale in [
        Fixed::from_ratio(229, 256),
        Fixed::from_ratio(3, 2),
        Fixed::from_ratio(9, 4),
    ] {
        for format in [
            ColorFormat::RGBA8888,
            ColorFormat::RGB565,
            ColorFormat::BGRA8888,
        ] {
            let mut app = App::new(FramebufSurface::with_scale_and_format(
                41,
                31,
                scale,
                format,
                (|_, _| {}) as fn(&[u8], PhysicalRect),
            ));
            app.with_default_widgets();
            let root = app.spawn_root().bg_color(Color::rgba(30, 50, 100, 90)).id();
            app.compose(root, layers());
            let a = app.world.find_by_id("a").unwrap();
            let b = app.world.find_by_id("b").unwrap();
            app.render().unwrap();
            let initial = pixels(&mut app);
            for _ in 0..3 {
                app.render().unwrap();
                assert_eq!(
                    pixels(&mut app),
                    initial,
                    "repeated frame {scale:?} {format:?}"
                );
            }
            for step in 0..5 {
                let before_query = pixels(&mut app);
                crate::ui::set_position(&mut app.world, a, 3 + step, 2 + step);
                assert!(!app.dirty_regions().unwrap().is_empty());
                assert_eq!(pixels(&mut app), before_query);
                crate::ui::set_position(&mut app.world, a, 8 + step, 3);
                if step == 2 {
                    app.world.insert(b, crate::ui::Hidden);
                    app.world.invalidate(b);
                }
                if step == 3 {
                    app.world.remove::<crate::ui::Hidden>(b);
                    app.world.invalidate(b);
                }
                app.render_dirty().unwrap();
                let dirty = pixels(&mut app);
                app.render().unwrap();
                let full = pixels(&mut app);
                assert!(
                    full == dirty,
                    "dirty/full step={step} scale={scale:?} {format:?}, first difference={:?}",
                    full.chunks_exact(4)
                        .zip(dirty.chunks_exact(4))
                        .enumerate()
                        .find(|(_, (a, b))| a != b)
                );
            }
            crate::ui::despawn_subtree(&mut app.world, b);
            app.render_dirty().unwrap();
            let dirty = pixels(&mut app);
            app.render().unwrap();
            assert_eq!(pixels(&mut app), dirty);
            app.world.insert(root, crate::ui::Hidden);
            app.world.invalidate(root);
            app.render_dirty().unwrap();
            let hidden = pixels(&mut app);
            app.render().unwrap();
            assert_eq!(pixels(&mut app), hidden);
            app.world.remove::<crate::ui::Hidden>(root);
            app.world.invalidate(root);
            app.render_dirty().unwrap();
            let shown = pixels(&mut app);
            app.render().unwrap();
            let full = pixels(&mut app);
            assert!(
                full == shown,
                "shown root scale={scale:?} {format:?}, first difference={:?}",
                full.iter().zip(&shown).position(|(a, b)| a != b)
            );
        }
    }
}
