use super::coverage_opacity;
use crate::types::Fixed;

#[test]
fn one_pixel_stroke_preserves_integrated_coverage_when_expanded() {
    assert_eq!(
        coverage_opacity(150, Fixed::ONE, Fixed::from_ratio(3, 2)),
        100
    );
}
