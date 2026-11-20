#![doc = " Height-field scan conversion into reusable ordinary Texture storage."]
#[cfg(test)]
mod compact;
use super::math::FixedMath as _;
#[cfg(test)]
use super::world::World;
use super::{
    flight::{Flight, Projection, V3, aircraft_axes_with_heading, aircraft_point},
    jet,
    world::Ground,
};
use mirui::render::renderer::RenderError;
#[cfg(test)]
use mirui::types::Color;
use mirui::types::Fixed;
#[cfg(test)]
use mirui_sprite_stack::raster::Workspace;
use mirui_sprite_stack::raster::{
    CameraWorkspace, DepthQuery, DepthSamples, FrameOverlay, RollMapping, SceneCanvas,
};
pub use mirui_sprite_stack::raster::{Canvas, Frame};
mod aircraft;
mod landscape;
mod palette;
mod wake;

use aircraft::PreparedJet;
use landscape::{PreparedTerrain, draw_sky, draw_world, terrain_distance};
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
pub struct Raster {
    compact: bool,
    workspace: CameraWorkspace,
    objects: mirui_sprite_stack::ObjectPass,
    pub palette: Palette,
    pub flames: [Flame; 6],
    pub flame_count: usize,
    seen: [[Fixed; 3]; 6],
}
/// Render pose and baked geometry of one aircraft placed in the scene.
#[derive(Clone, Copy)]
pub(crate) struct PlacedAircraft {
    pub(crate) position: V3,
    heading: (Fixed, Fixed),
    roll: Fixed,
    axes: [V3; 3],
    pub(crate) model: &'static mirui_sprite_stack::format::PreparedSpriteStack<'static>,
}
impl PlacedAircraft {
    pub(crate) fn new(
        position: V3,
        [yaw, pitch, roll]: V3,
        model: &'static mirui_sprite_stack::format::PreparedSpriteStack<'static>,
    ) -> Self {
        let heading = yaw.sin_cos();
        Self {
            position,
            heading,
            roll,
            axes: aircraft_axes_with_heading(heading, pitch, roll),
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
    placed: &'a Placements,
    ground: Ground<'a>,
    flight: &'a Flight,
    palette: &'a Palette,
    frame_height: usize,
    terrain: PreparedTerrain,
    projection: Projection,
    heading: (Fixed, Fixed),
}
impl<'a> Scene<'a> {
    fn new(
        placed: &'a Placements,
        ground: Ground<'a>,
        flight: &'a Flight,
        palette: &'a Palette,
        frame_height: usize,
        projection: Projection,
    ) -> Self {
        let heading = projection.yaw.sin_cos();
        let terrain = PreparedTerrain::new(ground, flight, &projection, palette, placed);
        Self {
            placed,
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
        self.draw_ordered::<false>(canvas, &order, &prepared, None, Fixed::MAX)
            .unwrap();
        order
    }
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
    fn draw_ordered<const DEPTH_ONLY: bool>(
        &self,
        canvas: &mut SceneCanvas<'_, '_>,
        order: &[usize; 3],
        prepared: &[Option<PreparedJet>; 3],
        objects: Option<&mirui_sprite_stack::ProjectedObjects<'_>>,
        limit: Fixed,
    ) -> Result<(), RenderError> {
        self.draw_ground::<DEPTH_ONLY>(canvas, limit);
        if !DEPTH_ONLY {
            draw_sky(
                canvas,
                &self.projection,
                self.heading,
                self.palette,
                self.frame_height,
            );
        }
        if let Some(objects) = objects {
            objects.draw(canvas)?;
        }
        if !DEPTH_ONLY {
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
        for i in &order[..self.flight.count] {
            if let Some(jet) = &prepared[*i] {
                jet.draw(canvas, &self.projection, self.palette);
            }
        }
        Ok(())
    }
}
struct CockpitOverlay<'a> {
    cockpit: &'a super::cockpit::Cockpit,
    prepared: super::cockpit::PreparedCockpit,
    world: Ground<'a>,
    flight: &'a Flight,
}

impl FrameOverlay for CockpitOverlay<'_> {
    fn covers(&self, rect: [i32; 4], scratch: &mut [u8]) -> bool {
        self.cockpit.covers(rect, &self.prepared, scratch)
    }

    fn paint(&self, canvas: &mut Canvas<'_>) {
        self.cockpit
            .draw_prepared(canvas, self.world, self.flight, &self.prepared);
    }
}

fn roll_pivot(height: usize) -> Fixed {
    (Fixed::from_int(height as i32) * Fixed::from_ratio(34, 100)).round()
}

fn map_projection(
    mut projection: Projection,
    width: usize,
    height: usize,
) -> (Projection, RollMapping) {
    let rotation = (projection.cockpit > Fixed::ZERO || projection.roll != Fixed::ZERO)
        .then(|| projection.roll.sin_cos());
    let roll = RollMapping::new([width, height], roll_pivot(height), rotation);
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
impl Raster {
    pub fn new(size: [usize; 2], compact: bool) -> Result<Self, RenderError> {
        Self::with_palette(size, compact, Palette::new(Daylight::Day))
    }

    pub(super) fn with_palette(
        size: [usize; 2],
        compact: bool,
        palette: Palette,
    ) -> Result<Self, RenderError> {
        Ok(Self {
            compact,
            workspace: CameraWorkspace::new(size, roll_pivot(size[1]), compact)?,
            objects: Default::default(),
            palette,
            flames: [Flame::default(); 6],
            flame_count: 0,
            seen: [[Fixed::ONE; 3]; 6],
        })
    }
    pub fn bytes(&self) -> usize {
        self.workspace.bytes() + self.objects.storage_bytes() + core::mem::size_of::<Self>()
    }
    #[cfg(test)]
    pub(crate) fn render(
        &mut self,
        frame: &mut Frame,
        world: &World,
        flight: &Flight,
        placed: &Placements,
    ) {
        self.render_scene(
            frame,
            world.at(Some(Default::default())),
            flight,
            None,
            placed,
            None,
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
        self.render_scene(
            frame,
            world.at(Some(Default::default())),
            flight,
            Some(cockpit),
            placed,
            None,
        )
        .unwrap();
    }

    pub(super) fn render_scene(
        &mut self,
        frame: &mut Frame,
        ground: Ground<'_>,
        flight: &Flight,
        cockpit: Option<&super::cockpit::Cockpit>,
        placed: &Placements,
        extra: Option<(
            &mirui_sprite_stack::StackScene,
            &[Option<mirui_sprite_stack::Object>],
        )>,
    ) -> Result<(), RenderError> {
        let Self {
            compact,
            workspace,
            objects,
            palette,
            flames,
            flame_count,
            seen,
            ..
        } = self;
        let screen = flight.projection(frame.width, frame.height);
        let (projection, mapping) = map_projection(screen, frame.width, frame.height);
        let scene = Scene::new(placed, ground, flight, palette, frame.height, projection);
        let extra = if let Some((shared, managed)) = extra {
            let [x, z, altitude] = projection.position;
            let (sin, cos) = scene.heading;
            let camera = mirui_sprite_stack::ViewProjection::horizontal(
                projection.camera,
                [x, altitude, -z],
                [-sin, cos],
                terrain_distance(altitude),
            );
            Some(objects.prepare(shared, camera, |id| !managed.contains(&Some(id)))?)
        } else {
            objects.clear();
            None
        };
        let order = scene.order();
        let mut models = scene.prepare(mapping.size);
        let compact = *compact;
        if compact {
            for model in models.iter_mut().flatten() {
                model.prepare_regions(&projection);
            }
        }
        let overlay = cockpit.and_then(|cockpit| {
            cockpit
                .prepare(flight, screen, palette)
                .map(|prepared| CockpitOverlay {
                    cockpit,
                    prepared,
                    world: ground,
                    flight,
                })
        });
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
        let depth = |canvas: &mut SceneCanvas<'_, '_>, limit| {
            scene.draw_ordered::<true>(canvas, &order, &models, extra.as_ref(), limit)
        };
        let mut next_seen = *seen;
        let mut pending = &mut pending[..count];
        if compact && count != 0 {
            mirui::trace_span!("flight.flame_depth");
            workspace.finish_depth_queries(&mapping, pending, depth)?;
            finish_flames(&mapping, pending, &mut next_seen, flames);
            pending = &mut [];
        }
        workspace.render(
            frame,
            &mapping,
            overlay.as_ref().map(|o| o as &dyn FrameOverlay),
            |tile| {
                scene.draw_ordered::<false>(tile, &order, &models, extra.as_ref(), Fixed::MAX)?;
                for query in &mut *pending {
                    query.resolve(tile);
                }
                Ok(())
            },
            |canvas| {
                if compact {
                    super::flame::paint_region(canvas, &flames[..count], flight.time)?;
                }
                Ok(())
            },
        )?;

        if !pending.is_empty() {
            // Effect samples behind an opaque cockpit can lie outside the visible scene tiles.
            workspace.finish_depth_queries(&mapping, pending, depth)?;
            finish_flames(&mapping, pending, &mut next_seen, flames);
        }
        *seen = next_seen;
        *flame_count = count;
        Ok(())
    }
}

fn finish_flames(
    mapping: &RollMapping,
    pending: &[PendingFlame],
    seen: &mut [[Fixed; 3]; 6],
    flames: &mut [Flame],
) {
    for (out, query) in flames.iter_mut().zip(pending.iter().copied()) {
        *out = query.finish(seen);
        if mapping.rolled {
            for point in &mut out.points {
                *point = mapping.unroll(*point);
            }
        }
    }
}
#[derive(Clone, Copy, Default)]
struct PendingFlame {
    flame: Flame,
    projected: [[Fixed; 4]; 3],
    slot: usize,
    samples: DepthSamples<15>,
}
impl PendingFlame {
    fn query(&self, index: usize) -> (i32, i32, Fixed) {
        let n = index / 5;
        let point = if n == 1 {
            core::array::from_fn(|k| {
                Fixed::from_ratio(25, 100) * self.projected[0][k]
                    + Fixed::from_ratio(5, 10) * self.projected[1][k]
                    + Fixed::from_ratio(25, 100) * self.projected[2][k]
            })
        } else {
            self.projected[n]
        };
        let radius = self.flame.radius;
        let (dx, dy) = [
            (Fixed::ZERO, Fixed::ZERO),
            (-radius, Fixed::ZERO),
            (radius, Fixed::ZERO),
            (Fixed::ZERO, -radius),
            (Fixed::ZERO, radius),
        ][index % 5];
        (
            (point[0] + dx).round().to_int(),
            (point[1] + dy).round().to_int(),
            point[3],
        )
    }
}
impl DepthQuery for PendingFlame {
    fn resolve(&mut self, canvas: &SceneCanvas<'_, '_>) {
        let mut samples = self.samples;
        samples.resolve(canvas, Fixed::ONE, |i| self.query(i));
        self.samples = samples;
    }
    fn next_point(&self) -> Option<[i32; 2]> {
        self.samples.next_missing().map(|i| {
            let (x, y, _) = self.query(i);
            [x, y]
        })
    }
    fn visit_pending(&self, visit: &mut dyn FnMut([i32; 2], Fixed)) {
        for index in self.samples.missing() {
            let (x, y, depth) = self.query(index);
            visit([x, y], depth - Fixed::ONE);
        }
    }
}
impl PendingFlame {
    fn finish(mut self, seen: &mut [[Fixed; 3]; 6]) -> Flame {
        let visible = self
            .samples
            .visibility()
            .expect("unresolved flame visibility");
        for (n, (value, target)) in seen[self.slot]
            .iter_mut()
            .zip(self.flame.visibility.iter_mut())
            .enumerate()
        {
            let open = ((visible >> (n * 5)) & 31).count_ones() as i32;
            *value +=
                (Fixed::from_int(open) / Fixed::from_int(5) - *value) * Fixed::from_ratio(3, 10);
            *target = *value;
        }
        self.flame
    }
}
fn project_flames(
    flight: &Flight,
    (index, jet): (usize, PlacedAircraft),
    p: &Projection,
    heading: (Fixed, Fixed),
    out: &mut [PendingFlame],
    count: &mut usize,
) {
    let alpha = if index == 0 { p.jet_alpha } else { Fixed::ONE } * (Fixed::ONE - p.cockpit_in());
    let flick = Fixed::from_ratio(82, 100)
        + Fixed::from_ratio(18, 100)
            * (flight.time * Fixed::from_int(47)
                + Fixed::from_int((index) as i32) * Fixed::from_int(2))
            .sin()
            * (flight.time * Fixed::from_int(31)).sin();
    let length = flight.flame_length()
        * (Fixed::from_ratio(94, 100)
            + Fixed::from_ratio(3, 10) * (flick - Fixed::from_ratio(82, 100)))
        * Fixed::from_ratio(13, 10);
    if length <= Fixed::from_ratio(6, 10)
        || alpha < Fixed::from_ratio(1, 100)
        || flight.spread >= Fixed::from_ratio(26, 10)
    {
        return;
    }
    let width = Fixed::from_ratio(163, 100)
        * (Fixed::ONE + Fixed::from_ratio(12, 100) * flight.boost)
        * Fixed::from_ratio(15, 10);
    for side in [-Fixed::ONE, Fixed::ONE] {
        if *count == out.len() {
            break;
        }
        let mut projected = [[Fixed::ZERO; 4]; 3];
        let mut visible = true;
        for (n, target) in projected.iter_mut().enumerate() {
            let curl = if n == 2 { length } else { Fixed::ZERO };
            let local = [
                side * Fixed::from_ratio(196, 100) + flight.flame_bend() * curl,
                jet::PIVOT[0]
                    - Fixed::from_ratio(436, 10)
                    - Fixed::from_int((n) as i32) * Fixed::from_ratio(5, 10) * length,
                (Fixed::from_ratio(41, 10) - Fixed::from_ratio(5, 10) - jet::PIVOT[1])
                    * flight.spread
                    + flight.climb_tilt * Fixed::from_ratio(35, 100) * curl,
            ];
            if let Some(value) = p.project_with_yaw(
                aircraft_point(jet.position, jet.axes, local, flight.settings.jet_scale),
                heading,
            ) {
                *target = value;
            } else {
                visible = false;
                break;
            }
        }
        if !visible {
            continue;
        }
        let radius = width * flight.settings.jet_scale * projected[0][2];
        let slot = index * 2 + usize::from(side > Fixed::ZERO);
        out[*count] = PendingFlame {
            flame: Flame {
                points: projected.map(|v| [v[0], v[1]]),
                radius,
                tip_scale: projected[2][2] / projected[0][2],
                power: flight.boost,
                alpha,
                visibility: [Fixed::ZERO; 3],
                seed: Fixed::from_int((slot) as i32),
                length: length / width,
            },
            projected,
            slot,
            samples: DepthSamples::default(),
        };
        *count += 1;
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
                    scene.draw_ground::<true>(tile, Fixed::MAX);
                } else if only_depth {
                    scene
                        .draw_ordered::<true>(tile, &order, &models, None, Fixed::MAX)
                        .unwrap();
                } else {
                    scene
                        .draw_ordered::<false>(tile, &order, &models, None, Fixed::MAX)
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
                &raster.palette,
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
