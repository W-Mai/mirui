use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use libc::{SIGINT, SIGTERM, c_int, sigaction, sigset_t};

// `'static` because `extern "C" fn` can't capture; the surface's
// `Arc<AtomicBool>` is cloned into this slot at install time.
static QUIT_FLAG: std::sync::OnceLock<Arc<AtomicBool>> = std::sync::OnceLock::new();

extern "C" fn handle_term(_sig: c_int) {
    if let Some(flag) = QUIT_FLAG.get() {
        flag.store(true, Ordering::Relaxed);
    }
}

/// Idempotent: a second call keeps the first flag (one shared shutdown
/// flag per process is fine). The handler is async-signal-safe because
/// `AtomicBool::store(Relaxed)` is one atomic instruction — no syscall
/// or allocator call.
pub(super) fn install(flag: &Arc<AtomicBool>) -> std::io::Result<()> {
    if QUIT_FLAG.set(flag.clone()).is_err() {
        return Ok(());
    }

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
