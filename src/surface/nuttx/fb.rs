use core::ffi::c_int;
use std::ffi::CString;
use std::io;

use libc::{O_RDWR, close, ioctl, open};

use super::ioctl::*;
use crate::render::texture::ColorFormat;

pub(super) struct FbDevice {
    fd: c_int,
    pub fbmem: *mut u8,
    pub fblen: usize,
    pub xres: u16,
    pub yres: u16,
    pub stride: usize,
    pub format: ColorFormat,
    pub supports_update: bool,
    pub supports_vsync: bool,
    pub use_paninfo: bool,
    pan_template: FbPlaneInfo,
}

// SAFETY: `fbmem` is a process-local pointer to driver-managed memory; the
// `FbDevice` owns the fd that keeps it valid until `Drop`. mirui's tick
// loop is single-threaded so no cross-thread aliasing happens.
unsafe impl Send for FbDevice {}

impl FbDevice {
    pub(super) fn open(path: &str, use_paninfo: Option<bool>) -> io::Result<Self> {
        let cpath = CString::new(path)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "fb_path contains NUL"))?;
        // SAFETY: `cpath` is a valid NUL-terminated C string for the duration
        // of this call.
        let fd = unsafe { open(cpath.as_ptr(), O_RDWR) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }

        let mut vi = FbVideoInfo {
            fmt: 0,
            xres: 0,
            yres: 0,
            nplanes: 0,
        };
        // SAFETY: kernel writes into `vi` (caller-allocated, sized per
        // FbVideoInfo's compile-time layout assertion).
        let r = unsafe { ioctl(fd, FBIOGET_VIDEOINFO as _, &mut vi as *mut _) };
        if r < 0 {
            let err = io::Error::last_os_error();
            // SAFETY: `fd` came from `open` above and hasn't been closed.
            unsafe { close(fd) };
            return Err(err);
        }

        let mut pi = FbPlaneInfo {
            fbmem: core::ptr::null_mut(),
            fblen: 0,
            stride: 0,
            display: 0,
            bpp: 0,
            xres_virtual: 0,
            yres_virtual: 0,
            xoffset: 0,
            yoffset: 0,
        };
        // SAFETY: kernel writes into `pi`; same caller-allocated discipline.
        let r = unsafe { ioctl(fd, FBIOGET_PLANEINFO as _, &mut pi as *mut _) };
        if r < 0 {
            let err = io::Error::last_os_error();
            // SAFETY: see above.
            unsafe { close(fd) };
            return Err(err);
        }

        if pi.fbmem.is_null() || pi.fblen == 0 {
            // SAFETY: see above.
            unsafe { close(fd) };
            return Err(io::Error::other("FBIOGET_PLANEINFO returned null fbmem"));
        }

        let format = format_from_videoinfo(&vi).ok_or_else(|| {
            // SAFETY: see above.
            unsafe { close(fd) };
            io::Error::new(
                io::ErrorKind::Unsupported,
                alloc::format!("unsupported FB_FMT_{}", vi.fmt),
            )
        })?;

        // These ioctls are `CONFIG_FB_UPDATE` / `CONFIG_FB_SYNC` gated; probe
        // because ENOTTY means the kernel didn't compile them in.
        let supports_update = probe_update(fd);
        let supports_vsync = probe_vsync(fd);

        // ≥32bpp ⇒ virtio/DRM fb (PAN), narrower ⇒ SPI LCD — see `NuttxConfig::use_paninfo`.
        let use_paninfo = use_paninfo.unwrap_or(format.bytes_per_pixel() >= 4);

        Ok(Self {
            fd,
            fbmem: pi.fbmem as *mut u8,
            fblen: pi.fblen,
            xres: vi.xres,
            yres: vi.yres,
            stride: pi.stride as usize,
            format,
            supports_update,
            supports_vsync,
            use_paninfo,
            pan_template: pi,
        })
    }

    pub(super) fn flush(&mut self, x: u16, y: u16, w: u16, h: u16) {
        if self.supports_update {
            let area = FbArea { x, y, w, h };
            // SAFETY: `area` is a caller-allocated struct of the layout the
            // kernel expects; ioctl reads through the const pointer.
            unsafe {
                ioctl(self.fd, FBIO_UPDATE as _, &area as *const _);
            }
        }
    }

    pub(super) fn pan(&mut self) {
        if !self.use_paninfo {
            return;
        }
        self.pan_template.yoffset = 0;
        // SAFETY: `pan_template` is a caller-allocated `fb_planeinfo_s`;
        // the kernel reads from the const pointer.
        unsafe {
            ioctl(
                self.fd,
                FBIOPAN_DISPLAY as _,
                &self.pan_template as *const _,
            );
        }
    }

    pub(super) fn wait_vsync(&self) {
        if !self.supports_vsync {
            return;
        }
        // SAFETY: ioctl with no arg. Return value ignored — vsync wait
        // failure on a one-frame slip is preferable to deadlock.
        unsafe {
            ioctl(self.fd, FBIO_WAITFORVSYNC as _);
        }
    }
}

impl Drop for FbDevice {
    fn drop(&mut self) {
        // SAFETY: `fd` was opened by `open()` above and is closed exactly
        // once here. mirui never hands the FbDevice to another thread so
        // there's no use-after-close window.
        unsafe { close(self.fd) };
    }
}

fn format_from_videoinfo(vi: &FbVideoInfo) -> Option<ColorFormat> {
    match vi.fmt {
        // SPI/MIPI-DBI panels clock MSB-first, so on a LE host fb mem is
        // byte-swapped vs the natural `(lo, hi)` u16 split.
        FB_FMT_RGB16_565 => Some(ColorFormat::RGB565Swapped),
        FB_FMT_RGB24 => Some(ColorFormat::RGB888),
        // NuttX names it RGB32 but byte 0 is B (like DRM XRGB8888 / fbdev BGRX).
        FB_FMT_RGB32 => Some(ColorFormat::BGRA8888),
        FB_FMT_RGBA32 => Some(ColorFormat::RGBA8888),
        _ => None,
    }
}

fn probe_update(fd: c_int) -> bool {
    let area = FbArea {
        x: 0,
        y: 0,
        w: 1,
        h: 1,
    };
    // SAFETY: caller-allocated area struct, kernel only reads through the
    // const pointer.
    let r = unsafe { ioctl(fd, FBIO_UPDATE as _, &area as *const _) };
    r >= 0
}

fn probe_vsync(fd: c_int) -> bool {
    // SAFETY: ioctl with no arg.
    let r = unsafe { ioctl(fd, FBIO_WAITFORVSYNC as _) };
    r >= 0
}
