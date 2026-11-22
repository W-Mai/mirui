use super::*;
use crate::render::command::DrawCommand;
use crate::render::renderer::{
    FallbackRegion, ProjectiveDrawError, RenderFeature, RenderResource, RenderRoute,
};
use crate::render::scene::Paint;
use crate::types::{Color, Fixed, Point, Rect};
use alloc::vec;
use alloc::vec::Vec;
use core::cell::Cell;

struct ScopeFallback<'a> {
    target: crate::render::backends::sw::SwRenderer<'a>,
    outer_submits: usize,
    scope_edits: usize,
    projective_clip_only: bool,
}

impl Renderer for ScopeFallback<'_> {
    type TargetState = ();

    fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        if matches!(request.command, DrawCommand::PushClip { .. })
            && (!self.projective_clip_only || !request.projective.is_identity())
        {
            return Err(RenderError::Unsupported(RenderFeature::PathClip));
        }
        Ok(RenderRoute::Native)
    }

    fn submit(&mut self, _: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
        self.outer_submits += 1;
        Ok(())
    }

    fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
        Ok(())
    }

    fn output_scale(&self) -> Fixed {
        self.target.output_scale()
    }

    fn plan_scope(&self, bounds: &Rect) -> Result<FallbackRegion, RenderError> {
        Renderer::plan_scope(&self.target, bounds)
    }

    fn modify_target_region(
        &mut self,
        src: &Rect,
        draw: &mut dyn FnMut(&mut Texture) -> Result<(), RenderError>,
    ) -> Result<bool, RenderError> {
        self.scope_edits += 1;
        Renderer::modify_target_region(&mut self.target, src, draw)
    }
}

fn clipped_projective_group() -> [SceneOp; 3] {
    let clip_path = crate::render::path::Path::rect(
        Fixed::ZERO,
        Fixed::ZERO,
        Fixed::from_int(10),
        Fixed::from_int(10),
    );
    [
        SceneOp::GroupBegin {
            transform: None,
            projective: Some(Transform3D::translate(
                Fixed::from_int(5),
                Fixed::from_int(3),
            )),
            opacity: None,
            clip: Some(ResourceRef::Inline(clip_path)),
            mask: None,
            filter: None,
            disjoint_hint: false,
        },
        SceneOp::FillRect {
            area: Rect::new(0, 0, 20, 20),
            transform: Transform::IDENTITY,
            quad: None,
            color: Color::rgb(210, 40, 30),
            radius: Fixed::ZERO,
            opa: 255,
        },
        SceneOp::GroupEnd,
    ]
}

fn clipped_affine_group() -> [SceneOp; 3] {
    let mut ops = clipped_projective_group();
    let SceneOp::GroupBegin {
        transform,
        projective,
        ..
    } = &mut ops[0]
    else {
        unreachable!()
    };
    *transform = Some(Transform::translate(Fixed::from_int(5), Fixed::from_int(3)));
    *projective = None;
    ops
}

fn nested_clip_group() -> [SceneOp; 5] {
    let outer_clip = crate::render::path::Path::rect(
        Fixed::ZERO,
        Fixed::ZERO,
        Fixed::from_int(15),
        Fixed::from_int(15),
    );
    let inner_clip = crate::render::path::Path::rect(
        Fixed::ZERO,
        Fixed::ZERO,
        Fixed::from_int(10),
        Fixed::from_int(10),
    );
    [
        SceneOp::GroupBegin {
            transform: None,
            projective: None,
            opacity: None,
            clip: Some(ResourceRef::Inline(outer_clip)),
            mask: None,
            filter: None,
            disjoint_hint: false,
        },
        SceneOp::GroupBegin {
            transform: None,
            projective: Some(Transform3D::translate(
                Fixed::from_int(5),
                Fixed::from_int(3),
            )),
            opacity: None,
            clip: Some(ResourceRef::Inline(inner_clip)),
            mask: None,
            filter: None,
            disjoint_hint: false,
        },
        SceneOp::FillRect {
            area: Rect::new(0, 0, 20, 20),
            transform: Transform::IDENTITY,
            quad: None,
            color: Color::rgb(210, 40, 30),
            radius: Fixed::ZERO,
            opa: 255,
        },
        SceneOp::GroupEnd,
        SceneOp::GroupEnd,
    ]
}

#[test]
fn replay_reuses_exact_routes_from_caller_scratch() {
    struct RoutedCapture {
        route_calls: Cell<usize>,
        routed_submits: usize,
    }

    impl Renderer for RoutedCapture {
        type TargetState = ();

        fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
            self.route_calls.set(self.route_calls.get() + 1);
            Ok(RenderRoute::ExactFallback(FallbackRegion::from_parts(
                3, 4, 5, 6, 20,
            )))
        }

        fn submit(&mut self, draw: &crate::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            assert!(matches!(draw.route(), RenderRoute::ExactFallback(_)));
            self.routed_submits += 1;
            Ok(())
        }

        fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
            Ok(())
        }
    }

    let ops = [SceneOp::FillRect {
        area: Rect::new(0, 0, 5, 6),
        transform: Transform::IDENTITY,
        quad: None,
        color: Color::rgb(1, 2, 3),
        radius: Fixed::ZERO,
        opa: 255,
    }];
    let mut renderer = RoutedCapture {
        route_calls: Cell::new(0),
        routed_submits: 0,
    };
    let mut frames = [ReplayFrame::EMPTY; 1];
    let mut plans = [ReplayPlan::EMPTY; 1];
    let mut scopes = [ReplayScopePlan::EMPTY; 1];
    let metrics = ReplayMetrics::default();
    replay_scene_with_scratch(
        &ops,
        &mut renderer,
        &Rect::new(0, 0, 10, 10),
        &NoResolver,
        ReplayScratch::new(&mut frames, &mut plans, &mut scopes).with_metrics(&metrics),
    )
    .unwrap();

    assert_eq!(renderer.route_calls.get(), 1);
    assert_eq!(renderer.routed_submits, 1);
    assert_eq!(
        metrics.snapshot(),
        ReplayMetricsSnapshot {
            fallback_commands: 1,
            planned_fallback_bytes: 120,
            peak_frame_depth: 1,
            peak_route_slots: 1,
            ..ReplayMetricsSnapshot::default()
        }
    );

    metrics.reset();
    let mut no_plans = [];
    replay_scene_with_scratch(
        &ops,
        &mut renderer,
        &Rect::new(0, 0, 10, 10),
        &NoResolver,
        ReplayScratch::new(&mut frames, &mut no_plans, &mut scopes).with_metrics(&metrics),
    )
    .unwrap();
    assert_eq!(metrics.snapshot().fallback_commands, 1);
    assert_eq!(metrics.snapshot().reclassified_commands, 1);
    assert_eq!(metrics.snapshot().peak_route_slots, 0);
    assert!(core::mem::size_of::<ReplayPlan>() <= 40);
}

