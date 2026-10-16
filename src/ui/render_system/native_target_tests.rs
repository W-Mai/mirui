use super::*;
use crate::render::renderer::RegionRenderer;
use crate::render::texture::{ColorFormat, TexturePixelsMut};
use crate::render::{TargetAlpha, TargetDescriptor, TargetUpdate};
use core::any::Any;

struct Target(TargetDescriptor);

impl BackendTarget for Target {
    fn descriptor(&self) -> TargetDescriptor {
        self.0
    }

    fn allocated_bytes(&self) -> usize {
        self.0.layout.required_bytes()
    }

    fn validate(&self) -> Result<(), RenderError> {
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn read_pixels_into(&self, _: &mut TexturePixelsMut<'_>) -> Result<(), RenderError> {
        Err(RenderError::Unsupported(
            crate::render::RenderFeature::Readback,
        ))
    }
}

#[derive(Debug, PartialEq)]
struct Sample {
    size: (u16, u16),
    scale: Fixed,
    clip: Rect,
    projection: Transform3D,
    quad: Option<[Point; 4]>,
}

struct Recorder {
    viewport: Viewport,
    samples: Vec<Sample>,
    restored: Vec<bool>,
    updates: Vec<TargetUpdate>,
    flush_error: Option<RenderError>,
}

impl Recorder {
    fn new() -> Self {
        Self {
            viewport: Viewport::new(80, 60, Fixed::ONE),
            samples: Vec::new(),
            restored: Vec::new(),
            updates: Vec::new(),
            flush_error: None,
        }
    }
}

impl Renderer for Recorder {
    type TargetState = Viewport;

    fn plan_target(&self, layout: PlaneLayout) -> Result<Option<TargetPlan>, RenderError> {
        Ok(Some(TargetPlan {
            descriptor: TargetDescriptor {
                layout,
                alpha: TargetAlpha::Straight,
            },
            storage_bytes: layout.required_bytes(),
            workspace_bytes: 0,
            readback_bytes: 0,
        }))
    }

    fn create_target(&mut self, plan: &TargetPlan) -> Result<Box<dyn BackendTarget>, RenderError> {
        Ok(Box::new(Target(plan.descriptor)))
    }

    fn can_reuse_target(
        &self,
        target: &dyn BackendTarget,
        plan: &TargetPlan,
    ) -> Result<bool, RenderError> {
        if let Some(error) = self.flush_error {
            return Err(error);
        }
        Ok(target.as_any().is::<Target>() && target.descriptor() == plan.descriptor)
    }

    fn bind_target(
        &mut self,
        target: &mut dyn BackendTarget,
        viewport: Viewport,
        update: TargetUpdate,
        _: &mut [u8],
    ) -> Result<Viewport, RenderError> {
        self.updates.push(update);
        if !target.as_any().is::<Target>() || update != TargetUpdate::Rebuild {
            return Err(RenderError::InvalidTexture);
        }
        Ok(core::mem::replace(&mut self.viewport, viewport))
    }

    fn restore_target(&mut self, viewport: Viewport, completed: bool) -> Result<(), RenderError> {
        self.viewport = viewport;
        self.restored.push(completed);
        Ok(())
    }

    fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        request.validate()?;
        Ok(RenderRoute::Native)
    }

    fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        self.route(request)?;
        let DrawCommand::Fill { quad, .. } = request.command else {
            return Err(RenderError::InvalidGeometry);
        };
        self.samples.push(Sample {
            size: self.viewport.physical_size(),
            scale: self.output_scale(),
            clip: request.clip,
            projection: request
                .projective
                .compose(&Transform3D::from_affine(request.command.transform())),
            quad: *quad,
        });
        Ok(())
    }

    fn flush(&mut self) -> Result<(), RenderError> {
        self.flush_error.map_or(Ok(()), Err)
    }

    fn output_scale(&self) -> Fixed {
        self.viewport.scale()
    }
}

fn draw(renderer: &mut dyn Renderer, quad: Option<[Point; 4]>) -> Result<(), RenderError> {
    let command = DrawCommand::Fill {
        area: Rect::new(1, 2, 4, 5),
        transform: Transform::IDENTITY,
        quad,
        color: crate::types::Color::rgb(10, 20, 30),
        radius: Fixed::ZERO,
        opa: 255,
    };
    let request = DrawRequest::new(&command, Rect::new(7, 9, 10, 11));
    let route = renderer.route(&request)?;
    renderer.submit_with_route(&request, route)
}

