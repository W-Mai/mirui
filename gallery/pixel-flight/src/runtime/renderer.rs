use super::{FlightModel, FlightModelHandle, Profile};
use crate::{
    cockpit::Cockpit,
    flame::Flames,
    raster::{Daylight, Frame, Palette, Raster},
};
use core::{cell::Cell, num::NonZeroU16};
use mirui::{
    core::model::ModelHandle,
    render::{RenderError, texture::AlphaMode},
    types::{Fixed, Point},
    ui::view::PaintCtx,
};
use mirui_sprite_stack::{SceneRenderer, StackScene};

pub(super) struct FlightRenderer {
    model: FlightModelHandle,
    output: FlightOutput,
    // StackViewport requests the size before preparing or rendering a new image.
    fit: Cell<[u16; 3]>,
}

impl FlightRenderer {
    pub(super) fn new(model: FlightModelHandle, profile: Profile) -> Self {
        Self {
            model,
            output: FlightOutput::new(profile),
            fit: Cell::new([0; 3]),
        }
    }
}

impl SceneRenderer for FlightRenderer {
    fn frame_size(&self, [width, height]: [u16; 2]) -> [u16; 2] {
        let (width, height, scale) = self.output.profile.scene_size(width, height);
        let [width, height, scale] = [width, height, scale].map(|value| value as u16);
        self.fit.set([width, height, scale]);
        [width, height]
    }

    fn opaque_frame_byte_limit(&self) -> Option<usize> {
        (self.output.profile == Profile::Compact).then_some(2)
    }

    fn prepare_frame(&mut self, _: [u16; 2]) -> Result<(), RenderError> {
        let [width, height, _] = self.fit.get();
        self.output
            .prepare_cockpit([width, height].map(usize::from));
        Ok(())
    }

    fn render(
        &mut self,
        scene: &StackScene,
        _: [u16; 2],
        frame: &mut Frame,
    ) -> Result<bool, RenderError> {
        frame.texture.alpha_mode = AlphaMode::Opaque;
        self.model.read(|model| {
            self.output
                .render(model, scene, frame, self.fit.get()[2].into())
        })?;
        Ok(true)
    }

    fn pixel_scale(&self) -> Option<NonZeroU16> {
        NonZeroU16::new(self.fit.get()[2])
    }

    fn overlay(&self, paint: &mut PaintCtx<'_>, origin: Point, scale: Fixed) {
        if let Some(flames) = &self.output.flames {
            flames.paint(paint, origin, scale);
        }
    }
}

pub(super) struct FlightOutput {
    profile: Profile,
    cockpit: Option<Cockpit>,
    raster: Option<Raster>,
    flames: Option<Flames>,
}

impl FlightOutput {
    pub(super) fn new(profile: Profile) -> Self {
        Self {
            profile,
            cockpit: None,
            raster: None,
            flames: (profile == Profile::Desktop)
                .then(|| Flames::new().expect("flight flame storage")),
        }
    }

    fn prepare_cockpit(&mut self, [width, height]: [usize; 2]) -> &mut Cockpit {
        if self
            .cockpit
            .as_ref()
            .is_some_and(|cockpit| cockpit.size() != [width, height])
        {
            self.raster = None;
            self.cockpit = None;
        }
        self.cockpit.get_or_insert_with(|| {
            Cockpit::with_storage(width, height, self.profile == Profile::Compact)
        })
    }

    pub(super) fn prepare(
        &mut self,
        size: [usize; 2],
        day: Daylight,
    ) -> Result<(&mut Cockpit, &mut Raster), RenderError> {
        let compact = self.profile == Profile::Compact;
        self.prepare_cockpit(size);
        let cockpit = self
            .cockpit
            .as_mut()
            .expect("cockpit prepared for this frame");
        let raster = match &mut self.raster {
            Some(raster) => {
                if raster.palette.day != day {
                    raster.palette = Palette::new(day);
                    cockpit.set_palette(&raster.palette);
                }
                raster
            }
            empty => {
                let raster = Raster::with_palette(size, compact, Palette::new(day))?;
                cockpit.set_palette(&raster.palette);
                empty.insert(raster)
            }
        };
        Ok((cockpit, raster))
    }

    pub(super) fn render(
        &mut self,
        model: &FlightModel,
        scene: &StackScene,
        frame: &mut Frame,
        scale: Fixed,
    ) -> Result<(), RenderError> {
        let (cockpit, raster) = self.prepare([frame.width, frame.height], model.day)?;
        let ground = model.terrain.at(scene.pose(model.objects.ground()));
        let placed = model.objects.placed(scene);
        let managed = model.objects.managed(ground.draws_surface());
        let extra = (scene.len()
            > usize::from(ground.draws_surface()) + placed.iter().flatten().count())
        .then_some((scene, managed.as_slice()));
        raster.render_scene(frame, ground, &model.flight, Some(cockpit), &placed, extra)?;
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
                .cockpit
                .as_ref()
                .map_or(0, |value| value.bytes() - core::mem::size_of::<Cockpit>())
            + self
                .flames
                .as_ref()
                .map_or(0, |value| value.bytes() - core::mem::size_of::<Flames>())
    }
}
