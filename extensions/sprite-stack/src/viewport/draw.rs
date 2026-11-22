use super::{Camera, FrameSize};
use crate::{
    Geometry, Pose, SampledStyle, StackScene, StackStage, StackVisual,
    raster::{
        Frame, FrameCamera, FrameOverlay, FramePass, SceneCanvas, SceneLayers, Workspace,
        draw_layers,
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
use objects::{ObjectSource, ObjectWork, Projected};
pub use projection::ViewProjection;

const BAND: usize = 16;

#[derive(Default)]
pub(super) struct DefaultRenderer {
    workspace: Option<([usize; 2], Workspace)>,
    objects: ViewObjects,
}

pub(super) enum ViewObjects {
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
    terrain: Option<Entity>,
}

pub(super) struct PreparedStageObjects<'a, 'scene>(Projected<'a, Entity, StageObjects<'scene>>);

#[derive(Clone, Copy)]
pub(super) struct StageFrame {
    pub(super) camera: FrameCamera,
    pub(super) size: [usize; 2],
    pub(super) bounded: bool,
    pub(super) terrain: Option<Entity>,
    pub(super) mask_pixels: usize,
}

impl PreparedStageObjects<'_, '_> {
    pub(super) fn draw(&self, tile: &mut SceneCanvas<'_, '_>) -> Result<(), RenderError> {
        self.0.draw(tile)
    }

    pub(super) fn draw_styled(&self, tile: &mut SceneCanvas<'_, '_>) -> Result<(), RenderError> {
        self.0.draw_styled(tile)
    }
}

impl ViewObjects {
    pub(super) fn storage_bytes(&self) -> usize {
        match self {
            Self::Scene(pass) => pass.storage_bytes(),
            Self::Stage(pass) => pass.storage_bytes(),
        }
    }

    pub(super) fn legacy(&mut self) -> &mut ObjectPass {
        if !matches!(self, Self::Scene(_)) {
            *self = Self::default();
        }
        let Self::Scene(objects) = self else {
            unreachable!()
        };
        objects
    }

    pub(super) fn with_stage<R>(
        &mut self,
        stage: &StackStage,
        world: &World,
        camera: ViewProjection,
        frame: Option<StageFrame>,
        use_objects: impl FnOnce(&PreparedStageObjects<'_, '_>) -> Result<R, RenderError>,
    ) -> Result<R, RenderError> {
        if !matches!(self, Self::Stage(_)) {
            *self = Self::Stage(ObjectWork::default());
        }
        let Self::Stage(objects) = self else {
            unreachable!()
        };
        stage.with_entities(|entities| {
            let source = StageObjects {
                entities,
                world,
                terrain: frame.and_then(|frame| frame.terrain),
            };
            let objects = objects.prepare(&source, camera, frame, |_, _, _| true)?;
            use_objects(&PreparedStageObjects(objects))
        })
    }
}

impl ObjectSource<Entity> for StageObjects<'_> {
    fn visit(&self, mut read: impl FnMut(Entity, &Geometry, Pose, Option<&SampledStyle>)) {
        for &entity in self.entities {
            if Some(entity) == self.terrain {
                continue;
            }
            if let Some(visual) = self.world.get::<StackVisual>(entity)
                && let Some(geometry) = visual.drawable_geometry()
            {
                read(entity, geometry, visual.pose(), visual.sampled_style());
            }
        }
    }

    fn with_geometry<R>(
        &self,
        id: Entity,
        read: impl FnOnce(&Geometry, crate::stage::Material, Option<&SampledStyle>) -> R,
    ) -> Option<R> {
        self.world
            .get::<StackVisual>(id)
            .map(|visual| read(visual.geometry(), visual.material(), visual.sampled_style()))
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
                let objects = self
                    .objects
                    .legacy()
                    .prepare(scene, projection, |_, _, _| true)?;
                render_rows(work, frame, overlay, &objects)
            }
            DrawSource::Stage { stage, world } => {
                self.objects
                    .with_stage(stage, world, projection, None, |objects| {
                        render_rows(work, frame, overlay, objects)
                    })
            }
        }
    }
}

fn render_rows(
    work: &mut Workspace,
    frame: &mut Frame,
    overlay: Option<&dyn FrameOverlay>,
    layers: &impl SceneLayers,
) -> Result<(), RenderError> {
    work.render_rows_with_overlay(frame, overlay, |tile| {
        draw_layers(layers, tile, FramePass::Color)
    })
}

impl SceneLayers for ProjectedObjects<'_> {
    fn background(&self, tile: &mut SceneCanvas<'_, '_>) {
        tile.color.bytes.fill(0);
    }

    fn objects(&self, tile: &mut SceneCanvas<'_, '_>) -> Result<(), RenderError> {
        self.draw(tile)
    }
}

impl SceneLayers for PreparedStageObjects<'_, '_> {
    fn background(&self, tile: &mut SceneCanvas<'_, '_>) {
        tile.color.bytes.fill(0);
    }

    fn objects(&self, tile: &mut SceneCanvas<'_, '_>) -> Result<(), RenderError> {
        self.draw(tile)
    }
}
