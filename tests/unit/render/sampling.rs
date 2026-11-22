use super::*;

#[test]
fn endpoints_and_iteration_match_exact_pixel_center_coordinates() {
    for samples in [1, 2, 7, 127, 1024] {
        for extent in [1, 128, 384, 1792, 32512, 262144] {
            for origin in [-1048575, -256, 0, 128, 16383] {
                let extent = Fixed::from_ratio(extent, 256);
                let origin = Fixed::from_ratio(origin, 256);
                for start in [-4096, -7, 0, 1, 127, 4096] {
                    let mut axis = SampleAxis::new(start, origin, extent, samples);
                    for pixel in start..start + 17 {
                        let center =
                            i128::from(pixel) * 256 + 128 - i128::from(storage::to_i32(origin));
                        let expected = (center * i128::from(samples))
                            .div_euclid(i128::from(storage::to_i32(extent)))
                            .clamp(0, i128::from(samples - 1))
                            as i32;
                        assert_eq!(SampleAxis::at(pixel, origin, extent, samples), expected);
                        assert_eq!(axis.next(samples), expected);
                    }
                }
            }
        }
    }
}

#[test]
fn runs_preserve_rational_samples_and_following_iterator_state() {
    for samples in [1, 2, 7, 127, 1024] {
        for extent in [1, 128, 384, 1792, 32512, 262144] {
            for origin in [-1048575, -256, 0, 128, 16383] {
                for start in [-4096, -7, 0, 1, 127, 4096] {
                    for limit in [1, 2, 7, 31] {
                        let extent = Fixed::from_ratio(extent, 256);
                        let origin = Fixed::from_ratio(origin, 256);
                        let mut runs = SampleAxis::new(start, origin, extent, samples);
                        let mut points = runs;
                        let mut consumed = 0;
                        while consumed < 33 {
                            let (sample, count) = runs.next_run(samples, limit.min(33 - consumed));
                            assert!(count > 0 && count <= limit);
                            for offset in consumed..consumed + count {
                                let pixel = i128::from(start) + i128::from(offset);
                                let center =
                                    pixel * 256 + 128 - i128::from(storage::to_i32(origin));
                                let expected = (center * i128::from(samples))
                                    .div_euclid(i128::from(storage::to_i32(extent)))
                                    .clamp(0, i128::from(samples - 1))
                                    as i32;
                                assert_eq!(sample, expected);
                                assert_eq!(sample, points.next(samples));
                            }
                            consumed += count;
                        }
                        for _ in 0..17 {
                            assert_eq!(runs.next(samples), points.next(samples));
                        }
                    }
                }
            }
        }
    }
    for samples in [1, 2, 65_535, 8_388_607] {
        let extent = Fixed::from_ratio(i32::MAX, 256);
        let mut axis = SampleAxis::new(0, Fixed::ZERO, extent, samples);
        let (_, count) = axis.next_run(samples, u32::MAX);
        assert_eq!(
            axis.next(samples),
            SampleAxis::at(count as i32, Fixed::ZERO, extent, samples)
        );
    }
}
