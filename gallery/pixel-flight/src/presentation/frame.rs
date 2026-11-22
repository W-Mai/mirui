use super::stage::{FlightTerrain, placements};
use crate::{
    Profile,
    flame::Flames,
    flight::cockpit_center,
    gameplay::{FlightModel, FlightModelHandle},
    raster::{Daylight, FlightScene, Palette, SceneView},
};
use mirui::{
    render::RenderError,
    types::{Fixed, Point},
    ui::view::PaintCtx,
};
use mirui_sprite_stack::{
    ContentFrame, FrameContent, FrameSize, TerrainTarget, ViewportContent, ViewportOutput,
    WorldCamera,
    raster::{FrameEffects, SceneLayers},
};

struct FlightContent {
    scene: FlightScene,
    flames: Option<Flames>,
}

pub(crate) fn flight_content(model: FlightModelHandle, profile: Profile) -> ViewportContent {
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
