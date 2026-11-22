use super::super::{
    flight::{Settings, View},
    raster::tests::reference::Raster,
};
use super::layer::LayerPixels;
use super::*;
#[test]
fn digit_group_clipping_preserves_partial_glyphs_and_skips_empty_groups() {
    struct Probe {
        clip: [i32; 4],
        pixels: [u32; 256],
        visits: usize,
    }
    impl Ink for Probe {
        fn clip(&self) -> [i32; 4] {
            self.clip
        }
        fn get(&self, _: i32, _: i32) -> u32 {
            0
        }
        fn put(&mut self, x: i32, y: i32, color: u32) {
            self.visits += 1;
            let [l, t, r, b] = self.clip;
            if x >= l && x < r && y >= t && y < b {
                self.pixels[(y * 16 + x) as usize] = color;
            }
        }
    }
    for x in (-16..19).step_by(3) {
        for y in (-8..19).step_by(2) {
            for digits in [1, 4] {
                let mut full = Probe {
                    clip: [0, 0, 16, 16],
                    pixels: [0; 256],
                    visits: 0,
                };
                let mut part = Probe {
                    clip: [3, 2, 11, 13],
                    pixels: [0; 256],
                    visits: 0,
                };
                full.number(x, y, 1234, digits, WHITE);
                part.number(x, y, 1234, digits, WHITE);
                for yy in 0..16 {
                    for xx in 0..16 {
                        let at = (yy * 16 + xx) as usize;
                        let expected = if (3..11).contains(&xx) && (2..13).contains(&yy) {
                            full.pixels[at]
                        } else {
                            0
                        };
                        assert_eq!(part.pixels[at], expected);
                    }
                }
                if x >= 11 || y >= 13 || x + digits as i32 * 4 - 1 <= 3 || y + 5 <= 2 {
                    assert_eq!(part.visits, 0);
                }
            }
        }
    }
}

#[test]
fn radar_geometry_preserves_ring_and_bearing_at_all_native_cells() {
    assert_eq!(core::mem::size_of::<RadarCell>(), 2);
    for (width, height) in [(1, 1), (2, 3), (37, 22), (86, 51)] {
        let radius = Fixed::from_int(width.min(height)) * Fixed::from_ratio(47, 100);
        for y in 0..height {
            for x in 0..width {
                let cell = RadarCell::new(x, y, width, height);
                let dx = x - width / 2;
                let dy = y - height / 2;
                let distance = Fixed::from_int(dx * dx + dy * dy).sqrt();
                let ring = [
                    Fixed::ONE,
                    Fixed::from_ratio(66, 100),
                    Fixed::from_ratio(33, 100),
                ]
                .into_iter()
                .any(|ring| (distance / radius - ring).abs() < Fixed::from_ratio(5, 10) / radius);
                assert_eq!(cell.on_ring(), ring);
                let bearing =
                    (distance <= radius).then(|| Fixed::from_int(dx).atan2(Fixed::from_int(-dy)));
                assert_eq!(cell.bearing(), bearing);
            }
        }
    }
}

#[test]
fn transparent_encoded_rows_do_not_read_or_clear_the_workspace() {
    let mut layer = Layer::new(8, 4, 0, true);
    let inks = INSTRUMENT_INKS;
    layer.put(3, 1, WHITE_INK);
    layer.seal(true);
    let LayerPixels::Encoded { rows, .. } = &layer.pixels else {
        panic!("sealed indexed rows");
    };
    assert_eq!(rows[0], rows[1]);
    assert!(rows[1] < rows[2]);
    assert_eq!(rows[2], rows[3]);
    assert_eq!(rows[3], rows[4]);
    let mut scratch = [0xa5; ROW_PIXELS];
    assert!(matches!(
        layer.row(0, &mut scratch, &inks),
        RowPixels::Empty
    ));
    assert!(scratch.iter().all(|v| *v == 0xa5));
    assert_eq!(layer.row(1, &mut scratch, &inks).color(3), WHITE);
    let saved = scratch;
    assert!(matches!(
        layer.row(2, &mut scratch, &inks),
        RowPixels::Empty
    ));
    assert_eq!(scratch, saved);
}
#[test]
fn opaque_coverage_requires_full_fractional_sample_coverage() {
    let mut inks = [0; 30];
    inks[1] = WHITE;
    inks[2] = BLACK;
    let row = RowPixels::Indexed(&[1, 0, 1, 0], &inks);
    let above = RowPixels::Indexed(&[0, 2, 2, 0], &inks);
    let fraction = Fixed::from_ratio(1, 4);
    let samples = core::array::from_fn::<_, 4, _>(|x| row.sample(Some(&above), x, fraction));
    assert_eq!(samples[0], (WHITE, Fixed::from_ratio(3, 4)));
    assert_eq!(samples[1], (BLACK, fraction));
    assert_eq!(samples[2].1, Fixed::ONE);
    assert_eq!(samples[2].0 >> 24, 255);
    assert_eq!(samples[3].0, 0);
}

