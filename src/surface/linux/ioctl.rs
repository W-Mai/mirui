#![cfg(all(feature = "linux-fb", target_os = "linux"))]

//! Minimal binding for `linux/fb.h` ioctls — only
//! [`FBIOGET_VSCREENINFO`] and [`FBIOGET_FSCREENINFO`] are needed.
//! `repr(C)` matches the kernel layout on 64-bit Linux targets;
//! `smem_start` / `mmio_start` follow the kernel's `unsigned long`
//! width via `usize`.

use std::io;

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct FbBitfield {
    pub offset: u32,
    pub length: u32,
    pub msb_right: u32,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct FbVarScreeninfo {
    pub xres: u32,
    pub yres: u32,
    pub xres_virtual: u32,
    pub yres_virtual: u32,
    pub xoffset: u32,
    pub yoffset: u32,
    pub bits_per_pixel: u32,
    pub grayscale: u32,
    pub red: FbBitfield,
    pub green: FbBitfield,
    pub blue: FbBitfield,
    pub transp: FbBitfield,
    pub nonstd: u32,
    pub activate: u32,
    pub height: u32,
    pub width: u32,
    pub accel_flags: u32,
    pub pixclock: u32,
    pub left_margin: u32,
    pub right_margin: u32,
    pub upper_margin: u32,
    pub lower_margin: u32,
    pub hsync_len: u32,
    pub vsync_len: u32,
    pub sync: u32,
    pub vmode: u32,
    pub rotate: u32,
    pub colorspace: u32,
    pub reserved: [u32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FbFixScreeninfo {
    pub id: [u8; 16],
    pub smem_start: usize,
    pub smem_len: u32,
    pub fb_type: u32,
    pub type_aux: u32,
    pub visual: u32,
    pub xpanstep: u16,
    pub ypanstep: u16,
    pub ywrapstep: u16,
    pub line_length: u32,
    pub mmio_start: usize,
    pub mmio_len: u32,
    pub accel: u32,
    pub capabilities: u16,
    pub reserved: [u16; 2],
}

impl Default for FbFixScreeninfo {
    fn default() -> Self {
        // SAFETY: `FbFixScreeninfo` is `repr(C)` of integers + arrays
        // with no niche, so an all-zero pattern is a valid value.
        unsafe { core::mem::zeroed() }
    }
}

// `<linux/fb.h>` predates the `_IOR` / `_IOW` macro convention and
// hard-codes its requests as bare `(type << 8) | nr` numbers. Using
// `_IOR` here would yield e.g. `0x80A04601` instead of the kernel's
// `0x4600`, and the driver would reject it with `EINVAL`.
//
// `libc::Ioctl` is a platform alias — `c_ulong` on glibc, `c_int` on
// musl — pinning to it avoids a fixed integer width mismatch.
pub const FBIOGET_VSCREENINFO: libc::Ioctl = 0x4600;
pub const FBIOGET_FSCREENINFO: libc::Ioctl = 0x4602;

pub unsafe fn fbioget_vscreeninfo(fd: libc::c_int) -> io::Result<FbVarScreeninfo> {
    let mut var: FbVarScreeninfo = FbVarScreeninfo::default();
    let rc = unsafe { libc::ioctl(fd, FBIOGET_VSCREENINFO, &mut var as *mut _) };
    if rc < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(var)
}

pub unsafe fn fbioget_fscreeninfo(fd: libc::c_int) -> io::Result<FbFixScreeninfo> {
    let mut fix: FbFixScreeninfo = FbFixScreeninfo::default();
    let rc = unsafe { libc::ioctl(fd, FBIOGET_FSCREENINFO, &mut fix as *mut _) };
    if rc < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(fix)
}
