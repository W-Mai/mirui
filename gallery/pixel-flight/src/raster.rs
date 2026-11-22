//! Prepares one flight scene and draws it through the SpriteStack workspace.
#[cfg(test)]
mod compact;
#[cfg(test)]
use super::jet;
use super::math::FixedMath as _;
use super::{
    flight::{Flight, Projection, V3, aircraft_axes_with_heading},
    world::Ground,
};
#[cfg(test)]
use crate::flight::cockpit_center;
#[cfg(test)]
use crate::world::World;
use mirui::render::renderer::RenderError;
#[cfg(test)]
use mirui::types::Color;
use mirui::types::Fixed;
#[cfg(test)]
use mirui_sprite_stack::raster::{
    CameraWorkspace, FrameOverlay, FramePass, Workspace, draw_layers,
};
pub use mirui_sprite_stack::raster::{Canvas, Frame};
use mirui_sprite_stack::raster::{FrameEffects, RollMapping, SceneCanvas, SceneLayers};
mod aircraft;
mod exhaust;
mod landscape;
mod palette;
mod wake;

#[cfg(test)]
use aircraft::PreparedJet;
pub(crate) use aircraft::aircraft_lights;
#[cfg(test)]
use exhaust::ExhaustFrame;
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

#[cfg(test)]
pub(crate) struct Raster {
    compact: bool,
    workspace: CameraWorkspace,
    pub(crate) scene: FlightScene,
}

/// Render pose and baked geometry of one aircraft placed in the scene.
#[derive(Clone, Copy)]
pub(crate) struct PlacedAircraft {
    pub(crate) position: V3,
    heading: (Fixed, Fixed),
    roll: Fixed,
    pub(crate) axes: [V3; 3],
    #[cfg(test)]
    pub(crate) model: &'static mirui_sprite_stack::format::PreparedSpriteStack<'static>,
}
impl PlacedAircraft {
    pub(crate) fn new(
        position: V3,
        [yaw, pitch, roll]: V3,
        #[cfg(test)] model: &'static mirui_sprite_stack::format::PreparedSpriteStack<'static>,
    ) -> Self {
        let heading = yaw.sin_cos();
        Self {
            position,
            heading,
            roll,
            axes: aircraft_axes_with_heading(heading, pitch, roll),
            #[cfg(test)]
            model,
        }
    }