#[test]
fn covered_regions_do_not_depend_on_background_or_retained_mask() {
    use super::super::raster::{Daylight, Frame};
    let world = World::with_size(7, Fixed::from_ratio(33, 100), 64, 32, 16);
    let mut flight = Flight::new(&world, Settings::default());
    flight.view = View::Cockpit;
    let mut a = Frame::new(32, 32).unwrap();
    let mut b = Frame::new(32, 32).unwrap();
    let mut mask = [255; 1024];
    let mut accepted = 0;
    for day in [Daylight::Day, Daylight::Sunset, Daylight::Night] {
        let palette = Palette::new(day);
        let art = Cockpit::new_compact(128, 128, &palette);
        for steps in [1, 12, 60, 240] {
            for _ in 0..steps {
                flight.step(&world, 16, 128, 128);
            }
            flight.head[0].position = Fixed::from_ratio(steps % 5 - 2, 4);
            flight.head[1].position = -flight.head[0].position;
            let Some(prepared) = art.prepare(
                &flight,
                flight.projection(art.width as usize, art.height as usize),
                &palette,
            ) else {
                continue;
            };
            for y in (0..128).step_by(32) {
                for x in (0..128).step_by(32) {
                    for [w, h] in [[32, 32], [17, 23]] {
                        let clip = [x, y, x + w, y + h];
                        mask.fill(255);
                        let covered = art.covers(clip, &prepared, &mut mask);
                        if !covered {
                            continue;
                        }
                        accepted += 1;
                        assert!(mask[..(w * h) as usize].iter().all(|value| *value == 1));
                        for (frame, background) in [(&mut a, 0xff000000), (&mut b, 0xffffffff)] {
                            frame.with_region([128, 128], [x, y], clip, |canvas| {
                                for yy in y..y + h {
                                    for xx in x..x + w {
                                        let at = canvas.index(xx, yy);
                                        canvas.put(at, background);
                                    }
                                }
                                art.draw_prepared(
                                    canvas,
                                    world.at(Some(Default::default())),
                                    &prepared,
                                );
                            });
                        }
                        let ap = a.texture.read_pixels().unwrap();
                        let bp = b.texture.read_pixels().unwrap();
                        for yy in 0..h {
                            let start = yy as usize * 32 * 4;
                            let end = start + w as usize * 4;
                            assert_eq!(
                                &ap.bytes()[start..end],
                                &bp.bytes()[start..end],
                                "{day:?}, steps {steps}, clip {clip:?}"
                            );
                        }
                    }
                }
            }
            assert!(!art.covers([0, 0, 0, 1], &prepared, &mut mask));
        }
    }
    assert!(
        accepted > 0,
        "settled cockpit must hide some background regions"
    );
}

