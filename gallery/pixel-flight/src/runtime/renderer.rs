use super::{
    FlightModel, FlightModelHandle, Profile, Terrain,
    stage_members::{FlightTerrain, placements},
};
use crate::{
    cockpit::{Cockpit, PreparedCockpit},
    flame::Flames,
    flight::cockpit_center,
    raster::{Canvas, Daylight, FlightScene, Palette, SceneView},
};
use mirui::{
    core::{cache::Handle, model::ModelHandle},
    render::RenderError,
    types::{Fixed, Point},
    ui::{Prop, view::PaintCtx},
};
use mirui_sprite_stack::{
    ContentFrame, FrameContent, FrameSize, Pose, TerrainTarget, ViewportContent, ViewportOutput,
    ViewportOverlay, WorldCamera,
    raster::{FrameEffects, FrameOverlay, OverlayFrame, SceneLayers},
};

struct FlightContent {
    scene: FlightScene,
    flames: Option<Flames>,
}

pub(super) fn flight_content(model: FlightModelHandle, profile: Profile) -> ViewportContent {
    ViewportContent::from_model(
        &model,
        FlightModelHandle::revision,
        FlightContent {
            scene: FlightScene::new(Palette::shared(Daylight::Day)),
            flames: (profile == Profile::Desktop)
                .then(|| Flames::new().expect("flight flame storage")),
        },
    )
}

impl FrameContent<FlightModel> for FlightContent {
    fn terrain(&self) -> Option<TerrainTarget> {
        Some(TerrainTarget::role::<FlightTerrain>())
    }

    fn output(&self) -> ViewportOutput {
        if self.flames.is_some() {
            ViewportOutput::Full
        } else {
            ViewportOutput::Bounded
        }
    }

    fn camera(&self, model: &FlightModel, size: FrameSize) -> Result<WorldCamera, RenderError> {
        let [width, height] = size.pixels().map(usize::from);
        let projection = model.flight.projection(width, height);
        Ok(WorldCamera {
            camera: projection.frame_camera(),
            pivot: cockpit_center(height),
            roll: projection.cockpit > Fixed::ZERO,
        })
    }

    fn prepare<'a>(
        &'a mut self,
        model: &'a FlightModel,
        frame: ContentFrame,
    ) -> Result<(impl SceneLayers + 'a, impl FrameEffects + 'a), RenderError> {
        let [_, height] = frame.size.pixels().map(usize::from);
        self.scene.set_daylight(model.day);
        let aircraft = placements(model);
        Ok(self.scene.prepare_frame(
            &model.flight,
            SceneView {
                projection: model.flight.render_projection(frame.camera),
                height,
            },
            model.terrain.at(frame.terrain.and(model.ground_pose)),
            &aircraft,
        ))
    }

    fn finish(
        &mut self,
        model: &FlightModel,
        frame: ContentFrame,
        storage: usize,
    ) -> Result<(), RenderError> {
        self.scene.finish_frame();
        let mut storage = storage + core::mem::size_of::<Self>();
        if let Some(flames) = &mut self.flames {
            let scale = Fixed::from(
                frame
                    .size
                    .integer_scale()
                    .expect("flight frame scale")
                    .get(),
            );
            flames.prepare(
                &self.scene.flames[..self.scene.flame_count],
                model.flight.time,
                scale,
                frame.size.pixels().map(usize::from),
            )?;
            storage += flames.bytes() - core::mem::size_of::<Flames>();
        }
        model.display_bytes.set(storage);
        Ok(())
    }

    fn present(&self, paint: &mut PaintCtx<'_>, origin: Point, scale: Fixed) {
        if let Some(flames) = &self.flames {
            flames.paint(paint, origin, scale);
        }
    }
}

pub(super) fn cockpit_layer(model: FlightModelHandle, profile: Profile) -> ViewportOverlay {
    let terrain = model.read(|model| model.terrain.clone());
    let observer = model.clone();
    ViewportOverlay::new(
        Prop::new(move || observer.revision()),
        CockpitLayer {
            model,
            terrain,
            profile,
            artwork: None,
            day: None,
            sun: [Fixed::ZERO; 2],
            pose: None,
            prepared: None,
        },
    )
}

struct CockpitLayer {
    model: FlightModelHandle,
    terrain: Handle<Terrain>,
    profile: Profile,
    artwork: Option<Cockpit>,
    day: Option<Daylight>,
    sun: [Fixed; 2],
    pose: Option<Pose>,
    prepared: Option<PreparedCockpit>,
}

impl FrameOverlay<u64> for CockpitLayer {
    fn prepare(&mut self, frame: OverlayFrame, _: &u64) -> Result<bool, RenderError> {
        let Some(camera) = frame.camera else {
            self.prepared = None;
            return Ok(false);
        };
        let [width, height] = frame.size;
        if self
            .artwork
            .as_ref()
            .is_some_and(|artwork| artwork.size() != frame.size)
        {
            self.artwork = None;
            self.day = None;
        }
        let artwork = self.artwork.get_or_insert_with(|| {
            Cockpit::with_storage(width, height, self.profile == Profile::Compact)
        });
        self.model.read(|model| {
            if self.day != Some(model.day) {
                let palette = Palette::shared(model.day);
                self.sun = palette.sun;
                artwork.set_palette(palette);
                self.day = Some(model.day);
            }
            let projection = model.flight.render_projection(camera);
            self.prepared = artwork.prepare_lit(&model.flight, projection, model.day, self.sun);
            if self.prepared.is_some() {
                self.pose = model.ground_pose;
            }
            model.cockpit_bytes.set(artwork.bytes());
        });
        Ok(self.prepared.is_some())
    }

    fn covers(&self, rect: [i32; 4], scratch: &mut [u8]) -> bool {
        self.artwork
            .as_ref()
            .zip(self.prepared.as_ref())
            .is_some_and(|(artwork, frame)| artwork.covers(rect, frame, scratch))
    }

    fn paint(&self, canvas: &mut Canvas<'_>) {
        if let Some((artwork, frame)) = self.artwork.as_ref().zip(self.prepared.as_ref()) {
            artwork.draw_prepared(canvas, self.terrain.at(self.pose), frame);
        }
    }
}