    #[cfg(test)]
    pub(crate) fn from_flight(flight: &Flight) -> [Option<Self>; 3] {
        core::array::from_fn(|i| {
            (i < flight.count).then(|| {
                let jet = &flight.jets[i];
                Self::new(
                    jet.position,
                    [jet.yaw, jet.pitch, jet.roll + jet.spin()],
                    &jet::MODEL,
                )
            })
        })
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
    #[cfg(test)]
    fn new(
        placed: &Placements,
        ground: Ground<'a>,
        flight: &'a Flight,
        palette: &'a Palette,
        frame_height: usize,
        projection: Projection,
    ) -> Self {
        Self::with_heading(
            placed,
            ground,
            flight,
            palette,
            frame_height,
            projection,
            projection.yaw.sin_cos(),
        )
    }

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

    #[cfg(test)]
    fn draw(&self, canvas: &mut SceneCanvas<'_, '_>) -> [usize; 3] {
        let order = self.order();
        let prepared = self.prepare([canvas.color.width, canvas.color.height]);
        self.draw_ordered::<false>(canvas, &order, &prepared, Fixed::MAX)
            .unwrap();
        order
    }
    #[cfg(test)]
    fn prepare(&self, size: [usize; 2]) -> [Option<PreparedJet>; 3] {
        core::array::from_fn(|index| {
            PreparedJet::new(
                self.flight,
                index,
                self.placed[index]?,
                &self.projection,
                self.heading,
                self.palette,
                size,
            )
        })
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
    #[cfg(test)]
    fn draw_ordered<const DEPTH_ONLY: bool>(
        &self,
        canvas: &mut SceneCanvas<'_, '_>,
        order: &[usize; 3],
        prepared: &[Option<PreparedJet>; 3],
        limit: Fixed,
    ) -> Result<(), RenderError> {
        let layers = FlightLayers {
            scene: self,
            order,
            prepared,
        };
        let pass = if DEPTH_ONLY {
            FramePass::Depth { limit }
        } else {
            FramePass::Color
        };
        draw_layers(&layers, canvas, pass)
    }
}

#[cfg(test)]
struct FlightLayers<'a> {
    scene: &'a Scene<'a>,
    order: &'a [usize; 3],
    prepared: &'a [Option<PreparedJet>; 3],
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

#[cfg(test)]
impl SceneLayers for FlightLayers<'_> {
    fn terrain(&self, canvas: &mut SceneCanvas<'_, '_>, limit: Option<Fixed>) {
        self.scene.terrain(canvas, limit);
    }

    fn background(&self, canvas: &mut SceneCanvas<'_, '_>) {
        self.scene.background(canvas);
    }

    fn effects_behind_sprites(&self, canvas: &mut SceneCanvas<'_, '_>) {
        self.scene.effects_behind_sprites(canvas);
    }

    fn sprites(&self, canvas: &mut SceneCanvas<'_, '_>) {
        let scene = self.scene;
        for i in &self.order[..scene.flight.count] {
            if let Some(jet) = &self.prepared[*i] {
                jet.draw(canvas, &scene.projection, scene.palette);
            }
        }
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

#[cfg(test)]
struct CockpitOverlay<'a> {
    cockpit: &'a super::cockpit::Cockpit,
    prepared: super::cockpit::PreparedCockpit,
    world: Ground<'a>,
}

#[cfg(test)]
impl FrameOverlay for CockpitOverlay<'_> {
    fn covers(&self, rect: [i32; 4], scratch: &mut [u8]) -> bool {
        self.cockpit.covers(rect, &self.prepared, scratch)
    }

    fn paint(&self, canvas: &mut Canvas<'_>) {
        self.cockpit
            .draw_prepared(canvas, self.world, &self.prepared);
    }
}

#[cfg(test)]
fn map_projection(
    mut projection: Projection,
    width: usize,
    height: usize,
) -> (Projection, RollMapping) {
    let rotation = (projection.cockpit > Fixed::ZERO || projection.roll != Fixed::ZERO)
        .then(|| projection.roll.sin_cos());
    let roll = RollMapping::new([width, height], cockpit_center(height), rotation);
    if roll.rolled {
        let axis = Fixed::from_int(width as i32) / 2;
        projection.camera.center = roll.projection_center([axis, projection.camera.center[1]]);
    }
    (projection, roll)
}

#[doc = " Integral scaling preserves hard scene pixels and caps CPU work independently of window size."]
pub fn scene_size(width: u16, height: u16) -> (usize, usize, usize) {
    super::Profile::Desktop.scene_size(width, height)
}
#[cfg(test)]
impl Raster {
    pub fn new(size: [usize; 2], compact: bool) -> Result<Self, RenderError> {
        Ok(Self {
            compact,
            workspace: CameraWorkspace::new(size, cockpit_center(size[1]), compact)?,
            scene: FlightScene::new(Palette::shared(Daylight::Day)),
        })
    }
    pub fn bytes(&self) -> usize {
        self.workspace.bytes() + core::mem::size_of::<Self>()
    }
    #[cfg(test)]
    pub(crate) fn render(
        &mut self,
        frame: &mut Frame,
        world: &World,
        flight: &Flight,
        placed: &Placements,
    ) {
        let projection = flight.projection(frame.width, frame.height);
        self.render_scene(
            frame,
            flight,
            (projection, None),
            world.at(Some(Default::default())),
            placed,
        )
        .unwrap();
    }

    #[cfg(test)]
    pub(super) fn render_with_cockpit(
        &mut self,
        frame: &mut Frame,
        world: &World,
        flight: &Flight,
        cockpit: &super::cockpit::Cockpit,
        placed: &Placements,
    ) {
        let projection = flight.projection(frame.width, frame.height);
        let ground = world.at(Some(Default::default()));
        let overlay = cockpit
            .prepare(flight, projection, self.scene.palette)
            .map(|prepared| CockpitOverlay {
                cockpit,
                prepared,
                world: ground,
            });
        self.render_scene(
            frame,
            flight,
            (
                projection,
                overlay.as_ref().map(|overlay| overlay as &dyn FrameOverlay),
            ),
            ground,
            placed,
        )
        .unwrap();
    }

