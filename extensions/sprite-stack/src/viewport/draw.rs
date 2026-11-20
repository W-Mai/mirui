use super::{Camera, FrameSize, SceneFrame, SceneRenderer};
use crate::{
    StackScene,
    raster::{Frame, FrameCamera, Workspace},
};
mod objects;
mod projection;
use mirui::render::RenderError;
use mirui::types::{Fixed, Fixed64};
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
    fn frame_camera(&self, size: FrameSize) -> Result<Option<FrameCamera>, RenderError> {
        let projection = ViewProjection::camera_plane(self.camera, size.pixels().map(usize::from))?;
        let radians =
            |degrees: Fixed| (Fixed64::from(degrees) * Fixed64::from(Fixed::PI) / 180).to_fixed();
        Ok(Some(FrameCamera {
            projection,
            position: self.camera.center,
            yaw: radians(self.camera.yaw),
            pitch: radians(self.camera.pitch),
            roll: Fixed::ZERO,
        }))
    }

    fn render(
        &mut self,
        scene: &StackScene,
        input: SceneFrame<'_>,
        frame: &mut Frame,
    ) -> Result<bool, RenderError> {
        let camera = self.camera;
        let [w, h] = [frame.width, frame.height];
        let projection = match input.camera {
            Some(frame) => ViewProjection::from_plane(camera, frame.projection),
            None => ViewProjection::from_camera(camera, [w, h])?,
        };
        if self
            .workspace
            .as_ref()
            .is_none_or(|(size, _)| *size != [w, h])
        {
            self.workspace = None;
            self.workspace = Some(([w, h], Workspace::new(w, BAND.min(h))?));
        }
        let (_, work) = self.workspace.as_mut().expect("viewport storage");
        let objects = self.objects.prepare(scene, projection, |_, _, _| true)?;
        work.render_rows_with_overlay(frame, input.overlay, |tile| {
            tile.color.bytes.fill(0);
            tile.clear_depth();
            objects.draw(tile)
        })?;
        Ok(true)
    }
}