#[test]
fn independent_targets_drop_parent_coordinates_and_restore_them_after_nested_failure() {
    for region_outside in [false, true] {
        let mut recorder = Recorder::new();
        let projection = Transform3D::translate(Fixed::from_int(10), Fixed::from_int(13));
        let origin = (Fixed::from_int(3), Fixed::from_int(4));
        let mut region;
        let mut projected;
        let renderer: &mut dyn Renderer = if region_outside {
            projected = ProjectiveRenderer {
                inner: &mut recorder,
                transform: projection,
            };
            region = RegionRenderer::new(&mut projected, origin.0, origin.1);
            &mut region
        } else {
            region = RegionRenderer::new(&mut recorder, origin.0, origin.1);
            projected = ProjectiveRenderer {
                inner: &mut region,
                transform: projection,
            };
            &mut projected
        };
        let layout = PlaneLayout::packed(40, 30, ColorFormat::RGBA8888).unwrap();
        let plan = renderer.plan_target(layout).unwrap().unwrap();
        assert_eq!(plan.descriptor.layout, layout);
        let mut first = renderer.create_target(&plan).unwrap();
        let mut second = renderer.create_target(&plan).unwrap();
        assert!(renderer.can_reuse_target(&*first, &plan).unwrap());
        let mut changed = plan;
        changed.descriptor.alpha = TargetAlpha::Premultiplied;
        assert!(!renderer.can_reuse_target(&*first, &changed).unwrap());
        let viewport = Viewport::new(40, 30, Fixed::from_int(2));
        let quad = [Point::new(Fixed::ONE, Fixed::ONE); 4];
        draw(renderer, None).unwrap();
        renderer
            .with_target(
                &mut *first,
                viewport,
                TargetUpdate::Rebuild,
                &mut [],
                &mut |local| {
                    draw(local, Some(quad))?;
                    let nested = local.with_target(
                        &mut *second,
                        Viewport::new(40, 30, Fixed::ONE),
                        TargetUpdate::Rebuild,
                        &mut [],
                        &mut |nested| {
                            draw(nested, None)?;
                            Err(RenderError::InvalidGeometry)
                        },
                    );
                    assert_eq!(nested, Err(RenderError::InvalidGeometry));
                    draw(local, None)
                },
            )
            .unwrap();
        draw(renderer, None).unwrap();
        assert_eq!(recorder.restored, [false, true]);
        assert_eq!(recorder.samples.len(), 5);
        assert_eq!(recorder.samples[0], recorder.samples[4]);
        assert_eq!(recorder.samples[0].size, (80, 60));
        assert_eq!(recorder.samples[0].clip, Rect::new(4, 5, 10, 11));
        assert_eq!(
            recorder.samples[0].projection,
            Transform3D::translate(Fixed::from_int(7), Fixed::from_int(9))
        );
        for (sample, scale) in recorder.samples[1..4].iter().zip([2, 1, 2]) {
            assert_eq!(sample.size, (40, 30));
            assert_eq!(sample.scale, Fixed::from_int(scale));
            assert_eq!(sample.clip, Rect::new(7, 9, 10, 11));
            assert_eq!(sample.projection, Transform3D::IDENTITY);
        }
        assert_eq!(recorder.samples[1].quad, Some(quad));
    }
}

#[test]
fn wrapped_target_scope_preserves_parent_flush_failure() {
    let mut recorder = Recorder::new();
    recorder.flush_error = Some(RenderError::BackendFailure);
    let mut projected = ProjectiveRenderer {
        inner: &mut recorder,
        transform: Transform3D::translate(Fixed::ONE, Fixed::ONE),
    };
    let mut region = RegionRenderer::new(&mut projected, Fixed::ONE, Fixed::ONE);
    let layout = PlaneLayout::packed(1, 1, ColorFormat::RGBA8888).unwrap();
    let plan = region.plan_target(layout).unwrap().unwrap();
    let mut target = region.create_target(&plan).unwrap();
    assert_eq!(
        region.can_reuse_target(&*target, &plan),
        Err(RenderError::BackendFailure)
    );
    assert_eq!(
        region.with_target(
            &mut *target,
            Viewport::new(1, 1, Fixed::ONE),
            TargetUpdate::Rebuild,
            &mut [],
            &mut |_| panic!("failed parent work must prevent target production"),
        ),
        Err(RenderError::BackendFailure)
    );
    assert!(recorder.restored.is_empty());
    assert!(recorder.samples.is_empty());
}

#[test]
fn coordinate_wrappers_forward_incremental_intent_without_rebuilding() {
    for region_outside in [false, true] {
        let mut recorder = Recorder::new();
        let mut region;
        let mut projected;
        let renderer: &mut dyn Renderer = if region_outside {
            projected = ProjectiveRenderer {
                inner: &mut recorder,
                transform: Transform3D::translate(Fixed::ONE, Fixed::ONE),
            };
            region = RegionRenderer::new(&mut projected, Fixed::ONE, Fixed::ONE);
            &mut region
        } else {
            region = RegionRenderer::new(&mut recorder, Fixed::ONE, Fixed::ONE);
            projected = ProjectiveRenderer {
                inner: &mut region,
                transform: Transform3D::translate(Fixed::ONE, Fixed::ONE),
            };
            &mut projected
        };
        let layout = PlaneLayout::packed(1, 1, ColorFormat::RGBA8888).unwrap();
        let plan = renderer.plan_target(layout).unwrap().unwrap();
        let mut target = renderer.create_target(&plan).unwrap();
        assert_eq!(
            renderer.with_target(
                target.as_mut(),
                Viewport::new(1, 1, Fixed::ONE),
                TargetUpdate::Incremental,
                &mut [],
                &mut |_| panic!("a rejected update must not run its callback"),
            ),
            Err(RenderError::InvalidTexture)
        );
        assert_eq!(recorder.updates, [TargetUpdate::Incremental]);
        assert!(recorder.restored.is_empty());
        assert!(recorder.samples.is_empty());
    }
}
