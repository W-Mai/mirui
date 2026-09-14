use alloc::borrow::Cow;

use crate::types::{Color, Fixed, Point, Transform};

#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    Color(Color),
    LinearGradient(LinearGradient),
    RadialGradient(RadialGradient),
}

#[derive(Clone, Debug, PartialEq)]
pub struct LinearGradient {
    pub start: Point,
    pub end: Point,
    pub stops: Cow<'static, [GradientStop]>,
    pub spread: SpreadMode,
    pub units: GradientUnits,
    pub transform: Transform,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RadialGradient {
    pub center: Point,
    pub radius: Fixed,
    pub focal: Point,
    pub focal_radius: Fixed,
    pub stops: Cow<'static, [GradientStop]>,
    pub spread: SpreadMode,
    pub units: GradientUnits,
    pub transform: Transform,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GradientStop {
    pub offset: Fixed,
    pub color: Color,
}

impl GradientStop {
    pub(crate) fn offset_follows(previous: Fixed, offset: Fixed) -> bool {
        offset >= previous && offset <= Fixed::ONE
    }

    /// Checks a nonempty stop sequence for ascending offsets within 0..=1.
    pub fn sequence_is_valid(stops: &[Self]) -> bool {
        let mut previous = Fixed::ZERO;
        !stops.is_empty()
            && stops.iter().all(|stop| {
                let valid = Self::offset_follows(previous, stop.offset);
                previous = stop.offset;
                valid
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpreadMode {
    Pad,
    Reflect,
    Repeat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientUnits {
    UserSpaceOnUse,
    ObjectBoundingBox,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_sequence_requires_bounded_ascending_offsets() {
        let stop = |offset| GradientStop {
            offset,
            color: Color::rgba(0, 0, 0, 255),
        };
        let negative = Fixed::from_int(-1);
        let too_large = Fixed::from_int(2);
        assert!(!GradientStop::sequence_is_valid(&[]));
        assert!(GradientStop::sequence_is_valid(&[
            stop(Fixed::ZERO),
            stop(Fixed::ONE),
        ]));
        assert!(GradientStop::sequence_is_valid(&[
            stop(Fixed::ZERO),
            stop(Fixed::ZERO),
        ]));
        assert!(!GradientStop::sequence_is_valid(&[
            stop(Fixed::ONE),
            stop(Fixed::ZERO),
        ]));
        assert!(!GradientStop::sequence_is_valid(&[stop(negative)]));
        assert!(!GradientStop::sequence_is_valid(&[stop(too_large)]));
    }
}
