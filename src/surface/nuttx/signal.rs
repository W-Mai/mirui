use std::sync::atomic::{AtomicBool, Ordering};

use libc::{SIGINT, SIGTERM, c_int, sigaction, sigset_t};

pub(super) static QUIT_FLAG: AtomicBool = AtomicBool::new(false);

extern "C" fn handle_term(_sig: c_int) {
    QUIT_FLAG.store(true, Ordering::Relaxed);
}

/// Idempotent: re-installing is harmless. Async-signal-safe: the handler
/// only does an atomic store on a `'static AtomicBool` — no allocator,
/// syscall, or lock.
pub(super) fn install() -> std::io::Result<()> {
    // NuttX libc's `sigaction` has a private `__reserved` padding field,
    // so `..core::mem::zeroed()` struct-update syntax is rejected.
    // SAFETY: zeroed all-bits-zero is a valid `sigaction` value.
    let mut sa: sigaction = unsafe { core::mem::zeroed() };
    // SAFETY: `sigemptyset` writes into a caller-allocated `sigset_t`.
    unsafe {
        libc::sigemptyset(&mut sa.sa_mask);
    }
    sa.sa_handler = handle_term as *const () as usize;
    sa.sa_flags = 0;
    // SAFETY: `sa` is caller-allocated; both `signum` are valid POSIX
    // signal numbers; null third arg means "don't return previous
    // handler".
    let r = unsafe { sigaction(SIGTERM, &sa, core::ptr::null_mut()) };
    if r < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: see above.
    let r = unsafe { sigaction(SIGINT, &sa, core::ptr::null_mut()) };
    if r < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}
