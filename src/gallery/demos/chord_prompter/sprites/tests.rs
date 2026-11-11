use super::*;
use crate::core::cache::MaxSize;

fn emit(r: &mut SwRenderer<'_>, cmd: &DrawCommand<'_>, clip: Rect) {
    assert!(matches!(
        r.poll_draw_request(
            DrawRequest::new(cmd, clip),
            None,
            &mut Context::from_waker(Waker::noop())
        ),
        Poll::Ready(Ok(()))
    ));
}
fn line(r: &mut SwRenderer<'_>, a: Point, b: Point, c: Color, opa: u8, clip: Rect) {
    emit(
        r,
        &DrawCommand::Line {
            p1: a,
            p2: b,
            transform: Transform::IDENTITY,
            color: c,
            width: 2.into(),
            opa,
        },
        clip,
    )
}
fn fallback() -> Texture<'static> {
    Texture::owned(1, 1, ColorFormat::RGBA8888).unwrap()
}
#[test]
fn layered_heads_match_vector_pixels_and_resource_lifecycle() {
    let images = ResourceManager::new(MaxSize::Bytes(8192), fallback());
    let state = Sprites::new(&images, [Color::rgb(31, 77, 191), Color::rgb(71, 143, 221)]);
    let mut cases = 0;
    for colors in [
        [Color::rgb(31, 77, 191), Color::rgb(71, 143, 221)],
        [
            Color::rgba(35, 113, 219, 127),
            Color::rgba(113, 77, 29, 200),
        ],
    ] {
        let atlas = state.atlas(&images, colors).unwrap();
        assert_eq!(atlas.read_pixels().unwrap().bytes().len(), 6400);
        for format in [
            ColorFormat::RGBA8888,
            ColorFormat::BGRA8888,
            ColorFormat::RGB888,
            ColorFormat::RGB565,
            ColorFormat::RGB565Swapped,
        ] {
            for mode in [AlphaMode::Opaque, AlphaMode::Blend] {
                for velocity in [30, 40, 50, 60, 70, 80, 90, 100] {
                    for up in [false, true] {
                        for active in [false, true] {
                            for clip in [Rect::new(0, 0, 24, 24), Rect::new(11, 9, 5, 7)] {
                                let mut a =
                                    SwRenderer::new(Texture::owned(24, 24, format).unwrap());
                                let mut b =
                                    SwRenderer::new(Texture::owned(24, 24, format).unwrap());
                                for r in [&mut a, &mut b] {
                                    r.target.alpha_mode = mode;
                                    for (i, v) in r
                                        .target
                                        .pixels_mut()
                                        .unwrap()
                                        .bytes_mut()
                                        .iter_mut()
                                        .enumerate()
                                    {
                                        *v = (i as u8).wrapping_mul(71).wrapping_add(11);
                                    }
                                }
                                let color = colors[usize::from(active)];
                                let point = Point::new(12, 12);
                                let tip = if up { 3 } else { -3 };
                                line(
                                    &mut a,
                                    point,
                                    Point::new(9, 12 + tip),
                                    color,
                                    100 + velocity,
                                    clip,
                                );
                                line(
                                    &mut a,
                                    point,
                                    Point::new(15, 12 + tip),
                                    color,
                                    100 + velocity,
                                    clip,
                                );
                                assert!(draw_heads(
                                    &atlas,
                                    point,
                                    up,
                                    velocity,
                                    active,
                                    Transform::IDENTITY,
                                    |cmd| emit(&mut b, cmd, clip)
                                ));
                                let aa = a.target.read_pixels().unwrap();
                                let bb = b.target.read_pixels().unwrap();
                                if let Some(i) =
                                    aa.bytes().iter().zip(bb.bytes()).position(|(a, b)| a != b)
                                {
                                    panic!(
                                        "{format:?} {mode:?} {velocity} {up} {active} {clip:?} byte={i} old={} new={}",
                                        aa.bytes()[i],
                                        bb.bytes()[i]
                                    );
                                }
                                cases += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    images.remove_token("chord-prompter/arrow-heads");
    assert!(
        state
            .atlas(&images, [Color::rgb(31, 77, 191), Color::rgb(71, 143, 221)])
            .is_some()
    );
    let tiny = ResourceManager::new(MaxSize::Bytes(1), fallback());
    let small = Sprites::new(&tiny, [Color::rgb(1, 2, 3); 2]);
    assert!(small.atlas(&tiny, [Color::rgb(1, 2, 3); 2]).is_none());
    assert!(small.atlas(&tiny, [Color::rgb(1, 2, 3); 2]).is_none());
    assert_eq!(cases, 1280);
}

#[test]
fn palette_replacement_preserves_leases_and_eviction_rebuilds_the_atlas() {
    let images = ResourceManager::new(MaxSize::Bytes(8192), fallback());
    let colors = [Color::rgb(31, 77, 191), Color::rgb(71, 143, 221)];
    let state = Sprites::new(&images, colors);
    let first = state.atlas(&images, colors).unwrap();
    let pointer = first.read_pixels().unwrap().bytes().as_ptr();
    assert_eq!(
        state
            .atlas(&images, colors)
            .unwrap()
            .read_pixels()
            .unwrap()
            .bytes()
            .as_ptr(),
        pointer
    );
    let old = first.read_pixels().unwrap().bytes().to_vec();
    let other = [Color::rgba(35, 113, 219, 127), Color::rgb(113, 77, 29)];
    let next = state.atlas(&images, other).unwrap();
    assert_ne!(next.cache_revision, first.cache_revision);
    assert_eq!(first.read_pixels().unwrap().bytes(), old);
    assert_ne!(next.read_pixels().unwrap().bytes(), old);
    drop(first);
    drop(next);
    images.add_factory("pressure", || {
        Some(Texture::owned(40, 40, ColorFormat::RGBA8888).unwrap())
    });
    drop(images.resolve("pressure"));
    let rebuilt = state.atlas(&images, other).unwrap();
    assert_eq!(rebuilt.cache_revision, revision(other));
    assert_eq!(
        rebuilt.read_pixels().unwrap().bytes(),
        bake(other).unwrap().read_pixels().unwrap().bytes()
    );
}

#[test]
fn fractional_scaled_and_projected_draws_keep_vector_admission() {
    assert!(eligible(
        Fixed::ONE,
        Transform::translate(3.into(), (-2).into()),
        None
    ));
    for transform in [
        Transform::translate(Fixed::from_ratio(1, 2), Fixed::ZERO),
        Transform::scale(2.into(), 2.into()),
        Transform::rotate_deg(45.into()),
        Transform::skew_deg(1.into(), 0.into()),
    ] {
        assert!(!eligible(Fixed::ONE, transform, None));
    }
    assert!(!eligible(Fixed::from_int(2), Transform::IDENTITY, None));
    assert!(!eligible(
        Fixed::ONE,
        Transform::IDENTITY,
        Some([Point::ZERO; 4])
    ));
}

#[test]
fn complete_ui_matches_budget_fallback_across_patterns_and_playback() {
    use crate::surface::FramebufferAccess;
    let build = |budget| {
        let mut app = App::headless(128, 128);
        app.with_default_widgets().with_default_systems();
        app.add_resource(MonoClock::new(|| 0));
        app.add_resource(ResourceManager::new(MaxSize::Bytes(budget), fallback()));
        let root = app.spawn_root().id();
        let session = super::super::runtime::install(&mut app, root);
        (app, session)
    };
    let (mut cached, session) = build(8192);
    let (mut vectors, control) = build(0);
    for pattern in super::super::Pattern::ALL {
        let source = super::super::PRACTICE
            .replace("strum: pop", &alloc::format!("strum: {}", pattern.name()));
        session.load(&source).unwrap();
        control.load(&source).unwrap();
        session.toggle(0);
        control.toggle(0);
        for time in [0, 16_667, 312_500, 625_000, 937_500, 1_875_000, 2_490_000] {
            session.advance(time);
            control.advance(time);
            cached.render_dirty().unwrap();
            vectors.render_dirty().unwrap();
            assert_eq!(
                cached.backend.framebuffer().read_pixels().unwrap().bytes(),
                vectors.backend.framebuffer().read_pixels().unwrap().bytes(),
                "{} at {time}",
                pattern.name()
            );
        }
    }
}
