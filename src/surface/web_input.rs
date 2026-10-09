use crate::input::event::input::WHEEL_LINE_PIXELS;
use crate::types::Fixed;

pub(super) fn wheel_delta(
    dx: f64,
    dy: f64,
    mode: u32,
    css_size: (f64, f64),
    logical_size: Option<(u16, u16)>,
) -> Option<(Fixed, Fixed)> {
    let (width, height) = css_size;
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let (logical_width, logical_height) = logical_size
        .map(|(width, height)| (f64::from(width), f64::from(height)))
        .unwrap_or(css_size);
    let (unit_x, unit_y) = match mode {
        0 => (logical_width / width, logical_height / height), // DOM_DELTA_PIXEL
        1 => {
            let step = f64::from(WHEEL_LINE_PIXELS.to_f32());
            (step, step)
        }
        2 => (logical_width, logical_height), // DOM_DELTA_PAGE
        _ => return None,
    };
    Some((
        Fixed::from_f32((dx * unit_x) as f32),
        Fixed::from_f32((-dy * unit_y) as f32),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_delta_scales_once_and_retains_subpixel_momentum() {
        assert_eq!(
            wheel_delta(0.25, -0.125, 0, (400.0, 300.0), Some((800, 900))),
            Some((Fixed::from_ratio(1, 2), Fixed::from_ratio(3, 8)))
        );
        assert_eq!(
            wheel_delta(0.5, 0.25, 0, (800.0, 600.0), Some((400, 300))),
            Some((Fixed::from_ratio(1, 4), Fixed::from_ratio(-1, 8)))
        );
        // Scaling happens before conversion to the fixed-point lattice.
        assert_eq!(
            wheel_delta(0.001, 0.001, 0, (10.0, 10.0), Some((200, 200))),
            Some((Fixed::from_f32(0.02), Fixed::from_f32(-0.02)))
        );
    }

    #[test]
    fn line_and_page_units_do_not_inherit_css_pixel_scaling() {
        assert_eq!(
            wheel_delta(1.0, -2.0, 1, (400.0, 300.0), Some((800, 900))),
            Some((Fixed::from_int(20), Fixed::from_int(40)))
        );
        assert_eq!(
            wheel_delta(-0.5, 1.0, 2, (400.0, 300.0), Some((800, 900))),
            Some((Fixed::from_int(-400), Fixed::from_int(-900)))
        );
    }

    #[test]
    fn unscaled_canvas_uses_its_current_css_extent() {
        assert_eq!(
            wheel_delta(0.25, 0.5, 0, (400.0, 300.0), None),
            Some((Fixed::from_ratio(1, 4), Fixed::from_ratio(-1, 2)))
        );
        assert_eq!(
            wheel_delta(1.0, 1.0, 2, (400.0, 300.0), None),
            Some((Fixed::from_int(400), Fixed::from_int(-300)))
        );
    }

    #[test]
    fn hidden_canvas_and_unknown_units_do_not_generate_wheel_motion() {
        for mode in 0..=2 {
            for css_size in [(0.0, 300.0), (400.0, 0.0)] {
                assert_eq!(
                    wheel_delta(1.0, 1.0, mode, css_size, Some((800, 600))),
                    None
                );
            }
        }
        assert_eq!(wheel_delta(1.0, 1.0, 3, (400.0, 300.0), None), None);
    }
}
