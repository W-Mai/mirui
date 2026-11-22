use super::{Camera, FrameSize};
use crate::{
    Geometry, Pose, StackScene, StackStage, StackVisual,
    raster::{
        Frame, FrameCamera, FrameOverlay, FramePass, SceneCanvas, SceneLayers, Workspace,
        draw_layers_with_objects,
    },
};
mod objects;
mod projection;
use mirui::types::{Fixed, Fixed64};
use mirui::{
    ecs::{Entity, World},
    render::RenderError,
};
pub use objects::{ObjectPass, ProjectedObjects};
use objects::{ObjectSource, ObjectWork};
pub use projection::ViewProjection;

const BAND: usize = 16;

#[derive(Default)]
pub(super) struct DefaultRenderer {
    workspace: Option<([usize; 2], Workspace)>,
    objects: ViewObjects,
}

enum ViewObjects {
    Scene(ObjectPass),
    Stage(ObjectWork<Entity>),
}

impl Default for ViewObjects {
    fn default() -> Self {
        Self::Scene(ObjectPass::default())
    }
}

#[derive(Clone, Copy)]
pub(super) enum DrawSource<'a> {
    Scene(&'a StackScene),
    Stage {
        stage: &'a StackStage,
        world: &'a World,
    },
}

struct StageObjects<'a> {
    entities: &'a [Entity],
    world: &'a World,
}

impl ObjectSource<Entity> for StageObjects<'_> {
    fn visit(&self, mut read: impl FnMut(Entity, &Geometry, Pose)) {
        for &entity in self.entities {
            if let Some(visual) = self.world.get::<StackVisual>(entity)
                && let Some(geometry) = visual.drawable_geometry()
            {
                read(entity, geometry, visual.pose());
            }
        }
    }

    fn with_geometry<R>(
        &self,
        id: Entity,
        read: impl FnOnce(&Geometry, crate::stage::Material) -> R,
    ) -> Option<R> {
        self.world
            .get::<StackVisual>(id)
            .map(|visual| read(visual.geometry(), visual.material()))
    }
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
        source: DrawSource<'_>,
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
        match source {
            DrawSource::Scene(scene) => {
                if !matches!(self.objects, ViewObjects::Scene(_)) {
                    self.objects = ViewObjects::default();
                }
                let ViewObjects::Scene(objects) = &mut self.objects else {
                    unreachable!()
                };
                let objects = objects.prepare(scene, projection, |_, _, _| true)?;
                render_rows(work, frame, overlay, |tile| objects.draw(tile))
            }
            DrawSource::Stage { stage, world } => {
                if !matches!(self.objects, ViewObjects::Stage(_)) {
                    self.objects = ViewObjects::Stage(ObjectWork::default());
                }
                let ViewObjects::Stage(objects) = &mut self.objects else {
                    unreachable!()
                };
                stage.with_entities(|entities| {
                    let source = StageObjects { entities, world };
                    let objects = objects.prepare(&source, projection, |_, _, _| true)?;
                    render_rows(work, frame, overlay, |tile| objects.draw(tile))
                })
            }
        }
    }
}

fn render_rows(
    work: &mut Workspace,
    frame: &mut Frame,
    overlay: Option<&dyn FrameOverlay>,
    draw_objects: impl Fn(&mut SceneCanvas<'_, '_>) -> Result<(), RenderError>,
) -> Result<(), RenderError> {
    work.render_rows_with_overlay(frame, overlay, |tile| {
        draw_layers_with_objects(&Transparent, tile, FramePass::Color, &draw_objects)
    })
}

struct Transparent;
impl SceneLayers for Transparent {
    fn background(&self, tile: &mut SceneCanvas<'_, '_>) {
        tile.color.bytes.fill(0);
    }
}

impl SceneLayers for ProjectedObjects<'_> {
    fn background(&self, tile: &mut SceneCanvas<'_, '_>) {
        Transparent.background(tile);
    }

    fn objects(&self, tile: &mut SceneCanvas<'_, '_>) -> Result<(), RenderError> {
        self.draw(tile)
    }
}
