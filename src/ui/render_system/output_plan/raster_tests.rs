use crate::ecs::Entity;
use crate::prelude::*;
use crate::render::command::DrawCommand;
use crate::render::path::{Path, PathCmd};
use crate::render::raster::FillRule;
use crate::render::renderer::RenderError;
use crate::surface::FramebufferAccess;
use crate::types::{Point, Transform};
use crate::ui::OffscreenRender;

const GREEN: Color = Color::rgb(20, 190, 60);
const BLUE: Color = Color::rgb(20, 60, 190);
const TRIANGLE: Path = Path::from_static(&[
    PathCmd::MoveTo(Point::ZERO),
    PathCmd::LineTo(Point {
        x: Fixed::from_int(16),
        y: Fixed::ZERO,
    }),
    PathCmd::LineTo(Point {
        x: Fixed::ZERO,
        y: Fixed::from_int(16),
    }),
    PathCmd::Close,
]);
const RECTANGLE: Path = Path::from_static(&[
    PathCmd::MoveTo(Point::ZERO),
    PathCmd::LineTo(Point {
        x: Fixed::from_int(16),
        y: Fixed::ZERO,
    }),
    PathCmd::LineTo(Point {
        x: Fixed::from_int(16),
        y: Fixed::from_int(16),
    }),
    PathCmd::LineTo(Point {
        x: Fixed::ZERO,
        y: Fixed::from_int(16),
    }),
    PathCmd::Close,
]);

#[crate::component]
struct RasterSample {
    clipped: bool,
    color: Color,
    fail: bool,
}

#[crate::view(component = RasterSample)]
fn raster_sample(sample: component!(), paint: paint!()) {
    let rect = paint.rect();
    let transform = paint
        .transform()
        .compose(&Transform::translate(rect.x, rect.y));
    let draw = |paint: &mut crate::ui::view::PaintCtx<'_>| {
        if sample.fail {
            paint.fail(RenderError::BackendFailure);
        } else {
            paint.draw(&DrawCommand::FillPath {
                path: &RECTANGLE,
                transform,
                paint: &crate::render::canvas::Paint::Color(sample.color.into()),
                opa: 255,
                fill_rule: FillRule::EvenOdd,
            });
        }
    };
    if sample.clipped {
        paint.with_clip_path(&TRIANGLE, transform, FillRule::EvenOdd, draw);
    } else {
        draw(paint);
    }
}

#[crate::compose]
fn scene() {
    ui! {
        Row (width: 32, height: 16) {
            View (id: "clipped", width: 16, height: 16) [
                OffscreenRender::new(),
                RasterSample { clipped: true, color: GREEN, fail: false },
            ]
            View (id: "plain", width: 16, height: 16) [
                OffscreenRender::new(),
                RasterSample { clipped: false, color: GREEN, fail: false },
            ]
        }
    };
}

type TestApp =
    App<crate::surface::framebuf::FramebufSurface<fn(&[u8], crate::types::PhysicalRect)>>;

fn fixture() -> (TestApp, [Entity; 2]) {
    let mut app = App::headless(32, 16);
    app.with_default_widgets()
        .with_widget(raster_sample::view())
        .with_offscreen_pool_budget(4096);
    let root = app.spawn_root().bg_color(Color::rgb(0, 0, 0)).id();
    app.compose(root, scene());
    let entities = [
        app.world.find_by_id("clipped").unwrap(),
        app.world.find_by_id("plain").unwrap(),
    ];
    (app, entities)
}

fn redraw(app: &mut TestApp, entities: &[Entity; 2], color: Color) {
    for &entity in entities {
        app.world.get_mut::<RasterSample>(entity).unwrap().color = color;
        app.world.invalidate_visual(entity);
    }
    app.render_dirty().unwrap();
    let pixels = app.backend.framebuffer();
    assert_eq!(pixels.get_pixel(2, 2), color);
    assert_eq!(pixels.get_pixel(13, 13), Color::rgb(0, 0, 0));
    assert_eq!(pixels.get_pixel(29, 13), color);
}

#[test]
fn warm_captures_reuse_raster_storage_without_leaking_clip_state() {
    let (mut app, entities) = fixture();
    app.render().unwrap();
    for color in [BLUE, GREEN, BLUE, GREEN] {
        redraw(&mut app, &entities, color);
    }
    let allocations = crate::test_support::tracked_allocations(|| {
        for color in [BLUE, GREEN, BLUE, GREEN] {
            redraw(&mut app, &entities, color);
        }
    });
    assert_eq!(allocations, 0);
}

#[test]
fn failed_capture_and_memory_warning_preserve_reusable_output_lifecycle() {
    let (mut app, entities) = fixture();
    app.render().unwrap();
    app.world.get_mut::<RasterSample>(entities[0]).unwrap().fail = true;
    app.world.invalidate_visual(entities[0]);
    assert_eq!(app.render_dirty(), Err(RenderError::BackendFailure));
    app.world.get_mut::<RasterSample>(entities[0]).unwrap().fail = false;
    redraw(&mut app, &entities, BLUE);
    let outputs = app
        .world
        .resource::<crate::ui::Outputs<crate::ui::WidgetTexture>>()
        .unwrap();
    let plan = outputs.take_texture_plan().unwrap();
    assert!(plan.raster.is_some());
    outputs.put_texture_plan(plan);
    app.handle_memory_warning();
    let outputs = app
        .world
        .resource::<crate::ui::Outputs<crate::ui::WidgetTexture>>()
        .unwrap();
    let plan = outputs.take_texture_plan().unwrap();
    assert!(plan.raster.is_none());
    outputs.put_texture_plan(plan);
    app.render().unwrap();
    redraw(&mut app, &entities, GREEN);
    let allocations = crate::test_support::tracked_allocations(|| {
        redraw(&mut app, &entities, BLUE);
    });
    assert_eq!(allocations, 0);
}

#[test]
fn releasing_capture_demand_or_trimming_a_leased_plan_releases_raster_storage() {
    let (mut app, entities) = fixture();
    app.render().unwrap();
    let outputs = app
        .world
        .resource::<crate::ui::Outputs<crate::ui::WidgetTexture>>()
        .unwrap();
    let plan = outputs.take_texture_plan().unwrap();
    assert!(plan.raster.is_some());
    outputs.trim_memory();
    outputs.put_texture_plan(plan);
    let plan = outputs.take_texture_plan().unwrap();
    assert!(plan.raster.is_none());
    outputs.put_texture_plan(plan);

    app.world.invalidate(entities[0]);
    app.render().unwrap();
    for entity in entities {
        app.world.remove::<OffscreenRender>(entity);
        app.world.invalidate_visual(entity);
    }
    app.render().unwrap();
    let outputs = app
        .world
        .resource::<crate::ui::Outputs<crate::ui::WidgetTexture>>()
        .unwrap();
    let plan = outputs.take_texture_plan().unwrap();
    assert!(plan.raster.is_none());
    outputs.put_texture_plan(plan);
}
