use super::{
    FrameSize, TerrainMember, TerrainTarget, ViewProjection, ViewportOutput, WorldCamera,
    draw::{DrawSource, StageFrame, ViewObjects},
};
use crate::{
    Geometry, StackVisual,
    raster::{
        CameraWorkspace, Frame, FrameCamera, FrameEffects, FrameOverlay, RollMapping, SceneLayers,
        draw_layers_with_objects,
    },
};
use mirui::{render::RenderError, types::Fixed};

pub(super) struct ContentStorage {
    workspace: CameraWorkspace,
    objects: ViewObjects,
    size: [usize; 2],
    pivot: Fixed,
    bounded: bool,
}

impl ContentStorage {
    pub(super) fn new(
        size: [usize; 2],
        pivot: Fixed,
        output: ViewportOutput,
    ) -> Result<Self, RenderError> {
        Ok(Self {
            workspace: CameraWorkspace::new(size, pivot, output == ViewportOutput::Bounded)?,
            objects: ViewObjects::default(),
            size,
            pivot,
            bounded: output == ViewportOutput::Bounded,
        })
    }

    pub(super) fn matches(&self, size: [usize; 2], pivot: Fixed) -> bool {
        self.size == size && self.pivot == pivot
    }

    pub(super) fn begin<'a>(
        &'a mut self,
        size: FrameSize,
        view: WorldCamera,
        source: DrawSource<'a>,
        range: Fixed,
        overlay: Option<&'a dyn FrameOverlay>,
        output: &'a mut Frame,
    ) -> FrameDraw<'a> {
        let mut camera = view.camera;
        let rotation = (view.roll || camera.roll != Fixed::ZERO)
            .then(|| (Fixed::sin_rad(camera.roll), Fixed::cos_rad(camera.roll)));
        let mapping = RollMapping::new(self.size, view.pivot, rotation);
        if mapping.rolled {
            let axis = Fixed::from_int(self.size[0] as i32) / 2;
            camera.projection.center =
                mapping.projection_center([axis, camera.projection.center[1]]);
        }
        FrameDraw {
            size,
            camera,
            source,
            range,
            mapping,
            overlay,
            output,
            workspace: &mut self.workspace,
            objects: &mut self.objects,
            bounded: self.bounded,
        }
    }
}

/// Inputs and reusable object preparation for one frame.
pub(super) struct FrameDraw<'a> {
    size: FrameSize,
    camera: FrameCamera,
    source: DrawSource<'a>,
    range: Fixed,
    mapping: RollMapping,
    overlay: Option<&'a dyn FrameOverlay>,
    output: &'a mut Frame,
    workspace: &'a mut CameraWorkspace,
    objects: &'a mut ViewObjects,
    bounded: bool,
}

impl FrameDraw<'_> {
    pub fn size(&self) -> FrameSize {
        self.size
    }

    /// Camera in source coordinates after roll mapping.
    pub fn camera(&self) -> FrameCamera {
        self.camera
    }

    pub fn source_size(&self) -> [usize; 2] {
        self.mapping.size
    }

    /// Bytes allocated for viewport pixels and reusable scene work.
    pub fn storage_bytes(&self) -> usize {
        core::mem::size_of::<ContentStorage>()
            + self.workspace.bytes()
            + self.objects.storage_bytes()
            + self.output.bytes()
    }

    pub(super) fn resolve_terrain(
        &self,
        target: TerrainTarget,
    ) -> Result<Option<TerrainMember>, RenderError> {
        let DrawSource { stage, world } = self.source;
        stage.with_entities(|members| {
            let mut matched = None;
            for &entity in members {
                let Some(visual) = world.get::<StackVisual>(entity) else {
                    continue;
                };
                if visual.role() != Some(target.0) {
                    continue;
                }
                let Some(geometry) = visual.drawable_geometry() else {
                    continue;
                };
                if matched.is_some()
                    || !matches!(geometry, Geometry::Field(_) | Geometry::LiveField(_))
                {
                    return Err(RenderError::InvalidGeometry);
                }
                matched = Some(TerrainMember {
                    entity,
                    pose: visual.pose(),
                });
            }
            Ok(matched)
        })
    }

    /// Submits procedural layers and the viewport's members through the same color and depth passes.
    pub(super) fn draw(
        &mut self,
        layers: &impl SceneLayers,
        effects: &mut impl FrameEffects,
        terrain: Option<TerrainMember>,
    ) -> Result<(), RenderError> {
        self.objects.with_stage(
            self.source.stage,
            self.source.world,
            ViewProjection::from_frame(self.camera, self.range),
            Some(StageFrame {
                camera: self.camera,
                size: self.mapping.size,
                bounded: self.bounded,
                terrain: terrain.map(|member| member.entity),
                mask_pixels: self.workspace.coverage_pixels(),
            }),
            |objects| {
                self.workspace.render_with_effects(
                    self.output,
                    &self.mapping,
                    self.overlay,
                    effects,
                    |tile, pass| {
                        draw_layers_with_objects(layers, tile, pass, |tile, styled| {
                            if styled {
                                objects.draw_styled(tile)
                            } else {
                                objects.draw(tile)
                            }
                        })
                    },
                )
            },
        )
    }
}
