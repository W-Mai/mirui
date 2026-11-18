use mirui::app::plugins::{FrameRateCapPlugin, StdInstantClockPlugin};
use mirui::core::cache::Handle;
use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;
use mirui_sprite_stack::{SpriteStack, SpriteStackAsset, view};

#[model]
struct Turntable {
    #[observe]
    heading: u16,
}

#[model]
impl Turntable {
    fn advance(&mut self) {
        self.heading = (self.heading + 2) % 360;
    }
}

#[timer(every = 32)]
fn rotate(table: model!(Turntable)) {
    table.advance();
}

#[compose]
fn preview(car: Handle<SpriteStackAsset>, table: model!(Turntable)) -> Entity {
    ui! {
        SpriteStack (
            grow: 1.0,
            asset: car,
            yaw: ${ Fixed::from(table.heading()) },
            pitch: 28,
            scale: 4
        )
    }
}

fn main() {
    let car = SpriteStackAsset::from_mirx(include_bytes!("car.mirx")).expect("embedded car atlas");
    let mut app = App::new(SdlSurface::new("SpriteStack", 240, 180));
    app.with_default_widgets()
        .with_default_systems()
        .with_offscreen_pool_budget(1024 * 1024)
        .with_widget(view())
        .add_plugin(StdInstantClockPlugin)
        .add_plugin(FrameRateCapPlugin::new(60));
    let table = app.add_model(Turntable { heading: 0 }).finish();
    app.add_timer(rotate::timer(table.clone()));
    let root = app.spawn_root().id();
    app.compose(root, preview(Handle::new(car), table));
    app.run();
}
