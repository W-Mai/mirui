use core::task::Waker;

/// Short-lived access to a loader's token, wake notification and execution allowance.
pub struct LoadContext<'a> {
    token: &'a str,
    waker: &'a Waker,
    remaining: usize,
}

impl<'a> LoadContext<'a> {
    pub(super) fn new(token: &'a str, waker: &'a Waker, steps: usize) -> Self {
        Self {
            token,
            waker,
            remaining: steps,
        }
    }

    pub fn token(&self) -> &str {
        self.token
    }
    pub fn waker(&self) -> &Waker {
        self.waker
    }

    /// Charge one bounded decoder operation. On exhaustion, schedule another poll and yield.
    pub fn consume_work(&mut self) -> bool {
        if self.remaining == 0 {
            self.waker.wake_by_ref();
            false
        } else {
            self.remaining -= 1;
            true
        }
    }
}
