//! SDL GPU backend demo — hardware-accelerated rendering with a
//! drag-to-move widget. The flex-laid children (solid, rounded, label,
//! blit) stack vertically centred; the orange "DRAG ME" box is
//! `Position::Absolute` and follows the mouse while the left button is
//! held.
//!
//! Runs through `App::run`. The SDL GPU backend reports its backbuffer
//! as `Transient`, so the run loop redraws every frame automatically —
//! no manual loop here.

use mirui::app::{App, RendererFactory};
use mirui::components::Image;
use mirui::components::assets::*;
use mirui::plugin::Plugin;
use mirui::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
use mirui::prelude::*;
use mirui::surface::sdl_gpu::{SdlGpuFactory, SdlGpuSurface};
use mirui::surface::{InputEvent, Surface};
use mirui::widget::{Children, Parent};

const DRAG_W: i32 = 160;
const DRAG_H: i32 = 60;

fn main() {
    let backend = SdlGpuSurface::new("mirui SDL GPU — drag me", 640, 480);
    let mut app = App::with_factory(backend, SdlGpuFactory::new());
    app.with_default_widgets();

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgba(30, 30, 46, 255))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::percent(100),
            height: Dimension::percent(100),
            justify: JustifyContent::Center,
            align: AlignItems::Center,
            ..Default::default()
        })
        .id();

    let solid = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgba(32, 160, 240, 255))
        .border(Color::rgba(240, 240, 255, 255), 1)
        .layout(LayoutStyle {
            width: Dimension::px(200),
            height: Dimension::px(80),
            ..Default::default()
        })
        .id();

    let translucent = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgba(80, 240, 160, 128))
        .layout(LayoutStyle {
            width: Dimension::px(200),
            height: Dimension::px(80),
            ..Default::default()
        })
        .id();

    let label = WidgetBuilder::new(&mut app.world)
        .text("SDL GPU BACKEND")
        .text_color(Color::rgba(255, 255, 255, 255))
        .layout(LayoutStyle {
            width: Dimension::px(200),
            height: Dimension::px(20),
            ..Default::default()
        })
        .id();

    let img = WidgetBuilder::new(&mut app.world)
        .layout(LayoutStyle {
            width: Dimension::px(IMG_THUMBS_UP.width as i32),
            height: Dimension::px(IMG_THUMBS_UP.height as i32),
            ..Default::default()
        })
        .id();
    app.world.insert(img, Image::new(&IMG_THUMBS_UP));

    let drag = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgba(240, 120, 60, 230))
        .border(Color::rgba(255, 255, 255, 255), 2)
        .border_radius(12)
        .text("DRAG ME")
        .text_color(Color::rgba(255, 255, 255, 255))
        .layout(LayoutStyle {
            position: Position::Absolute,
            width: Dimension::px(DRAG_W),
            height: Dimension::px(DRAG_H),
            left: Dimension::px(240),
            top: Dimension::px(30),
            ..Default::default()
        })
        .id();

    for child in [solid, translucent, label, img, drag] {
        app.world.insert(child, Parent(root));
        if let Some(children) = app.world.get_mut::<Children>(root) {
            children.0.push(child);
        }
    }

    app.set_root(root);
    app.add_plugin(StdInstantClockPlugin)
        .add_plugin(FpsSummaryPlugin::default())
        .add_plugin(DragPlugin {
            target: drag,
            pos: (Fixed::from_int(240), Fixed::from_int(30)),
            offset: None,
        });
    app.run();
}

/// Drag `target` around by its top-left corner while the mouse button is
/// held. Runs entirely off `on_event`; consumes Touch / TouchMove /
/// Release to keep them out of the widget dispatch path.
struct DragPlugin {
    target: Entity,
    pos: (Fixed, Fixed),
    offset: Option<(Fixed, Fixed)>,
}

impl<B, F> Plugin<B, F> for DragPlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, _app: &mut App<B, F>) {}

    fn on_event(&mut self, world: &mut World, event: &InputEvent) -> bool {
        let w = Fixed::from_int(DRAG_W);
        let h = Fixed::from_int(DRAG_H);
        match *event {
            InputEvent::PointerDown { x, y, .. } => {
                let (dx, dy) = self.pos;
                if x >= dx && x < dx + w && y >= dy && y < dy + h {
                    self.offset = Some((x - dx, y - dy));
                    return true;
                }
                false
            }
            InputEvent::PointerMove { x, y, .. } => {
                if let Some((ox, oy)) = self.offset {
                    let nx = x - ox;
                    let ny = y - oy;
                    self.pos = (nx, ny);
                    mirui::widget::set_position(world, self.target, nx, ny);
                    return true;
                }
                false
            }
            InputEvent::PointerUp { .. } => {
                if self.offset.take().is_some() {
                    return true;
                }
                false
            }
            _ => false,
        }
    }
}
