//! Smallest SDL GPU backend demo: drives the full
//! `App + Backend + RendererFactory<B>` pipeline on `SdlGpuBackend`, with
//! a single full-screen background widget so exactly one `DrawCommand`
//! flows through. The GPU path for `Fill { radius: 0 }` needs to land for
//! this to paint — see `SdlGpuRenderer` for the actual work.

use mirui::app::App;
use mirui::backend::sdl_gpu::{SdlGpuBackend, SdlGpuFactory};
use mirui::layout::LayoutStyle;
use mirui::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
use mirui::types::{Color, Dimension};
use mirui::widget::builder::WidgetBuilder;

fn main() {
    let backend = SdlGpuBackend::new("mirui SDL GPU — hello clear", 640, 480);
    let mut app = App::with_factory(backend, SdlGpuFactory::new());

    let root = WidgetBuilder::new(&mut app.world)
        .layout(LayoutStyle {
            width: Dimension::Percent(100.0.into()),
            height: Dimension::Percent(100.0.into()),
            ..Default::default()
        })
        .bg_color(Color::rgba(32, 96, 192, 255))
        .id();
    app.set_root(root);

    app.add_plugin(StdInstantClockPlugin)
        .add_plugin(FpsSummaryPlugin::default());
    app.run();
}