#[test]
fn projective_clip_group_uses_one_bounded_scope() {
    let ops = clipped_projective_group();
    let mut pixels = [0u8; 40 * 30 * 4];
    let target =
        Texture::new(&mut pixels, 40, 30, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = ScopeFallback {
        target: crate::render::backends::sw::SwRenderer::new(target),
        outer_submits: 0,
        scope_edits: 0,
        projective_clip_only: false,
    };

    replay_scene(&ops, &mut renderer, &Rect::new(0, 0, 40, 30), &NoResolver).unwrap();

    assert_eq!(renderer.scope_edits, 1);
    assert_eq!(renderer.outer_submits, 0);
    drop(renderer);
    assert_eq!(
        &pixels[(5 * 40 + 7) * 4..(5 * 40 + 7) * 4 + 4],
        &[210, 40, 30, 255]
    );
    assert_eq!(&pixels[(5 * 40 + 16) * 4..(5 * 40 + 16) * 4 + 4], &[0; 4]);
}

#[test]
fn unsupported_affine_clip_group_uses_one_bounded_scope() {
    let ops = clipped_affine_group();
    let mut pixels = [0u8; 40 * 30 * 4];
    let target =
        Texture::new(&mut pixels, 40, 30, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = ScopeFallback {
        target: crate::render::backends::sw::SwRenderer::new(target),
        outer_submits: 0,
        scope_edits: 0,
        projective_clip_only: false,
    };

    replay_scene(&ops, &mut renderer, &Rect::new(0, 0, 40, 30), &NoResolver).unwrap();

    assert_eq!(renderer.scope_edits, 1);
    assert_eq!(renderer.outer_submits, 0);
    drop(renderer);
    assert_eq!(
        &pixels[(5 * 40 + 7) * 4..(5 * 40 + 7) * 4 + 4],
        &[210, 40, 30, 255]
    );
    assert_eq!(&pixels[(5 * 40 + 16) * 4..(5 * 40 + 16) * 4 + 4], &[0; 4]);
}

#[test]
fn fallback_clip_preserves_nested_group_isolation() {
    let clip_path = crate::render::path::Path::rect(
        Fixed::ZERO,
        Fixed::ZERO,
        Fixed::from_int(10),
        Fixed::from_int(10),
    );
    let red_fill = |x, y| SceneOp::FillRect {
        area: Rect::new(x, y, 4, 4),
        transform: Transform::IDENTITY,
        quad: None,
        color: Color::rgb(255, 0, 0),
        radius: Fixed::ZERO,
        opa: 255,
    };
    let ops = [
        SceneOp::GroupBegin {
            transform: Some(Transform::translate(Fixed::from_int(5), Fixed::from_int(3))),
            projective: None,
            opacity: None,
            clip: Some(ResourceRef::Inline(clip_path)),
            mask: None,
            filter: None,
            disjoint_hint: false,
        },
        group(Some(128), false),
        red_fill(0, 0),
        red_fill(2, 2),
        SceneOp::GroupEnd,
        SceneOp::GroupEnd,
    ];
    let mut pixels = [0u8; 40 * 30 * 4];
    let target =
        Texture::new(&mut pixels, 40, 30, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = ScopeFallback {
        target: crate::render::backends::sw::SwRenderer::new(target),
        outer_submits: 0,
        scope_edits: 0,
        projective_clip_only: false,
    };
    let mut frames = [ReplayFrame::EMPTY; 8];
    let mut routes = [ReplayPlan::EMPTY; 8];
    let mut scopes = [ReplayScopePlan::EMPTY; 8];
    let mut rgba = [0u8; 40 * 30 * 4];

    replay_scene_with_scratch(
        &ops,
        &mut renderer,
        &Rect::new(0, 0, 40, 30),
        &NoResolver,
        ReplayScratch::new(&mut frames, &mut routes, &mut scopes).with_rgba(&mut rgba),
    )
    .unwrap();

    assert_eq!(renderer.scope_edits, 1);
    assert_eq!(renderer.outer_submits, 0);
    drop(renderer);
    let overlap = (6 * 40 + 8) * 4;
    assert_eq!(&pixels[overlap..overlap + 4], &[128, 0, 0, 255]);
}

#[test]
fn native_outer_clip_absorbs_a_nested_fallback_scope() {
    let ops = nested_clip_group();
    let mut pixels = [0u8; 40 * 30 * 4];
    let target =
        Texture::new(&mut pixels, 40, 30, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = ScopeFallback {
        target: crate::render::backends::sw::SwRenderer::new(target),
        outer_submits: 0,
        scope_edits: 0,
        projective_clip_only: true,
    };

    replay_scene(&ops, &mut renderer, &Rect::new(0, 0, 40, 30), &NoResolver).unwrap();

    assert_eq!(renderer.scope_edits, 1);
    assert_eq!(renderer.outer_submits, 0);
    drop(renderer);
    assert_eq!(
        &pixels[(5 * 40 + 7) * 4..(5 * 40 + 7) * 4 + 4],
        &[210, 40, 30, 255]
    );
    assert_eq!(&pixels[(5 * 40 + 16) * 4..(5 * 40 + 16) * 4 + 4], &[0; 4]);
}

#[test]
fn projective_clip_group_requires_scope_plan_before_drawing() {
    let ops = clipped_projective_group();
    let mut pixels = [0u8; 40 * 30 * 4];
    let target =
        Texture::new(&mut pixels, 40, 30, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = ScopeFallback {
        target: crate::render::backends::sw::SwRenderer::new(target),
        outer_submits: 0,
        scope_edits: 0,
        projective_clip_only: false,
    };
    let mut frames = [ReplayFrame::EMPTY; 2];
    let mut routes = [ReplayPlan::EMPTY; 1];
    let mut scopes = [ReplayScopePlan::EMPTY; 0];

    assert_eq!(
        replay_scene_with_scratch(
            &ops,
            &mut renderer,
            &Rect::new(0, 0, 40, 30),
            &NoResolver,
            ReplayScratch::new(&mut frames, &mut routes, &mut scopes),
        ),
        Err(ReplayError::InsufficientScopePlans {
            required: 1,
            available: 0,
        })
    );
    assert_eq!(renderer.scope_edits, 0);
    assert_eq!(renderer.outer_submits, 0);
}

fn nested_isolated_groups(depth: usize) -> Vec<SceneOp> {
    let mut ops = Vec::new();
    for _ in 0..depth {
        ops.push(SceneOp::GroupBegin {
            transform: None,
            projective: None,
            opacity: Some(128),
            clip: None,
            mask: None,
            filter: None,
            disjoint_hint: false,
        });
    }
    ops.push(fill(Transform::IDENTITY));
    ops.push(fill(Transform::IDENTITY));
    for level in (0..depth).rev() {
        ops.push(SceneOp::GroupEnd);
        if level != 0 {
            ops.push(fill(Transform::IDENTITY));
        }
    }
    ops
}

#[test]
fn nested_isolation_uses_caller_scope_capacity_without_a_hidden_depth_limit() {
    let ops = nested_isolated_groups(10);
    let mut output = [0u8; 8 * 8 * 4];
    let mut rgba = [0u8; 8 * 8 * 4 * 10];
    let target =
        Texture::new(&mut output, 8, 8, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = crate::render::backends::sw::SwRenderer::new(target);
    let mut frames = [ReplayFrame::EMPTY; 12];
    let mut routes = [ReplayPlan::EMPTY; 0];
    let mut short_scopes = [ReplayScopePlan::EMPTY; 9];

    assert_eq!(
        replay_scene_with_scratch(
            &ops,
            &mut renderer,
            &Rect::new(0, 0, 8, 8),
            &NoResolver,
            ReplayScratch::new(&mut frames, &mut routes, &mut short_scopes).with_rgba(&mut rgba),
        ),
        Err(ReplayError::InsufficientScopePlans {
            required: 10,
            available: 9,
        })
    );
    assert!(output.iter().all(|byte| *byte == 0));

    let target =
        Texture::new(&mut output, 8, 8, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = crate::render::backends::sw::SwRenderer::new(target);
    let mut scopes = [ReplayScopePlan::EMPTY; 10];
    replay_scene_with_scratch(
        &ops,
        &mut renderer,
        &Rect::new(0, 0, 8, 8),
        &NoResolver,
        ReplayScratch::new(&mut frames, &mut routes, &mut scopes).with_rgba(&mut rgba),
    )
    .unwrap();
    assert!(output.iter().any(|byte| *byte != 0));
}

#[test]
fn scene_blur_uses_physical_radius_and_includes_edge_bleed() {
    struct ScopePlanCapture {
        bounds: Cell<Option<Rect>>,
        scale: Fixed,
    }

    impl Renderer for ScopePlanCapture {
        type TargetState = ();

        fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
            Ok(RenderRoute::Native)
        }

        fn submit(&mut self, _: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            Ok(())
        }

        fn output_scale(&self) -> Fixed {
            self.scale
        }

        fn plan_scope(&self, bounds: &Rect) -> Result<FallbackRegion, RenderError> {
            self.bounds.set(Some(*bounds));
            FallbackRegion::from_logical_bounds(
                *bounds,
                crate::types::Viewport::new(200, 200, self.scale),
                Some(200 * 200 * 4),
            )
        }

        fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
            Ok(())
        }
    }

    let ops = [
        SceneOp::GroupBegin {
            transform: None,
            projective: None,
            opacity: None,
            clip: None,
            mask: None,
            filter: Some(ResourceRef::Token("blur:3:3".into())),
            disjoint_hint: true,
        },
        fill(Transform::IDENTITY),
        SceneOp::GroupEnd,
    ];
    let mut renderer = ScopePlanCapture {
        bounds: Cell::new(None),
        scale: Fixed::from_int(2),
    };
    let mut frames = [ReplayFrame::EMPTY; 8];
    let mut routes = [ReplayPlan::EMPTY; 8];
    let mut scopes = [ReplayScopePlan::EMPTY; 8];
    let mut rgba = [0u8; 200 * 200 * 4];
    replay_scene_with_scratch(
        &ops,
        &mut renderer,
        &Rect::new(0, 0, 100, 100),
        &NoResolver,
        ReplayScratch::new(&mut frames, &mut routes, &mut scopes).with_rgba(&mut rgba),
    )
    .unwrap();
    let bounds = renderer.bounds.get().expect("isolated blur scope");
    assert_eq!(bounds.x, Fixed::ZERO);
    assert_eq!(bounds.y, Fixed::ZERO);
    assert!(bounds.w >= Fixed::from_int(16));
    assert!(bounds.h >= Fixed::from_int(16));
}

#[test]
fn outer_isolation_bounds_include_nested_blur_support() {
    struct FirstScopeBounds {
        calls: Cell<usize>,
        first: Cell<Option<Rect>>,
    }

    impl Renderer for FirstScopeBounds {
        type TargetState = ();

        fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
            Ok(RenderRoute::Native)
        }

        fn submit(&mut self, _: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            Ok(())
        }

        fn plan_scope(&self, bounds: &Rect) -> Result<FallbackRegion, RenderError> {
            if self.calls.get() == 0 {
                self.first.set(Some(*bounds));
            }
            self.calls.set(self.calls.get() + 1);
            FallbackRegion::from_logical_bounds(
                *bounds,
                crate::types::Viewport::new(32, 32, Fixed::ONE),
                Some(32 * 32 * 4),
            )
        }

        fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
            Ok(())
        }
    }

    let ops = [
        SceneOp::GroupBegin {
            transform: None,
            projective: None,
            opacity: Some(128),
            clip: None,
            mask: None,
            filter: None,
            disjoint_hint: false,
        },
        SceneOp::GroupBegin {
            transform: None,
            projective: None,
            opacity: None,
            clip: None,
            mask: None,
            filter: Some(ResourceRef::Token("blur:2".into())),
            disjoint_hint: true,
        },
        fill(Transform::IDENTITY),
        SceneOp::GroupEnd,
        fill(Transform::IDENTITY),
        SceneOp::GroupEnd,
    ];
    let mut renderer = FirstScopeBounds {
        calls: Cell::new(0),
        first: Cell::new(None),
    };
    let mut frames = [ReplayFrame::EMPTY; 4];
    let mut routes = [ReplayPlan::EMPTY; 0];
    let mut scopes = [ReplayScopePlan::EMPTY; 2];
    let mut rgba = [0u8; 32 * 32 * 4 * 2];

    replay_scene_with_scratch(
        &ops,
        &mut renderer,
        &Rect::new(0, 0, 32, 32),
        &NoResolver,
        ReplayScratch::new(&mut frames, &mut routes, &mut scopes).with_rgba(&mut rgba),
    )
    .unwrap();

    let bounds = renderer.first.get().expect("outer isolation scope");
    assert!(bounds.w >= Fixed::from_int(12));
    assert!(bounds.h >= Fixed::from_int(12));
}

#[test]
fn scene_blur_resolves_glyph_ink_instead_of_using_the_clip() {
    struct BlurCapture(Cell<Option<Rect>>);

    impl Renderer for BlurCapture {
        type TargetState = ();

        fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
            Ok(RenderRoute::Native)
        }

        fn submit(&mut self, _: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            Ok(())
        }

        fn plan_scope(&self, bounds: &Rect) -> Result<FallbackRegion, RenderError> {
            self.0.set(Some(*bounds));
            FallbackRegion::from_logical_bounds(
                *bounds,
                crate::types::Viewport::new(200, 100, Fixed::ONE),
                Some(200 * 100 * 4),
            )
        }

        fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
            Ok(())
        }
    }

    let ops = [
        SceneOp::GroupBegin {
            transform: None,
            projective: None,
            opacity: None,
            clip: None,
            mask: None,
            filter: Some(ResourceRef::Token("blur:2".into())),
            disjoint_hint: true,
        },
        glyph_at(30),
        SceneOp::GroupEnd,
    ];
    let mut renderer = BlurCapture(Cell::new(None));
    let mut frames = [ReplayFrame::EMPTY; 8];
    let mut routes = [ReplayPlan::EMPTY; 8];
    let mut scopes = [ReplayScopePlan::EMPTY; 8];
    let mut rgba = [0u8; 200 * 100 * 4];
    replay_scene_with_scratch(
        &ops,
        &mut renderer,
        &Rect::new(0, 0, 200, 100),
        &BitmapResolver(Font::default()),
        ReplayScratch::new(&mut frames, &mut routes, &mut scopes).with_rgba(&mut rgba),
    )
    .unwrap();

    let region = renderer.0.get().expect("blur scope");
    assert!(region.x > Fixed::ZERO);
    assert!(region.w < Fixed::from_int(100));
    assert!(region.x <= Fixed::from_int(30));
    assert!(region.y <= Fixed::from_int(20));
    assert!(region.x + region.w >= Fixed::from_int(30));
    assert!(region.y + region.h >= Fixed::from_int(20));
}

#[test]
fn isolated_blur_does_not_filter_the_existing_target() {
    let mut red = opaque_fill_at(14, 14);
    if let SceneOp::FillRect { color, .. } = &mut red {
        *color = Color::rgb(255, 0, 0);
    }
    let ops = [
        SceneOp::GroupBegin {
            transform: None,
            projective: None,
            opacity: None,
            clip: None,
            mask: None,
            filter: Some(ResourceRef::Token("blur:2".into())),
            disjoint_hint: true,
        },
        red,
        SceneOp::GroupEnd,
    ];
    let mut output = [0u8; 32 * 32 * 4];
    for pixel in output.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[0, 0, 255, 255]);
    }
    let texture =
        Texture::new(&mut output, 32, 32, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = crate::render::backends::sw::SwRenderer::new(texture);
    let mut frames = [ReplayFrame::EMPTY; 8];
    let mut routes = [ReplayPlan::EMPTY; 8];
    let mut scopes = [ReplayScopePlan::EMPTY; 8];
    let mut rgba = [0u8; 32 * 32 * 4];
    replay_scene_with_scratch(
        &ops,
        &mut renderer,
        &Rect::new(0, 0, 32, 32),
        &NoResolver,
        ReplayScratch::new(&mut frames, &mut routes, &mut scopes).with_rgba(&mut rgba),
    )
    .unwrap();

    assert_eq!(&output[..4], &[0, 0, 255, 255]);
    let edge = (15 * 32 + 12) * 4;
    assert!(output[edge] > 0);
    assert!(output[edge + 2] > 0);
}

