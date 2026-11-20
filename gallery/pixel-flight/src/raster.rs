#![doc = " Height-field scan conversion into reusable ordinary Texture storage."]
#[cfg(test)]
mod compact;
use super::math::FixedMath as _;
use super::{
    flight::{Flight, Projection, TRAIL_CAPACITY, V3, aircraft_axes, aircraft_point},
    jet,
    world::{self, Ground, World},
};
use mirui::render::renderer::RenderError;
use mirui::types::{Color, Fixed, Fixed64};
#[cfg(test)]
use mirui_sprite_stack::raster::Workspace;
use mirui_sprite_stack::raster::sprites::{ProjectedStack, SpritePaint, StackTransform};
use mirui_sprite_stack::raster::{
    CameraWorkspace, DepthQuery, DepthSamples, FrameOverlay, RollMapping, SceneCanvas,
};
pub use mirui_sprite_stack::raster::{Canvas, Frame};
pub const MAX_WIDTH: usize = 480;
pub const MAX_HEIGHT: usize = 300;
pub const BAYER: [u8; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];
pub fn rgba(r: Fixed, g: Fixed, b: Fixed) -> u32 {
    u32::from_le_bytes([
        (r.clamp(Fixed::ZERO, Fixed::from_int(255))).trunc_to_int() as u8,
        (g.clamp(Fixed::ZERO, Fixed::from_int(255))).trunc_to_int() as u8,
        (b.clamp(Fixed::ZERO, Fixed::from_int(255))).trunc_to_int() as u8,
        255,
    ])
}
pub fn blend(bg: u32, fg: u32, amount: Fixed) -> u32 {
    let [r, g, b, _] = bg.to_le_bytes();
    let background = Color::rgb(r, g, b);
    let [r, g, b, _] = fg.to_le_bytes();
    let mixed = Color::lerp(background, Color::rgb(r, g, b), amount);
    u32::from_le_bytes([mixed.r, mixed.g, mixed.b, 255])
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Daylight {
    #[default]
    Day,
    Sunset,
    Night,
}
impl Daylight {
    pub fn next(self) -> Self {
        match self {
            Self::Day => Self::Sunset,
            Self::Sunset => Self::Night,
            Self::Night => Self::Day,
        }
    }
}
pub struct Palette {
    pub day: Daylight,
    pub top: [u32; 17],
    pub wall: [u32; 17],
    pub top_shade: [u32; 17],
    pub wall_shade: [u32; 17],
    pub cloud_top: [u32; 8],
    pub cloud_wall: [u32; 8],
    pub cloud_under: u32,
    pub sky: [u32; 8],
    pub haze: u32,
    pub sun: [Fixed; 2],
    pub sun_color: u32,
    pub sun_glow: u32,
    pub jets: [[u32; 96]; 3],
    pub outline: u32,
    pub spark: [u32; 2],
    pub foam: u32,
    pub trails: [u32; 3],
}
fn grade(c: [u8; 3], day: Daylight, amount: Fixed) -> u32 {
    let [r, g, b] = c.map(|value| Fixed::from_int(i32::from(value)));
    let target = match day {
        Daylight::Day => [r, g, b],
        Daylight::Sunset => [
            Fixed::from_ratio(95, 100) * r + Fixed::from_ratio(1, 10) * g + Fixed::from_int(12),
            Fixed::from_ratio(72, 100) * g + Fixed::from_ratio(5, 100) * r + Fixed::from_int(4),
            Fixed::from_ratio(7, 10) * b + Fixed::from_ratio(8, 100) * r + Fixed::from_int(10),
        ],
        Daylight::Night => [
            Fixed::from_ratio(22, 100) * r + Fixed::from_int(2),
            Fixed::from_ratio(3, 10) * g + Fixed::from_int(6),
            Fixed::from_ratio(5, 10) * b + Fixed::from_int(22),
        ],
    };
    rgba(
        r + (target[0] - r) * amount,
        g + (target[1] - g) * amount,
        b + (target[2] - b) * amount,
    )
}
impl Palette {
    pub fn new(day: Daylight) -> Self {
        let top = world::TOP.map(|c| grade(c, day, Fixed::ONE));
        let wall = world::WALL.map(|c| grade(c, day, Fixed::ONE));
        let shade = |c: u32| {
            let c = c.to_le_bytes();
            rgba(
                Fixed::from_int((c[0]) as i32) * Fixed::from_ratio(62, 100),
                Fixed::from_int((c[1]) as i32) * Fixed::from_ratio(67, 100),
                Fixed::from_int((c[2]) as i32) * Fixed::from_ratio(8, 10),
            )
        };
        let cloud_amount = if day == Daylight::Night {
            Fixed::from_ratio(72, 100)
        } else {
            Fixed::ONE
        };
        let cloud_top = core::array::from_fn(|k| {
            let f = (Fixed::from_int((k) as i32) / Fixed::from_int(2)).min(Fixed::ONE);
            grade(
                [
                    (Fixed::from_int(218) + Fixed::from_int(37) * f).trunc_to_int() as u8,
                    (Fixed::from_int(230) + Fixed::from_int(25) * f).trunc_to_int() as u8,
                    (Fixed::from_int(247) + Fixed::from_int(8) * f).trunc_to_int() as u8,
                ],
                day,
                cloud_amount,
            )
        });
        let cloud_wall = core::array::from_fn(|k| {
            let f = Fixed::from_int((k) as i32) / Fixed::from_int(7);
            grade(
                [
                    (Fixed::from_int(158) + Fixed::from_int(44) * f).trunc_to_int() as u8,
                    (Fixed::from_int(182) + Fixed::from_int(36) * f).trunc_to_int() as u8,
                    (Fixed::from_int(222) + Fixed::from_int(18) * f).trunc_to_int() as u8,
                ],
                day,
                cloud_amount,
            )
        });
        let (sky_a, sky_m, sky_b, haze, sun) = match day {
            Daylight::Day => (
                [198, 232, 250],
                [122, 190, 240],
                [62, 142, 222],
                [188, 224, 246],
                [Fixed::from_ratio(5, 10), Fixed::from_ratio(32, 100)],
            ),
            Daylight::Sunset => (
                [255, 198, 132],
                [238, 122, 130],
                [84, 68, 150],
                [250, 182, 134],
                [Fixed::from_ratio(95, 100), Fixed::from_ratio(5, 10)],
            ),
            Daylight::Night => (
                [36, 56, 110],
                [18, 28, 70],
                [7, 10, 34],
                [28, 44, 92],
                [Fixed::from_ratio(3, 10), Fixed::from_ratio(42, 100)],
            ),
        };
        let pack = |c: [u8; 3]| {
            rgba(
                Fixed::from_int((c[0]) as i32),
                Fixed::from_int((c[1]) as i32),
                Fixed::from_int((c[2]) as i32),
            )
        };
        let sky = core::array::from_fn(|i| {
            let t = Fixed::from_int((i) as i32) / Fixed::from_int(7);
            if t < Fixed::from_ratio(5, 10) {
                blend(pack(sky_a), pack(sky_m), t * Fixed::from_int(2))
            } else {
                blend(
                    pack(sky_m),
                    pack(sky_b),
                    t * Fixed::from_int(2) - Fixed::ONE,
                )
            }
        });
        let mut jets = [[0; 96]; 3];
        for row in jet::PALETTE {
            jets[0][usize::from(row[0])] = grade(
                [row[1], row[2], row[3]],
                day,
                if day == Daylight::Sunset {
                    Fixed::from_ratio(5, 10)
                } else if day == Daylight::Night {
                    Fixed::from_ratio(16, 100)
                } else {
                    Fixed::ZERO
                },
            );
        }
        for row in jet::PALETTE {
            let i = usize::from(row[0]);
            jets[1][i] = if row[4] < 3 && jets[0][i + 1] != 0 {
                jets[0][i + 1]
            } else {
                jets[0][i]
            };
            jets[2][i] = if row[4] < 3 && (i - 1) & 15 > 0 {
                jets[0][i - 1]
            } else {
                jets[0][i]
            };
        }
        Self {
            day,
            top,
            wall,
            top_shade: top.map(shade),
            wall_shade: wall.map(shade),
            cloud_top,
            cloud_wall,
            cloud_under: grade([138, 160, 200], day, cloud_amount),
            sky,
            haze: pack(haze),
            sun,
            sun_color: pack(if day == Daylight::Night {
                [232, 238, 255]
            } else {
                [255, 244, 200]
            }),
            sun_glow: pack(if day == Daylight::Night {
                [70, 96, 160]
            } else {
                [255, 222, 150]
            }),
            jets,
            outline: if day == Daylight::Night {
                pack([44, 64, 118])
            } else {
                grade([13, 15, 26], day, Fixed::from_ratio(3, 10))
            },
            spark: match day {
                Daylight::Day => [pack([120, 190, 236]), pack([236, 250, 255])],
                Daylight::Sunset => [pack([255, 170, 110]), pack([255, 226, 170])],
                Daylight::Night => [pack([70, 110, 190]), pack([170, 200, 240])],
            },
            foam: pack(match day {
                Daylight::Day => [240, 252, 255],
                Daylight::Sunset => [255, 226, 214],
                Daylight::Night => [110, 140, 200],
            }),
            trails: [[255, 255, 255], [214, 232, 250], [170, 204, 240]]
                .map(|c| grade(c, day, Fixed::from_ratio(8, 10))),
        }
    }
}
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
    pub(crate) size: [usize; 2],
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
    pub(crate) rotation: V3,
    pub(crate) model: &'static mirui_sprite_stack::format::PreparedSpriteStack<'static>,
}
#[cfg(test)]
impl PlacedAircraft {
    pub(crate) fn from_flight(flight: &Flight) -> [Option<Self>; 3] {
        core::array::from_fn(|i| {
            (i < flight.count).then(|| {
                let jet = &flight.jets[i];
                Self {
                    position: jet.position,
                    rotation: [jet.yaw, jet.pitch, jet.roll + jet.spin()],
                    model: &jet::MODEL,
                }
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
}
impl Scene<'_> {
    fn draw_ground<const DEPTH_ONLY: bool>(
        &self,
        canvas: &mut SceneCanvas<'_, '_>,
        projection: &Projection,
        limit: Fixed,
    ) {
        if self.ground.is_original() {
            draw_world::<false, DEPTH_ONLY>(
                canvas,
                self.ground,
                projection,
                self.palette,
                &self.terrain,
                limit,
            );
        } else {
            draw_world::<true, DEPTH_ONLY>(
                canvas,
                self.ground,
                projection,
                self.palette,
                &self.terrain,
                limit,
            );
        }
    }

    #[cfg(test)]
    fn draw(&self, canvas: &mut SceneCanvas<'_, '_>, projection: Projection) -> [usize; 3] {
        let order = self.order(projection);
        let prepared = self.prepare(projection, [canvas.color.width, canvas.color.height]);
        self.draw_ordered::<false>(canvas, projection, &order, &prepared, None, Fixed::MAX)
            .unwrap();
        order
    }
    fn prepare(&self, projection: Projection, size: [usize; 2]) -> [Option<PreparedJet>; 3] {
        core::array::from_fn(|index| {
            PreparedJet::new(
                self.flight,
                index,
                self.placed[index]?,
                &projection,
                self.palette,
                size,
            )
        })
    }
    fn order(&self, projection: Projection) -> [usize; 3] {
        let mut order = [0, 1, 2];
        let (sn, cs) = projection.yaw.sin_cos();
        order[..self.flight.count].sort_unstable_by(|a, b| {
            let depth = |i: usize| {
                self.placed[i].map_or(Fixed::MAX, |jet| {
                    (jet.position[0] - projection.position[0]) * sn
                        + (jet.position[1] - projection.position[1]) * cs
                })
            };
            depth(*a).cmp(&depth(*b))
        });
        order
    }
    fn draw_ordered<const DEPTH_ONLY: bool>(
        &self,
        canvas: &mut SceneCanvas<'_, '_>,
        projection: Projection,
        order: &[usize; 3],
        prepared: &[Option<PreparedJet>; 3],
        objects: Option<&mirui_sprite_stack::ProjectedObjects<'_>>,
        limit: Fixed,
    ) -> Result<(), RenderError> {
        self.draw_ground::<DEPTH_ONLY>(canvas, &projection, limit);
        if !DEPTH_ONLY {
            draw_sky(canvas, &projection, self.palette, self.frame_height);
        }
        if let Some(objects) = objects {
            objects.draw(canvas)?;
        }
        if !DEPTH_ONLY {
            draw_trails(canvas, self.flight, &projection, self.palette);
            draw_smoke(canvas, self.flight, &projection, self.palette.day);
        }
        for i in &order[..self.flight.count] {
            if let Some(jet) = &prepared[*i] {
                jet.draw(canvas, &projection, self.palette);
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

struct Mapping {
    projection: Projection,
    roll: RollMapping,
}

impl core::ops::Deref for Mapping {
    type Target = RollMapping;
    fn deref(&self) -> &RollMapping {
        &self.roll
    }
}

impl Mapping {
    fn new(mut projection: Projection, width: usize, height: usize) -> Self {
        let rotation = (projection.cockpit > Fixed::ZERO || projection.roll != Fixed::ZERO)
            .then(|| projection.roll.sin_cos());
        let roll = RollMapping::new([width, height], roll_pivot(height), rotation);
        if roll.rolled {
            let axis = Fixed::from_int(width as i32) / 2;
            projection.center = roll.projection_center([axis, projection.center[1]]);
        }
        Self { projection, roll }
    }
}

#[doc = " Integral scaling preserves hard scene pixels and caps CPU work independently of window size."]
pub fn scene_size(width: u16, height: u16) -> (usize, usize, usize) {
    super::Profile::Desktop.scene_size(width, height)
}
impl Raster {
    pub fn new(size: [usize; 2], compact: bool) -> Result<Self, RenderError> {
        Ok(Self {
            size,
            compact,
            workspace: CameraWorkspace::new(size, roll_pivot(size[1]), compact)?,
            objects: Default::default(),
            palette: Palette::new(Daylight::Day),
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
        let mapping = Mapping::new(
            flight.projection(frame.width, frame.height),
            frame.width,
            frame.height,
        );
        let projection = mapping.projection;
        let extra = if let Some((shared, managed)) = extra {
            let [x, z, altitude] = projection.position;
            let (sin, cos) = projection.yaw.sin_cos();
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
        let scene = Scene {
            placed,
            ground,
            flight,
            palette,
            frame_height: frame.height,
            terrain: PreparedTerrain::new(ground, flight, &projection, palette, placed),
        };
        let order = scene.order(projection);
        let mut models = scene.prepare(projection, mapping.size);
        let compact = *compact;
        if compact {
            for model in models.iter_mut().flatten() {
                model.prepare_regions(&projection);
            }
        }
        let overlay = cockpit.and_then(|cockpit| {
            cockpit
                .prepare(flight, palette)
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
                project_flames(flight, (index, jet), &projection, &mut pending, &mut count);
            }
        }
        let depth = |canvas: &mut SceneCanvas<'_, '_>, limit| {
            scene.draw_ordered::<true>(canvas, projection, &order, &models, extra.as_ref(), limit)
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
                scene.draw_ordered::<false>(
                    tile,
                    projection,
                    &order,
                    &models,
                    extra.as_ref(),
                    Fixed::MAX,
                )?;
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
struct AircraftShadow {
    center: [Fixed; 2],
    sin: Fixed,
    cos: Fixed,
    span: Fixed,
}
struct PreparedTerrain {
    base: [Fixed; 2],
    drift: [Fixed; 2],
    cloud_sun: [Fixed; 2],
    shadows: [AircraftShadow; 3],
    shadow_count: u8,
    jet_scale: Fixed,
    shadow_bounds: [i32; 4],
    reach: Fixed,
    water: [u16; 3],
    rank: u8,
}
impl PreparedTerrain {
    fn new(
        ground: Ground<'_>,
        flight: &Flight,
        p: &Projection,
        pal: &Palette,
        placed: &Placements,
    ) -> Self {
        let world = ground.world;
        let period = Fixed::from_int(world.period() as i32);
        let base = [
            p.position[0].rem_euclid(period) + period * Fixed::from_int(2),
            p.position[1].rem_euclid(period) + period * Fixed::from_int(2),
        ];
        let reach = Fixed::from_int(jet::LENGTH as i32)
            * Fixed::from_ratio(75, 100)
            * flight.settings.jet_scale
            + Fixed::from_int(3);
        let displacement = pal.sun.map(|sun| sun * world::MAX_HEIGHT as i32);
        let mut shadow_bounds = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
        let mut shadows = [AircraftShadow::default(); 3];
        let mut shadow_count = 0;
        for (index, placed) in placed.iter().enumerate() {
            let Some(jet) = placed else {
                continue;
            };
            let center = core::array::from_fn(|axis| {
                base[axis] + jet.position[axis] - p.position[axis] + pal.sun[axis] * jet.position[2]
            });
            let (sin, cos) = jet.rotation[0].sin_cos();
            let bank = jet.rotation[2] - flight.jets[index].spin();
            let span = bank.cos().abs().max(Fixed::from_ratio(3, 10)) * flight.settings.jet_scale;
            shadows[shadow_count] = AircraftShadow {
                center,
                sin,
                cos,
                span,
            };
            shadow_count += 1;
            for axis in 0..2 {
                let far = center[axis] - displacement[axis];
                shadow_bounds[axis] =
                    shadow_bounds[axis].min((center[axis].min(far) - reach).to_int());
                shadow_bounds[axis + 2] =
                    shadow_bounds[axis + 2].max((center[axis].max(far) + reach).ceil().to_int());
            }
        }
        let elevation = ground.elevation();
        if shadow_count != 0 && !elevation.is_zero() {
            for (axis, sun) in pal.sun.iter().enumerate() {
                let offset = *sun * elevation;
                shadow_bounds[axis] -= offset.ceil().to_int();
                shadow_bounds[axis + 2] -= offset.floor().to_int();
            }
        }
        Self {
            base,
            drift: [
                (flight.time * Fixed::from_int(7))
                    .rem_euclid(Fixed::from_int(world.cloud_period() as i32)),
                (flight.time * Fixed::from_int(3))
                    .rem_euclid(Fixed::from_int(world.cloud_period() as i32)),
            ],
            cloud_sun: pal.sun.map(|value| value * world::CLOUD_ALT),
            shadows,
            shadow_count: shadow_count as u8,
            jet_scale: flight.settings.jet_scale,
            shadow_bounds,
            reach,
            water: [36, 6, 4]
                .map(|rate| ((flight.time * Fixed::from_int(rate)).trunc_to_int() & 1023) as u16),
            rank: (flight.settings.clouds.clamp(Fixed::ZERO, Fixed::ONE) * Fixed::from_int(16))
                .round()
                .trunc_to_int() as u8,
        }
    }
}
use mirui_sprite_stack::raster::terrain::{TerrainColumn, TerrainRow, TerrainSurface, TerrainView};

struct Landscape<'a, const PLACED: bool, const DEPTH_ONLY: bool> {
    ground: Ground<'a>,
    to_world: [Fixed; 2],
    world: &'a World,
    palette: &'a Palette,
    prepared: &'a PreparedTerrain,
    levels: &'a [u8],
    materials: &'a [u8],
    fog_start: Fixed,
    far: Fixed,
    fog: Fixed,
    pixel_width: Fixed,
    cast_shadows: bool,
}

impl<const PLACED: bool, const DEPTH_ONLY: bool> TerrainSurface
    for Landscape<'_, PLACED, DEPTH_ONLY>
{
    type Sample = usize;

    fn shade(&self, color: u32) -> u32 {
        if DEPTH_ONLY || self.fog.is_zero() {
            color | 0xff00_0000
        } else {
            blend(color, self.palette.haze, self.fog)
        }
    }
    fn begin_row(&mut self, row: TerrainRow) {
        if DEPTH_ONLY {
            return;
        }
        let [xmin, ymin, xmax, ymax] = self.prepared.shadow_bounds;
        let [left, top, right, bottom] = row.bounds.map(Fixed::trunc_to_int);
        self.cast_shadows = left <= xmax && right >= xmin && top <= ymax && bottom >= ymin;
        self.pixel_width = row.pixel_width;
        self.fog = if row.depth > self.fog_start {
            (((row.depth - self.fog_start) / (self.far - self.fog_start)).sqrt()
                * Fixed::from_int(10))
            .round()
            .min(Fixed::from_int(10))
                / 10
        } else {
            Fixed::ZERO
        };
    }

    fn overlay(&self, position: [Fixed; 2], column: &mut TerrainColumn<'_, '_, '_>) {
        let prepared = self.prepared;
        if prepared.rank == 0 {
            return;
        }
        let [gx, gy] = position.map(Fixed::trunc_to_int);
        let cloud = self.world.cloud(
            (Fixed::from_int(gx) - prepared.drift[0]).trunc_to_int(),
            (Fixed::from_int(gy) - prepared.drift[1]).trunc_to_int(),
        );
        let thick = usize::from(cloud & 15);
        if thick > 0 && cloud >> 4 < prepared.rank {
            let colors = self.palette;
            column.slab(
                world::CLOUD_ALT,
                Fixed::ONE,
                self.shade(colors.cloud_top[thick - 1]),
                self.shade(colors.cloud_under),
                colors.cloud_wall[..thick - 1]
                    .iter()
                    .map(|&color| self.shade(color)),
            );
        }
    }

    fn height_at(&self, position: [Fixed; 2]) -> Option<(Fixed, usize)> {
        if PLACED {
            return self.ground.placed_sample([
                position[0] + self.to_world[0],
                position[1] + self.to_world[1],
            ]);
        }
        let [gx, gy] = position.map(Fixed::trunc_to_int);
        let index = self.world.ground_index(gx, gy);
        Some((Fixed::from_int(i32::from(self.levels[index])), index))
    }

    #[inline(always)]
    fn colors(&self, position: [Fixed; 2], height: Fixed, index: usize) -> [u32; 2] {
        if DEPTH_ONLY {
            return [0; 2];
        }
        let [gx, gy] = position.map(Fixed::trunc_to_int);
        let world = self.world;
        let pal = self.palette;
        let prepared = self.prepared;
        let material = usize::from(self.materials[index]);
        let mut shaded = false;
        if prepared.rank > 0 {
            let c = world.cloud(
                (Fixed::from_int(gx) + prepared.cloud_sun[0] - prepared.drift[0]).trunc_to_int(),
                (Fixed::from_int(gy) + prepared.cloud_sun[1] - prepared.drift[1]).trunc_to_int(),
            );
            shaded = c & 15 > 1 && c >> 4 < prepared.rank;
        }
        if !shaded && self.cast_shadows {
            for shadow in &prepared.shadows[..usize::from(prepared.shadow_count)] {
                let dx = Fixed::from_int(gx) - shadow.center[0] + pal.sun[0] * height;
                let dy = Fixed::from_int(gy) - shadow.center[1] + pal.sun[1] * height;
                if dx.abs() < prepared.reach && dy.abs() < prepared.reach {
                    let a = (dx * shadow.cos + dy * shadow.sin) / shadow.span;
                    let b = (dx * shadow.sin - dy * shadow.cos) / prepared.jet_scale;
                    let i = (a + (jet::PIVOT_X + Fixed::HALF)).floor().trunc_to_int();
                    let k = ((jet::PIVOT[0] - b + Fixed::from_ratio(5, 10)).floor()).trunc_to_int();
                    if (0..jet::WIDTH as i32).contains(&i)
                        && (0..jet::LENGTH as i32).contains(&k)
                        && jet::PROJECTIONS.shadow[k as usize * jet::WIDTH + i as usize] != 0
                    {
                        shaded = true;
                        break;
                    }
                }
            }
        }
        let mut top = if shaded {
            pal.top_shade[material]
        } else {
            pal.top[material]
        };
        let mut wall = if shaded {
            pal.wall_shade[material]
        } else {
            pal.wall[material]
        };
        if material < 4 {
            let noise = |x: i32, y: i32| world.noise_at(x, y);
            if material == 3 && noise(gx, gy).wrapping_add(prepared.water[0] as u8) < 120 {
                top = if shaded { pal.top_shade[2] } else { pal.foam };
            } else if !shaded && self.pixel_width < Fixed::from_ratio(18, 10) {
                let n = noise((gx + i32::from(prepared.water[1])) >> 2, gy);
                if n > 243 && noise((gx + 1024 - i32::from(prepared.water[2])) >> 2, gy + 91) > 128
                {
                    top = pal.spark[usize::from(n > 252)];
                }
            }
            wall = top;
        }
        [top, wall]
    }
}

fn terrain_distance(altitude: Fixed) -> Fixed {
    Fixed::from_int(1500) + altitude * 3
}

fn draw_world<const PLACED: bool, const DEPTH_ONLY: bool>(
    canvas: &mut SceneCanvas<'_, '_>,
    ground: Ground<'_>,
    p: &Projection,
    pal: &Palette,
    prepared: &PreparedTerrain,
    limit: Fixed,
) {
    mirui::trace_span!("flight.terrain");
    canvas.clear_depth();
    let altitude = p.position[2];
    let far = terrain_distance(altitude);
    let world = ground.world;
    let (levels, materials) = world.ground_planes();
    let mut surface = Landscape::<PLACED, DEPTH_ONLY> {
        ground,
        to_world: [
            p.position[0] - prepared.base[0],
            p.position[1] - prepared.base[1],
        ],
        world,
        palette: pal,
        prepared,
        levels,
        materials,
        far,
        fog_start: if p.perspective > Fixed64::ZERO {
            Fixed::from_int(300) + altitude * 2
        } else {
            Fixed::MAX
        },
        fog: Fixed::ZERO,
        pixel_width: Fixed::ZERO,
        cast_shadows: false,
    };
    TerrainView {
        projection: p.camera,
        origin: prepared.base,
        altitude,
        yaw: p.yaw.sin_cos(),
        heights: if PLACED {
            let elevation = ground.elevation();
            [
                elevation.min(world::CLOUD_ALT),
                (elevation + Fixed::from_int(world::MAX_HEIGHT as i32))
                    .max(world::CLOUD_ALT + Fixed::from_int(world::CLOUD_LAYERS as i32)),
            ]
        } else {
            [Fixed::ZERO, Fixed::from_int(world::MAX_HEIGHT as i32)]
        },
        depth: [-Fixed::from_int(1_000_000), far.min(limit)],
    }
    .draw(canvas, &mut surface);
}
fn draw_sky(canvas: &mut SceneCanvas<'_, '_>, p: &Projection, pal: &Palette, frame_height: usize) {
    mirui::trace_span!("flight.sky");
    let [left, top, right, bottom] = canvas.color.clip;
    if left >= right || top >= bottom {
        return;
    }
    // Ground spans cover every pixel from the column boundary to the clip bottom.
    // Clouds above that boundary can leave holes, so keep testing those pixels.
    let local_x = |x: i32| (x - canvas.color.origin[0]) as usize;
    let sky_bottom = *canvas.columns[local_x(left)..local_x(right)]
        .iter()
        .max()
        .expect("nonempty clipped columns");
    if sky_bottom <= top {
        return;
    }
    let horizon = if p.perspective > Fixed64::ZERO {
        p.center[1] - p.divide_perspective(p.scale * p.slope)
    } else {
        -Fixed::from_int(1_000_000)
    };
    let mut star_offset = 0;
    let mut disk = None;
    if Fixed::from_int(top) < horizon && pal.day != Daylight::Day {
        let focal = p.divide_perspective(p.scale);
        if pal.day == Daylight::Night {
            star_offset = (p.yaw * focal).trunc_to_int();
        }
        let length = (pal.sun[0] * pal.sun[0] + pal.sun[1] * pal.sun[1]).sqrt();
        let dx = -pal.sun[0] / length;
        let dy = -pal.sun[1] / length;
        let (sn, cs) = p.yaw.sin_cos();
        let along = -(dx * sn + dy * cs);
        if along > Fixed::from_ratio(2, 10) {
            let (size, elevation) = if pal.day == Daylight::Sunset {
                (Fixed::from_ratio(5, 100), Fixed::from_ratio(12, 100))
            } else {
                (Fixed::from_ratio(22, 1000), Fixed::from_ratio(3, 10))
            };
            disk = Some((
                p.center[0] + focal * (dx * cs - dy * sn) / along,
                horizon - focal * elevation / along,
                Fixed::from_int(canvas.color.width as i32) * size,
            ));
        }
    }
    for y in top..sky_bottom {
        let gradient = (Fixed::from_int(y) < horizon).then(|| {
            let t = (horizon - Fixed::from_int(y))
                / (Fixed::from_int(frame_height as i32) * Fixed::from_ratio(75, 100))
                * Fixed::from_int(7);
            let band = t.trunc_to_int();
            let threshold = (t - Fixed::from_int(band) - Fixed::from_ratio(7, 10))
                * Fixed::from_ratio(34, 10)
                * Fixed::from_int(16);
            (band as usize, threshold)
        });
        for x in left..right {
            let at = canvas.color.index(x, y);
            if canvas.depth[at] != Fixed::MAX {
                continue;
            }
            let mut color = pal.haze;
            if let Some((base, threshold)) = gradient {
                let dither = Fixed::from_int(BAYER[((y & 3) * 4 + (x & 3)) as usize] as i32);
                let band = base + usize::from(threshold > dither);
                color = pal.sky[band.min(7)];
                if pal.day == Daylight::Night
                    && band > 1
                    && world::hash8((((x + star_offset) & 4095) * 73 + y * 9973) as u32) < 2
                {
                    color = pal.sun_color;
                }
                if let Some((sx, sy, radius)) = disk {
                    let distance = ((Fixed::from_int(x) - sx).powi(2)
                        + (Fixed::from_int(y) - sy).powi(2))
                    .sqrt()
                        / radius;
                    if distance < Fixed::ONE {
                        color = pal.sun_color;
                    } else if distance < Fixed::from_ratio(19, 10)
                        && (Fixed::from_ratio(19, 10) - distance) * Fixed::from_int(14) > dither
                    {
                        color = pal.sun_glow;
                    }
                }
            } else if p.perspective.is_zero() {
                color = pal.top[0];
            }
            canvas.put(at, color);
        }
    }
}
fn draw_trails(canvas: &mut SceneCanvas<'_, '_>, flight: &Flight, p: &Projection, pal: &Palette) {
    mirui::trace_span!("flight.trails");
    let first = usize::from(p.jet_alpha < Fixed::HALF).min(flight.count);
    let jets = &flight.jets[first..flight.count];
    if jets.is_empty() {
        return;
    }
    let yaw = p.yaw.sin_cos();
    let [left, top, right, bottom] = canvas.color.clip;
    for jet in jets {
        let lifetime = Fixed::from_ratio(26, 10)
            * (Fixed::from_ratio(3, 10) + Fixed::from_ratio(7, 10) * jet.load);
        for wing in &jet.trails {
            let mut last: Option<[Fixed; 4]> = None;
            for n in 0..jet.trail_len {
                let point = wing[(jet.trail_head + TRAIL_CAPACITY - 1 - n) % TRAIL_CAPACITY];
                let age = (flight.time - point.born) / lifetime;
                if age >= Fixed::ONE {
                    break;
                }
                let Some(next) = p.project_with_yaw(point.position, yaw) else {
                    last = None;
                    continue;
                };
                let Some(prev) = last.replace(next) else {
                    continue;
                };
                let dx = next[0] - prev[0];
                let dy = next[1] - prev[1];
                let span = dx.abs().max(dy.abs());
                if span >= Fixed::from_int(600) {
                    continue;
                }
                let width = if next[2] > Fixed::from_ratio(24, 10) {
                    2
                } else {
                    1
                };
                // Every rounded sample is between the two rounded endpoints.
                if prev[0].max(next[0]).round().to_int() + width <= left
                    || prev[0].min(next[0]).round().to_int() >= right
                    || prev[1].max(next[1]).round().to_int() + width <= top
                    || prev[1].min(next[1]).round().to_int() >= bottom
                {
                    continue;
                }
                let steps = span.ceil().to_int().max(1);
                let alpha = if age < Fixed::from_ratio(5, 10) {
                    Fixed::ONE
                } else {
                    (Fixed::ONE - age) * Fixed::from_int(2)
                };
                let color = pal.trails[if age < Fixed::from_ratio(25, 100) {
                    0
                } else if age < Fixed::from_ratio(6, 10) {
                    1
                } else {
                    2
                }];
                for k in 0..steps {
                    let x = (prev[0] + dx * k / steps).round().to_int();
                    let y = (prev[1] + dy * k / steps).round().to_int();
                    for yy in y..y + width {
                        for xx in x..x + width {
                            if !canvas.color.contains(xx, yy) {
                                continue;
                            }
                            let at = canvas.color.index(xx, yy);
                            if canvas.depth[at] >= next[3] - Fixed::ONE
                                && alpha * Fixed::from_int(16)
                                    > Fixed::from_int(
                                        (BAYER[((yy & 3) * 4 + (xx & 3)) as usize]) as i32,
                                    )
                            {
                                canvas.put(at, color);
                            }
                        }
                    }
                }
            }
        }
    }
}
fn draw_smoke(canvas: &mut SceneCanvas<'_, '_>, flight: &Flight, p: &Projection, day: Daylight) {
    mirui::trace_span!("flight.smoke");
    for smoke in &flight.smoke {
        let age = flight.time - smoke.born;
        if smoke.life <= Fixed::ZERO || age < Fixed::ZERO || age >= smoke.life {
            continue;
        }
        let t = age / smoke.life;
        let push = (Fixed::ONE - (-age * Fixed::from_int(5)).exp()) / Fixed::from_int(5);
        let Some(point) = p.project([
            smoke.position[0] + smoke.push[0] * push,
            smoke.position[1] + smoke.push[1] * push,
            smoke.position[2] + age * Fixed::from_ratio(25, 10),
        ]) else {
            continue;
        };
        let radius = (smoke.size * (Fixed::ONE + Fixed::from_ratio(24, 10) * t) * point[2])
            .max(Fixed::from_ratio(8, 10));
        if radius > Fixed::from_int(60) {
            continue;
        }
        let alpha = smoke.strength
            * (Fixed::ONE - t).powi(2)
            * if t < Fixed::from_ratio(8, 100) {
                t / Fixed::from_ratio(8, 100)
            } else {
                Fixed::ONE
            }
            * if day == Daylight::Night {
                Fixed::from_ratio(55, 100)
            } else {
                Fixed::ONE
            };
        let shade = Fixed::from_int(26) + Fixed::from_int(70) * t;
        let color = rgba(shade, shade + Fixed::ONE, shade + Fixed::from_int(7));
        for y in ((point[1] - radius).floor())
            .trunc_to_int()
            .max(canvas.color.clip[1])
            ..=((point[1] + radius).ceil())
                .trunc_to_int()
                .min(canvas.color.clip[3] - 1)
        {
            for x in ((point[0] - radius).floor())
                .trunc_to_int()
                .max(canvas.color.clip[0])
                ..=((point[0] + radius).ceil())
                    .trunc_to_int()
                    .min(canvas.color.clip[2] - 1)
            {
                let d = ((Fixed::from_int(x) - point[0]).powi(2)
                    + (Fixed::from_int(y) - point[1]).powi(2))
                    / (radius * radius);
                let at = canvas.color.index(x, y);
                if d <= Fixed::ONE && canvas.depth[at] >= point[3] - Fixed::ONE {
                    canvas.put(
                        at,
                        blend(
                            canvas.get(at),
                            color,
                            alpha * (Fixed::ONE - d * Fixed::from_ratio(7, 10)),
                        ),
                    );
                }
            }
        }
    }
}

struct PreparedJet {
    sprite: ProjectedStack<'static, { jet::LAYERS }>,
    fade: Fixed,
    lights: [usize; 2],
    right_side_x: i32,
}
impl PreparedJet {
    fn new(
        flight: &Flight,
        index: usize,
        jet: PlacedAircraft,
        p: &Projection,
        pal: &Palette,
        size: [usize; 2],
    ) -> Option<Self> {
        mirui::trace_span!("flight.aircraft.prepare");
        let fade = if index == 0 { p.jet_alpha } else { Fixed::ONE };
        if fade < Fixed::from_ratio(4, 1000) {
            return None;
        }
        let center = p.project(jet.position)?;
        if center[2] <= Fixed::ZERO {
            return None;
        }
        let reach =
            Fixed::from_int((jet.model.extent()[1]) as i32) * flight.settings.jet_scale * center[2]
                + Fixed::from_int(40);
        if center[0] < -reach
            || center[1] < -reach - Fixed::from_int((size[1]) as i32)
            || center[0] > Fixed::from_int((size[0]) as i32) + reach
            || center[1] > Fixed::from_int((size[1]) as i32) + reach
        {
            return None;
        }
        let axes = aircraft_axes(jet.rotation);
        let (sn, cs) = p.yaw.sin_cos();
        let sc = flight.settings.jet_scale;
        let project_axis = |a: [Fixed; 3]| [a[0] * cs - a[1] * sn, -(a[0] * sn + a[1] * cs), a[2]];
        let offset = core::array::from_fn(|i| jet.position[i] - p.position[i]);
        let light = |side: Fixed| {
            let n = core::array::from_fn::<_, 3, _>(|i| {
                axes[2][i] + side * Fixed::from_ratio(4, 10) * axes[0][i]
            });
            let dot = -n[0] * pal.sun[0] - n[1] * pal.sun[1] + n[2] * Fixed::from_ratio(11, 10);
            let norm =
                ((pal.sun[0] * pal.sun[0] + pal.sun[1] * pal.sun[1] + Fixed::from_ratio(121, 100))
                    * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]))
                    .sqrt();
            let v = dot / norm;
            if v > if pal.day == Daylight::Night {
                Fixed::from_ratio(74, 100)
            } else {
                Fixed::from_ratio(9, 10)
            } {
                1
            } else if v < if pal.day == Daylight::Night {
                Fixed::from_ratio(4, 10)
            } else {
                Fixed::from_ratio(66, 100)
            } {
                2
            } else {
                0
            }
        };
        let lights = {
            mirui::trace_span!("flight.aircraft.light");
            [light(-Fixed::ONE), light(Fixed::ONE)]
        };
        let sprite = ProjectedStack::new(
            jet.model,
            StackTransform {
                scale: sc,
                layer_spacing: flight.spread * Fixed::from_ratio(jet.model.spacing_q8(), 256),
                depth_radius: Fixed::from(jet.model.extent()[1]) * Fixed::from_ratio(6, 10) * sc,
                ..StackTransform::from_basis(offset, axes, project_axis)
            },
            &p.camera,
            Fixed::from_int(3),
            |layer, first_slice| {
                let intro =
                    flight.intro - Fixed::from_int((index) as i32) * Fixed::from_ratio(25, 100);
                let t = (intro
                    - Fixed::from_int((layer.saturating_sub(first_slice)) as i32)
                        * Fixed::from_ratio(7, 100))
                    / Fixed::from_ratio(36, 100);
                if t <= Fixed::ZERO {
                    return None;
                }
                let drop = if t < Fixed::ONE {
                    -(Fixed::ONE - t * t)
                        * (Fixed::from_int((size[1]) as i32) / Fixed::from_int(2)
                            + Fixed::from_int(44))
                } else {
                    Fixed::ZERO
                };
                Some(drop)
            },
        )?;
        let (step, pivot) = (
            sprite.level().step(),
            Fixed::from_ratio(jet.model.pivot_q8()[0], 256),
        );
        let right_side_x = (pivot - (Fixed::from(step) - Fixed::ONE) / 2)
            .ceil()
            .to_int();
        Some(Self {
            sprite,
            fade,
            lights,
            right_side_x,
        })
    }

    fn prepare_regions(&mut self, p: &Projection) {
        self.sprite.prepare_regions(p);
    }
    fn draw(&self, canvas: &mut SceneCanvas<'_, '_>, p: &Projection, pal: &Palette) {
        mirui::trace_span!("flight.aircraft");
        self.sprite.view().draw_masked(
            canvas,
            p,
            SpritePaint {
                depth_bias: Fixed::ONE,
                opacity: self.fade,
                outline: (self.fade >= Fixed::HALF).then_some(pal.outline),
            },
            |voxel| voxel.color | (u8::from(i32::from(voxel.x) >= self.right_side_x) << 7),
            |material| {
                pal.jets[self.lights[usize::from(material >> 7)]][usize::from(material & 0x7f)]
            },
        );
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
    let axes = aircraft_axes(jet.rotation);
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
            if let Some(value) = p.project(aircraft_point(
                jet.position,
                axes,
                local,
                flight.settings.jet_scale,
            )) {
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
        let projection = flight.projection(128, 128);
        let palette = Palette::new(Daylight::Day);
        let placed = PlacedAircraft::from_flight(&flight);
        let ground = world.at(Some(Default::default()));
        let scene = Scene {
            placed: &placed,
            ground,
            flight: &flight,
            palette: &palette,
            frame_height: 128,
            terrain: PreparedTerrain::new(ground, &flight, &projection, &palette, &placed),
        };
        let models = scene.prepare(projection, [128; 2]);
        let order = scene.order(projection);
        let mut frame = Frame::new(128, 128).unwrap();
        let mut work = Workspace::new(128, 128).unwrap();
        let mut depths = |only_depth: bool, aircraft: bool| {
            work.with_canvas(&mut frame, |tile| {
                if !aircraft {
                    scene.draw_ground::<true>(tile, &projection, Fixed::MAX);
                } else if only_depth {
                    scene
                        .draw_ordered::<true>(tile, projection, &order, &models, None, Fixed::MAX)
                        .unwrap();
                } else {
                    scene
                        .draw_ordered::<false>(tile, projection, &order, &models, None, Fixed::MAX)
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
        let scene = Scene {
            placed: &placed,
            ground: world.at(Some(Default::default())),
            flight: &flight,
            palette: &palette,
            frame_height: 128,
            terrain: PreparedTerrain::new(
                world.at(Some(Default::default())),
                &flight,
                &projection,
                &palette,
                &placed,
            ),
        };
        let prepared = scene.prepare(projection, [128; 2]);
        let jet = prepared[0].as_ref().unwrap();
        assert_eq!(
            jet.sprite.center(),
            projection.project(active.position).unwrap()
        );
        assert!(jet.sprite.level().layer(0).unwrap().is_empty());
        assert!(prepared[1].is_none());
    }
    #[test]
    fn terrain_frame_preparation_keeps_every_active_shadow_caster() {
        let world = World::with_size(7, Fixed::from_ratio(33, 100), 64, 32, 16);
        let mut flight = Flight::new(&world, Settings::default());
        flight.set_wingmen(2);
        flight.jets[2].position = [
            Fixed::from_int(2000),
            Fixed::from_int(-2000),
            Fixed::from_int(77),
        ];
        let palette = Palette::new(Daylight::Sunset);
        let projection = flight.projection(128, 128);
        for elevation in [0, -82, 137] {
            let prepared = PreparedTerrain::new(
                world.at(Some(mirui_sprite_stack::Pose::at(0, elevation, 0))),
                &flight,
                &projection,
                &palette,
                &PlacedAircraft::from_flight(&flight),
            );
            for (index, jet) in flight.jets[..flight.count].iter().enumerate() {
                for axis in 0..2 {
                    let base = projection.position[axis]
                        .rem_euclid(Fixed::from_int(world.period() as i32))
                        + Fixed::from_int(world.period() as i32) * Fixed::from_int(2);
                    assert_eq!(
                        prepared.shadows[index].center[axis],
                        base + jet.position[axis] - projection.position[axis]
                            + palette.sun[axis] * jet.position[2]
                    );
                    for h in elevation..=elevation + world::MAX_HEIGHT as i32 {
                        let center = prepared.shadows[index].center[axis] - palette.sun[axis] * h;
                        assert!(
                            Fixed::from_int(prepared.shadow_bounds[axis])
                                <= center - prepared.reach
                        );
                        assert!(
                            Fixed::from_int(prepared.shadow_bounds[axis + 2])
                                >= center + prepared.reach
                        );
                    }
                }
            }
        }
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                let _ = PreparedTerrain::new(
                    world.at(Some(Default::default())),
                    &flight,
                    &projection,
                    &palette,
                    &PlacedAircraft::from_flight(&flight),
                );
            }),
            0
        );
        assert!(core::mem::size_of::<PreparedTerrain>() <= 120);
        let mut placed = PlacedAircraft::from_flight(&flight);
        placed[1] = None;
        let jet = placed[2].as_mut().unwrap();
        jet.rotation[0] = Fixed::from_ratio(1, 3);
        jet.rotation[2] = Fixed::from_ratio(1, 5);
        let (sin, cos) = jet.rotation[0].sin_cos();
        let bank = jet.rotation[2] - flight.jets[2].spin();
        let span = bank.cos().abs().max(Fixed::from_ratio(3, 10)) * flight.settings.jet_scale;
        let prepared = PreparedTerrain::new(
            world.at(Some(Default::default())),
            &flight,
            &projection,
            &palette,
            &placed,
        );
        assert_eq!(prepared.shadow_count, 2);
        let shadow = prepared.shadows[1];
        assert_eq!((shadow.sin, shadow.cos, shadow.span), (sin, cos, span));
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
            let scene = Scene {
                placed: &placed,
                ground: world.at(Some(Default::default())),
                flight: &flight,
                palette: &raster.palette,
                frame_height: 128,
                terrain: PreparedTerrain::new(
                    world.at(Some(Default::default())),
                    &flight,
                    &flight.projection(128, 128),
                    &raster.palette,
                    &placed,
                ),
            };
            let projection = flight.projection(128, 128);
            let reference_depth =
                full_workspace.with_region(&mut full, [128, 128], [0, 0], bounds, |canvas| {
                    scene.draw(canvas, projection);
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
                        scene.draw(canvas, projection);
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
