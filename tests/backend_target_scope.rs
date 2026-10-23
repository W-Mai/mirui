use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};

use mirui::render::texture::{ColorFormat, TexturePixelsMut};
use mirui::render::{
    BackendTarget, PlaneLayout, RenderError, Renderer, TargetAlpha, TargetDescriptor, TargetUpdate,
};
use mirui::types::{Fixed, Viewport};

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

struct Target(u8);

impl BackendTarget for Target {
    fn descriptor(&self) -> TargetDescriptor {
        TargetDescriptor {
            layout: PlaneLayout::packed(1, 1, ColorFormat::RGBA8888).unwrap(),
            alpha: TargetAlpha::Straight,
        }
    }

    fn allocated_bytes(&self) -> usize {
        4
    }

    fn validate(&self) -> Result<(), RenderError> {
        if self.0 == 0 {
            Err(RenderError::InvalidTexture)
        } else {
            Ok(())
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn read_pixels_into(
        &self,
        _: mirui::render::texture::Region,
        _: &mut TexturePixelsMut<'_>,
    ) -> Result<(), RenderError> {
        Err(RenderError::BackendFailure)
    }
}

#[derive(Default)]
struct Recorder {
    active: u8,
    events: Vec<(u8, u8)>,
    flushes: usize,
    fail_flush: usize,
    fail_bind: bool,
    fail_restore: bool,
    poisoned: bool,
}

impl Renderer for Recorder {
    type TargetState = u8;

    fn submit(&mut self, _: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
        self.flush()
    }

    fn flush(&mut self) -> Result<(), RenderError> {
        self.flushes += 1;
        self.events.push((self.active, 1));
        if self.poisoned || self.flushes == self.fail_flush {
            Err(RenderError::BackendFailure)
        } else {
            Ok(())
        }
    }

    fn bind_target(
        &mut self,
        target: &mut dyn BackendTarget,
        _: Viewport,
        _: TargetUpdate,
        _: &mut [u8],
    ) -> Result<u8, RenderError> {
        if self.fail_bind {
            return Err(RenderError::InvalidScope);
        }
        let next = target
            .as_any()
            .downcast_ref::<Target>()
            .ok_or(RenderError::InvalidTexture)?
            .0;
        self.events.push((next, 2));
        Ok(core::mem::replace(&mut self.active, next))
    }

    fn restore_target(&mut self, state: u8, completed: bool) -> Result<(), RenderError> {
        self.events
            .push((self.active, if completed { 3 } else { 4 }));
        self.active = state;
        if self.fail_restore {
            self.poisoned = true;
            Err(RenderError::InvalidScope)
        } else {
            Ok(())
        }
    }
}

fn viewport() -> Viewport {
    Viewport::new(1, 1, Fixed::ONE)
}

#[test]
fn dynamic_scopes_restore_nested_targets_without_allocating() {
    let mut renderer = Recorder {
        events: Vec::with_capacity(16),
        ..Default::default()
    };
    let mut outer = Target(1);
    let mut inner = Target(2);
    let allocations = tracking_allocator::tracked_allocations(|| {
        let erased: &mut dyn Renderer = &mut renderer;
        erased
            .with_target(
                &mut outer,
                viewport(),
                TargetUpdate::Rebuild,
                &mut [],
                &mut |r| {
                    r.with_target(
                        &mut inner,
                        viewport(),
                        TargetUpdate::Rebuild,
                        &mut [],
                        &mut |_| Ok(()),
                    )
                },
            )
            .unwrap();
    });
    assert_eq!(allocations, 0);
    assert_eq!(renderer.active, 0);
    assert_eq!(
        renderer.events,
        [
            (0, 1),
            (1, 2),
            (1, 1),
            (2, 2),
            (2, 1),
            (2, 3),
            (1, 1),
            (1, 3)
        ]
    );
}

#[test]
fn preparation_failures_never_execute_or_restore_an_unbound_target() {
    for (fail_flush, fail_bind, target_id, error) in [
        (1, false, 1, RenderError::BackendFailure),
        (0, true, 1, RenderError::InvalidScope),
        (0, false, 0, RenderError::InvalidTexture),
    ] {
        let mut renderer = Recorder {
            fail_flush,
            fail_bind,
            ..Default::default()
        };
        let result = renderer.with_target(
            &mut Target(target_id),
            viewport(),
            TargetUpdate::Rebuild,
            &mut [],
            &mut |_| panic!("body must not execute"),
        );
        assert_eq!(result, Err(error));
        assert_eq!(renderer.active, 0);
        assert_eq!(renderer.events, [(0, 1)]);
    }
}

#[test]
fn child_flush_failure_restores_as_incomplete() {
    let mut renderer = Recorder {
        fail_flush: 2,
        ..Default::default()
    };
    let result = renderer.with_target(
        &mut Target(1),
        viewport(),
        TargetUpdate::Rebuild,
        &mut [],
        &mut |_| Ok(()),
    );
    assert_eq!(result, Err(RenderError::BackendFailure));
    assert_eq!(renderer.active, 0);
    assert_eq!(renderer.events, [(0, 1), (1, 2), (1, 1), (1, 4)]);
}

#[test]
fn body_failure_skips_child_flush_and_preserves_first_error() {
    let mut renderer = Recorder {
        fail_restore: true,
        ..Default::default()
    };
    let result = renderer.with_target(
        &mut Target(1),
        viewport(),
        TargetUpdate::Rebuild,
        &mut [],
        &mut |_| Err(RenderError::InvalidGeometry),
    );
    assert_eq!(result, Err(RenderError::InvalidGeometry));
    assert_eq!(renderer.events, [(0, 1), (1, 2), (1, 4)]);
    assert!(renderer.poisoned);
    assert_eq!(renderer.flush(), Err(RenderError::BackendFailure));
}

#[test]
fn successful_body_reports_restore_failure() {
    let mut renderer = Recorder {
        fail_restore: true,
        ..Default::default()
    };
    let result = renderer.with_target(
        &mut Target(1),
        viewport(),
        TargetUpdate::Rebuild,
        &mut [],
        &mut |_| Ok(()),
    );
    assert_eq!(result, Err(RenderError::InvalidScope));
    assert_eq!(renderer.events, [(0, 1), (1, 2), (1, 1), (1, 3)]);
    assert!(renderer.poisoned);
}

#[test]
fn unwind_restores_each_nested_target_once() {
    let mut renderer = Recorder::default();
    let result = catch_unwind(AssertUnwindSafe(|| {
        renderer.with_target(
            &mut Target(1),
            viewport(),
            TargetUpdate::Rebuild,
            &mut [],
            &mut |r| {
                r.with_target(
                    &mut Target(2),
                    viewport(),
                    TargetUpdate::Rebuild,
                    &mut [],
                    &mut |_| panic!("draw failure"),
                )
            },
        )
    }));
    assert!(result.is_err());
    assert_eq!(renderer.active, 0);
    assert_eq!(
        renderer.events,
        [(0, 1), (1, 2), (1, 1), (2, 2), (2, 4), (1, 4)]
    );
}

#[test]
fn caught_inner_error_does_not_fail_the_outer_scope() {
    let mut renderer = Recorder::default();
    renderer
        .with_target(
            &mut Target(1),
            viewport(),
            TargetUpdate::Rebuild,
            &mut [],
            &mut |r| {
                assert_eq!(
                    r.with_target(
                        &mut Target(2),
                        viewport(),
                        TargetUpdate::Rebuild,
                        &mut [],
                        &mut |_| Err(RenderError::InvalidGeometry)
                    ),
                    Err(RenderError::InvalidGeometry)
                );
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(renderer.active, 0);
    assert_eq!(
        renderer.events,
        [(0, 1), (1, 2), (1, 1), (2, 2), (2, 4), (1, 1), (1, 3)]
    );
}