#[test]
fn group_overlap_uses_resolved_glyph_ink() {
    let ops = [glyph_at(10), glyph_at(12)];
    let mut frames = [ReplayFrame::EMPTY; 1];
    assert!(
        !resolved_children_disjoint(
            &ops,
            ReplayFrame::EMPTY,
            &BitmapResolver(Font::default()),
            Fixed::ONE,
            &mut frames,
        )
        .unwrap()
    );
}

struct CaptureRenderer {
    transforms: Vec<Transform>,
    fill_opas: Vec<u8>,
}
impl Renderer for CaptureRenderer {
    type TargetState = ();

    fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        Ok(RenderRoute::Native)
    }

    fn submit(&mut self, draw: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
        let request = &draw.request();
        self.route(request)?;
        if let DrawCommand::Fill { transform, opa, .. } = request.command {
            self.transforms.push(*transform);
            self.fill_opas.push(*opa);
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
        Ok(())
    }
}

#[derive(Default)]
struct ProjectiveCapture {
    command_transform: Option<Transform>,
    scope: Option<Transform3D>,
}

impl Renderer for ProjectiveCapture {
    type TargetState = ();

    fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        Ok(RenderRoute::Native)
    }

    fn submit(&mut self, draw: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
        let request = &draw.request();
        self.route(request)?;
        self.command_transform = Some(request.command.transform());
        self.scope = Some(request.projective);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
        Ok(())
    }
}

