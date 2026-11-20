use mirui::app::plugins::{FrameRateCapPlugin, StdInstantClockPlugin};
use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;
use mirui::ui::IgnoreHitTest;
use mirui_sprite_stack::{Camera, Object, StackScene, StackViewport};

#[compose]
fn minimap(scene: model!(StackScene), player: Object) -> Entity {
    ui! {
        StackViewport (
            position: Position::Absolute,
            top: 8,
            left: 8,
            width: 160,
            height: 160,
            scene: scene,
            camera: Camera {
                target: Some(player),
                ..Camera::overhead()
            }
        ) [
            IgnoreHitTest,
        ]
    }
}

fn main() {
    let mut app = App::new(SdlSurface::new("Pixel Flight · Minimap", 720, 480));
    app.with_default_widgets()
        .with_default_systems()
        .add_plugin(StdInstantClockPlugin)
        .add_plugin(FrameRateCapPlugin::new(60));
    let root = app.spawn_root().id();
    let game = mirui_pixel_flight::setup_app(&mut app, root);
    app.compose(root, minimap(game.scene(), game.player()));
    app.run();
}
