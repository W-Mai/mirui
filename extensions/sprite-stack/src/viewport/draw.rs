use super::{Camera, FrameSize};
use crate::{
    StackScene,
    raster::{
        Frame, FrameCamera, FrameOverlay, FramePass, SceneCanvas, SceneLayers, Workspace,
        draw_layers,
    },
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
    objects: ObjectPass,
}

impl DefaultRenderer {
    pub(super) fn frame_camera(
        camera: Camera,
        size: FrameSize,
    ) -> Result<FrameCamera, RenderError> {
        let projection = ViewProjection::camera_plane(camera, size.pixels().map(usize::from))?;
        let radians =
            |degrees: Fixed| (Fixed64::from(degrees) * Fixed64::from(Fixed::PI) / 180).to_fixed();
        Ok(FrameCamera {
            projection,
            position: camera.center,
            yaw: radians(camera.yaw),
            pitch: radians(camera.pitch),
            roll: Fixed::ZERO,
        })
    }

    pub(super) fn render(
        &mut self,
        scene: &StackScene,
        camera: Camera,
        frame_camera: Option<FrameCamera>,
        overlay: Option<&dyn FrameOverlay>,
        frame: &mut Frame,
    ) -> Result<(), RenderError> {
        let [w, h] = [frame.width, frame.height];
        let projection = match frame_camera {
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
        work.render_rows_with_overlay(frame, overlay, |tile| {
            draw_layers(&objects, tile, FramePass::Color)
        })
    }
}

impl SceneLayers for ProjectedObjects<'_> {
    fn background(&self, tile: &mut SceneCanvas<'_, '_>) {
        tile.color.bytes.fill(0);
    }

    fn objects(&self, tile: &mut SceneCanvas<'_, '_>) -> Result<(), RenderError> {
        self.draw(tile)
    }
}