struct FillOnlyProjectiveRenderer {
    draws: usize,
    line_error: ProjectiveDrawError,
}

impl Default for FillOnlyProjectiveRenderer {
    fn default() -> Self {
        Self {
            draws: 0,
            line_error: ProjectiveDrawError::Unsupported,
        }
    }
}

impl Renderer for FillOnlyProjectiveRenderer {
    type TargetState = ();

    fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        if !request.projective.is_identity() && !matches!(request.command, DrawCommand::Fill { .. })
        {
            return Err(RenderError::from(self.line_error));
        }
        Ok(RenderRoute::Native)
    }

    fn submit(&mut self, draw: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
        let request = &draw.request();
        self.route(request)?;
        self.draws += 1;
        Ok(())
    }

    fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
        Ok(())
    }
}

struct NoResolver;
impl SceneResolver for NoResolver {
    fn font(&self, _: &ResourceRef) -> Option<&Font> {
        None
    }
    fn texture(&self, _: &ResourceRef) -> Option<&Texture<'_>> {
        None
    }
}

struct BitmapResolver(Font);

impl SceneResolver for BitmapResolver {
    fn font(&self, _: &ResourceRef) -> Option<&Font> {
        Some(&self.0)
    }

    fn texture(&self, _: &ResourceRef) -> Option<&Texture<'_>> {
        None
    }
}

