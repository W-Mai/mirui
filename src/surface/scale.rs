use crate::types::Fixed;

#[derive(Debug, Clone, Copy)]
pub enum ScaleMode {
    AutoDpi { baseline_dpi: u32 },
    Fixed(Fixed),
}

impl Default for ScaleMode {
    fn default() -> Self {
        ScaleMode::AutoDpi { baseline_dpi: 96 }
    }
}

pub(crate) fn compute_scale(
    mode: ScaleMode,
    xres: u16,
    yres: u16,
    width_mm: u32,
    height_mm: u32,
) -> Fixed {
    let ScaleMode::AutoDpi { baseline_dpi } = mode else {
        let ScaleMode::Fixed(s) = mode else {
            unreachable!()
        };
        return s;
    };
    if width_mm == 0 || height_mm == 0 || baseline_dpi == 0 {
        return Fixed::ONE;
    }
    let dpi_x = (xres as u32 * 254) / (width_mm * 10);
    let dpi_y = (yres as u32 * 254) / (height_mm * 10);
    let dpi = dpi_x.max(dpi_y);
    if dpi == 0 {
        return Fixed::ONE;
    }
    let raw = Fixed::from_int(dpi as i32) / Fixed::from_int(baseline_dpi as i32);
    let quarters = (raw * Fixed::from_int(4)).to_int().clamp(4, 16);
    Fixed::from_int(quarters) / Fixed::from_int(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_falls_back_to_one_when_panel_reports_zero_mm() {
        let s = compute_scale(ScaleMode::default(), 800, 600, 0, 0);
        assert_eq!(s, Fixed::ONE);
    }

    #[test]
    fn scale_quantises_high_dpi_to_quarter_steps() {
        let s = compute_scale(ScaleMode::default(), 800, 480, 154, 86);
        assert_eq!(s, Fixed::from_f32(1.25));
    }

    #[test]
    fn scale_phone_class_panel() {
        let s = compute_scale(ScaleMode::default(), 720, 1440, 72, 145);
        assert_eq!(s, Fixed::from_f32(2.5));
    }

    #[test]
    fn scale_clamps_at_four_when_driver_reports_bogus_size() {
        let s = compute_scale(ScaleMode::default(), 800, 600, 1, 1);
        assert_eq!(s, Fixed::from_int(4));
    }

    #[test]
    fn scale_fixed_mode_passes_through_unchanged() {
        let s = compute_scale(ScaleMode::Fixed(Fixed::from_f32(2.5)), 800, 600, 154, 86);
        assert_eq!(s, Fixed::from_f32(2.5));
    }
}
