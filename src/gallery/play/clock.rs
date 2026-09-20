#[derive(Clone, Copy, Debug)]
pub(crate) struct BoundedClock {
    carry_us: u32,
    step_us: u32,
    max_elapsed_ms: u16,
    max_steps: u8,
}

impl BoundedClock {
    pub(crate) const fn new(step_hz: u16, max_elapsed_ms: u16, max_steps: u8) -> Self {
        assert!(step_hz > 0);
        assert!(max_steps > 0);
        Self {
            carry_us: 0,
            step_us: 1_000_000 / step_hz as u32,
            max_elapsed_ms,
            max_steps,
        }
    }

    pub(crate) fn steps(&mut self, elapsed_ms: u16, active: bool) -> u8 {
        if !active {
            self.carry_us = 0;
            return 0;
        }
        let elapsed_us = u32::from(elapsed_ms.min(self.max_elapsed_ms)) * 1_000;
        self.carry_us = self.carry_us.saturating_add(elapsed_us);
        let steps = (self.carry_us / self.step_us).min(u32::from(self.max_steps)) as u8;
        self.carry_us -= u32::from(steps) * self.step_us;
        if steps == self.max_steps {
            self.carry_us = self.carry_us.min(self.step_us - 1);
        }
        steps
    }

    pub(crate) fn reset(&mut self) {
        self.carry_us = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractional_time_survives_between_frames() {
        let mut clock = BoundedClock::new(100, 60, 8);
        assert_eq!(clock.steps(6, true), 0);
        assert_eq!(clock.steps(6, true), 1);
        assert_eq!(clock.steps(8, true), 1);
    }

    #[test]
    fn catch_up_is_bounded_and_drops_excess_backlog() {
        let mut clock = BoundedClock::new(120, 60, 8);
        assert_eq!(clock.steps(u16::MAX, true), 7);
        assert!(clock.carry_us < clock.step_us);
    }

    #[test]
    fn inactive_frames_do_not_accumulate_debt() {
        let mut clock = BoundedClock::new(100, 60, 8);
        assert_eq!(clock.steps(9, true), 0);
        assert_eq!(clock.steps(20, false), 0);
        assert_eq!(clock.steps(1, true), 0);
    }
}
