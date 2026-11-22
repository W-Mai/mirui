//! Prepares one flight scene and draws it through the SpriteStack workspace.
use super::math::FixedMath as _;
use super::{
    flight::{Flight, Projection, V3, aircraft_axes_with_heading},
    world::Ground,
};
use mirui::render::renderer::RenderError;
use mirui::types::Fixed;
pub use mirui_sprite_stack::raster::{Canvas, Frame};
use mirui_sprite_stack::raster::{FrameEffects, RollMapping, SceneCanvas, SceneLayers};
mod aircraft;
mod exhaust;
mod landscape;
mod palette;
mod wake;

pub(crate) use aircraft::aircraft_lights;
use exhaust::{PendingFlame, finish_flames, project_flames};
use landscape::{PreparedTerrain, draw_sky, draw_world};
pub use palette::{BAYER, Daylight, Palette, blend, rgba};
use wake::{draw_smoke, draw_trails};
pub const MAX_WIDTH: usize = 480;
pub const MAX_HEIGHT: usize = 300;
#[derive(Clone, Copy, Default)]
pub struct Flame {
    pub points: [[Fixed; 2]; 3],
    pub radius: Fixed,
    pub tip_scale: Fixed,
    pub power: Fixed,
    pub alpha: Fixed,
    pub visibility: [Fixed; 3],
    pub seed: Fixed,
    pub length: Fixed,
}
pub(crate) struct FlightScene {
    pub palette: &'static Palette,
    pub flames: [Flame; 6],
    pub flame_count: usize,
    seen: [[Fixed; 3]; 6],
    prepared_seen: [[Fixed; 3]; 6],
}

/// Render pose of an aircraft placed in the scene.
#[derive(Clone, Copy)]
pub(crate) struct PlacedAircraft {
    pub(crate) position: V3,
    heading: (Fixed, Fixed),
    roll: Fixed,
    pub(crate) axes: [V3; 3],
}
impl PlacedAircraft {
    pub(crate) fn new(position: V3, [yaw, pitch, roll]: V3) -> Self {
        let heading = yaw.sin_cos();
        Self {
            position,
            heading,
            roll,
            axes: aircraft_axes_with_heading(heading, pitch, roll),
        }
    }
}
pub(crate) type Placements = [Option<PlacedAircraft>; 3];

struct Scene<'a> {
    placed: Placements,
    ground: Ground<'a>,
    flight: &'a Flight,
    palette: &'a Palette,
    frame_height: usize,
    terrain: PreparedTerrain,
    projection: Projection,
    heading: (Fixed, Fixed),
}
impl<'a> Scene<'a> {
    fn with_heading(
        placed: &Placements,
        ground: Ground<'a>,
        flight: &'a Flight,
        palette: &'a Palette,
        frame_height: usize,
        projection: Projection,
        heading: (Fixed, Fixed),
    ) -> Self {
        let terrain = PreparedTerrain::new(ground, flight, &projection, palette, placed);
        Self {
            placed: *placed,
            ground,
            flight,
            palette,
            frame_height,
            terrain,
            projection,
            heading,
        }
    }

    fn draw_ground<const DEPTH_ONLY: bool>(&self, canvas: &mut SceneCanvas<'_, '_>, limit: Fixed) {
        if self.ground.is_original() {
            draw_world::<false, DEPTH_ONLY>(
                canvas,
                self.ground,
                &self.projection,
                self.heading,
                self.palette,
                &self.terrain,
                limit,
            );
        } else {
            draw_world::<true, DEPTH_ONLY>(
                canvas,
                self.ground,
                &self.projection,
                self.heading,
                self.palette,
                &self.terrain,
                limit,
            );
        }
    }

    fn order(&self) -> [usize; 3] {
        let mut order = [0, 1, 2];
        if self.flight.count < 2 {
            return order;
        }
        let (sn, cs) = self.heading;
        let position = self.projection.position;
        let depth: [Fixed; 3] = core::array::from_fn(|i| {
            self.placed[i].as_ref().map_or(Fixed::MAX, |jet| {
                (jet.position[0] - position[0]) * sn + (jet.position[1] - position[1]) * cs
            })
        });
        order[..self.flight.count].sort_unstable_by(|a, b| depth[*a].cmp(&depth[*b]));
        order
    }
}

