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

    pub fn velocity(&self, now: u32) -> (Fixed, Fixed) {
        let Some(last) = self.samples[..self.len].last() else {
            return (Fixed::ZERO, Fixed::ZERO);
        };
        if now.wrapping_sub(last.time) >= WINDOW_MS {
            return (Fixed::ZERO, Fixed::ZERO);
        }
        let first = self.samples[..self.len]
            .iter()
            .find(|sample| now.wrapping_sub(sample.time) <= WINDOW_MS)
            .unwrap_or(last);
        let elapsed = last.time.wrapping_sub(first.time);
        if elapsed == 0 {
            return (Fixed::ZERO, Fixed::ZERO);
        }
        let speed = |distance: Fixed| {
            let limit = Fixed::from_int(4000);
            let max_distance = limit * elapsed as i32 / 1000;
            distance.clamp(-max_distance, max_distance) * 1000 / elapsed as i32
        };
        (speed(first.x - last.x), speed(first.y - last.y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                samples.velocity((3 * interval) as u32).1,
                Fixed::from_int(1000)
            );
            assert_eq!(
                samples.velocity((3 * interval) as u32 + WINDOW_MS).1,
                Fixed::ZERO
            );
        }
    }

    #[test]
    fn batched_samples_and_clock_wrap_do_not_invent_velocity() {
        let mut samples = VelocityTracker::default();
        samples.record(Fixed::ZERO, 100.into(), u32::MAX - 7);
        samples.record(Fixed::ZERO, 90.into(), u32::MAX - 7);
        assert_eq!(samples.velocity(u32::MAX - 7), (Fixed::ZERO, Fixed::ZERO));
        samples.record(Fixed::ZERO, 82.into(), 0);
        assert_eq!(samples.velocity(0).1, Fixed::from_int(1000));
    }
}
