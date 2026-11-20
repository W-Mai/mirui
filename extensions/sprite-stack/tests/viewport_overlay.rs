#![cfg(feature = "widget")]

use core::cell::Cell;
use mirui::{prelude::*, render::RenderError};
use mirui_sprite_stack::{
    Resolution, SpriteStackPlugin, StackScene, StackViewport, ViewportOverlay,
    raster::{Canvas, FrameOverlay, OverlayFrame},
};
use std::rc::Rc;

#[path = "support/allocations.rs"]
mod allocations;

const WHITE: Color = Color::rgb(255, 255, 255);
const BLACK: Color = Color::rgb(0, 0, 0);

struct Panel {
    ink: Color,
    prepares: Rc<Cell<usize>>,
}

impl FrameOverlay<Color> for Panel {
    fn prepare(&mut self, frame: OverlayFrame, input: &Color) -> Result<bool, RenderError> {
        assert_eq!(frame.size, [16, 16]);
        let camera = frame.camera.expect("built-in scene camera");
        assert_eq!(camera.projection.center, [Fixed::from_int(8); 2]);
        self.ink = *input;
        self.prepares.set(self.prepares.get() + 1);
        Ok(input.a != 0)
    }

    fn paint(&self, canvas: &mut Canvas<'_>) {
        let [left, top, right, bottom] = canvas.clip;
        let ink = self.ink;
        let rgba = u32::from_le_bytes([ink.r, ink.g, ink.b, ink.a]);
        for y in top..bottom {
            for x in left..right {
                canvas.put(canvas.index(x, y), rgba);
            }
        }
    }
}

#[mirui::compose]
fn screen(
    scene: model!(StackScene),
    paint: Signal<Color>,
    first: Rc<Cell<usize>>,
    second: Rc<Cell<usize>>,
) -> Entity {
    let static_overlay = ViewportOverlay::new(
        WHITE,
        Panel {
            ink: BLACK,
            prepares: second,
        },
    );
    let dynamic_overlay = ViewportOverlay::new(
        Prop::new(move || paint.get()),
        Panel {
            ink: BLACK,
            prepares: first,
        },
    );
    ui! {
        Row (width: 32, height: 16) {
            StackViewport (width: 16, height: 16, scene: scene.clone(), resolution: Resolution::Native) [dynamic_overlay]
            StackViewport (width: 16, height: 16, scene: scene) [static_overlay]
        }
    }
}

#[test]
fn attached_overlay_observes_its_input_and_reuses_the_completed_frame() {
    let mut app = App::headless(32, 16);
    app.with_default_widgets().add_plugin(SpriteStackPlugin);
    let root = app.spawn_root().id();
    let scene = app.add_model(StackScene::new()).finish();
    let ink = Signal::new(Color::rgb(179, 25, 40));
    let draws = [Rc::new(Cell::new(0)), Rc::new(Cell::new(0))];
    app.compose(
        root,
        screen(scene, ink.clone(), draws[0].clone(), draws[1].clone()),
    );
    let check = |app: &mut App<_, _>, expected: Color| {
        let snapshot = app.snapshot().unwrap().unwrap();
        let pixels = snapshot.read_pixels().unwrap();
        assert_eq!(pixels.get_pixel(8, 8), expected);
        assert_eq!(pixels.get_pixel(24, 8), WHITE);
    };
    check(&mut app, ink.get());
    assert_eq!([draws[0].get(), draws[1].get()], [1, 1]);
    ink.set(Color::rgb(10, 80, 130));
    check(&mut app, ink.get());
    assert_eq!([draws[0].get(), draws[1].get()], [2, 1]);
    check(&mut app, ink.get());
    assert_eq!([draws[0].get(), draws[1].get()], [2, 1]);
    assert_eq!(allocations::tracked_allocations(|| ink.set(ink.get())), 0);
    ink.set(Color::rgba(0, 0, 0, 0));
    let snapshot = app.snapshot().unwrap().unwrap();
    let pixels = snapshot.read_pixels().unwrap();
    assert_ne!(pixels.get_pixel(8, 8), Color::rgb(10, 80, 130));
    assert_eq!(pixels.get_pixel(24, 8), WHITE);
    assert_eq!([draws[0].get(), draws[1].get()], [3, 1]);
}
