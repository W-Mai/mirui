use super::{FlightModel, FlightModelHandle, Profile, Terrain};
use crate::{
    cockpit::{Cockpit, PreparedCockpit},
    flame::Flames,
    raster::{Canvas, Daylight, Frame, Palette, Raster},
};
use mirui::{
    core::{cache::Handle, model::ModelHandle},
    render::{RenderError, texture::AlphaMode},
    types::{Fixed, Point},
    ui::{Prop, view::PaintCtx},
};
use mirui_sprite_stack::{
    FrameSize, Pose, SceneFrame, SceneRenderer, StackScene, ViewportOverlay,
    raster::{FrameCamera, FrameOverlay, OverlayFrame},
};

pub(super) struct FlightRenderer {
    model: FlightModelHandle,
    output: FlightOutput,
}

impl FlightRenderer {
    pub(super) fn new(model: FlightModelHandle, profile: Profile) -> Self {
        Self {
            model,
            output: FlightOutput::new(profile),
        }
    }
}

impl SceneRenderer for FlightRenderer {
    fn frame_size(&self, dimensions: [u16; 2]) -> FrameSize {
        self.output.profile.frame_size(dimensions)
    }

    fn opaque_frame_byte_limit(&self) -> Option<usize> {
        (self.output.profile == Profile::Compact).then_some(2)
    }

    fn prepare_frame(&mut self, size: FrameSize) -> Result<(), RenderError> {
        self.output.resize(size.pixels().map(usize::from));
        Ok(())
    }

    fn frame_camera(&self, size: FrameSize) -> Option<FrameCamera> {
        let [width, height] = size.pixels().map(usize::from);
        Some(
            self.model
                .read(|model| model.flight.projection(width, height).frame_camera()),
        )
    }

    fn render(
        &mut self,
        scene: &StackScene,
        input: SceneFrame<'_>,
        frame: &mut Frame,
    ) -> Result<bool, RenderError> {
        frame.texture.alpha_mode = AlphaMode::Opaque;
        let scale = Fixed::from(
            input
                .size
                .integer_scale()
                .expect("flight frame scale")
                .get(),
        );
        self.model.read(|model| {
            self.output
                .render_view(model, scene, frame, scale, input.camera, input.overlay)
        })?;
        Ok(true)
    }

    fn overlay(&self, paint: &mut PaintCtx<'_>, origin: Point, scale: Fixed) {
        if let Some(flames) = &self.output.flames {
            flames.paint(paint, origin, scale);
        }
    }
}

pub(super) struct FlightOutput {
    profile: Profile,
    size: [usize; 2],
    raster: Option<Raster>,
    flames: Option<Flames>,
}

impl FlightOutput {
    pub(super) fn new(profile: Profile) -> Self {
        Self {
            profile,
            size: [0, 0],
            raster: None,
            flames: (profile == Profile::Desktop)
                .then(|| Flames::new().expect("flight flame storage")),
        }
    }

    fn resize(&mut self, size: [usize; 2]) {
        if self.size != size {
            self.raster = None;
            self.size = size;
        }
    }

    pub(super) fn prepare(
        &mut self,
        size: [usize; 2],
        day: Daylight,
    ) -> Result<&mut Raster, RenderError> {
        let compact = self.profile == Profile::Compact;
        self.resize(size);
        let raster = match &mut self.raster {
            Some(raster) => {
                if raster.palette.day != day {
                    raster.palette = Palette::new(day);
                }
                raster
            }
            empty => {
                let raster = Raster::with_palette(size, compact, Palette::new(day))?;
                empty.insert(raster)
            }
        };
        Ok(raster)
    }

    #[cfg(test)]
    pub(super) fn render(
        &mut self,
        model: &FlightModel,
        scene: &StackScene,
        frame: &mut Frame,
        scale: Fixed,
    ) -> Result<(), RenderError> {
        self.render_view(model, scene, frame, scale, None, None)
    }

    fn render_view(
        &mut self,
        model: &FlightModel,
        scene: &StackScene,
        frame: &mut Frame,
        scale: Fixed,
        camera: Option<FrameCamera>,
        overlay: Option<&dyn FrameOverlay>,
    ) -> Result<(), RenderError> {
        let projection = camera.map_or_else(
            || model.flight.projection(frame.width, frame.height),
            |camera| model.flight.render_projection(camera),
        );
        let raster = self.prepare([frame.width, frame.height], model.day)?;
        let ground = model.terrain.at(scene.pose(model.objects.ground()));
        let placed = model.objects.placed(scene);
        let managed = model.objects.managed(ground.draws_surface());
        let extra = (scene.len()
            > usize::from(ground.draws_surface()) + placed.iter().flatten().count())
        .then_some((scene, managed.as_slice()));
        raster.render_scene(
            frame,
            ground,
            &model.flight,
            (projection, overlay),
            &placed,
            extra,
        )?;
        if let (Some(raster), Some(flames)) = (&self.raster, &mut self.flames) {
            flames.prepare(
                &raster.flames[..raster.flame_count],
                model.flight.time,
                scale,
                [frame.width, frame.height],
            )?;
        }
        model.display_bytes.set(self.bytes() + frame.bytes());
        Ok(())
    }

    pub(super) fn bytes(&self) -> usize {
        core::mem::size_of::<Self>()
            + self
                .raster
                .as_ref()
                .map_or(0, |value| value.bytes() - core::mem::size_of::<Raster>())
            + self
                .flames
                .as_ref()
                .map_or(0, |value| value.bytes() - core::mem::size_of::<Flames>())
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
                let palette = Palette::new(model.day);
                self.sun = palette.sun;
                artwork.set_palette(&palette);
                self.day = Some(model.day);
            }
            let projection = model.flight.render_projection(camera);
            self.prepared = artwork.prepare_lit(&model.flight, projection, model.day, self.sun);
            if self.prepared.is_some() {
                self.pose = model.scene.pose(model.objects.ground());
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
