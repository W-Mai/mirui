use crate::types::Fixed;

#[derive(Clone, Copy, Debug)]
pub(super) struct BrakedPhase {
    phase: Fixed,
    rate: Fixed,
}

impl Default for BrakedPhase {
    fn default() -> Self {
        Self {
            phase: Fixed::ZERO,
            rate: Fixed::ONE,
        }
    }
}

impl BrakedPhase {
    pub(super) const fn phase(self) -> Fixed {
        self.phase
    }

    #[cfg(test)]
    pub(super) const fn rate(self) -> Fixed {
        self.rate
    }

    pub(super) fn advance(
        &mut self,
        dt_ms: u16,
        ramp_ms: u16,
        speed: Fixed,
        direction: Fixed,
        paused: bool,
        period: Fixed,
    ) -> Fixed {
        let previous_rate = self.rate;
        let rate_step = Fixed::from_ratio(i32::from(dt_ms), i32::from(ramp_ms.max(1)));
        self.rate = if paused {
            (self.rate - rate_step).max(Fixed::ZERO)
        } else {
            (self.rate + rate_step).min(Fixed::ONE)
        };
        let average_rate = (previous_rate + self.rate) / 2;
        self.phase += speed * average_rate * direction * Fixed::from(i32::from(dt_ms))
            / Fixed::from_int(1_000);
        while self.phase >= period {
            self.phase -= period;
        }
        while self.phase < Fixed::ZERO {
            self.phase += period;
        }
        self.phase
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_brakes_and_resume_uses_relative_time() {
        let mut motion = BrakedPhase::default();
        let period = Fixed::from_int(360);
        let speed = Fixed::from_int(90);

        let moving = motion.advance(50, 500, speed, Fixed::ONE, false, period);
        for _ in 0..11 {
            motion.advance(50, 500, speed, Fixed::ONE, true, period);
        }
        let stopped = motion.phase();
        assert!(stopped > moving);
        assert_eq!(motion.rate(), Fixed::ZERO);
        assert_eq!(
            motion.advance(50, 500, speed, Fixed::ONE, true, period),
            stopped
        );
        assert!(motion.advance(50, 500, speed, Fixed::ONE, false, period) > stopped);
        assert!(motion.rate() < Fixed::ONE);
    }

    #[test]
    fn phase_wraps_in_both_directions() {
        let period = Fixed::from_int(360);
        let speed = Fixed::from_int(7_200);
        let mut forward = BrakedPhase::default();
        let mut reverse = BrakedPhase::default();

        let forward_phase = forward.advance(50, 1, speed, Fixed::ONE, false, period);
        let reverse_phase = reverse.advance(50, 1, speed, -Fixed::ONE, false, period);

        assert_eq!(forward_phase, Fixed::ZERO);
        assert_eq!(reverse_phase, Fixed::ZERO);
    }
}
