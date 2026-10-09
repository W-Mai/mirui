use crate::types::Fixed;

const WINDOW_MS: u32 = 80;

#[derive(Clone, Copy, Default)]
struct Sample {
    x: Fixed,
    y: Fixed,
    time: u32,
}

#[derive(Default)]
pub(crate) struct VelocityTracker {
    samples: [Sample; 4],
    len: usize,
}

impl VelocityTracker {
    pub fn last_velocity(&self) -> Option<(Fixed, Fixed)> {
        self.velocity(self.samples[..self.len].last()?.time)
    }

    pub fn is_recent(&self, now: u32) -> bool {
        self.samples[..self.len]
            .last()
            .is_some_and(|last| now.wrapping_sub(last.time) < WINDOW_MS)
    }

    pub fn record_delta(&mut self, dx: Fixed, dy: Fixed, time: u32) {
        if let Some(origin) = self.samples[..self.len].last().copied() {
            for sample in &mut self.samples[..self.len] {
                sample.x -= origin.x;
                sample.y -= origin.y;
            }
        }
        self.record(-dx, -dy, time);
    }

    pub fn record(&mut self, x: Fixed, y: Fixed, time: u32) {
        let sample = Sample { x, y, time };
        if self.len > 0 && self.samples[self.len - 1].time == time {
            self.samples[self.len - 1] = sample;
        } else {
            if self.len == self.samples.len() {
                self.samples.rotate_left(1);
                self.len -= 1;
            }
            self.samples[self.len] = sample;
            self.len += 1;
        }
    }

    pub fn velocity(&self, now: u32) -> Option<(Fixed, Fixed)> {
        let last = self.samples[..self.len].last()?;
        if !self.is_recent(now) {
            return None;
        }
        let first = self.samples[..self.len]
            .iter()
            .find(|sample| now.wrapping_sub(sample.time) <= WINDOW_MS)
            .unwrap_or(last);
        let elapsed = last.time.wrapping_sub(first.time);
        if elapsed == 0 {
            return None;
        }
        let speed = |distance: Fixed| {
            let limit = Fixed::from_int(4000);
            let max_distance = limit * elapsed as i32 / 1000;
            distance.clamp(-max_distance, max_distance) * 1000 / elapsed as i32
        };
        Some((speed(first.x - last.x), speed(first.y - last.y)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_velocity_is_distinct_from_measured_zero() {
        let mut samples = VelocityTracker::default();
        assert_eq!(samples.velocity(0), None);
        samples.record(Fixed::ZERO, Fixed::ZERO, 0);
        assert_eq!(samples.velocity(0), None);
        samples.record(Fixed::ZERO, Fixed::ZERO, 8);
        assert_eq!(samples.velocity(8), Some((Fixed::ZERO, Fixed::ZERO)));
    }

    #[test]
    fn wheel_velocity_accumulates_batched_displacement_without_position_drift() {
        let mut samples = VelocityTracker::default();
        for time in 0..128 {
            samples.record_delta(4.into(), (-4).into(), time * 8);
            samples.record_delta(4.into(), (-4).into(), time * 8);
        }
        assert_eq!(
            samples.velocity(127 * 8),
            Some((1000.into(), (-1000).into()))
        );
        assert!(
            samples
                .samples
                .iter()
                .all(|sample| sample.x.abs() < Fixed::from_int(32))
        );
    }

    #[test]
    fn release_velocity_uses_time_and_expires_after_a_pause() {
        for interval in [4, 8, 16] {
            let mut samples = VelocityTracker::default();
            for i in 0..4 {
                samples.record(
                    Fixed::ZERO,
                    Fixed::from_int(100 - i * interval),
                    (i * interval) as u32,
                );
            }
            assert_eq!(
                samples.velocity((3 * interval) as u32).unwrap().1,
                Fixed::from_int(1000)
            );
            assert_eq!(samples.velocity((3 * interval) as u32 + WINDOW_MS), None);
        }
    }

    #[test]
    fn batched_samples_and_clock_wrap_do_not_invent_velocity() {
        let mut samples = VelocityTracker::default();
        samples.record(Fixed::ZERO, 100.into(), u32::MAX - 7);
        samples.record(Fixed::ZERO, 90.into(), u32::MAX - 7);
        assert_eq!(samples.velocity(u32::MAX - 7), None);
        samples.record(Fixed::ZERO, 82.into(), 0);
        assert_eq!(samples.velocity(0).unwrap().1, Fixed::from_int(1000));
    }
}