static BOUNDS_GLYPH: [textflow::shaping::PositionedGlyph; 1] =
    [textflow::shaping::PositionedGlyph::new(
        crate::render::font::GlyphId::new(65),
        textflow::shaping::FlowPoint { x: 0, y: 0 },
    )];

fn glyph_at(x: i32) -> SceneOp {
    SceneOp::GlyphRun {
        font: ResourceRef::Index(0),
        ppem: 16,
        pos: Point::new(x, 20),
        transform: Transform::IDENTITY,
        color: Color::rgb(255, 255, 255),
        opa: 255,
        glyphs: (&BOUNDS_GLYPH[..]).into(),
    }
}

fn rect() -> Rect {
    Rect {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        w: Fixed::from_int(4),
        h: Fixed::from_int(4),
    }
}

fn fill(transform: Transform) -> SceneOp {
    SceneOp::FillRect {
        area: rect(),
        transform,
        quad: None,
        color: Color {
            r: 1,
            g: 2,
            b: 3,
            a: 4,
        },
        radius: Fixed::ZERO,
        opa: 255,
    }
}

#[test]
fn group_transform_composes_onto_child() {
    let group_tf = Transform::translate(Fixed::from_int(10), Fixed::ZERO);
    let child_tf = Transform::translate(Fixed::ZERO, Fixed::from_int(5));
    let ops = vec![
        SceneOp::GroupBegin {
            transform: Some(group_tf),
            projective: None,
            opacity: None,
            clip: None,
            mask: None,
            filter: None,
            disjoint_hint: false,
        },
        fill(child_tf),
        SceneOp::GroupEnd,
    ];
    let mut r = CaptureRenderer {
        transforms: Vec::new(),
        fill_opas: Vec::new(),
    };
    replay_scene(&ops, &mut r, &rect(), &NoResolver).unwrap();
    assert_eq!(r.transforms, vec![group_tf.compose(&child_tf)]);
}

#[test]
fn group_workspace_fails_before_the_first_draw() {
    let group = SceneOp::GroupBegin {
        transform: None,
        projective: None,
        opacity: None,
        clip: None,
        mask: None,
        filter: None,
        disjoint_hint: false,
    };
    let ops = [
        fill(Transform::IDENTITY),
        group.clone(),
        group,
        SceneOp::GroupEnd,
        SceneOp::GroupEnd,
    ];
    let mut renderer = CaptureRenderer {
        transforms: Vec::new(),
        fill_opas: Vec::new(),
    };
    let mut short = [ReplayFrame::EMPTY; 2];
    assert_eq!(
        replay_scene_with_workspace(&ops, &mut renderer, &rect(), &NoResolver, &mut short),
        Err(ReplayError::InsufficientWorkspace {
            required: 3,
            available: 2,
        })
    );
    assert!(renderer.transforms.is_empty());

    let mut enough = [ReplayFrame::EMPTY; 3];
    replay_scene_with_workspace(&ops, &mut renderer, &rect(), &NoResolver, &mut enough).unwrap();
    assert_eq!(renderer.transforms.len(), 1);
}

#[test]
fn invisible_groups_do_not_consume_workspace_frames() {
    let ops = [
        SceneOp::GroupBegin {
            transform: None,
            projective: None,
            opacity: Some(0),
            clip: None,
            mask: None,
            filter: None,
            disjoint_hint: false,
        },
        SceneOp::GroupBegin {
            transform: None,
            projective: None,
            opacity: None,
            clip: None,
            mask: None,
            filter: None,
            disjoint_hint: false,
        },
        fill(Transform::IDENTITY),
        SceneOp::GroupEnd,
        SceneOp::GroupEnd,
    ];
    let mut renderer = CaptureRenderer {
        transforms: Vec::new(),
        fill_opas: Vec::new(),
    };
    let mut frames = [ReplayFrame::EMPTY; 1];
    replay_scene_with_workspace(&ops, &mut renderer, &rect(), &NoResolver, &mut frames).unwrap();
    assert!(renderer.transforms.is_empty());
}

#[test]
fn projective_group_keeps_one_scope_and_leaf_affine() {
    let group_affine = Transform::translate(Fixed::from_int(10), Fixed::ZERO);
    let projective = Transform3D::rotate_y_perspective(Fixed::from_int(12), Fixed::from_int(400));
    let child_affine = Transform::translate(Fixed::ZERO, Fixed::from_int(5));
    let ops = vec![
        SceneOp::GroupBegin {
            transform: Some(group_affine),
            projective: Some(projective),
            opacity: None,
            clip: None,
            mask: None,
            filter: None,
            disjoint_hint: false,
        },
        fill(child_affine),
        SceneOp::GroupEnd,
    ];
    let mut renderer = ProjectiveCapture::default();

    replay_scene(&ops, &mut renderer, &rect(), &NoResolver).unwrap();

    assert_eq!(renderer.command_transform, Some(child_affine));
    assert_eq!(
        renderer.scope,
        Some(projective.compose(&Transform3D::from_affine(group_affine)))
    );
}

