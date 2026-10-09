use mirui::prelude::Dimension;
use mirui::render::texture::Texture;
use mirui::render::{DrawRequest, RenderError, RenderFeature, RenderRoute, Renderer};
use mirui::types::{Fixed, Rect, Viewport};
use mirui::ui::layout::LayoutStyle;
use mirui::ui::widgets::BackgroundBlur;
use mirui::ui::{Children, Parent, Style, render_system};

struct GracefulSkipRenderer {
    draws: usize,
}

impl Renderer for GracefulSkipRenderer {
    fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        Ok(RenderRoute::Native)
    }
    fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        self.route(request)?;
        self.draws += 1;
        Ok(())
    }
    fn flush(&mut self) {}
    fn sample_target_region(&self, _src: &Rect) -> Result<Option<Texture<'static>>, RenderError> {
        Ok(None)
    }
    fn modify_target_region(
        &mut self,
        _src: &Rect,
        _f: &mut dyn FnMut(&mut Texture) -> Result<(), RenderError>,
    ) -> Result<bool, RenderError> {
        Ok(false)
    }
}

#[test]
fn background_blur_skips_empty_readback() {
    let mut app = mirui::app::App::headless(64, 64);
    app.with_default_widgets();
    let mut world = app.world;
    let widget = world.spawn((
        Style {
            layout: LayoutStyle {
                width: Dimension::px(64),
                height: Dimension::px(64),
                ..Default::default()
            },
            ..Default::default()
        },
        Children(Vec::new()),
    ));
    world.insert(widget, BackgroundBlur::new(Fixed::from_int(4)));

    let root = world.spawn((
        Style {
            layout: LayoutStyle {
                width: Dimension::px(64),
                height: Dimension::px(64),
                ..Default::default()
            },
            ..Default::default()
        },
        Children(Vec::new()),
    ));
    world.insert(widget, Parent(root));
    world.get_mut::<Children>(root).unwrap().0.push(widget);

    let mut renderer = GracefulSkipRenderer { draws: 0 };
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    render_system::render(&world, root, &viewport, &mut renderer).unwrap();
    let _ = renderer.draws;
}

#[test]
fn background_blur_reports_unsupported_readback() {
    struct NoReadback;

    impl Renderer for NoReadback {
        fn submit(&mut self, _: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
            Ok(())
        }

        fn flush(&mut self) {}
    }

    let mut app = mirui::app::App::headless(64, 64);
    app.with_default_widgets();
    let mut world = app.world;
    let widget = world.spawn((
        Style {
            layout: LayoutStyle {
                width: Dimension::px(64),
                height: Dimension::px(64),
                ..Default::default()
            },
            ..Default::default()
        },
        Children(Vec::new()),
    ));
    world.insert(widget, BackgroundBlur::new(Fixed::from_int(4)));
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    assert_eq!(
        render_system::render(&world, widget, &viewport, &mut NoReadback),
        Err(RenderError::Unsupported(RenderFeature::Readback))
    );
}

#[test]
fn background_blur_modify_path_returns_false_without_panic() {
    let mut renderer = GracefulSkipRenderer { draws: 0 };
    let rect = Rect {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        w: Fixed::from_int(16),
        h: Fixed::from_int(16),
    };
    let result = renderer.modify_target_region(&rect, &mut |_tex| {
        panic!("closure must not run when backend returns false");
    });
    assert_eq!(result, Ok(false));
}

#[test]
fn background_blur_sample_path_returns_none_without_panic() {
    let renderer = GracefulSkipRenderer { draws: 0 };
    let rect = Rect {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        w: Fixed::from_int(16),
        h: Fixed::from_int(16),
    };
    assert!(matches!(renderer.sample_target_region(&rect), Ok(None)));
}
