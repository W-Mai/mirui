#[cfg(feature = "std")]
use super::composition::build_widgets;
#[cfg(feature = "std")]
use super::state::{custom_theme, dark_with_accent, light_with_accent};
#[cfg(feature = "std")]
use crate::prelude::*;

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.register_theme(dark_with_accent())
        .register_theme(light_with_accent())
        .register_theme(custom_theme());
    app.set_theme("midnight-amber").expect("registered theme");
    app.compose(parent, build_widgets);
}
