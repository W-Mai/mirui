//! Demonstrates the `View` registration API: a user-defined
//! `Diamond` widget that doesn't ship with mirui, registered at
//! startup and rendered through the same pipeline as built-ins.
//!
//! Three diamonds in a row, each tappable to cycle through colours.
//! Run with `cargo run -p gallery --example custom_view_demo`.

use mirui::draw::command::DrawCommand;
use mirui::draw::renderer::Renderer;
use mirui::event::GestureHandler;
use mirui::event::gesture::GestureEvent;
use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;
use mirui::widget::dirty::Dirty;
use mirui::widget::view::{View, ViewCtx};

pub struct Diamond {
    pub color: Color,
    pub line_width: Fixed,
}

impl Default for Diamond {
    fn default() -> Self {
        Self {
            color: Color::rgb(255, 255, 255),
            line_width: Fixed::from_int(1),
        }
    }
}

/// `ViewRender` for the user widget. Reads `Diamond` from World,
/// emits four Line commands that trace the rect's diamond inscribed.
fn diamond_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(d) = world.get::<Diamond>(entity) else {
        return;
    };
    let half_w = rect.w / Fixed::from_int(2);
    let half_h = rect.h / Fixed::from_int(2);
    let cx = rect.x + half_w;
    let cy = rect.y + half_h;
    let top = Point { x: cx, y: rect.y };
    let right = Point {
        x: rect.x + rect.w,
        y: cy,
    };
    let bottom = Point {
        x: cx,
        y: rect.y + rect.h,
    };
    let left = Point { x: rect.x, y: cy };

    let segs = [(top, right), (right, bottom), (bottom, left), (left, top)];
    for (p1, p2) in segs {
        renderer.draw(
            &DrawCommand::Line {
                p1,
                p2,
                transform: ctx.transform,
                color: d.color,
                width: d.line_width,
                opa: 255,
            },
            ctx.clip,
        );
    }
}

/// Constructor the user passes to `App::with_widget`.
pub fn diamond_view() -> View {
    View::new("Diamond", 60, diamond_render)
}

/// Tap handler — cycle through three colours and mark Dirty.
const PALETTE: [Color; 3] = [
    Color::rgb(244, 167, 89),
    Color::rgb(140, 211, 255),
    Color::rgb(190, 240, 140),
];

fn cycle_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    if !matches!(event, GestureEvent::Tap { .. }) {
        return false;
    }
    if let Some(d) = world.get_mut::<Diamond>(entity) {
        let i = PALETTE.iter().position(|c| *c == d.color).unwrap_or(0);
        d.color = PALETTE[(i + 1) % PALETTE.len()];
    }
    world.insert(entity, Dirty);
    true
}

fn main() {
    let backend = SdlSurface::new("custom_view_demo", 480, 200);
    let mut app = App::new(backend);
    app.with_default_widgets().with_widget(diamond_view());

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(28, 28, 36))
        .layout(LayoutStyle {
            direction: FlexDirection::Row,
            justify: JustifyContent::SpaceEvenly,
            align: AlignItems::Center,
            width: Dimension::px(480),
            height: Dimension::px(200),
            ..Default::default()
        })
        .id();

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        row (
            direction: FlexDirection::Row,
            justify: JustifyContent::SpaceEvenly,
            align: AlignItems::Center,
            width: 480,
            height: 200
        ) {
            Diamond (
                color: PALETTE[0],
                line_width: Fixed::from_int(2),
                width: 100,
                height: 100
            ) [
                GestureHandler {
                    on_gesture: cycle_handler,
                },
            ] {}
            Diamond (
                color: PALETTE[1],
                line_width: Fixed::from_int(3),
                width: 100,
                height: 100
            ) [
                GestureHandler {
                    on_gesture: cycle_handler,
                },
            ] {}
            Diamond (
                color: PALETTE[2],
                line_width: Fixed::from_int(4),
                width: 100,
                height: 100
            ) [
                GestureHandler {
                    on_gesture: cycle_handler,
                },
            ] {}
        }
    };
    app.set_root(root);
    app.run();
}