    pub(super) fn render_scene(
        &mut self,
        frame: &mut Frame,
        flight: &Flight,
        (screen, overlay): (Projection, Option<&dyn FrameOverlay>),
        ground: Ground<'_>,
        placed: &Placements,
    ) -> Result<(), RenderError> {
        let (projection, mapping) = map_projection(screen, frame.width, frame.height);
        self.scene.prepare_reference(
            flight,
            SceneView {
                projection,
                source_size: mapping.size,
                height: frame.height,
                compact: self.compact,
            },
            ground,
            placed,
            |layers, effects| {
                self.workspace
                    .render_layers(frame, &mapping, overlay, layers, effects)
            },
        )
    }
}

pub(super) struct SceneView {
    pub projection: Projection,
    #[cfg(test)]
    pub source_size: [usize; 2],
    pub height: usize,
    #[cfg(test)]
    pub compact: bool,
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

    #[cfg(test)]
    fn prepare_reference(
        &mut self,
        flight: &Flight,
        view: SceneView,
        ground: Ground<'_>,
        placed: &Placements,
        draw: impl FnOnce(&FlightLayers<'_>, &mut ExhaustFrame<'_>) -> Result<(), RenderError>,
    ) -> Result<(), RenderError> {
        let Self {
            palette,
            flames,
            flame_count,
            seen,
            ..
        } = self;
        let projection = view.projection;
        let heading = projection.yaw.sin_cos();
        let scene = Scene::with_heading(
            placed,
            ground,
            flight,
            palette,
            view.height,
            projection,
            heading,
        );
        let order = scene.order();
        let mut models = scene.prepare(view.source_size);
        if view.compact {
            for model in models.iter_mut().flatten() {
                model.prepare_regions(&projection);
            }
        }
        let mut pending = [PendingFlame::default(); 6];
        let mut count = 0;
        for &index in &order[..flight.count] {
            if let Some(jet) = placed[index] {
                project_flames(
                    flight,
                    (index, jet),
                    &projection,
                    scene.heading,
                    &mut pending,
                    &mut count,
                );
            }
        }
        let mut next_seen = *seen;
        draw(
            &FlightLayers {
                scene: &scene,
                order: &order,
                prepared: &models,
            },
            &mut ExhaustFrame {
                queries: &mut pending[..count],
                visibility: &mut next_seen,
                flames: &mut flames[..count],
                time: flight.time,
            },
        )?;

        *seen = next_seen;
        *flame_count = count;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::flight::{Settings, View};
    use super::*;
    #[test]
    fn deferred_effect_queries_see_the_same_aircraft_depth_as_the_visible_tiles() {
        let world = World::with_size(7, Fixed::HALF, 64, 32, 16);
        let mut flight = Flight::new(&world, Settings::default());
        flight.intro = Fixed::from_int(8);
        let palette = Palette::new(Daylight::Day);
        let placed = PlacedAircraft::from_flight(&flight);
        let ground = world.at(Some(Default::default()));
        let scene = Scene::new(
            &placed,
            ground,
            &flight,
            &palette,
            128,
            flight.projection(128, 128),
        );
        let models = scene.prepare([128; 2]);
        let order = scene.order();
        let mut frame = Frame::new(128, 128).unwrap();
        let mut work = Workspace::new(128, 128).unwrap();
        let mut depths = |only_depth: bool, aircraft: bool| {
            work.with_canvas(&mut frame, |tile| {
                if !aircraft {
                    tile.clear_depth();
                    scene.draw_ground::<true>(tile, Fixed::MAX);
                } else if only_depth {
                    scene
                        .draw_ordered::<true>(tile, &order, &models, Fixed::MAX)
                        .unwrap();
                } else {
                    scene
                        .draw_ordered::<false>(tile, &order, &models, Fixed::MAX)
                        .unwrap();
                }
                tile.depth.to_vec()
            })
        };
        let ground = depths(true, false);
        let visible = depths(false, true);
        assert!(visible.iter().zip(ground).any(|(v, g)| *v < g));
        assert_eq!(depths(true, true), visible);
    }

    #[test]
    fn placement_selects_the_visible_object_position_and_its_sparse_level() {
        use mirui_sprite_stack::format::{PreparedSpriteStack, SparseLevel};
        const EMPTY: SparseLevel<'static> = SparseLevel::new(1, &[0, 0], &[]).unwrap();
        static MODEL: PreparedSpriteStack<'static> =
            PreparedSpriteStack::new([1, 1, 1], [0; 3], 256, &[[0; 4]], &[EMPTY]).unwrap();
        let world = World::with_size(7, Fixed::HALF, 64, 32, 16);
        let flight = Flight::new(&world, Settings::default());
        let projection = flight.projection(128, 128);
        let palette = Palette::new(Daylight::Day);
        let mut placed = PlacedAircraft::from_flight(&flight);
        let mut active = placed[0].unwrap();
        active.position[0] += Fixed::from_int(5);
        active.model = &MODEL;
        placed[0] = Some(active);
        placed[1] = None;
        let scene = Scene::new(
            &placed,
            world.at(Some(Default::default())),
            &flight,
            &palette,
            128,
            flight.projection(128, 128),
        );
        let prepared = scene.prepare([128; 2]);
        let jet = prepared[0].as_ref().unwrap();
        assert_eq!(
            jet.sprite.center(),
            projection.project(active.position).unwrap()
        );
        assert!(jet.sprite.level().layer(0).unwrap().is_empty());
        assert!(prepared[1].is_none());
    }
    #[test]
    fn region_writes_preserve_global_scene_samples_and_depth() {
        let world = World::with_size(7, Fixed::from_ratio(33, 100), 64, 32, 16);
        let mut flight = Flight::new(&world, Settings::default());
        flight.set_wingmen(2);
        flight.controls.key(0, true);
        let raster = Raster::new([128, 128], false).unwrap();
        let mut full = Frame::new(128, 128).unwrap();
        let mut full_workspace = Workspace::new(128, 128).unwrap();
        let mut region = Frame::new(40, 40).unwrap();
        let mut region_workspace = Workspace::new(40, 40).unwrap();
        let bounds = [3, 2, 126, 124];
        for view in [View::Overhead, View::Chase, View::Cockpit] {
            flight.view = view;
            flight.roll();
            for _ in 0..300 {
                flight.step(&world, 16, 128, 128);
            }
            flight.settings.clouds = match view {
                View::Overhead => Fixed::ZERO,
                View::Chase => Fixed::from_ratio(1, 16),
                View::Cockpit => Fixed::ONE,
            };
            let placed = PlacedAircraft::from_flight(&flight);
            let scene = Scene::new(
                &placed,
                world.at(Some(Default::default())),
                &flight,
                raster.scene.palette,
                128,
                flight.projection(128, 128),
            );
            let reference_depth =
                full_workspace.with_region(&mut full, [128, 128], [0, 0], bounds, |canvas| {
                    scene.draw(canvas);
                    canvas.depth.to_vec()
                });
            for origin in [
                [-42, -42],
                [-2, -2],
                [0, 0],
                [17, 37],
                [44, 20],
                [104, 90],
                [128, 128],
            ] {
                let local_depth = region_workspace.with_region(
                    &mut region,
                    [128, 128],
                    origin,
                    bounds,
                    |canvas| {
                        assert_eq!(
                            canvas.sample_depth(80, 80),
                            if canvas.color.contains(80, 80) {
                                Some(canvas.depth[canvas.color.index(80, 80)])
                            } else {
                                None
                            }
                        );
                        scene.draw(canvas);
                        if canvas.color.clip[0] < canvas.color.clip[2]
                            && canvas.color.clip[1] < canvas.color.clip[3]
                        {
                            assert!(canvas.columns.iter().all(|y| *y <= canvas.color.clip[3]));
                        }
                        canvas.depth.to_vec()
                    },
                );
                let reference = full.texture.read_pixels().unwrap();
                let local = region.texture.read_pixels().unwrap();
                for y in (origin[1] + 1).max(bounds[1])..(origin[1] + 39).min(bounds[3]) {
                    for x in (origin[0] + 1).max(bounds[0])..(origin[0] + 39).min(bounds[2]) {
                        let a = y as usize * 128 + x as usize;
                        let b = ((y - origin[1]) * 40 + x - origin[0]) as usize;
                        assert_eq!(
                            &reference.bytes()[a * 4..a * 4 + 4],
                            &local.bytes()[b * 4..b * 4 + 4],
                            "view {view:?} origin {origin:?} at {x},{y}"
                        );
                        assert_eq!(reference_depth[a], local_depth[b]);
                    }
                }
            }
        }
    }
    #[test]
    fn all_camera_frames_are_opaque_and_storage_is_bounded() {
        let world = World::new(7, Fixed::from_ratio(33, 100));
        let mut flight = Flight::new(&world, Settings::default());
        let mut raster = Raster::new([240, 150], false).unwrap();
        let mut frame = Frame::new(240, 150).unwrap();
        for view in [View::Overhead, View::Chase, View::Cockpit] {
            flight.view = view;
            for _ in 0..240 {
                flight.step(&world, 16, 240, 150);
            }
            raster.render(
                &mut frame,
                &world,
                &flight,
                &PlacedAircraft::from_flight(&flight),
            );
            let pixels = frame.texture.read_pixels().unwrap();
            assert!(pixels.bytes().chunks_exact(4).all(|c| c[3] == 255));
            let first = &pixels.bytes()[..4];
            assert!(pixels.bytes().chunks_exact(4).any(|c| c != first));
        }
        let allocation = crate::test_support::tracked_allocations(|| {
            raster.render(
                &mut frame,
                &world,
                &flight,
                &PlacedAircraft::from_flight(&flight),
            )
        });
        assert_eq!(allocation, 0);
        let max = Raster::new([MAX_WIDTH, MAX_HEIGHT], false).unwrap();
        assert!(max.bytes() + MAX_WIDTH * MAX_HEIGHT * 4 < 8 * 1024 * 1024);
        for size in [(0, 0), (1280, 720), (65535, 65535), (1, 65535), (65535, 1)] {
            let (w, h, _) = scene_size(size.0, size.1);
            assert!(w <= MAX_WIDTH && h <= MAX_HEIGHT);
        }
    }
    #[test]
    fn foreground_depth_blocks_background_writes() {
        let mut frame = Frame::new(8, 8).unwrap();
        let mut workspace = Workspace::new(8, 8).unwrap();
        assert_eq!(frame.bytes(), 8 * 8 * 4);
        assert_eq!(workspace.bytes(), 8 * 8 * 5 + 8 * 4);
        workspace.with_canvas(&mut frame, |canvas| {
            canvas.color.bytes.fill(0xa5);
            canvas.clear_depth();
            canvas.fill_depth(
                3,
                2,
                5,
                rgba(
                    Fixed::from_int(10),
                    Fixed::from_int(20),
                    Fixed::from_int(30),
                ),
                Fixed::ONE,
            );
            canvas.fill_depth(
                3,
                0,
                8,
                rgba(
                    Fixed::from_int(200),
                    Fixed::from_int(20),
                    Fixed::from_int(30),
                ),
                Fixed::from_int(9),
            );
            assert_eq!(
                canvas.get(3 * 8 + 3),
                rgba(
                    Fixed::from_ratio(10, 1),
                    Fixed::from_ratio(20, 1),
                    Fixed::from_ratio(30, 1)
                )
            );
            assert_eq!(canvas.depth[3 * 8 + 3], Fixed::from_ratio(1, 1));
            canvas.color.bounds = [0, 0, 2, 2];
            assert_eq!(canvas.sample_depth(3, 3), Some(Fixed::MAX));
            canvas.color.bounds = [0, 0, 8, 8];
            assert_eq!(canvas.sample_depth(3, 3), Some(Fixed::ONE));
        });
    }
}