impl SceneLayers for Scene<'_> {
    fn terrain(&self, canvas: &mut SceneCanvas<'_, '_>, depth_limit: Option<Fixed>) {
        match depth_limit {
            Some(limit) => self.draw_ground::<true>(canvas, limit),
            None => self.draw_ground::<false>(canvas, Fixed::MAX),
        }
    }

    fn background(&self, canvas: &mut SceneCanvas<'_, '_>) {
        draw_sky(
            canvas,
            &self.projection,
            self.heading,
            self.palette,
            self.frame_height,
        );
    }

    fn effects_behind_sprites(&self, canvas: &mut SceneCanvas<'_, '_>) {
        draw_trails(
            canvas,
            self.flight,
            &self.projection,
            self.heading,
            self.palette,
        );
        draw_smoke(
            canvas,
            self.flight,
            &self.projection,
            self.heading,
            self.palette.day,
        );
    }
}

struct PreparedExhaust<'a> {
    queries: [PendingFlame; 6],
    flames: &'a mut [Flame],
    visibility: &'a mut [[Fixed; 3]; 6],
    time: Fixed,
}

impl FrameEffects for PreparedExhaust<'_> {
    type Query = PendingFlame;

    fn queries(&mut self) -> &mut [PendingFlame] {
        &mut self.queries[..self.flames.len()]
    }

    fn finish_queries(&mut self, mapping: &RollMapping) {
        finish_flames(
            mapping,
            &self.queries[..self.flames.len()],
            self.visibility,
            self.flames,
        );
    }

    fn composite(&self, canvas: &mut Canvas<'_>) -> Result<(), RenderError> {
        crate::flame::paint_region(canvas, self.flames, self.time)
    }
}

#[doc = " Integral scaling preserves hard scene pixels and caps CPU work independently of window size."]
pub fn scene_size(width: u16, height: u16) -> (usize, usize, usize) {
    super::Profile::Desktop.scene_size(width, height)
}

pub(super) struct SceneView {
    pub projection: Projection,
    pub height: usize,
}

impl FlightScene {
    pub(super) fn new(palette: &'static Palette) -> Self {
        Self {
            palette,
            flames: [Flame::default(); 6],
            flame_count: 0,
            seen: [[Fixed::ONE; 3]; 6],
            prepared_seen: [[Fixed::ONE; 3]; 6],
        }
    }

    pub(super) fn set_daylight(&mut self, day: Daylight) {
        if self.palette.day != day {
            self.palette = Palette::shared(day);
        }
    }

    pub(super) fn prepare_frame<'a>(
        &'a mut self,
        flight: &'a Flight,
        view: SceneView,
        ground: Ground<'a>,
        placed: &Placements,
    ) -> (impl SceneLayers + use<'a>, impl FrameEffects + use<'a>) {
        let projection = view.projection;
        let heading = projection.yaw.sin_cos();
        let scene = Scene::with_heading(
            placed,
            ground,
            flight,
            self.palette,
            view.height,
            projection,
            heading,
        );
        let order = scene.order();
        let mut queries = [PendingFlame::default(); 6];
        let mut count = 0;
        for &index in &order[..flight.count] {
            if let Some(jet) = placed[index] {
                project_flames(
                    flight,
                    (index, jet),
                    &projection,
                    heading,
                    &mut queries,
                    &mut count,
                );
            }
        }
        self.prepared_seen = self.seen;
        self.flame_count = count;
        (
            scene,
            PreparedExhaust {
                queries,
                flames: &mut self.flames[..count],
                visibility: &mut self.prepared_seen,
                time: flight.time,
            },
        )
    }

    pub(crate) fn finish_frame(&mut self) {
        self.seen = self.prepared_seen;
    }
}

#[cfg(test)]
#[path = "../tests/unit/raster.rs"]
pub(crate) mod tests;