#[test]
fn cockpit_regions_match_full_resolution_composition() {
    use super::super::raster::{Daylight, Frame};
    let world = World::with_size(7, Fixed::from_ratio(33, 100), 64, 32, 16);
    let mut flight = Flight::new(&world, Settings::default());
    flight.view = View::Cockpit;
    for _ in 0..360 {
        flight.step(&world, 16, 128, 128);
    }
    flight.head[0].position = Fixed::from_ratio(1, 2);
    flight.head[1].position = Fixed::from_ratio(-3, 4);
    let palette = Palette::new(Daylight::Day);
    let art = Cockpit::new_compact(128, 128, &palette);
    let mut full = Frame::new(128, 128).unwrap();
    full.with_canvas(|canvas| {
        for at in 0..128 * 128 {
            canvas.put(at, 0xff603010);
        }
        art.draw(canvas, &world, &flight, &palette);
    });
    let reference = full.texture.read_pixels().unwrap();
    let mut tile = Frame::new(24, 24).unwrap();
    for y in (0..128).step_by(24) {
        for x in (0..128).step_by(24) {
            tile.with_region([128, 128], [x, y], [0, 0, 128, 128], |canvas| {
                for at in 0..24 * 24 {
                    canvas.put(at, 0xff603010);
                }
                art.draw(canvas, &world, &flight, &palette);
            });
            let pixels = tile.texture.read_pixels().unwrap();
            for yy in y..(y + 24).min(128) {
                for xx in x..(x + 24).min(128) {
                    let a = (yy * 128 + xx) as usize;
                    let b = ((yy - y) * 24 + xx - x) as usize;
                    assert_eq!(
                        &reference.bytes()[a * 4..a * 4 + 4],
                        &pixels.bytes()[b * 4..b * 4 + 4],
                        "cockpit region at {xx},{yy}"
                    );
                }
            }
        }
    }
}
#[test]
fn compressed_layers_preserve_pixels_and_fractional_live_composition() {
    let world = World::with_size(7, Fixed::from_ratio(33, 100), 64, 32, 16);
    let mut flight = Flight::new(&world, Settings::default());
    flight.view = View::Cockpit;
    for _ in 0..360 {
        flight.step(&world, 16, 128, 128);
    }
    for day in [
        super::super::raster::Daylight::Day,
        super::super::raster::Daylight::Sunset,
        super::super::raster::Daylight::Night,
    ] {
        let palette = Palette::new(day);
        for (width, height) in [(128, 128), (128, 64), (64, 128), (26, 26)] {
            let dense = Cockpit::new(width, height, &palette);
            let compact = Cockpit::new_compact(width, height, &palette);
            assert!(compact.bytes() < 20 * 1024);
            for (plain, encoded) in dense.layers.iter().zip(&compact.layers) {
                let mut a = [0; ROW_PIXELS];
                let mut b = [0; ROW_PIXELS];
                for y in 0..plain.height as usize {
                    let left = plain.row(y, &mut a, &dense.inks);
                    let right = encoded.row(y, &mut b, &compact.inks);
                    for x in 0..plain.width as usize {
                        assert_eq!(left.color(x), right.color(x));
                    }
                }
            }
            if (width, height) != (128, 128) {
                continue;
            }
            let mut a = super::super::raster::Frame::new(width, height).unwrap();
            let mut b = super::super::raster::Frame::new(width, height).unwrap();
            for offset in [
                Fixed::ZERO,
                Fixed::from_ratio(1, 2),
                Fixed::from_ratio(-3, 4),
            ] {
                flight.head[0].position = offset;
                flight.head[1].position = offset;
                let allocations = crate::test_support::tracked_allocations(|| {
                    for (frame, art) in [(&mut a, &dense), (&mut b, &compact)] {
                        frame.with_canvas(|canvas| {
                            for at in 0..width * height {
                                canvas.put(at, 0xff60_3010);
                            }
                            art.draw(canvas, &world, &flight, &palette);
                        });
                    }
                });
                assert_eq!(allocations, 0);
                assert_eq!(
                    a.texture.read_pixels().unwrap().bytes(),
                    b.texture.read_pixels().unwrap().bytes()
                );
            }
        }
    }
}
#[test]
fn layered_cockpit_has_bounded_storage_and_live_instruments_allocate_nothing() {
    let world = World::new(7, Fixed::from_ratio(33, 100));
    let mut flight = Flight::new(&world, Settings::default());
    flight.view = View::Cockpit;
    for _ in 0..300 {
        flight.step(&world, 16, 480, 300);
    }
    let mut raster = Raster::new([480, 300], false).unwrap();
    let mut frame = crate::raster::Frame::new(480, 300).unwrap();
    let art = Cockpit::new(480, 300, raster.scene.palette);
    assert!(art.bytes() < 3 * 1024 * 1024);
    assert!(
        art.layers.iter().all(|layer| {
            (0..layer.width * layer.height).any(|at| layer.ink_at(at as usize) != 0)
        })
    );
    raster.render(
        &mut frame,
        &world,
        &flight,
        &crate::raster::PlacedAircraft::from_flight(&flight),
    );
    let allocations = crate::test_support::tracked_allocations(|| {
        frame.with_canvas(|canvas| art.draw(canvas, &world, &flight, raster.scene.palette))
    });
    assert_eq!(allocations, 0);
    let pixels = frame.texture.read_pixels().unwrap();
    assert!(pixels.bytes().chunks_exact(4).all(|c| c[3] == 255));
}

impl Cockpit {
    pub(crate) fn new_compact(width: usize, height: usize, palette: &Palette) -> Self {
        let mut cockpit = Self::with_storage(width, height, true);
        cockpit.set_palette(palette);
        cockpit
    }
}
