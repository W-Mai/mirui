use super::{Camera, SceneRenderer};
use crate::{
    StackScene,
    raster::{Frame, Workspace},
};
mod objects;
mod projection;
use mirui::render::RenderError;
pub use objects::{ObjectPass, ProjectedObjects};
pub use projection::ViewProjection;

const BAND: usize = 16;

#[derive(Default)]
pub(super) struct DefaultRenderer {
    output: Option<(Frame, Workspace)>,
    pub(super) camera: Camera,
    objects: ObjectPass,
}

impl SceneRenderer for DefaultRenderer {
    fn render(
        &mut self,
        scene: &StackScene,
        size: [u16; 2],
    ) -> Result<Option<&mut Frame>, RenderError> {
        let camera = self.camera;
        let [w, h] = size.map(usize::from);
        let projection = ViewProjection::from_camera(camera, [w, h])?;
        if self
            .output
            .as_ref()
            .is_none_or(|(f, _)| (f.width, f.height) != (w, h))
        {
            let mut frame = Frame::new(w, h)?;
            frame.texture.alpha_mode = mirui::render::texture::AlphaMode::Blend;
            self.output = Some((frame, Workspace::new(w, BAND.min(h))?));
        }
        let (frame, work) = self.output.as_mut().expect("viewport storage");
        let objects = self.objects.prepare(scene, projection, |_| true)?;
        work.render_rows(frame, |tile| {
            tile.color.bytes.fill(0);
            tile.clear_depth();
            objects.draw(tile)
        })?;
        Ok(Some(frame))
    }

    fn frame(&self) -> Option<&Frame> {
        self.output.as_ref().map(|(frame, _)| frame)
    }
}
