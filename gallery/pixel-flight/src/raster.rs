#![doc = " Height-field scan conversion into reusable ordinary Texture storage."]
mod compact;
use super::math::FixedMath as _;
use super::{
    flight::{Flight, Projection, TRAIL_CAPACITY, V3, aircraft_axes, aircraft_point},
    jet,
    world::{self, World},
};
use mirui::render::{renderer::RenderError, texture::ColorFormat};
use mirui::types::{Color, Fixed, Fixed64, GridBasis};
pub use mirui_sprite_stack::raster::{Canvas, Frame};
use mirui_sprite_stack::raster::{DepthSamples, RollMapping, SceneCanvas, Workspace};
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
    pub frame: Frame,
    pub sheet: Frame,
    workspace: Workspace,
    output_band: Option<Frame>,
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
    world: &'a World,
    flight: &'a Flight,
    palette: &'a Palette,
    frame_height: usize,
    terrain: PreparedTerrain,
}
impl Scene<'_> {
    fn draw(&self, canvas: &mut SceneCanvas<'_, '_>, projection: Projection) -> [usize; 3] {
        let order = self.order(projection);
        let prepared = self.prepare(projection, [canvas.color.width, canvas.color.height]);
        self.draw_ordered(canvas, projection, &order, &prepared);
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
    fn draw_ordered(
        &self,
        canvas: &mut SceneCanvas<'_, '_>,
        projection: Projection,
        order: &[usize; 3],
        prepared: &[Option<PreparedJet>; 3],
    ) {
        draw_world(canvas, self.world, &projection, self.palette, &self.terrain);
        draw_sky(canvas, &projection, self.palette, self.frame_height);
        draw_trails(canvas, self.flight, &projection, self.palette);
        draw_smoke(canvas, self.flight, &projection, self.palette.day);
        for i in &order[..self.flight.count] {
            if let Some(jet) = &prepared[*i] {
                jet.draw(canvas, &projection, self.palette);
            }
        }
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
    pub fn new(width: usize, height: usize) -> Result<Self, RenderError> {
        Self::allocate(width, height, false)
    }
    pub(super) fn new_compact(width: usize, height: usize) -> Result<Self, RenderError> {
        assert!(width <= 128 && height <= 128);
        Self::allocate(width, height, true)
    }
    fn allocate(width: usize, height: usize, compact: bool) -> Result<Self, RenderError> {
        let width = width.clamp(8, MAX_WIDTH);
        let height = height.clamp(8, MAX_HEIGHT);
        let side = if compact {
            compact::SOURCE_SIDE
        } else {
            mirui_sprite_stack::raster::sheet_side(width, height, roll_pivot(height))
        };
        Ok(Self {
            frame: Frame::with_format(
                width,
                height,
                if compact {
                    ColorFormat::RGB565
                } else {
                    ColorFormat::RGBA8888
                },
            )?,
            sheet: Frame::new(side, side)?,
            workspace: Workspace::new(side, side)?,
            output_band: if compact {
                Some(Frame::new(width, height.min(compact::TILE_SIDE))?)
            } else {
                None
            },
            palette: Palette::new(Daylight::Day),
            flames: [Flame::default(); 6],
            flame_count: 0,
            seen: [[Fixed::ONE; 3]; 6],
        })
    }
    pub fn bytes(&self) -> usize {
        self.frame.bytes()
            + self.sheet.bytes()
            + self.workspace.bytes()
            + self.output_band.as_ref().map_or(0, Frame::bytes)
            + core::mem::size_of::<Self>()
    }
    pub(crate) fn render(&mut self, world: &World, flight: &Flight, placed: &Placements) {
        assert!(
            self.output_band.is_none(),
            "compact rendering requires its cockpit pass"
        );
        let p = flight.projection(self.frame.width, self.frame.height);
        self.flame_count = 0;
        let Self {
            frame,
            sheet,
            workspace,
            palette,
            flames,
            flame_count,
            seen,
            output_band: _,
        } = self;
        let frame_height = frame.height;
        let mapping = Mapping::new(p, frame.width, frame.height);
        let scene = Scene {
            placed,
            world,
            flight,
            palette,
            frame_height,
            terrain: PreparedTerrain::new(world, flight, &mapping.projection, palette, placed),
        };
        let mut draw = |canvas: &mut SceneCanvas<'_, '_>, projection: Projection| {
            let order = scene.draw(canvas, projection);
            for i in &order[..flight.count] {
                if let Some(jet) = placed[*i] {
                    record_flames(
                        canvas,
                        flight,
                        (*i, jet),
                        &projection,
                        flames,
                        flame_count,
                        seen,
                    );
                }
            }
        };
        if mapping.rolled {
            workspace.with_region(sheet, mapping.size, [0, 0], mapping.bounds, |canvas| {
                draw(canvas, mapping.projection)
            });
            let pixels = sheet.texture.read_pixels().expect("flight roll sheet");
            frame.with_canvas(|canvas| {
                for y in 0..canvas.height {
                    for x in 0..canvas.width {
                        let [sx, sy] = mapping.source_at(x as i32, y as i32);
                        let at = (sy as usize * sheet.width + sx as usize) * 4;
                        canvas.bytes[(y * canvas.width + x) * 4..(y * canvas.width + x + 1) * 4]
                            .copy_from_slice(&pixels.bytes()[at..at + 4]);
                    }
                }
            });
            for flame in &mut flames[..*flame_count] {
                for point in &mut flame.points {
                    *point = mapping.unroll(*point);
                }
            }
        } else {
            workspace.with_canvas(frame, |canvas| draw(canvas, p));
        }
    }
    pub(super) fn render_with_cockpit(
        &mut self,
        world: &World,
        flight: &Flight,
        cockpit: &super::cockpit::Cockpit,
        placed: &Placements,
    ) {
        if self.output_band.is_some() {
            self.render_compact(world, flight, cockpit, placed);
        } else {
            self.render(world, flight, placed);
            self.frame
                .with_canvas(|canvas| cockpit.draw(canvas, world, flight, &self.palette));
        }
    }
}
#[derive(Clone, Copy)]
struct HeightProjection {
    origin: Fixed,
    scale: Fixed,
}
impl HeightProjection {
    fn new(center: Fixed, scale: Fixed, slope: Fixed, depth: Fixed, altitude: Fixed) -> Self {
        Self {
            origin: center - scale * (slope * depth - altitude),
            scale,
        }
    }
    fn at(&self, height: usize) -> i32 {
        assert!(
            height <= world::MAX_HEIGHT,
            "terrain height outside projection"
        );
        (self.origin - self.scale * Fixed::from_int(height as i32) + Fixed::from_ratio(1, 100000))
            .floor()
            .trunc_to_int()
    }
    fn minimum_y(&self) -> i32 {
        self.at(if self.scale >= Fixed::ZERO {
            world::MAX_HEIGHT
        } else {
            0
        })
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
        world: &World,
        flight: &Flight,
        p: &Projection,
        pal: &Palette,
        placed: &Placements,
    ) -> Self {
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
fn draw_world(
    canvas: &mut SceneCanvas<'_, '_>,
    world: &World,
    p: &Projection,
    pal: &Palette,
    prepared: &PreparedTerrain,
) {
    mirui::trace_span!("flight.terrain");
    // Terrain and clouds claim depth first. Sky fills every remaining color pixel.
    canvas.clear_depth();
    let [left, clip_top, right, bottom] = canvas.color.clip;
    if left >= right || clip_top >= bottom {
        return;
    }
    // Coverage moves upward; farther samples cannot change a completed local column.
    let mut open_columns = right - left;
    let (sn, cs) = p.yaw.sin_cos();
    let q = p.perspective;
    let scale = p.scale;
    let slope = p.slope;
    let [cx, cy] = p.center;
    let alt = p.position[2];
    let [bx, by] = prepared.base;
    let rank = prepared.rank;
    let drift = prepared.drift;
    let (ground_levels, ground_materials) = world.ground_planes();
    let below = Fixed::from_int(canvas.color.bounds[3]) - cy;
    let nearest = if q > Fixed64::ZERO {
        -p.divide_perspective(Fixed::from_ratio(85, 100))
    } else {
        -Fixed::from_int(1_000_000)
    };
    let eye_height = if q > Fixed64::ZERO {
        alt + p.divide_perspective(slope)
    } else {
        Fixed::from_int(1_000_000)
    };
    let dn = scale * slope + p.apply_perspective(below);
    let mut v = if eye_height > Fixed::from_int((world::MAX_HEIGHT) as i32) + Fixed::ONE
        && dn > Fixed::from_ratio(1, 1000000)
    {
        nearest.max(
            (scale * (alt - Fixed::from_int((world::MAX_HEIGHT) as i32)) - below) / dn
                - Fixed::from_int(2),
        )
    } else {
        nearest
    };
    let limit = Fixed::from_int(1500) + Fixed::from_int(3) * alt;
    let fog_start = Fixed::from_int(300) + Fixed::from_int(2) * alt;
    let above = cy - Fixed::from_int(canvas.color.bounds[1]) + Fixed::from_int(2);
    let end = if scale * slope > p.apply_perspective(above) {
        limit.min((scale * alt + above) / (scale * slope - p.apply_perspective(above)))
    } else {
        limit
    };
    let mut den = Fixed::ONE + p.apply_perspective(v);
    let mut m = scale / den;
    let mut row = HeightProjection::new(cy, m, slope, v, alt);
    let rows = scale * (slope + p.apply_perspective(alt));
    for _ in 0..4000 {
        if v >= end || open_columns == 0 {
            break;
        }
        let step = (den * den / rows)
            .min(Fixed::from_ratio(25, 10) * den / scale)
            .max(Fixed::from_ratio(5, 10));
        v += step;
        den = Fixed::ONE + p.apply_perspective(v);
        m = scale / den;
        let previous = row;
        row = HeightProjection::new(cy, m, slope, v, alt);
        // Advance the global ray sequence, but do not sample below the local clip.
        if row.minimum_y() >= bottom && previous.minimum_y() >= bottom {
            continue;
        }
        let inverse = den / scale;
        let ax = cs * inverse;
        let ay = -sn * inverse;
        let mut wx = bx - cx * ax - v * sn + ax * Fixed::from_int(canvas.color.clip[0]);
        let mut wy = by - cx * ay - v * cs + ay * Fixed::from_int(canvas.color.clip[0]);
        // All world samples along this scanline lie between its two endpoints.
        let last = right - left - 1;
        let x0 = wx.trunc_to_int();
        let x1 = (wx + ax * last).trunc_to_int();
        let y0 = wy.trunc_to_int();
        let y1 = (wy + ay * last).trunc_to_int();
        let [xmin, ymin, xmax, ymax] = prepared.shadow_bounds;
        let cast_shadows =
            x0.min(x1) <= xmax && x0.max(x1) >= xmin && y0.min(y1) <= ymax && y0.max(y1) >= ymin;
        let fog = if q > Fixed64::ZERO && v > fog_start {
            (((v - fog_start) / (limit - fog_start)).sqrt() * Fixed::from_int(10))
                .round()
                .min(Fixed::from_int(10))
                / Fixed::from_int(10)
        } else {
            Fixed::ZERO
        };
        let shade_fog = |color| {
            if fog.is_zero() {
                // Terrain spans are opaque, including custom palette colors.
                color | 0xff00_0000
            } else {
                blend(color, pal.haze, fog)
            }
        };
        let mut ground_height = usize::MAX;
        let mut ground_rows = [0; 2];
        for px in canvas.color.clip[0]..canvas.color.clip[2] {
            let x = px as usize;
            let column_index = (px - canvas.color.origin[0]) as usize;
            let mut column = canvas.columns[column_index];
            let sample_x = wx;
            let sample_y = wy;
            wx += ax;
            wy += ay;
            if column <= clip_top {
                continue;
            }
            let gx = sample_x.trunc_to_int();
            let gy = sample_y.trunc_to_int();
            if rank > 0 {
                let cloud = world.cloud(
                    (Fixed::from_int(gx) - drift[0]).trunc_to_int(),
                    (Fixed::from_int(gy) - drift[1]).trunc_to_int(),
                );
                let thick = usize::from(cloud & 15);
                if thick > 0 && cloud >> 4 < rank {
                    let top = (world::CLOUD_ALT).trunc_to_int() as usize + thick - 1;
                    canvas.fill_span(
                        x,
                        row.at(top),
                        previous.at(top),
                        shade_fog(pal.cloud_top[thick - 1]),
                        v,
                        thick == 1,
                    );
                    canvas.fill_span(
                        x,
                        previous.at((world::CLOUD_ALT).trunc_to_int() as usize),
                        row.at((world::CLOUD_ALT).trunc_to_int() as usize),
                        shade_fog(pal.cloud_under),
                        v,
                        thick == 1,
                    );
                    for layer in (0..thick - 1).rev() {
                        let z = (world::CLOUD_ALT).trunc_to_int() as usize + layer;
                        canvas.fill_depth(
                            x,
                            previous.at(z + 1),
                            previous.at(z),
                            shade_fog(pal.cloud_wall[layer]),
                            v,
                        );
                    }
                }
            }
            let cell = world.ground_index(gx, gy);
            let h = usize::from(ground_levels[cell]);
            if h != ground_height {
                ground_rows = [row.at(h), previous.at(h)];
                ground_height = h;
            }
            let [yt, yr] = ground_rows;
            if yt >= column && yr >= column {
                continue;
            }
            let material = usize::from(ground_materials[cell]);
            let mut shaded = false;
            if rank > 0 {
                let c = world.cloud(
                    (Fixed::from_int(gx) + prepared.cloud_sun[0] - drift[0]).trunc_to_int(),
                    (Fixed::from_int(gy) + prepared.cloud_sun[1] - drift[1]).trunc_to_int(),
                );
                shaded = c & 15 > 1 && c >> 4 < rank;
            }
            if !shaded && cast_shadows {
                for shadow in &prepared.shadows[..usize::from(prepared.shadow_count)] {
                    let dx = Fixed::from_int(gx) - shadow.center[0]
                        + pal.sun[0] * Fixed::from_int(h as i32);
                    let dy = Fixed::from_int(gy) - shadow.center[1]
                        + pal.sun[1] * Fixed::from_int(h as i32);
                    if dx.abs() < prepared.reach && dy.abs() < prepared.reach {
                        let a = (dx * shadow.cos + dy * shadow.sin) / shadow.span;
                        let b = (dx * shadow.sin - dy * shadow.cos) / prepared.jet_scale;
                        let i = (a + (jet::PIVOT_X + Fixed::HALF)).floor().trunc_to_int();
                        let k =
                            ((jet::PIVOT[0] - b + Fixed::from_ratio(5, 10)).floor()).trunc_to_int();
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
                } else if !shaded && inverse < Fixed::from_ratio(18, 10) {
                    let n = noise((gx + i32::from(prepared.water[1])) >> 2, gy);
                    if n > 243
                        && noise((gx + 1024 - i32::from(prepared.water[2])) >> 2, gy + 91) > 128
                    {
                        top = pal.spark[usize::from(n > 252)];
                    }
                }
                wall = top;
            }
            if yr < column {
                canvas.fill_depth(x, yr, column, shade_fog(wall), v);
                column = yr;
            }
            if yt < column {
                canvas.fill_depth(x, yt, column, shade_fog(top), v);
                column = yt;
            }
            canvas.columns[column_index] = column;
            if column <= clip_top {
                open_columns -= 1;
            }
        }
    }
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
#[derive(Clone, Copy)]
struct PreparedLayer {
    camera: [Fixed; 3],
    drop: Fixed,
    bounds: Option<[[i16; 4]; 2]>,
}

struct PreparedJet {
    center: [Fixed; 4],
    fade: Fixed,
    grid: GridBasis,
    layers: [Option<PreparedLayer>; jet::LAYERS],
    order: [usize; jet::LAYERS],
    lights: [usize; 2],
    samples: usize,
    offsets: [[Fixed; 2]; 64],
    sample_bounds: [[Fixed; 2]; 2],
    detail: jet::Detail,
    level: mirui_sprite_stack::format::SparseLevel<'static>,
    baked_bounds: bool,
    right_side_x: u8,
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
        let reach = Fixed::from_int((jet::LENGTH) as i32) * flight.settings.jet_scale * center[2]
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
        let project_axis = |a: [Fixed; 3]| {
            [
                sc * (a[0] * cs - a[1] * sn),
                -sc * (a[0] * sn + a[1] * cs),
                sc * a[2],
            ]
        };
        let [right, forward, up] = axes.map(project_axis);
        let dx = jet.position[0] - p.position[0];
        let dy = jet.position[1] - p.position[1];
        let cu = dx * cs - dy * sn;
        let cv = -(dx * sn + dy * cs);
        let cw = jet.position[2] - p.position[2];
        let plane = if p.perspective.is_zero() {
            if p.slope * up[2] - up[1] >= Fixed::ZERO {
                Fixed::from_int(1_000_000)
            } else {
                -Fixed::from_int(1_000_000)
            }
        } else {
            (-cu * up[0]
                + (-p.divide_perspective(Fixed::ONE) - cv) * up[1]
                + (p.divide_perspective(p.slope) - cw) * up[2])
                / (sc * sc)
        };
        let mut order: [usize; jet::LAYERS] = core::array::from_fn(|i| i);
        order.sort_unstable_by(|a, b| {
            (((Fixed::from_int((*b) as i32) - jet::PIVOT[1]) * flight.spread - plane).abs()).cmp(
                &((Fixed::from_int((*a) as i32) - jet::PIVOT[1]) * flight.spread - plane).abs(),
            )
        });
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
        let big = sc * p.scale
            / (Fixed::ONE + p.apply_perspective(cv)
                - p.apply_perspective(
                    Fixed::from_int(jet::LENGTH as i32) * Fixed::from_ratio(6, 10) * sc,
                ))
            .max(Fixed::from_ratio(2, 10));
        let mut detail = jet::Detail::for_scale(big);
        if jet.model.levels().len() == 1 {
            detail = jet::Detail::Full;
        }
        let level = jet.model.levels()[detail as usize];
        let cell_scale = detail.cell_scale();
        // Coarse cells reuse the existing grid over a larger footprint at lower density.
        let samples = ((big * Fixed::from_int(2) - Fixed::from_ratio(24, 100))
            .ceil()
            .clamp(Fixed::from_int(2), Fixed::from_int(8)))
        .trunc_to_int() as usize;
        let mut offsets = [[Fixed::ZERO; 2]; 64];
        for i in 0..samples {
            for k in 0..samples {
                let a = ((Fixed::from_int((i) as i32) + Fixed::from_ratio(5, 10))
                    / Fixed::from_int((samples) as i32)
                    - Fixed::from_ratio(5, 10))
                    * cell_scale;
                let b = ((Fixed::from_int((k) as i32) + Fixed::from_ratio(5, 10))
                    / Fixed::from_int((samples) as i32)
                    - Fixed::from_ratio(5, 10))
                    * cell_scale;
                offsets[i * samples + k] = [
                    center[2] * (a * right[0] + b * forward[0]),
                    -center[2]
                        * (a * (right[2] + p.slope * right[1])
                            + b * (forward[2] + p.slope * forward[1])),
                ];
            }
        }
        let mut sample_bounds = [offsets[0]; 2];
        for offset in &offsets[1..samples * samples] {
            for axis in 0..2 {
                sample_bounds[0][axis] = sample_bounds[0][axis].min(offset[axis]);
                sample_bounds[1][axis] = sample_bounds[1][axis].max(offset[axis]);
            }
        }
        let first_slice = (0..jet::LAYERS)
            .position(|layer| level.layer(layer).is_some_and(|slice| !slice.is_empty()))
            .unwrap_or(jet::LAYERS);
        // All cells share a fractional pivot. Project that fraction once;
        // whole-cell steps along either axis are exact integer products.
        let grid_x = jet::lateral(0) + detail.center_offset();
        let grid_y = jet::longitudinal(0) - detail.center_offset();
        let grid: [Fixed; 3] = core::array::from_fn(|i| grid_x * right[i] + grid_y * forward[i]);
        let layers = core::array::from_fn(|layer: usize| {
            let intro = flight.intro - Fixed::from_int((index) as i32) * Fixed::from_ratio(25, 100);
            let t = (intro
                - Fixed::from_int((layer.saturating_sub(first_slice)) as i32)
                    * Fixed::from_ratio(7, 100))
                / Fixed::from_ratio(36, 100);
            if t <= Fixed::ZERO {
                return None;
            }
            let drop = if t < Fixed::ONE {
                -(Fixed::ONE - t * t)
                    * (Fixed::from_int((size[1]) as i32) / Fixed::from_int(2) + Fixed::from_int(44))
            } else {
                Fixed::ZERO
            };
            let height = (Fixed::from_int((layer) as i32) - jet::PIVOT[1]) * flight.spread;
            let bu = cu + height * up[0];
            let bv = cv + height * up[1];
            let bw = cw + height * up[2];
            Some(PreparedLayer {
                camera: [bu + grid[0], bv + grid[1], bw + grid[2]],
                drop,
                bounds: None,
            })
        });
        Some(Self {
            center,
            fade,
            grid: GridBasis::new(right, forward.map(|step| -step)),
            layers,
            order,
            lights,
            samples,
            offsets,
            sample_bounds,
            detail,
            level,
            baked_bounds: core::ptr::eq(jet.model, &jet::MODEL),
            right_side_x: (-grid_x).ceil().to_int() as u8,
        })
    }
    fn draw(&self, canvas: &mut SceneCanvas<'_, '_>, p: &Projection, pal: &Palette) {
        mirui::trace_span!("flight.aircraft");
        let center = self.center;
        let fade = self.fade;
        let samples = self.samples;
        let offsets = &self.offsets;
        canvas.mask.fill(0);
        let mut bounds = [
            canvas.color.width as i32,
            canvas.color.height as i32,
            -1,
            -1,
        ];
        {
            mirui::trace_span!("flight.aircraft.samples");
            for index in self.order {
                let Some(layer) = &self.layers[index] else {
                    continue;
                };
                let voxels = self.level.layer(index).unwrap_or_default();
                let voxels = if let Some(bands) = layer.bounds {
                    let [left, top, right, bottom] = canvas.color.clip;
                    let visible = bands.map(|[x0, y0, x1, y1]| {
                        i32::from(x0) < right
                            && i32::from(x1) > left
                            && i32::from(y0) < bottom
                            && i32::from(y1) > top
                    });
                    let mid = voxels.len() / 2;
                    match visible {
                        [false, false] => continue,
                        [true, false] => &voxels[..mid],
                        [false, true] => &voxels[mid..],
                        [true, true] => voxels,
                    }
                } else {
                    voxels
                };
                for voxel in voxels {
                    let Some([x, y, m, depth]) = self.project(layer, voxel, p) else {
                        continue;
                    };
                    let xs = self.sample_ends(x, 0, m);
                    if xs[0].max(xs[1]) < canvas.color.clip[0]
                        || xs[0].min(xs[1]) >= canvas.color.clip[2]
                    {
                        continue;
                    }
                    let ys = self.sample_ends(y, 1, m);
                    if ys[0].max(ys[1]) < canvas.color.clip[1]
                        || ys[0].min(ys[1]) >= canvas.color.clip[3]
                    {
                        continue;
                    }
                    // Zero is uncovered. Keep the visible palette index and its lighting side.
                    let material = voxel.color | (u8::from(voxel.x >= self.right_side_x) << 7);
                    // Coincident extrema make that coordinate identical for every offset.
                    let fixed_x = xs[0] == xs[1];
                    let fixed_y = ys[0] == ys[1];
                    let clip = canvas.color.clip;
                    let mut mark = |xx, yy| {
                        if !canvas.color.contains(xx, yy) {
                            return;
                        }
                        let at = canvas.color.index(xx, yy);
                        if canvas.depth[at] >= depth - Fixed::ONE {
                            canvas.mask[at] = material;
                            bounds = [
                                bounds[0].min(xx),
                                bounds[1].min(yy),
                                bounds[2].max(xx),
                                bounds[3].max(yy),
                            ];
                        }
                    };
                    // Both extrema are sampled. On a single row or column they cover
                    // the entire footprint if they are at most one pixel apart.
                    if (fixed_x && ys[0].abs_diff(ys[1]) <= 1)
                        || (fixed_y && xs[0].abs_diff(xs[1]) <= 1)
                    {
                        mark(xs[0], ys[0]);
                        if !(fixed_x && fixed_y) {
                            mark(xs[1], ys[1]);
                        }
                        continue;
                    }
                    for offset in &offsets[..samples * samples] {
                        let xx = if fixed_x {
                            xs[0]
                        } else {
                            ((x + offset[0] * m / center[2]).round()).trunc_to_int()
                        };
                        if xx < clip[0] || xx >= clip[2] {
                            continue;
                        }
                        let yy = if fixed_y {
                            ys[0]
                        } else {
                            ((y + offset[1] * m / center[2]).round()).trunc_to_int()
                        };
                        mark(xx, yy);
                    }
                }
            }
        }
        {
            mirui::trace_span!("flight.aircraft.outline");
            let [left, top, right, bottom] = canvas.color.clip;
            for y in (bounds[1] - 1).max(top)..=(bounds[3] + 1).min(bottom - 1) {
                for x in (bounds[0] - 1).max(left)..=(bounds[2] + 1).min(right - 1) {
                    let at = canvas.color.index(x, y);
                    let interior = x > left && x + 1 < right && y > top && y + 1 < bottom;
                    let material = canvas.mask[at];
                    if material != 0 {
                        let light = self.lights[usize::from(material >> 7)];
                        let mut color = pal.jets[light][usize::from(material & 0x7f)];
                        if fade < Fixed::ONE && interior {
                            color = blend(canvas.get(at), color, fade);
                        }
                        canvas.put(at, color);
                    } else if interior
                        && fade >= Fixed::from_ratio(5, 10)
                        && (canvas.mask[at - 1] != 0
                            || canvas.mask[at + 1] != 0
                            || canvas.mask[at - canvas.color.pitch] != 0
                            || canvas.mask[at + canvas.color.pitch] != 0)
                    {
                        canvas.put(at, pal.outline);
                    }
                }
            }
        }
    }

    fn band_bounds(&self, index: usize, half: usize) -> Option<[[u8; 2]; 2]> {
        if self.baked_bounds {
            return self.detail.band_bounds(index, half);
        }
        let voxels = self.level.layer(index)?;
        let (first, second) = voxels.split_at(voxels.len() / 2);
        let mut samples = if half == 0 { first } else { second }.iter();
        let first = samples.next()?;
        let mut bounds = [[first.x; 2], [first.y; 2]];
        for v in samples {
            for (axis, point) in bounds.iter_mut().zip([v.x, v.y]) {
                axis[0] = axis[0].min(point);
                axis[1] = axis[1].max(point);
            }
        }
        Some(bounds)
    }
    fn prepare_regions(&mut self, p: &Projection) {
        mirui::trace_span!("flight.aircraft.regions");
        for index in 0..jet::LAYERS {
            let Some(layer) = self.layers[index] else {
                continue;
            };
            let bounds = core::array::from_fn(|half| {
                self.band_bounds(index, half)
                    .map_or(Self::EMPTY_BAND, |coords| {
                        self.project_band(&layer, p, coords)
                    })
            });
            self.layers[index] = Some(PreparedLayer {
                bounds: Some(bounds),
                ..layer
            });
        }
    }

    const EMPTY_BAND: [i16; 4] = [i16::MAX, i16::MAX, i16::MIN, i16::MIN];

    fn project_band(
        &self,
        layer: &PreparedLayer,
        p: &Projection,
        [x, y]: [[u8; 2]; 2],
    ) -> [i16; 4] {
        let axis_range = |axis: usize| {
            self.grid
                .axis_range(layer.camera[axis], axis, x.map(i32::from), y.map(i32::from))
        };
        let depth = axis_range(1);
        let den = depth.map(|d| Fixed::ONE + p.apply_perspective(d));
        let near = Fixed::from_ratio(2, 10);
        if den[1] < near {
            return Self::EMPTY_BAND;
        }
        // A layer crossing the near plane keeps per-voxel clipping.
        if den[0] < near {
            return [i16::MIN, i16::MIN, i16::MAX, i16::MAX];
        }
        let scale = [p.scale / den[1], p.scale / den[0]];
        // Scale stays positive; signs select which endpoint gives each extreme.
        let scaled = |[lo, hi]: [Fixed; 2]| {
            [
                lo * scale[usize::from(lo < Fixed::ZERO)],
                hi * scale[usize::from(hi >= Fixed::ZERO)],
            ]
        };
        let u = scaled(axis_range(0));
        let z = axis_range(2);
        let [s0, s1] = depth.map(|d| p.slope * d);
        let v = scaled([z[0] + s0.min(s1), z[1] + s0.max(s1)]);
        let position = [
            [p.center[0] + u[0], p.center[0] + u[1]],
            [
                p.center[1] - v[1] + layer.drop,
                p.center[1] - v[0] + layer.drop,
            ],
        ];
        let mut bounds = [0; 4];
        for axis in 0..2 {
            let offset = scaled(self.sample_bounds.map(|bound| bound[axis]));
            let ends = core::array::from_fn::<_, 2, _>(|i| {
                (position[axis][i] + offset[i] / self.center[2])
                    .round()
                    .trunc_to_int()
            });
            bounds[axis] = ends[0];
            bounds[axis + 2] = ends[1].saturating_add(1);
        }
        // Coordinates outside i16 are outside every supported scene clip.
        bounds.map(|x| x.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16)
    }

    fn project(
        &self,
        layer: &PreparedLayer,
        voxel: &jet::Voxel,
        p: &Projection,
    ) -> Option<[Fixed; 4]> {
        let [bu, bv, bw] = layer.camera;
        let x = i32::from(voxel.x);
        let y = i32::from(voxel.y);
        let depth = self.grid.axis(bv, 1, x, y);
        let den = Fixed::ONE + p.apply_perspective(depth);
        if den < Fixed::from_ratio(2, 10) {
            return None;
        }
        let m = p.scale / den;
        Some([
            p.center[0] + m * self.grid.axis(bu, 0, x, y),
            p.center[1] - m * (self.grid.axis(bw, 2, x, y) + p.slope * depth) + layer.drop,
            m,
            depth,
        ])
    }

    fn sample_ends(&self, position: Fixed, axis: usize, scale: Fixed) -> [i32; 2] {
        self.sample_bounds.map(|bound| {
            (position + bound[axis] * scale / self.center[2])
                .round()
                .trunc_to_int()
        })
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
    fn resolve(&mut self, canvas: &SceneCanvas<'_, '_>) {
        let mut samples = self.samples;
        samples.resolve(canvas, Fixed::ONE, |i| self.query(i));
        self.samples = samples;
    }
    fn missing(&self) -> Option<[i32; 2]> {
        self.samples.next_missing().map(|i| {
            let (x, y, _) = self.query(i);
            [x, y]
        })
    }

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
fn record_flames(
    canvas: &SceneCanvas<'_, '_>,
    flight: &Flight,
    instance: (usize, PlacedAircraft),
    p: &Projection,
    out: &mut [Flame; 6],
    count: &mut usize,
    seen: &mut [[Fixed; 3]; 6],
) {
    let mut pending = [PendingFlame::default(); 2];
    let mut length = 0;
    project_flames(flight, instance, p, &mut pending, &mut length);
    for mut value in pending[..length].iter().copied() {
        value.resolve(canvas);
        assert!(value.missing().is_none());
        out[*count] = value.finish(seen);
        *count += 1;
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
            world: &world,
            flight: &flight,
            palette: &palette,
            frame_height: 128,
            terrain: PreparedTerrain::new(&world, &flight, &projection, &palette, &placed),
        };
        let prepared = scene.prepare(projection, [128; 2]);
        let jet = prepared[0].as_ref().unwrap();
        assert_eq!(jet.center, projection.project(active.position).unwrap());
        assert!(jet.level.layer(0).unwrap().is_empty());
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
        let prepared = PreparedTerrain::new(
            &world,
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
                for h in 0..=world::MAX_HEIGHT as i32 {
                    let center = prepared.shadows[index].center[axis] - palette.sun[axis] * h;
                    assert!(
                        Fixed::from_int(prepared.shadow_bounds[axis]) <= center - prepared.reach
                    );
                    assert!(
                        Fixed::from_int(prepared.shadow_bounds[axis + 2])
                            >= center + prepared.reach
                    );
                }
            }
        }
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                let _ = PreparedTerrain::new(
                    &world,
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
        let prepared = PreparedTerrain::new(&world, &flight, &projection, &palette, &placed);
        assert_eq!(prepared.shadow_count, 2);
        let shadow = prepared.shadows[1];
        assert_eq!((shadow.sin, shadow.cos, shadow.span), (sin, cos, span));
    }

    #[test]
    fn height_projection_minimum_bounds_every_supported_height() {
        for origin in [-256, -1, 0, 37, 256] {
            for scale in [-512, -37, -1, 0, 1, 37, 512] {
                let row = HeightProjection {
                    origin: Fixed::from_int(origin),
                    scale: Fixed::from_ratio(scale, 256),
                };
                let minimum = row.minimum_y();
                for height in 0..=world::MAX_HEIGHT {
                    assert!(row.at(height) >= minimum);
                }
            }
        }
    }
    #[test]
    #[should_panic(expected = "terrain height outside projection")]
    fn height_projection_rejects_heights_outside_the_world_domain() {
        HeightProjection::new(Fixed::ONE, Fixed::ONE, Fixed::ONE, Fixed::ONE, Fixed::ONE)
            .at(world::MAX_HEIGHT + 1);
    }
    #[test]
    fn prepared_regions_cover_all_samples_without_allocating() {
        assert!(core::mem::size_of::<PreparedLayer>() <= 36);
        let world = World::with_size(7, Fixed::from_ratio(33, 100), 64, 32, 16);
        let mut flight = Flight::new(&world, Settings::default());
        let palette = Palette::new(Daylight::Day);
        let mut details = [false; 2];
        for size in [32, 128, 480] {
            for view in [View::Overhead, View::Chase] {
                flight.view = view;
                for _ in 0..180 {
                    flight.step(&world, 16, 128, 128);
                }
                let projection = flight.projection(size, size);
                let mut prepared = PreparedJet::new(
                    &flight,
                    0,
                    PlacedAircraft::from_flight(&flight)[0].unwrap(),
                    &projection,
                    &palette,
                    [size, size],
                )
                .expect("visible aircraft");
                details[usize::from(prepared.detail == jet::Detail::Coarse)] = true;
                assert!(prepared.layers.iter().flatten().all(|l| l.bounds.is_none()));
                let original = prepared.layers;
                assert_eq!(
                    crate::test_support::tracked_allocations(|| {
                        prepared.prepare_regions(&projection)
                    }),
                    0
                );
                for (index, layer) in original.iter().enumerate() {
                    let Some(layer) = layer else { continue };
                    let voxels = prepared.detail.slice(index);
                    for (i, voxel) in voxels.iter().enumerate() {
                        let Some([x, y, scale, _]) = prepared.project(layer, voxel, &projection)
                        else {
                            continue;
                        };
                        let bounds = prepared.layers[index]
                            .expect("visible layer")
                            .bounds
                            .expect("prepared layer")[usize::from(i >= voxels.len() / 2)]
                        .map(i32::from);
                        for offset in &prepared.offsets[..prepared.samples * prepared.samples] {
                            for (axis, position) in [x, y].into_iter().enumerate() {
                                let at = (position + offset[axis] * scale / prepared.center[2])
                                    .round()
                                    .trunc_to_int();
                                assert!(at >= bounds[axis] && at < bounds[axis + 2]);
                            }
                        }
                    }
                }
            }
        }
        assert!(details.into_iter().all(|seen| seen));
    }
    #[test]
    fn near_plane_rejects_only_wholly_hidden_model_bands() {
        let world = World::with_size(7, Fixed::from_ratio(33, 100), 64, 32, 16);
        let mut flight = Flight::new(&world, Settings::default());
        for _ in 0..180 {
            flight.step(&world, 16, 128, 128);
        }
        let mut p = flight.projection(128, 128);
        let palette = Palette::new(Daylight::Day);
        let mut jet = PreparedJet::new(
            &flight,
            0,
            PlacedAircraft::from_flight(&flight)[0].unwrap(),
            &p,
            &palette,
            [128, 128],
        )
        .unwrap();
        p.perspective = Fixed64::from_ratio(1, 64);
        jet.grid = GridBasis::new([Fixed::ZERO, Fixed::ONE, Fixed::ZERO], [Fixed::ZERO; 3]);
        let coords = [[0, 16], [0; 2]];
        let bounds = |depth| {
            let layer = PreparedLayer {
                camera: [Fixed::ZERO, Fixed::from_int(depth - 8), Fixed::ZERO],
                drop: Fixed::ZERO,
                bounds: None,
            };
            jet.project_band(&layer, &p, coords)
        };
        assert_eq!(bounds(-80), PreparedJet::EMPTY_BAND);
        assert_eq!(bounds(-51), [i16::MIN, i16::MIN, i16::MAX, i16::MAX]);
        let [left, top, right, bottom] = bounds(0);
        assert!(left < right && top < bottom);
        assert!(left > i16::MIN && right < i16::MAX);
    }
    #[test]
    fn sample_bounds_never_reject_a_sample_inside_the_region() {
        let world = World::with_size(7, Fixed::from_ratio(33, 100), 64, 32, 16);
        let mut flight = Flight::new(&world, Settings::default());
        let palette = Palette::new(Daylight::Day);
        for view in [View::Overhead, View::Chase] {
            flight.view = view;
            for _ in 0..180 {
                flight.step(&world, 16, 128, 128);
            }
            let projection = flight.projection(128, 128);
            let prepared = PreparedJet::new(
                &flight,
                0,
                PlacedAircraft::from_flight(&flight)[0].unwrap(),
                &projection,
                &palette,
                [128, 128],
            )
            .expect("visible aircraft");
            for scale in [1, 63, 256, 512, 1024] {
                let scale = Fixed::from_ratio(scale, 256);
                for axis in 0..2 {
                    for raw in (-32 * 256..160 * 256).step_by(67) {
                        let position = Fixed::from_ratio(raw, 256);
                        for clip in [[0, 0, 32, 32], [31, 47, 63, 79], [127, 127, 128, 128]] {
                            let ends = prepared.sample_ends(position, axis, scale);
                            if ends[0] == ends[1] {
                                for offset in
                                    &prepared.offsets[..prepared.samples * prepared.samples]
                                {
                                    assert_eq!(
                                        (position + offset[axis] * scale / prepared.center[2])
                                            .round()
                                            .trunc_to_int(),
                                        ends[0]
                                    );
                                }
                            }
                            if ends[0].max(ends[1]) < clip[axis]
                                || ends[0].min(ends[1]) >= clip[axis + 2]
                            {
                                assert!(
                                    prepared.offsets[..prepared.samples * prepared.samples]
                                        .iter()
                                        .all(|offset| {
                                            let at = (position
                                                + offset[axis] * scale / prepared.center[2])
                                                .round()
                                                .trunc_to_int();
                                            at < clip[axis] || at >= clip[axis + 2]
                                        })
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn region_writes_preserve_global_scene_samples_and_depth() {
        let world = World::with_size(7, Fixed::from_ratio(33, 100), 64, 32, 16);
        let mut flight = Flight::new(&world, Settings::default());
        flight.set_wingmen(2);
        flight.controls.key(0, true);
        let raster = Raster::new(128, 128).unwrap();
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
                world: &world,
                flight: &flight,
                palette: &raster.palette,
                frame_height: 128,
                terrain: PreparedTerrain::new(
                    &world,
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
        let mut raster = Raster::new(240, 150).unwrap();
        for view in [View::Overhead, View::Chase, View::Cockpit] {
            flight.view = view;
            for _ in 0..240 {
                flight.step(&world, 16, 240, 150);
            }
            raster.render(&world, &flight, &PlacedAircraft::from_flight(&flight));
            let pixels = raster.frame.texture.read_pixels().unwrap();
            assert!(pixels.bytes().chunks_exact(4).all(|c| c[3] == 255));
            let first = &pixels.bytes()[..4];
            assert!(pixels.bytes().chunks_exact(4).any(|c| c != first));
        }
        let allocation = crate::test_support::tracked_allocations(|| {
            raster.render(&world, &flight, &PlacedAircraft::from_flight(&flight))
        });
        assert_eq!(allocation, 0);
        let max = Raster::new(MAX_WIDTH, MAX_HEIGHT).unwrap();
        assert!(max.sheet.width <= 628);
        assert!(max.bytes() < 8 * 1024 * 1024);
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
