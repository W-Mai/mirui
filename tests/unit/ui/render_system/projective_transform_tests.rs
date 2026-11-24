use super::*;
use crate::ui::geometry::{effective_transform, effective_transform_3d};
use crate::ui::widgets::transform::WidgetTransform;
use crate::ui::widgets::transform_3d::WidgetTransform3D;

#[derive(Default)]
struct AffineOnlyRenderer {
    draws: usize,
}

#[derive(Default)]
struct ProjectiveLeafCapture {
    quad_was_none: Option<bool>,
    transform: Option<Transform3D>,
}

impl Renderer for AffineOnlyRenderer {
    type TargetState = ();

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

impl Renderer for ProjectiveLeafCapture {
    type TargetState = ();

    fn route(&self, _: &DrawRequest<'_, '_>) -> Result<crate::render::RenderRoute, RenderError> {
        Ok(crate::render::RenderRoute::Native)
    }

    fn submit(&mut self, draw: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
        let request = &draw.request();
        self.route(request)?;
        if let DrawCommand::Fill { quad, .. } = request.command {
            self.quad_was_none = Some(quad.is_none());
            self.transform = Some(request.projective);
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
        Ok(())
    }
}

#[test]
fn first_projective_boundary_folds_affine_ancestors_once() {
    let mut world = World::new();
    let entity = world.spawn_empty();
    let affine = Transform::translate(Fixed::from_int(3), Fixed::from_int(2));
    let projective = Transform3D::rotate_y_perspective(Fixed::from_int(18), Fixed::from_int(400));
    world.insert(entity, WidgetTransform(affine));
    world.insert(entity, WidgetTransform3D(projective));
    let parent = Transform::scale(Fixed::from_int(2), Fixed::from_int(2));
    let rect = Rect::new(10, 10, 20, 10);

    let (command, scope) = render_transforms(parent, Transform3D::IDENTITY, &world, entity, rect);
    let local_affine = effective_transform(&Transform::IDENTITY, &world, entity, rect);
    let local_projective = effective_transform_3d(&Transform3D::IDENTITY, &world, entity, rect);
    let expected = Transform3D::from_affine(parent)
        .compose(&local_projective)
        .compose(&Transform3D::from_affine(local_affine));

    assert_eq!(command, Transform::IDENTITY);
    assert_eq!(scope, expected);
}

#[test]
fn affine_descendant_composes_inside_existing_projective_scope() {
    let mut world = World::new();
    let entity = world.spawn_empty();
    world.insert(
        entity,
        WidgetTransform(Transform::translate(
            Fixed::from_int(4),
            Fixed::from_int(-2),
        )),
    );
    let parent = Transform3D::rotate_x_perspective(Fixed::from_int(12), Fixed::from_int(360));
    let rect = Rect::new(8, 6, 24, 12);

    let (command, scope) = render_transforms(Transform::IDENTITY, parent, &world, entity, rect);
    let local = effective_transform(&Transform::IDENTITY, &world, entity, rect);

    assert_eq!(command, Transform::IDENTITY);
    assert_eq!(scope, parent.compose(&Transform3D::from_affine(local)));
}

#[test]
fn affine_only_renderer_does_not_receive_projective_approximation() {
    let mut renderer = AffineOnlyRenderer::default();
    let transform = Transform3D::translate(Fixed::from_int(4), Fixed::ZERO);
    let mut scoped = ScopedRenderer::new(&mut renderer, &transform, None);
    let command = DrawCommand::Fill {
        area: Rect::new(0, 0, 8, 8),
        transform: Transform::IDENTITY,
        quad: None,
        color: crate::types::Color::rgb(255, 255, 255),
        radius: Fixed::ZERO,
        opa: u8::MAX,
    };
    assert_eq!(
        scoped.draw_request(&DrawRequest::new(&command, Rect::new(0, 0, 16, 16))),
        Err(RenderError::Unsupported(
            crate::render::RenderFeature::ProjectiveGeometry
        ))
    );
    assert_eq!(renderer.draws, 0);
}

#[test]
fn projective_scope_forwards_text_run_identity() {
    #[derive(Default)]
    struct Recorder {
        seen: Option<(crate::render::TextRunIdentity, Transform3D)>,
    }

    impl Renderer for Recorder {
        type TargetState = ();

        fn route(
            &self,
            _: &::mirui::render::DrawRequest<'_, '_>,
        ) -> Result<::mirui::render::RenderRoute, ::mirui::render::RenderError> {
            Ok(::mirui::render::RenderRoute::Native)
        }

        fn submit(&mut self, _: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            Ok(())
        }

        fn submit_text_run(
            &mut self,
            draw: &::mirui::render::PreparedDraw<'_>,
            identity: crate::render::TextRunIdentity,
        ) -> Result<(), RenderError> {
            let request = &draw.request();
            self.seen = Some((identity, request.projective));
            Ok(())
        }

        fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
            Ok(())
        }
    }

