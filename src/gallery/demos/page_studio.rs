mod composition;
mod following;
mod model;
mod pages;
mod style;

#[cfg(test)]
mod tests;

use crate::prelude::*;
use crate::ui::widgets::{navigation, page_view};
pub use composition::build_widgets;
pub use model::{Route, Scene, Studio};

pub fn setup_app<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>, parent: Entity) {
    crate::gallery::showcase_theme::install(app);
    app.add_plugin(crate::gallery::play::font::PlayFontPlugin);
    #[cfg(feature = "std")]
    app.add_plugin(crate::app::plugins::StdInstantClockPlugin);
    app.with_widget(navigation::view::<model::Route>())
        .with_widget(page_view::view::<model::Scene>());
    app.with_offscreen_pool_budget(960 * 680 * 8);
    let path = app
        .add_model(navigation::NavigationPath::<model::Route>::with_capacity(8))
        .finish();
    let controls = app.add_model(model::Studio::default()).finish();
    app.compose(parent, build_widgets(path, controls));
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::fixed(960, 680);
