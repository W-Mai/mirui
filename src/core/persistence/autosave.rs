use core::time::Duration;

/// An autosave interval supplied as a duration or an integer number of milliseconds.
///
/// Implemented for `Duration` and signed and unsigned integer types. Negative or unrepresentable intervals are rejected.
pub trait IntoDuration: sealed::Sealed {
    /// Convert the interval without truncation. Integer values are milliseconds.
    fn try_into_duration(self) -> Option<Duration>;
}

mod sealed {
    pub trait Sealed {}
}

impl sealed::Sealed for Duration {}

impl IntoDuration for Duration {
    fn try_into_duration(self) -> Option<Duration> {
        Some(self)
    }
}

macro_rules! integer_intervals {
    ($($ty:ty),+ $(,)?) => {$ (
        impl sealed::Sealed for $ty {}

        impl IntoDuration for $ty {
            fn try_into_duration(self) -> Option<Duration> {
                let milliseconds = u128::try_from(self).ok()?;
                let seconds = u64::try_from(milliseconds / 1000).ok()?;
                Some(Duration::new(seconds, ((milliseconds % 1000) as u32) * 1_000_000))
            }
        }
    )+};
}

integer_intervals!(
    u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize
);

#[derive(Default)]
pub(super) struct Autosave {
    interval: Duration,
    elapsed: Duration,
    last_ns: Option<u64>,
}

impl Autosave {
    pub(super) fn configure(&mut self, interval: impl IntoDuration) {
        let Some(interval) = interval.try_into_duration() else {
            crate::warn!(target: "mirui::persistence", "invalid autosave interval; configuration unchanged");
            return;
        };
        self.interval = interval;
        self.reset(None);
    }

    pub(super) fn enabled(&self) -> bool {
        !self.interval.is_zero()
    }

    pub(super) fn reset(&mut self, now_ns: Option<u64>) {
        self.elapsed = Duration::ZERO;
        self.last_ns = now_ns;
    }

    pub(super) fn due(&mut self, now_ns: Option<u64>) -> bool {
        let previous = core::mem::replace(&mut self.last_ns, now_ns);
        let Some((previous, now)) = previous.zip(now_ns) else {
            self.elapsed = Duration::ZERO;
            return false;
        };
        if !self.enabled() {
            return false;
        }
        self.elapsed = self
            .elapsed
            .saturating_add(Duration::from_nanos(now.wrapping_sub(previous)));
        self.elapsed >= self.interval
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_inputs_share_millisecond_semantics_without_narrowing() {
        let expected = Some(Duration::from_secs(1));
        assert_eq!(1000.try_into_duration(), expected);
        assert_eq!(1000_u64.try_into_duration(), expected);
        assert_eq!(Duration::from_secs(1).try_into_duration(), expected);
        assert_eq!((-1).try_into_duration(), None);
        assert_eq!(u128::MAX.try_into_duration(), None);
        let large = u128::from(u64::MAX) * 1000 + 999;
        assert_eq!(
            large.try_into_duration(),
            Some(Duration::new(u64::MAX, 999_000_000))
        );
        assert_eq!((large + 1).try_into_duration(), None);
    }

    #[test]
    fn interval_configuration_and_missing_clock_do_not_create_elapsed_time() {
        let mut timer = Autosave::default();
        timer.configure(10);
        timer.reset(Some(0));
        assert!(!timer.due(Some(5_000_000)));
        timer.configure(-1);
        assert!(timer.due(Some(10_000_000)));
        timer.reset(Some(10_000_000));
        assert!(!timer.due(None));
        assert!(!timer.due(Some(100_000_000)));
        assert!(timer.due(Some(110_000_000)));
        timer.configure(0);
        assert!(!timer.enabled());
        assert!(!timer.due(Some(200_000_000)));
        timer.configure(Duration::from_nanos(1));
        assert!(!timer.due(Some(300_000_000)));
        assert!(timer.due(Some(300_000_001)));
        timer.configure(Duration::ZERO);
        assert!(!timer.enabled());
    }

    #[test]
    fn nanosecond_deltas_preserve_submillisecond_and_long_intervals() {
        let mut timer = Autosave::default();
        timer.configure(Duration::from_nanos(500));
        timer.reset(Some(u64::MAX - 200));
        assert!(!timer.due(Some(98)));
        assert!(timer.due(Some(299)));

        timer.configure(Duration::from_secs(100 * 24 * 60 * 60));
        timer.reset(Some(0));
        assert!(!timer.due(Some(u64::from(u32::MAX) * 1_000_000)));
        assert!(timer.due(Some(100 * 24 * 60 * 60 * 1_000_000_000)));

        timer.configure(Duration::MAX);
        timer.reset(Some(0));
        assert!(!timer.due(Some(u64::MAX)));
        assert!(!timer.due(Some(u64::MAX - 1)));
    }
}
