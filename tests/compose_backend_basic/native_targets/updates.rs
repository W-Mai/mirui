use std::panic::{AssertUnwindSafe, catch_unwind};

use super::*;

fn target(renderer: &mut dyn Renderer) -> Box<dyn BackendTarget> {
    let layout = PlaneLayout::packed(1, 1, ColorFormat::RGBA8888).unwrap();
    let plan = renderer.plan_target(layout).unwrap().unwrap();
    renderer.create_target(&plan).unwrap()
}

#[test]
fn failed_updates_require_rebuilding_before_reading_or_appending() {
    let viewport = Viewport::new(1, 1, Fixed::ONE);
    for update in [TargetUpdate::Rebuild, TargetUpdate::Incremental] {
        for fault in [Some(Fault::Initialize), Some(Fault::Flush), None] {
            let mut screen = [0; 4];
            let inner_count = Cell::new(0);
            let outer_count = Cell::new(0);
            let mut renderer = NativeOuter::new(
                NativeInner::new(BorrowedDummy::new(&mut screen), Inner(&inner_count)),
                Outer(&outer_count),
            );
            let mut image = target(&mut renderer);
            renderer
                .with_target(
                    image.as_mut(),
                    viewport,
                    TargetUpdate::Rebuild,
                    &mut [],
                    &mut draw,
                )
                .unwrap();
            renderer.target.target.fault = fault;
            let mut calls = 0;
            let error = renderer.with_target(image.as_mut(), viewport, update, &mut [], &mut |r| {
                calls += 1;
                draw(r)?;
                if fault.is_none() {
                    Err(RenderError::InvalidGeometry)
                } else {
                    Ok(())
                }
            });
            assert_eq!(
                error,
                Err(if fault.is_none() {
                    RenderError::InvalidGeometry
                } else {
                    RenderError::BackendFailure
                })
            );
            assert_eq!(calls, usize::from(fault != Some(Fault::Initialize)));
            assert!(renderer.target.target.active.is_none());
            assert_eq!(read(image.as_ref()), Err(RenderError::InvalidTexture));
            let plan = renderer
                .plan_target(image.descriptor().layout)
                .unwrap()
                .unwrap();
            assert!(renderer.can_reuse_target(image.as_ref(), &plan).unwrap());
            assert_eq!(
                renderer.with_target(
                    image.as_mut(),
                    viewport,
                    TargetUpdate::Incremental,
                    &mut [],
                    &mut |_| panic!("failed contents cannot be reused"),
                ),
                Err(RenderError::InvalidTexture)
            );
            renderer
                .with_target(
                    image.as_mut(),
                    viewport,
                    TargetUpdate::Rebuild,
                    &mut [],
                    &mut draw,
                )
                .unwrap();
            assert_eq!(read(image.as_ref()), Ok(1));
            draw(&mut renderer).unwrap();
            assert_eq!(screen[0], 1);
        }
    }
}

#[test]
fn nested_unwind_invalidates_both_writes_without_poisoning_the_parent() {
    let mut screen = [0; 4];
    let count = Cell::new(0);
    let mut renderer = NativeInner::new(BorrowedDummy::new(&mut screen), Inner(&count));
    let mut first = target(&mut renderer);
    let mut second = target(&mut renderer);
    let viewport = Viewport::new(1, 1, Fixed::ONE);
    renderer
        .with_target(
            first.as_mut(),
            viewport,
            TargetUpdate::Rebuild,
            &mut [],
            &mut draw,
        )
        .unwrap();
    let panic = catch_unwind(AssertUnwindSafe(|| {
        let _ = renderer.with_target(
            first.as_mut(),
            viewport,
            TargetUpdate::Incremental,
            &mut [],
            &mut |outer| {
                draw(outer)?;
                outer.with_target(
                    second.as_mut(),
                    viewport,
                    TargetUpdate::Rebuild,
                    &mut [],
                    &mut |inner| {
                        draw(inner)?;
                        panic!("interrupted nested production")
                    },
                )
            },
        );
    }));
    assert!(panic.is_err());
    assert!(renderer.target.active.is_none());
    for image in [&mut first, &mut second] {
        assert_eq!(read(image.as_ref()), Err(RenderError::InvalidTexture));
        assert_eq!(
            renderer.with_target(
                image.as_mut(),
                viewport,
                TargetUpdate::Incremental,
                &mut [],
                &mut |_| panic!("unwound contents cannot be reused"),
            ),
            Err(RenderError::InvalidTexture)
        );
        renderer
            .with_target(
                image.as_mut(),
                viewport,
                TargetUpdate::Rebuild,
                &mut [],
                &mut draw,
            )
            .unwrap();
        assert_eq!(read(image.as_ref()), Ok(1));
    }
    draw(&mut renderer).unwrap();
    assert_eq!(screen[0], 1);
}

