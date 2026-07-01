//! Single-source clock plumbing.
//!
//! One process, one epoch, one fn-ptr hook, one read entry. Every other
//! subsystem — `core::perf`, `core::log`, `ecs::MonoClock`,
//! `App::clock_ns` — delegates here. std auto-anchors on first read
//! (uptime); no_std needs `set_clock(f)` from a board plugin.

#[cfg(feature = "std")]
mod imp {
    use std::sync::OnceLock;
    use web_time::Instant;

    static EPOCH: OnceLock<Instant> = OnceLock::new();

    pub fn try_clock_now_ns() -> Option<u64> {
        if super::mock::is_installed() {
            return Some(super::mock::read_ns());
        }
        Some(EPOCH.get_or_init(Instant::now).elapsed().as_nanos() as u64)
    }

    pub fn is_clock_installed() -> bool {
        true
    }

    /// API symmetry with the no_std imp; std auto-anchors.
    pub fn set_clock(_f: fn() -> u64) {}
}

#[cfg(not(feature = "std"))]
mod imp {
    struct State {
        clock: usize,
    }

    static mut STATE: State = State { clock: 0 };

    fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
        critical_section::with(|_| {
            #[allow(static_mut_refs)]
            unsafe {
                f(&mut STATE)
            }
        })
    }

    pub fn set_clock(f: fn() -> u64) {
        with_state(|s| s.clock = f as usize);
    }

    fn read_clock() -> Option<fn() -> u64> {
        let raw = with_state(|s| s.clock);
        if raw == 0 {
            None
        } else {
            // SAFETY: only `set_clock` writes this slot, only with a
            // valid `fn() -> u64` cast to usize.
            Some(unsafe { core::mem::transmute::<usize, fn() -> u64>(raw) })
        }
    }

    pub fn try_clock_now_ns() -> Option<u64> {
        read_clock().map(|f| f())
    }

    pub fn is_clock_installed() -> bool {
        with_state(|s| s.clock) != 0
    }
}

pub use imp::{is_clock_installed, set_clock, try_clock_now_ns};

/// Returns 0 both before the first tick and when no clock is installed;
/// disambiguate with `is_clock_installed`.
pub fn clock_now_ns() -> u64 {
    try_clock_now_ns().unwrap_or(0)
}

#[cfg(any(test, feature = "std"))]
pub mod mock {
    use core::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, MutexGuard};

    static MOCK_NS: Mutex<u64> = Mutex::new(0);
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    static SERIAL: Mutex<()> = Mutex::new(());

    pub(super) fn is_installed() -> bool {
        INSTALLED.load(Ordering::Acquire)
    }

    pub(super) fn read_ns() -> u64 {
        *MOCK_NS.lock().expect("mock clock poisoned")
    }

    pub fn set_ns(ns: u64) {
        *MOCK_NS.lock().expect("mock clock poisoned") = ns;
    }

    pub fn set_ms(ms: u64) {
        set_ns(ms.saturating_mul(1_000_000));
    }

    pub fn advance_ms(ms: u64) {
        let mut guard = MOCK_NS.lock().expect("mock clock poisoned");
        *guard = guard.saturating_add(ms.saturating_mul(1_000_000));
    }

    /// Serializes tests against a shared static mutex and routes
    /// `clock_now_ns` reads to the mock buffer for the guard's
    /// lifetime. Drop releases both.
    pub fn install() -> MockHandle {
        let serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        INSTALLED.store(true, Ordering::Release);
        MockHandle { _serial: serial }
    }

    pub struct MockHandle {
        _serial: MutexGuard<'static, ()>,
    }

    impl Drop for MockHandle {
        fn drop(&mut self) {
            INSTALLED.store(false, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "std")]
    #[test]
    fn std_monotonic_across_calls() {
        let a = clock_now_ns();
        let b = clock_now_ns();
        let c = clock_now_ns();
        assert!(b >= a, "b={b} a={a}");
        assert!(c >= b, "c={c} b={b}");
        assert!(is_clock_installed());
    }

    #[cfg(feature = "std")]
    #[test]
    fn mock_overrides_when_installed() {
        let _guard = mock::install();
        mock::set_ns(42);
        assert_eq!(clock_now_ns(), 42);
        mock::advance_ms(1);
        assert_eq!(clock_now_ns(), 42 + 1_000_000);
    }

    #[cfg(feature = "std")]
    #[test]
    fn mock_uninstall_restores_real_clock() {
        {
            let _guard = mock::install();
            mock::set_ns(123);
            assert_eq!(clock_now_ns(), 123);
        }
        let real = clock_now_ns();
        assert!(real > 0);
        assert_ne!(real, 123);
    }
}
