use super::*;
use mirui::render::texture::TexturePixelsMut;
use mirui::types::Color;

// A rotated 32-pixel core spans at most 45 samples; retain two halo pixels per side.
pub(super) const TILE_SIDE: usize = 32;
pub(super) const SOURCE_SIDE: usize = 50;

fn write_rgb565(canvas: &Canvas<'_>, target: &mut TexturePixelsMut<'_>, rect: [i32; 4]) {
    let [left, top, right, bottom] = rect;
    let width = (right - left) as usize;
    debug_assert_eq!(target.format(), ColorFormat::RGB565);
    let stride = target.stride();
    let bytes = target.bytes_mut();
    for y in top..bottom {
        let src = canvas.index(left, y) * 4;
        let dst = y as usize * stride + left as usize * 2;
        let row = &canvas.bytes[src..src + width * 4];
        let output = &mut bytes[dst..dst + width * 2];
        for (rgba, out) in row.chunks_exact(4).zip(output.chunks_exact_mut(2)) {
            let color = Color::rgb(rgba[0], rgba[1], rgba[2]);
            out.copy_from_slice(&(ColorFormat::RGB565.pack(&color) as u16).to_le_bytes());
        }
    }
}

impl Raster {
    pub(super) fn render_compact(
        &mut self,
        world: &World,
        flight: &Flight,
        cockpit: &super::super::cockpit::Cockpit,
    ) {
        mirui::trace_span!("flight.native_frame");
        let Self {
            frame,
            sheet,
            workspace,
            output_band,
            palette,
            flames,
            flame_count,
            seen,
        } = self;
        let width = frame.width;
        let height = frame.height;
        let mapping = Mapping::new(flight.projection(width, height), width, height);
        let scene = Scene {
            world,
            flight,
            palette,
            frame_height: height,
            terrain: PreparedTerrain::new(world, flight, &mapping.projection, palette),
        };
        let prepared_cockpit = cockpit.prepare(flight, palette);
        let order = scene.order(mapping.projection);
        let mut prepared_jets = scene.prepare(mapping.projection, mapping.size);
        for jet in prepared_jets.iter_mut().flatten() {
            jet.prepare_regions(&mapping.projection);
        }
        let mut pending = [PendingFlame::default(); 6];
        let mut count = 0;
        for index in &order[..flight.count] {
            project_flames(
                flight,
                *index,
                &mapping.projection,
                &mut pending,
                &mut count,
            );
        }
        let band = output_band.as_mut().expect("compact color workspace");
        let mut target = frame.texture.pixels_mut().expect("owned compact frame");
        // Without an inverse roll or cockpit overlay, the source columns are the output columns.
        // March them over the full height, keeping the two-pixel outline halo on either side.
        let strip_pitch = (!mapping.rolled && prepared_cockpit.is_none())
            .then(|| (SOURCE_SIDE * SOURCE_SIDE / height).min(SOURCE_SIDE));
        let (tile_width, tile_height) =
            strip_pitch.map_or((TILE_SIDE, TILE_SIDE), |pitch| (pitch - 4, height));
        for y in (0..height).step_by(tile_height) {
            for x in (0..width).step_by(tile_width) {
                let right = (x + tile_width).min(width) as i32;
                let bottom = (y + tile_height).min(height) as i32;
                let x = x as i32;
                let y = y as i32;
                if let Some(pitch) = strip_pitch {
                    workspace.with_extent(
                        sheet,
                        [pitch, height],
                        mapping.size,
                        [x - 2, 0],
                        mapping.bounds,
                        |canvas| {
                            scene.draw_ordered(canvas, mapping.projection, &order, &prepared_jets);
                            for query in &mut pending[..count] {
                                query.resolve(canvas);
                            }
                            write_rgb565(canvas.color, &mut target, [x, y, right, bottom]);
                        },
                    );
                    continue;
                }
                let covered = prepared_cockpit.as_ref().is_some_and(|prepared| {
                    cockpit.covers([x, y, right, bottom], prepared, &mut workspace.mask)
                });
                let origin = if covered {
                    [0, 0]
                } else {
                    let corners = [
                        [x, y],
                        [right - 1, y],
                        [x, bottom - 1],
                        [right - 1, bottom - 1],
                    ]
                    .map(|[x, y]| mapping.source_at(x, y));
                    let origin = [
                        corners.iter().map(|p| p[0]).min().unwrap() - 2,
                        corners.iter().map(|p| p[1]).min().unwrap() - 2,
                    ];
                    assert!(
                        corners.iter().all(|p| p[0] > origin[0]
                            && p[1] > origin[1]
                            && p[0] < origin[0] + SOURCE_SIDE as i32 - 1
                            && p[1] < origin[1] + SOURCE_SIDE as i32 - 1),
                        "inverse roll exceeds source workspace"
                    );
                    let source_end = [
                        corners.iter().map(|p| p[0]).max().unwrap() + 3,
                        corners.iter().map(|p| p[1]).max().unwrap() + 3,
                    ];
                    workspace.with_region(sheet, mapping.size, origin, mapping.bounds, |canvas| {
                        // Keep global ray bounds; only produce samples and their outline halo.
                        canvas.color.clip[2] = canvas.color.clip[2].min(source_end[0]);
                        canvas.color.clip[3] = canvas.color.clip[3].min(source_end[1]);
                        scene.draw_ordered(canvas, mapping.projection, &order, &prepared_jets);
                        for query in &mut pending[..count] {
                            query.resolve(canvas);
                        }
                    });
                    origin
                };
                let source =
                    (!covered).then(|| sheet.texture.read_pixels().expect("compact source pixels"));
                band.with_region(
                    [width, height],
                    [0, y],
                    [x, y, right, bottom],
                    |canvas| {
                        if let Some(source) = &source {
                            for yy in y..bottom {
                                for xx in x..right {
                                    let [sx, sy] = mapping.source_at(xx, yy);
                                    assert!(
                                        sx >= mapping.bounds[0]
                                            && sy >= mapping.bounds[1]
                                            && sx < mapping.bounds[2]
                                            && sy < mapping.bounds[3],
                                    "roll sample {sx},{sy} outside {:?}, virtual {:?}, output {width}x{height} at {xx},{yy}, roll {:?}", mapping.bounds, mapping.size, mapping.projection.roll
                                    );
                                    let at = (((sy - origin[1]) as usize) * SOURCE_SIDE
                                        + (sx - origin[0]) as usize)
                                        * 4;
                                    canvas.put(
                                        canvas.index(xx, yy),
                                        u32::from_le_bytes(
                                            source.bytes()[at..at + 4]
                                                .try_into()
                                                .expect("RGBA source pixel"),
                                        ),
                                    );
                                }
                            }
                        } else {
                            for yy in y..bottom {
                                for xx in x..right {
                                    canvas.put(canvas.index(xx, yy), 0xff00_0000);
                                }
                            }
                        }
                    },
                );
            }
            if strip_pitch.is_none() {
                let rect = [
                    0,
                    y as i32,
                    width as i32,
                    (y + tile_height).min(height) as i32,
                ];
                band.with_region([width, height], [0, y as i32], rect, |canvas| {
                    if let Some(prepared) = &prepared_cockpit {
                        cockpit.draw_prepared(canvas, world, flight, prepared);
                    }
                    write_rgb565(canvas, &mut target, rect);
                });
            }
        }
        while let Some(origin) = pending[..count].iter().find_map(PendingFlame::missing) {
            let before: u32 = pending[..count]
                .iter()
                .map(|p| p.resolved.count_ones())
                .sum();
            workspace.with_region(sheet, mapping.size, origin, mapping.bounds, |canvas| {
                draw_world(
                    canvas,
                    world,
                    flight,
                    &mapping.projection,
                    palette,
                    &scene.terrain,
                );
                for query in &mut pending[..count] {
                    query.resolve(canvas);
                }
            });
            let after: u32 = pending[..count]
                .iter()
                .map(|p| p.resolved.count_ones())
                .sum();
            assert!(after > before, "flame visibility region made no progress");
        }
        for (out, query) in flames.iter_mut().zip(pending[..count].iter().copied()) {
            *out = query.finish(seen);
            for point in &mut out.points {
                *point = mapping.unroll(*point);
            }
        }
        *flame_count = count;
        super::super::flame::composite_native(&mut target, &flames[..count], flight.time);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cockpit::Cockpit,
        flight::{Settings, View},
    };