    let identity = crate::render::TextRunIdentity {
        entity: Entity {
            id: 3,
            generation: 2,
        },
        run_index: 1,
    };
    let projection = Transform3D::translate(Fixed::from_int(2), Fixed::from_int(3));
    let clip = Rect::new(0, 0, 10, 10);
    let command = DrawCommand::ApplyBlur {
        alpha: Fixed::ONE,
        region: clip,
    };
    let mut recorder = Recorder::default();
    ScopedRenderer::new(&mut recorder, &projection, None)
        .draw_text_run(&DrawRequest::new(&command, clip), identity)
        .unwrap();
    assert_eq!(recorder.seen, Some((identity, projection)));
}

#[test]
fn projective_widget_keeps_the_leaf_unprojected() {
    let mut app = crate::app::App::headless(32, 24);
    app.with_default_widgets();
    let root = app.world.spawn_empty();
    app.world.insert(root, Widget);
    app.world.insert(
        root,
        Style {
            bg_color: Some(crate::types::Color::rgb(20, 40, 60).into()),
            layout: crate::ui::layout::LayoutStyle {
                width: crate::types::Dimension::px(32),
                height: crate::types::Dimension::px(24),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let projective = Transform3D::rotate_y_perspective(Fixed::from_int(12), Fixed::from_int(320));
    app.world.insert(root, WidgetTransform3D(projective));

    let mut capture = ProjectiveLeafCapture::default();
    render(
        &app.world,
        root,
        &Viewport::new(32, 24, Fixed::ONE),
        &mut capture,
    )
    .unwrap();

    assert_eq!(capture.quad_was_none, Some(true));
    assert!(capture.transform.is_some_and(|value| !value.is_identity()));
}

#[test]
fn projective_widget_failure_reaches_render_caller() {
    let mut app = crate::app::App::headless(32, 24);
    app.with_default_widgets();
    let root = app.world.spawn_empty();
    app.world.insert(root, Widget);
    app.world.insert(
        root,
        Style {
            bg_color: Some(crate::types::Color::rgb(20, 40, 60).into()),
            layout: crate::ui::layout::LayoutStyle {
                width: crate::types::Dimension::px(32),
                height: crate::types::Dimension::px(24),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    app.world.insert(
        root,
        WidgetTransform3D(Transform3D::rotate_y_perspective(
            Fixed::from_int(12),
            Fixed::from_int(320),
        )),
    );

    let mut renderer = AffineOnlyRenderer::default();
    assert_eq!(
        render(
            &app.world,
            root,
            &Viewport::new(32, 24, Fixed::ONE),
            &mut renderer
        ),
        Err(RenderError::Unsupported(
            crate::render::RenderFeature::ProjectiveGeometry
        ))
    );
    assert_eq!(renderer.draws, 0);
}

#[test]
fn affine_widget_failure_reaches_render_caller() {
    let mut app = crate::app::App::headless(32, 24);
    app.with_default_widgets();
    let root = app.world.spawn_empty();
    app.world.insert(root, Widget);
    app.world.insert(
        root,
        Style {
            bg_color: Some(crate::types::Color::rgb(20, 40, 60).into()),
            layout: crate::ui::layout::LayoutStyle {
                width: crate::types::Dimension::px(32),
                height: crate::types::Dimension::px(24),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let mut renderer = AffineOnlyRenderer::default();
    assert_eq!(
        render(
            &app.world,
            root,
            &Viewport::new(32, 24, Fixed::ONE),
            &mut renderer
        ),
        Err(RenderError::Unsupported(
            crate::render::RenderFeature::AffineGeometry
        ))
    );
    assert_eq!(renderer.draws, 0);
}

#[test]
fn child_clip_uses_the_effective_transform() {
    let rect = Rect::new(8, 6, 20, 12);
    let affine = Transform::translate(Fixed::from_int(4), Fixed::from_int(-2));
    assert_eq!(
        transformed_clip_bounds(rect, affine, Transform3D::IDENTITY),
        Some(affine.apply_rect_bbox(rect))
    );

    let projective = Transform3D::rotate_y_perspective(Fixed::from_int(12), Fixed::from_int(320));
    let expected = projective.apply_rect(rect).map(quad_bbox);
    assert_eq!(
        transformed_clip_bounds(rect, Transform::IDENTITY, projective),
        expected
    );

    let mut behind = Transform3D::IDENTITY;
    behind.m22 = -crate::types::Fixed64::ONE;
    assert_eq!(
        transformed_clip_bounds(rect, Transform::IDENTITY, behind),
        None
    );
}
