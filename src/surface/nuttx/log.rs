//! NuttX `syslog(3)` wrapper. mirui's NuttX backend never uses
//! `eprintln!` / `println!` because libstd's stderr/stdout writes go
//! to `/dev/console` in **blocking** mode. On boards where the
//! console is USB-Serial/JTAG (most ESP32-C3 modules) the host PC
//! must drain the CDC endpoint actively or NuttX's TX ring fills,
//! `write()` blocks on the TX semaphore, and the calling task hangs
//! — looking exactly like mirui crashed when in fact it's stuck in
//! a syscall waiting for the host to read.
//!
//! `syslog(3)` goes through `CONFIG_SYSLOG_*` channels (ramlog,
//! console, file, …). With ramlog enabled it's lossy-but-non-blocking:
//! a full ring overwrites the oldest entry instead of stalling.

use core::ffi::{c_char, c_int};

#[doc(hidden)]
pub const _LOG_ERR: c_int = 3;
#[doc(hidden)]
pub const _LOG_WARNING: c_int = 4;
#[doc(hidden)]
pub const _LOG_INFO: c_int = 6;
#[doc(hidden)]
pub const _LOG_DEBUG: c_int = 7;

unsafe extern "C" {
    fn syslog(priority: c_int, fmt: *const c_char, ...);
}

#[doc(hidden)]
pub fn _log_str(priority: c_int, msg: &str) {
    let mut buf = [0u8; 256];
    let n = msg.len().min(buf.len() - 1);
    buf[..n].copy_from_slice(&msg.as_bytes()[..n]);
    buf[n] = 0;
    let fmt = b"%s\n\0".as_ptr() as *const c_char;
    // SAFETY: `fmt` is a valid C string literal, `buf` is NUL-terminated
    // within its 256-byte stack lifetime, syslog reads through the ptr
    // before returning.
    unsafe {
        syslog(priority, fmt, buf.as_ptr());
    }
}

#[macro_export]
#[doc(hidden)]
macro_rules! __mirui_nuttx_error {
    ($($arg:tt)*) => {{
        let msg = ::alloc::format!($($arg)*);
        $crate::surface::nuttx::log::_log_str($crate::surface::nuttx::log::_LOG_ERR, &msg);
    }};
}

#[macro_export]
#[doc(hidden)]
macro_rules! __mirui_nuttx_warn {
    ($($arg:tt)*) => {{
        let msg = ::alloc::format!($($arg)*);
        $crate::surface::nuttx::log::_log_str($crate::surface::nuttx::log::_LOG_WARNING, &msg);
    }};
}

#[macro_export]
#[doc(hidden)]
macro_rules! __mirui_nuttx_info {
    ($($arg:tt)*) => {{
        let msg = ::alloc::format!($($arg)*);
        $crate::surface::nuttx::log::_log_str($crate::surface::nuttx::log::_LOG_INFO, &msg);
    }};
}

#[macro_export]
#[doc(hidden)]
macro_rules! __mirui_nuttx_debug {
    ($($arg:tt)*) => {{
        let msg = ::alloc::format!($($arg)*);
        $crate::surface::nuttx::log::_log_str($crate::surface::nuttx::log::_LOG_DEBUG, &msg);
    }};
}

#[allow(unused_imports)]
pub(super) use crate::{
    __mirui_nuttx_debug as debug, __mirui_nuttx_error as error, __mirui_nuttx_info as info,
    __mirui_nuttx_warn as warn,
};