#[test]
fn nested_restoration_failure_cannot_publish_either_target() {
    for body_fails in [false, true] {
        let mut screen = [0; 4];
        let count = Cell::new(0);
        let mut renderer = NativeInner::new(BorrowedDummy::new(&mut screen), Inner(&count));
        let mut first = target(&mut renderer);
        let mut second = target(&mut renderer);
        let viewport = Viewport::new(1, 1, Fixed::ONE);
        renderer.target.fault = Some(Fault::Restore);
        let result = renderer.with_target(
            first.as_mut(),
            viewport,
            TargetUpdate::Rebuild,
            &mut [],
            &mut |outer| {
                draw(outer)?;
                let inner = outer.with_target(
                    second.as_mut(),
                    viewport,
                    TargetUpdate::Rebuild,
                    &mut [],
                    &mut |inner| {
                        draw(inner)?;
                        if body_fails {
                            Err(RenderError::InvalidGeometry)
                        } else {
                            Ok(())
                        }
                    },
                );
                assert_eq!(
                    inner,
                    Err(if body_fails {
                        RenderError::InvalidGeometry
                    } else {
                        RenderError::InvalidScope
                    })
                );
                assert_eq!(draw(outer), Err(RenderError::InvalidScope));
                Ok(())
            },
        );
        assert_eq!(result, Err(RenderError::InvalidScope));
        assert!(renderer.target.active.is_none());
        assert!(renderer.target.poisoned);
        for image in [&mut first, &mut second] {
            assert_eq!(read(image.as_ref()), Err(RenderError::InvalidTexture));
            assert_eq!(
                renderer.with_target(
                    image.as_mut(),
                    viewport,
                    TargetUpdate::Rebuild,
                    &mut [],
                    &mut |_| panic!("poisoned device cannot start production"),
                ),
                Err(RenderError::InvalidScope)
            );
        }
    }
}

#[test]
fn nested_incremental_scopes_and_flushes_preserve_pixels_without_allocating() {
    let mut screen = [0; 4];
    let inner_count = Cell::new(0);
    let outer_count = Cell::new(0);
    let mut renderer = NativeOuter::new(
        NativeInner::new(BorrowedDummy::new(&mut screen), Inner(&inner_count)),
        Outer(&outer_count),
    );
    let mut first = target(&mut renderer);
    let mut second = target(&mut renderer);
    let viewport = Viewport::new(1, 1, Fixed::ONE);
    for image in [&mut first, &mut second] {
        renderer
            .with_target(
                image.as_mut(),
                viewport,
                TargetUpdate::Rebuild,
                &mut [],
                &mut draw,
            )
            .unwrap();
    }
    let path = Path::rect(0.into(), 0.into(), 1.into(), 1.into());
    let paint = Paint::Color(Color::rgb(1, 0, 0).into());
    let command = mirui::render::DrawCommand::FillPath {
        path: &path,
        paint: &paint,
        transform: Transform::IDENTITY,
        opa: 255,
        fill_rule: mirui::render::raster::FillRule::NonZero,
    };
    let request = DrawRequest::new(&command, Rect::new(0, 0, 1, 1));
    let allocations = tracking_allocator::tracked_allocations(|| {
        for _ in 0..8 {
            renderer
                .with_target(
                    first.as_mut(),
                    viewport,
                    TargetUpdate::Incremental,
                    &mut [],
                    &mut |outer| {
                        outer.draw_request(&request)?;
                        outer.flush()?;
                        outer.with_target(
                            second.as_mut(),
                            viewport,
                            TargetUpdate::Incremental,
                            &mut [],
                            &mut |inner| inner.draw_request(&request),
                        )?;
                        outer.draw_request(&request)
                    },
                )
                .unwrap();
        }
    });
    assert_eq!(allocations, 0);
    assert_eq!(read_color(first.as_ref()), Ok(Color::rgba(17, 0, 0, 255)));
    assert_eq!(read_color(second.as_ref()), Ok(Color::rgba(9, 0, 0, 255)));
    assert_eq!(inner_count.get(), 26);
    assert_eq!(outer_count.get(), 26);
}
