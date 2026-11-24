use crate::app::{App, RendererFactory, plugin::Plugin};
use crate::surface::Surface;
use crate::ui::StyleSheetPlugin;

impl<B: Surface, F: RendererFactory<B>> Plugin<B, F> for StyleSheetPlugin {
    fn build(&mut self, app: &mut App<B, F>) {
        self.install(&mut app.world);
    }
}