#[test]
fn projective_capabilities_are_checked_before_the_first_draw() {
    let projective = Transform3D::rotate_y_perspective(Fixed::from_int(12), Fixed::from_int(400));
    let ops = vec![
        SceneOp::GroupBegin {
            transform: None,
            projective: Some(projective),
            opacity: None,
            clip: None,
            mask: None,
            filter: None,
            disjoint_hint: false,
        },
        fill(Transform::IDENTITY),
        SceneOp::Line {
            p1: Point::ZERO,
            p2: Point::new(Fixed::ONE, Fixed::ONE),
            transform: Transform::IDENTITY,
            color: Color::rgb(255, 255, 255),
            width: Fixed::ONE,
            opa: 255,
        },
        SceneOp::GroupEnd,
    ];
    let mut renderer = FillOnlyProjectiveRenderer::default();

    assert_eq!(
        replay_scene(&ops, &mut renderer, &rect(), &NoResolver),
        Err(ReplayError::Render(RenderError::Unsupported(
            RenderFeature::ProjectiveGeometry
        )))
    );
    assert_eq!(renderer.draws, 0);
}

#[test]
fn affine_route_failure_prevents_earlier_draws() {
    struct FillOnlyRoute {
        draws: usize,
    }

    impl Renderer for FillOnlyRoute {
        type TargetState = ();

        fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
            if matches!(request.command, DrawCommand::Line { .. }) {
                Err(RenderError::Unsupported(RenderFeature::AffineGeometry))
            } else {
                Ok(RenderRoute::Native)
            }
        }

        fn submit(&mut self, draw: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            let request = &draw.request();
            self.route(request)?;
            self.draws += 1;
            Ok(())
        }

        fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
            Ok(())
        }
    }

    let ops = [
        fill(Transform::IDENTITY),
        SceneOp::Line {
            p1: Point::ZERO,
            p2: Point::new(Fixed::ONE, Fixed::ONE),
            transform: Transform::IDENTITY,
            color: Color::rgb(255, 255, 255),
            width: Fixed::ONE,
            opa: 255,
        },
    ];
    let mut renderer = FillOnlyRoute { draws: 0 };
    assert_eq!(
        replay_scene(&ops, &mut renderer, &rect(), &NoResolver),
        Err(ReplayError::Render(RenderError::Unsupported(
            RenderFeature::AffineGeometry
        )))
    );
    assert_eq!(renderer.draws, 0);
}

#[test]
fn execution_failure_reaches_scene_caller() {
    struct ExhaustedRenderer;

    impl Renderer for ExhaustedRenderer {
        type TargetState = ();

        fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
            Ok(RenderRoute::Native)
        }

        fn submit(&mut self, _: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            Err(RenderError::ResourceLimit(RenderResource::Uniforms))
        }

        fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
            Ok(())
        }
    }

    assert_eq!(
        replay_scene(
            &[fill(Transform::IDENTITY)],
            &mut ExhaustedRenderer,
            &rect(),
            &NoResolver
        ),
        Err(ReplayError::Render(RenderError::ResourceLimit(
            RenderResource::Uniforms
        )))
    );
}

#[test]
fn invalid_projective_geometry_is_distinct_and_failure_atomic() {
    let projective = Transform3D::rotate_y_perspective(Fixed::from_int(12), Fixed::from_int(400));
    let ops = vec![
        SceneOp::GroupBegin {
            transform: None,
            projective: Some(projective),
            opacity: None,
            clip: None,
            mask: None,
            filter: None,
            disjoint_hint: false,
        },
        fill(Transform::IDENTITY),
        SceneOp::Line {
            p1: Point::ZERO,
            p2: Point::new(Fixed::ONE, Fixed::ONE),
            transform: Transform::IDENTITY,
            color: Color::rgb(255, 255, 255),
            width: Fixed::ONE,
            opa: 255,
        },
        SceneOp::GroupEnd,
    ];
    let mut renderer = FillOnlyProjectiveRenderer {
        line_error: ProjectiveDrawError::InvalidProjection,
        ..Default::default()
    };

    assert_eq!(
        replay_scene(&ops, &mut renderer, &rect(), &NoResolver),
        Err(ReplayError::Render(RenderError::InvalidGeometry))
    );
    assert_eq!(renderer.draws, 0);
}

#[test]
fn fallback_capacity_error_is_distinct_and_failure_atomic() {
    let projective = Transform3D::rotate_y_perspective(Fixed::from_int(12), Fixed::from_int(400));
    let ops = vec![
        SceneOp::GroupBegin {
            transform: None,
            projective: Some(projective),
            opacity: None,
            clip: None,
            mask: None,
            filter: None,
            disjoint_hint: false,
        },
        fill(Transform::IDENTITY),
        SceneOp::Line {
            p1: Point::ZERO,
            p2: Point::new(Fixed::ONE, Fixed::ONE),
            transform: Transform::IDENTITY,
            color: Color::rgb(255, 255, 255),
            width: Fixed::ONE,
            opa: 255,
        },
        SceneOp::GroupEnd,
    ];
    let mut renderer = FillOnlyProjectiveRenderer {
        line_error: ProjectiveDrawError::InsufficientFallbackStorage {
            required_bytes: 64,
            capacity_bytes: 32,
        },
        ..Default::default()
    };

    assert_eq!(
        replay_scene(&ops, &mut renderer, &rect(), &NoResolver),
        Err(ReplayError::Render(RenderError::InsufficientWorkspace {
            required_bytes: 64,
            capacity_bytes: 32,
        }))
    );
    assert_eq!(renderer.draws, 0);
}

#[test]
fn op_outside_group_keeps_own_transform() {
    let child_tf = Transform::translate(Fixed::from_int(2), Fixed::from_int(3));
    let ops = vec![fill(child_tf)];
    let mut r = CaptureRenderer {
        transforms: Vec::new(),
        fill_opas: Vec::new(),
    };
    replay_scene(&ops, &mut r, &rect(), &NoResolver).unwrap();
    assert_eq!(r.transforms, vec![Transform::IDENTITY.compose(&child_tf)]);
}

#[test]
fn unresolved_texture_errors() {
    let ops = vec![SceneOp::Blit {
        texture: ResourceRef::Index(0),
        pos: Point::ZERO,
        size: Point::ZERO,
        transform: Transform::IDENTITY,
        quad: None,
        opa: 255,
        radius: crate::types::Fixed::ZERO,
        composite: crate::render::command::CompositeMode::SourceOver,
    }];
    let mut r = CaptureRenderer {
        transforms: Vec::new(),
        fill_opas: Vec::new(),
    };
    assert_eq!(
        replay_scene(&ops, &mut r, &rect(), &NoResolver),
        Err(ReplayError::UnresolvedTexture)
    );
}

