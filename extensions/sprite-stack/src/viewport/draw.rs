use super::{Camera, SceneFrame, SceneRenderer};
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
    workspace: Option<([usize; 2], Workspace)>,
    pub(super) camera: Camera,
    objects: ObjectPass,
}

impl SceneRenderer for DefaultRenderer {
    fn render(
        &mut self,
        scene: &StackScene,
        input: SceneFrame<'_>,
        frame: &mut Frame,
    ) -> Result<bool, RenderError> {
        let camera = self.camera;
        let [w, h] = [frame.width, frame.height];
        let projection = ViewProjection::from_camera(camera, [w, h])?;
        if self
            .workspace
            .as_ref()
            .is_none_or(|(size, _)| *size != [w, h])
        {
            self.workspace = None;
            self.workspace = Some(([w, h], Workspace::new(w, BAND.min(h))?));
        }
        let (_, work) = self.workspace.as_mut().expect("viewport storage");
        let objects = self.objects.prepare(scene, projection, |_| true)?;
        work.render_rows_with_overlay(frame, input.overlay, |tile| {
            tile.color.bytes.fill(0);
            tile.clear_depth();
            objects.draw(tile)
        })?;
        Ok(true)
    }
}
