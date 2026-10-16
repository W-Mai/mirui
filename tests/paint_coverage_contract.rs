use mirui::prelude::*;
use mirui::surface::FramebufferAccess;

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

#[model]
struct Frame {
    #[observe]
    x: Fixed,
    #[observe]
    y: Fixed,
    #[observe]
    stroke: Fixed,
}

#[model]
impl Frame {
    fn change(&mut self, x: Fixed, y: Fixed, stroke: Fixed) {
        self.x = x;
        self.y = y;
        self.stroke = stroke;
    }
}

#[compose]
fn panel(frame: model!(Frame), radius: Fixed) {
    ui! {
        View (grow: 1.0) {
            View (
                position: Position::Absolute,
                left: ${ frame.x() },
                top: ${ frame.y() },
                width: 120,
                height: 70,
                border_color: Color::rgb(40, 200, 110),
                border_width: ${ frame.stroke() },
                border_radius: radius
            )
        }
    };
}

#[test]
fn single_border_retains_precise_coverage_across_queries_and_style_changes() {
    for scale in [0.5, 1.0, 1.5, 2.0] {
        for radius in [0.0, 8.0] {
            render_border_sequence(radius, scale, false);
            render_border_sequence(radius, scale, true);
        }
    }
}

#[compose]
fn following_panel(frame: model!(Frame), radius: Fixed) {
    ui! {
        View (grow: 1.0) {
            View (
                id: "source",
                position: Position::Absolute,
                left: ${ frame.x() },
                top: ${ frame.y() },
                width: 120,
                height: 70
            )
            View (
                border_color: Color::rgba(40, 200, 110, 150),
                border_width: ${ frame.stroke() },
                border_radius: radius
            ) [
                mirui::ui::widgets::FollowBounds::new(id("source")),
            ]
        }
    };
}

fn render_border_sequence(radius: f32, scale: f32, follow: bool) {
    let surface = mirui::surface::framebuf::FramebufSurface::with_scale(
        (160.0 * scale) as u16,
        (120.0 * scale) as u16,
        scale.into(),
        |_, _| {},
    );
    let mut app = App::new(surface);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let frame = app
        .add_model(Frame {
            x: 10.into(),
            y: 10.into(),
            stroke: 2.into(),
        })
        .finish();
    if follow {
        app.compose(root, following_panel(frame.clone(), radius.into()));
    } else {
        app.compose(root, panel(frame.clone(), radius.into()));
    }
    app.render().unwrap();
    for (x, y, stroke) in [
        (11.0, 10.0, 2.0),
        (11.0, 11.0, 2.0),
        (11.5, 11.25, 3.5),
        (12.25, 12.5, 2.25),
        (11.0, 10.0, 0.0),
        (10.0, 10.0, 2.0),
    ] {
        frame.change(x.into(), y.into(), stroke.into());
        let plan = app.dirty_regions().unwrap();
        if x == 11.0 && radius == 0.0 && stroke == 2.0 && scale == 1.0 {
            assert!(plan.rects.len() <= 4, "{:?}", plan.rects);
            let area: i32 = plan.rects.iter().map(|r| r.w.to_int() * r.h.to_int()).sum();
            assert!(area < 1520, "{area}: {:?}", plan.rects);
        }
        for (index, rect) in plan.rects.iter().enumerate() {
            assert!(
                plan.rects[index + 1..]
                    .iter()
                    .all(|other| rect.intersect(other).is_none())
            );
        }
        app.dirty_regions().unwrap();
        app.render_dirty().unwrap();
        let partial = app
            .backend
            .framebuffer()
            .read_pixels()
            .expect("read pixels")
            .bytes()
            .to_vec();
        app.render().unwrap();
        assert!(
            partial
                == app
                    .backend
                    .framebuffer()
                    .read_pixels()
                    .expect("read pixels")
                    .bytes(),
            "x={x} y={y} stroke={stroke} radius={radius} scale={scale} follow={follow}"
        );
    }
    frame.change(30.into(), 20.into(), 8.into());
    app.dirty_regions().unwrap();
    frame.change(10.into(), 10.into(), 0.into());
    app.render_dirty().unwrap();
    let partial = app
        .backend
        .framebuffer()
        .read_pixels()
        .expect("read pixels")
        .bytes()
        .to_vec();
    app.render().unwrap();
    assert!(
        partial
            == app
                .backend
                .framebuffer()
                .read_pixels()
                .expect("read pixels")
                .bytes()
    );
}

#[test]
fn unchanged_structure_reuses_coverage_and_region_storage() {
    let mut app = App::headless(160, 120);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let frame = app
        .add_model(Frame {
            x: 10.into(),
            y: 10.into(),
            stroke: 2.into(),
        })
        .finish();
    app.compose(root, panel(frame.clone(), Fixed::ZERO));
    app.render().unwrap();
    frame.change(11.into(), 10.into(), 2.into());
    app.render_dirty().unwrap();
    let allocations = tracking_allocator::tracked_allocations(|| {
        for index in 0..16 {
            frame.change((10 + index % 2).into(), 10.into(), 2.into());
            app.dirty_regions().unwrap();
            app.render_dirty().unwrap();
        }
    });
    assert_eq!(allocations, 0);
}
