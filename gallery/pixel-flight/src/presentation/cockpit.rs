use crate::{
    Profile,
    cockpit::{Cockpit, PreparedCockpit},
    gameplay::FlightModelHandle,
    palette::{Daylight, Palette},
    raster::Canvas,
    world::World as Terrain,
};
use mirui::{
    core::{cache::Handle, model::ModelHandle},
    render::RenderError,
    types::Fixed,
    ui::Prop,
};
use mirui_sprite_stack::{
    Pose, ViewportOverlay,
    raster::{FrameOverlay, OverlayFrame},
};

pub(crate) fn cockpit_layer(model: FlightModelHandle, profile: Profile) -> ViewportOverlay {
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