    #[test]
    fn roll_bounds_cover_every_output_corner_in_all_quadrants() {
        let world = World::with_size(7, Fixed::from_ratio(33, 100), 64, 32, 16);
        let flight = Flight::new(&world, Settings::default());
        for (width, height) in [
            (8, 8),
            (32, 32),
            (128, 128),
            (128, 64),
            (64, 128),
            (127, 113),
        ] {
            for degrees in (-360..360).step_by(5) {
                let mut p = flight.projection(width, height);
                p.roll = Fixed::from_int(degrees) * super::super::super::math::PI / 180;
                p.cockpit = Fixed::ONE;
                let mapping = Mapping::new(p, width, height);
                for y in (0..height).step_by(TILE_SIDE) {
                    for x in (0..width).step_by(TILE_SIDE) {
                        let right = (x + TILE_SIDE).min(width) - 1;
                        let bottom = (y + TILE_SIDE).min(height) - 1;
                        let corners = [[x, y], [right, y], [x, bottom], [right, bottom]]
                            .map(|[x, y]| mapping.source_at(x as i32, y as i32));
                        let low = [
                            corners.iter().map(|p| p[0]).min().unwrap() - 2,
                            corners.iter().map(|p| p[1]).min().unwrap() - 2,
                        ];
                        for point in corners {
                            assert!(
                                point[0] >= mapping.bounds[0]
                                    && point[1] >= mapping.bounds[1]
                                    && point[0] < mapping.bounds[2]
                                    && point[1] < mapping.bounds[3],
                                "{width}x{height} at {degrees}: {point:?} outside {:?}",
                                mapping.bounds
                            );
                            assert!(
                                point[0] > low[0]
                                    && point[1] > low[1]
                                    && point[0] < low[0] + SOURCE_SIDE as i32 - 1
                                    && point[1] < low[1] + SOURCE_SIDE as i32 - 1
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn native_tiles_match_full_scene_and_keep_fixed_workspaces() {
        let world = World::with_size(7, Fixed::from_ratio(33, 100), 64, 32, 16);
        let mut flight = Flight::new(&world, Settings::default());
        flight.set_wingmen(2);
        flight.controls.key(0, true);
        let mut full = Raster::new(128, 128).unwrap();
        let mut native = Raster::new_compact(128, 128).unwrap();
        let mut expected = Frame::with_format(128, 128, ColorFormat::RGB565).unwrap();
        for day in [Daylight::Day, Daylight::Sunset, Daylight::Night] {
            full.palette = Palette::new(day);
            native.palette = Palette::new(day);
            let cockpit = Cockpit::new_compact(128, 128, &full.palette);
            for view in [View::Overhead, View::Chase, View::Cockpit] {
                flight.view = view;
                for (frames, degrees) in [(1, 0), (12, 60), (100, 150), (200, 240), (1, 315)] {
                    for _ in 0..frames {
                        flight.step(&world, 16, 128, 128);
                    }
                    flight.jets[0].roll =
                        Fixed::from_int(degrees) * super::super::super::math::PI / 180;
                    full.render_with_cockpit(&world, &flight, &cockpit);
                    let rgba = full.frame.texture.read_pixels().unwrap();
                    {
                        let mut target = expected.texture.pixels_mut().unwrap();
                        for y in 0..128 {
                            for x in 0..128 {
                                let at = ((y * 128 + x) * 4) as usize;
                                let [r, g, b, a] = rgba.bytes()[at..at + 4].try_into().unwrap();
                                target.set_pixel(x, y, &Color::rgba(r, g, b, a));
                            }
                        }
                        super::super::super::flame::composite_native(
                            &mut target,
                            &full.flames[..full.flame_count],
                            flight.time,
                        );
                    }
                    let allocations = crate::test_support::tracked_allocations(|| {
                        native.render_with_cockpit(&world, &flight, &cockpit)
                    });
                    assert_eq!(allocations, 0);
                    let a = expected.texture.read_pixels().unwrap();
                    let b = native.frame.texture.read_pixels().unwrap();
                    let mismatch = a.bytes().iter().zip(b.bytes()).position(|(a, b)| a != b);
                    assert!(
                        mismatch.is_none(),
                        "native {day:?} {view:?} after {frames} steps, roll {degrees}: first differing byte {mismatch:?}"
                    );
                    assert_eq!(full.seen, native.seen);
                    assert!(native.bytes() < 86 * 1024);
                    assert_eq!(
                        (native.sheet.width, native.sheet.height),
                        (SOURCE_SIDE, SOURCE_SIDE)
                    );
                }
            }
        }
    }
}