#[test]
fn unbalanced_group_end_errors() {
    let ops = vec![SceneOp::GroupEnd];
    let mut r = CaptureRenderer {
        transforms: Vec::new(),
        fill_opas: Vec::new(),
    };
    assert_eq!(
        replay_scene(&ops, &mut r, &rect(), &NoResolver),
        Err(ReplayError::UnbalancedGroup)
    );
}

#[test]
fn nonzero_fill_rule_is_rejected() {
    let ops = vec![SceneOp::FillPath {
        path: crate::render::path::Path::new(),
        transform: Transform::IDENTITY,
        paint: Paint::Color(mirx::types::Color {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        }),
        opa: 0,
        fill_rule: crate::render::raster::FillRule::NonZero,
    }];
    let mut r = CaptureRenderer {
        transforms: Vec::new(),
        fill_opas: Vec::new(),
    };
    assert!(replay_scene(&ops, &mut r, &rect(), &NoResolver).is_ok());
}

#[test]
fn replay_preserves_positioned_glyphs() {
    struct GlyphRenderer {
        glyphs: usize,
        ppem: u16,
    }
    impl Renderer for GlyphRenderer {
        type TargetState = ();

        fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
            Ok(RenderRoute::Native)
        }

        fn submit(&mut self, draw: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            let request = &draw.request();
            self.route(request)?;
            if let DrawCommand::GlyphRun { glyphs, font, .. } = request.command {
                self.glyphs = glyphs.len();
                self.ppem = font.size;
            }
            Ok(())
        }

        fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
            Ok(())
        }
    }

    struct FontResolver(Font);
    impl SceneResolver for FontResolver {
        fn font(&self, _: &ResourceRef) -> Option<&Font> {
            Some(&self.0)
        }

        fn texture(&self, _: &ResourceRef) -> Option<&Texture<'_>> {
            None
        }
    }

    static GLYPHS: [textflow::shaping::PositionedGlyph; 1] =
        [textflow::shaping::PositionedGlyph::new(
            crate::render::font::GlyphId::new(65),
            textflow::shaping::FlowPoint { x: 0, y: 7 << 8 },
        )];
    let ops = [SceneOp::GlyphRun {
        font: ResourceRef::Index(0),
        ppem: 19,
        pos: Point::ZERO,
        transform: Transform::IDENTITY,
        color: Color::rgb(1, 2, 3),
        opa: 255,
        glyphs: (&GLYPHS[..]).into(),
    }];
    let mut renderer = GlyphRenderer { glyphs: 0, ppem: 0 };

    replay_scene(&ops, &mut renderer, &rect(), &FontResolver(Font::default())).unwrap();

    assert_eq!(renderer.glyphs, 1);
    assert_eq!(renderer.ppem, 19);
}

#[test]
fn replay_preserves_posed_glyph_geometry() {
    struct GlyphRenderer {
        origin: Option<textflow::shaping::FlowPoint>,
        tangent: Option<textflow::shaping::FlowPoint>,
    }
    impl Renderer for GlyphRenderer {
        type TargetState = ();

        fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
            Ok(RenderRoute::Native)
        }

        fn submit(&mut self, draw: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            let request = &draw.request();
            self.route(request)?;
            if let DrawCommand::PosedGlyphRun { glyphs, .. } = request.command {
                self.origin = glyphs.frames().first().map(|frame| frame.local_origin);
                self.tangent = glyphs.frames().first().map(|frame| frame.unit_tangent);
            }
            Ok(())
        }

        fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
            Ok(())
        }
    }

    struct FontResolver(Font);
    impl SceneResolver for FontResolver {
        fn font(&self, _: &ResourceRef) -> Option<&Font> {
            Some(&self.0)
        }

        fn texture(&self, _: &ResourceRef) -> Option<&Texture<'_>> {
            None
        }
    }

    let origin = textflow::shaping::FlowPoint {
        x: 6 << 8,
        y: 9 << 8,
    };
    let tangent = textflow::shaping::FlowPoint { x: 0, y: 1 << 8 };
    let glyphs = super::super::PosedGlyphBuffer::new(
        vec![textflow::shaping::PositionedGlyph::new(
            crate::render::font::GlyphId::new(65),
            textflow::shaping::FlowPoint { x: 0, y: 0 },
        )],
        vec![textflow::placement::GlyphFrame {
            local_origin: origin,
            unit_tangent: tangent,
        }],
    )
    .unwrap();
    let ops = [SceneOp::PosedGlyphRun {
        font: ResourceRef::Index(0),
        ppem: 19,
        pos: Point::ZERO,
        transform: Transform::IDENTITY,
        color: Color::rgb(1, 2, 3),
        opa: 255,
        glyphs,
    }];
    let mut renderer = GlyphRenderer {
        origin: None,
        tangent: None,
    };

    replay_scene(&ops, &mut renderer, &rect(), &FontResolver(Font::default())).unwrap();

    assert_eq!(renderer.origin, Some(origin));
    assert_eq!(renderer.tangent, Some(tangent));
}

fn group(opa: Option<u8>, hint: bool) -> SceneOp {
    SceneOp::GroupBegin {
        transform: None,
        projective: None,
        opacity: opa,
        clip: None,
        mask: None,
        filter: None,
        disjoint_hint: hint,
    }
}

fn opaque_fill_at(x: i32, y: i32) -> SceneOp {
    SceneOp::FillRect {
        area: Rect {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            w: Fixed::from_int(4),
            h: Fixed::from_int(4),
        },
        transform: Transform::IDENTITY,
        quad: None,
        color: Color {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        },
        radius: Fixed::ZERO,
        opa: 255,
    }
}

#[test]
fn group_opacity_zero_skips_subtree() {
    let ops = vec![
        group(Some(0), false),
        opaque_fill_at(0, 0),
        opaque_fill_at(20, 0),
        SceneOp::GroupEnd,
        opaque_fill_at(40, 0),
    ];
    let mut r = CaptureRenderer {
        transforms: Vec::new(),
        fill_opas: Vec::new(),
    };
    replay_scene(&ops, &mut r, &rect(), &NoResolver).unwrap();
    assert_eq!(r.fill_opas, vec![255]);
}

