use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cockpit::Cockpit,
        flight::{Settings, View},
    };
    use mirui::render::texture::ColorFormat;

    #[test]
    fn native_tiles_match_full_scene_and_keep_fixed_workspaces() {
        let world = World::with_size(7, Fixed::from_ratio(33, 100), 64, 32, 16);
        let mut flight = Flight::new(&world, Settings::default());
        flight.set_wingmen(2);
        flight.controls.key(0, true);
        let mut full = Raster::new([128, 128], false).unwrap();
        let mut full_frame = Frame::new(128, 128).unwrap();
        let mut native = Raster::new([128, 128], true).unwrap();
        let mut native_frame = Frame::with_format(128, 128, ColorFormat::RGB565).unwrap();
        let mut expected = Frame::with_format(128, 128, ColorFormat::RGB565).unwrap();
        for day in [Daylight::Day, Daylight::Sunset, Daylight::Night] {
            full.scene.set_daylight(day);
            native.scene.set_daylight(day);
            let cockpit = Cockpit::new_compact(128, 128, &full.scene.palette);
            for view in [View::Overhead, View::Chase, View::Cockpit] {
                flight.view = view;
                for (frames, degrees) in [(1, 0), (12, 60), (100, 150), (200, 240), (1, 315)] {
                    for _ in 0..frames {
                        flight.step(&world, 16, 128, 128);
                    }
                    flight.jets[0].roll =
                        Fixed::from_int(degrees) * super::super::super::math::PI / 180;
                    full.render_with_cockpit(
                        &mut full_frame,
                        &world,
                        &flight,
                        &cockpit,
                        &PlacedAircraft::from_flight(&flight),
                    );
                    full_frame.with_canvas(|canvas| {
                        super::super::super::flame::paint_region(
                            canvas,
                            &full.scene.flames[..full.scene.flame_count],
                            flight.time,
                        )
                        .unwrap();
                    });
                    let rgba = full_frame.texture.read_pixels().unwrap();
                    {
                        let mut target = expected.texture.pixels_mut().unwrap();
                        for y in 0..128 {
                            for x in 0..128 {
                                let at = ((y * 128 + x) * 4) as usize;
                                let [r, g, b, a] = rgba.bytes()[at..at + 4].try_into().unwrap();
                                target.set_pixel(x, y, &Color::rgba(r, g, b, a));
                            }
                        }
                    }
                    let allocations = crate::test_support::tracked_allocations(|| {
                        native.render_with_cockpit(
                            &mut native_frame,
                            &world,
                            &flight,
                            &cockpit,
                            &PlacedAircraft::from_flight(&flight),
                        )
                    });
                    assert_eq!(allocations, 0);
                    let a = expected.texture.read_pixels().unwrap();
                    let b = native_frame.texture.read_pixels().unwrap();
                    let mismatch = a.bytes().iter().zip(b.bytes()).position(|(a, b)| a != b);
                    assert!(
                        mismatch.is_none(),
                        "native {day:?} {view:?} after {frames} steps, roll {degrees}: first differing byte {mismatch:?}"
                    );
                    assert_eq!(full.scene.seen, native.scene.seen);
                    assert!(native.bytes() + native_frame.bytes() < 86 * 1024);
                }
            }
        }
    }
}
