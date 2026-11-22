use super::Daylight;
use mirui::types::Fixed;

pub(crate) fn aircraft_lights(axes: [[Fixed; 3]; 3], sun: [Fixed; 2], day: Daylight) -> [usize; 2] {
    [-Fixed::ONE, Fixed::ONE].map(|side| {
        let n = core::array::from_fn::<_, 3, _>(|i| {
            axes[2][i] + side * Fixed::from_ratio(4, 10) * axes[0][i]
        });
        let dot = -n[0] * sun[0] - n[1] * sun[1] + n[2] * Fixed::from_ratio(11, 10);
        let norm = ((sun[0] * sun[0] + sun[1] * sun[1] + Fixed::from_ratio(121, 100))
            * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]))
            .sqrt();
        let value = dot / norm;
        let (low, high) = if day == Daylight::Night {
            (Fixed::from_ratio(4, 10), Fixed::from_ratio(74, 100))
        } else {
            (Fixed::from_ratio(66, 100), Fixed::from_ratio(9, 10))
        };
        if value > high {
            1
        } else if value < low {
            2
        } else {
            0
        }
    })
}