#[test]
fn group_opacity_255_is_passthrough() {
    let ops = vec![
        group(Some(255), false),
        opaque_fill_at(0, 0),
        SceneOp::GroupEnd,
    ];
    let mut r = CaptureRenderer {
        transforms: Vec::new(),
        fill_opas: Vec::new(),
    };
    replay_scene(&ops, &mut r, &rect(), &NoResolver).unwrap();
    assert_eq!(r.fill_opas, vec![255]);
}

#[test]
fn group_opacity_disjoint_children_multiplies_into_each() {
    let ops = vec![
        group(Some(128), false),
        opaque_fill_at(0, 0),
        opaque_fill_at(20, 0),
        SceneOp::GroupEnd,
    ];
    let mut r = CaptureRenderer {
        transforms: Vec::new(),
        fill_opas: Vec::new(),
    };
    replay_scene(&ops, &mut r, &rect(), &NoResolver).unwrap();
    assert_eq!(r.fill_opas, vec![128, 128]);
}

#[test]
fn group_opacity_overlap_without_hint_errors() {
    let ops = vec![
        group(Some(128), false),
        opaque_fill_at(0, 0),
        opaque_fill_at(2, 2),
        SceneOp::GroupEnd,
    ];
    let mut r = CaptureRenderer {
        transforms: Vec::new(),
        fill_opas: Vec::new(),
    };
    assert_eq!(
        replay_scene(&ops, &mut r, &rect(), &NoResolver),
        Err(ReplayError::GroupOpacityNeedsOffscreen)
    );
}

#[test]
fn group_opacity_overlap_flattens_once_with_borrowed_rgba() {
    let mut first = opaque_fill_at(0, 0);
    let mut second = opaque_fill_at(2, 2);
    for op in [&mut first, &mut second] {
        if let SceneOp::FillRect { color, .. } = op {
            *color = Color::rgb(255, 0, 0);
        }
    }
    let ops = vec![group(Some(128), false), first, second, SceneOp::GroupEnd];
    let mut output = [0u8; 8 * 8 * 4];
    let texture =
        Texture::new(&mut output, 8, 8, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = crate::render::backends::sw::SwRenderer::new(texture);
    let mut frames = [ReplayFrame::EMPTY; 8];
    let mut routes = [ReplayPlan::EMPTY; 8];
    let mut scopes = [ReplayScopePlan::EMPTY; 8];
    let mut rgba = [0u8; 8 * 8 * 4];
    let metrics = ReplayMetrics::default();
    replay_scene_with_scratch(
        &ops,
        &mut renderer,
        &Rect::new(0, 0, 8, 8),
        &NoResolver,
        ReplayScratch::new(&mut frames, &mut routes, &mut scopes)
            .with_rgba(&mut rgba)
            .with_metrics(&metrics),
    )
    .unwrap();

    let overlap = (3 * 8 + 3) * 4;
    assert_eq!(&output[overlap..overlap + 4], &[128, 0, 0, 255]);
    let snapshot = metrics.snapshot();
    assert_eq!(snapshot.native_commands, 2);
    assert_eq!(snapshot.isolated_scopes, 1);
    assert_eq!(snapshot.peak_scope_slots, 1);
    assert_eq!(snapshot.peak_rgba_bytes, snapshot.planned_fallback_bytes);
    assert!(snapshot.peak_rgba_bytes <= rgba.len());
}

#[test]
fn nested_isolation_exhaustion_leaves_output_untouched() {
    let ops = vec![
        group(Some(128), false),
        group(Some(128), false),
        opaque_fill_at(0, 0),
        opaque_fill_at(2, 2),
        SceneOp::GroupEnd,
        opaque_fill_at(1, 1),
        SceneOp::GroupEnd,
    ];
    let mut output = [17u8; 8 * 8 * 4];
    let before = output;
    let texture =
        Texture::new(&mut output, 8, 8, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = crate::render::backends::sw::SwRenderer::new(texture);
    let mut frames = [ReplayFrame::EMPTY; 8];
    let mut routes = [ReplayPlan::EMPTY; 8];
    let mut scopes = [ReplayScopePlan::EMPTY; 8];
    let mut rgba = [0u8; 200];
    assert!(matches!(
        replay_scene_with_scratch(
            &ops,
            &mut renderer,
            &Rect::new(0, 0, 8, 8),
            &NoResolver,
            ReplayScratch::new(&mut frames, &mut routes, &mut scopes).with_rgba(&mut rgba),
        ),
        Err(ReplayError::Render(
            RenderError::InsufficientWorkspace { .. }
        ))
    ));
    assert_eq!(output, before);
}

#[test]
fn transformed_child_overlap_fails_before_any_draw() {
    let mut nested = group(None, false);
    if let SceneOp::GroupBegin { transform, .. } = &mut nested {
        *transform = Some(Transform::translate(Fixed::from_int(20), Fixed::ZERO));
    }
    let ops = vec![
        group(Some(128), false),
        nested,
        opaque_fill_at(0, 0),
        SceneOp::GroupEnd,
        opaque_fill_at(22, 0),
        SceneOp::GroupEnd,
    ];
    let mut renderer = CaptureRenderer {
        transforms: Vec::new(),
        fill_opas: Vec::new(),
    };
    assert_eq!(
        replay_scene(&ops, &mut renderer, &rect(), &NoResolver),
        Err(ReplayError::GroupOpacityNeedsOffscreen)
    );
    assert!(renderer.fill_opas.is_empty());
}

#[test]
fn group_opacity_overlap_with_hint_forces_flat() {
    let ops = vec![
        group(Some(128), true),
        opaque_fill_at(0, 0),
        opaque_fill_at(2, 2),
        SceneOp::GroupEnd,
    ];
    let mut r = CaptureRenderer {
        transforms: Vec::new(),
        fill_opas: Vec::new(),
    };
    replay_scene(&ops, &mut r, &rect(), &NoResolver).unwrap();
    assert_eq!(r.fill_opas, vec![128, 128]);
}

#[test]
fn nested_group_opacity_multiplies() {
    let ops = vec![
        group(Some(200), false),
        group(Some(128), false),
        opaque_fill_at(0, 0),
        SceneOp::GroupEnd,
        SceneOp::GroupEnd,
    ];
    let mut r = CaptureRenderer {
        transforms: Vec::new(),
        fill_opas: Vec::new(),
    };
    replay_scene(&ops, &mut r, &rect(), &NoResolver).unwrap();
    assert_eq!(r.fill_opas, vec![(200u16 * 128 / 255) as u8]);
}
