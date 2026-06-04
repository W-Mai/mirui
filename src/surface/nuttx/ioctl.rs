// Constants are NuttX header values: `<nuttx/video/fb.h>`,
// `<nuttx/input/{touchscreen,keyboard}.h>`, `<nuttx/fs/ioctl.h>`.
use core::ffi::{c_int, c_void};

pub(super) type FbCoord = u16;

#[repr(C)]
pub(super) struct FbVideoInfo {
    pub fmt: u8,
    pub xres: FbCoord,
    pub yres: FbCoord,
    pub nplanes: u8,
}

// `fb_videoinfo_s` with `CONFIG_FB_OVERLAY` / `CONFIG_FB_MODULEINFO` off;
// either on appends fields and trips the size assert.
const _: () = {
    assert!(core::mem::size_of::<FbVideoInfo>() == 8);
};

#[repr(C)]
pub(super) struct FbPlaneInfo {
    pub fbmem: *mut c_void,
    pub fblen: usize,
    pub stride: FbCoord,
    pub display: u8,
    pub bpp: u8,
    pub xres_virtual: u32,
    pub yres_virtual: u32,
    pub xoffset: u32,
    pub yoffset: u32,
}

#[repr(C)]
pub(super) struct FbArea {
    pub x: FbCoord,
    pub y: FbCoord,
    pub w: FbCoord,
    pub h: FbCoord,
}

const _: () = {
    assert!(core::mem::size_of::<FbArea>() == 8);
};

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct TouchPoint {
    pub id: u8,
    pub flags: u8,
    pub x: i16,
    pub y: i16,
    pub h: i16,
    pub w: i16,
    pub gesture: u16,
    pub pressure: u16,
    pub timestamp: u64,
}

const _: () = {
    assert!(core::mem::size_of::<TouchPoint>() == 24);
};

#[repr(C)]
pub(super) struct KeyboardEvent {
    pub event_type: u32,
    pub code: u32,
}

const _: () = {
    assert!(core::mem::size_of::<KeyboardEvent>() == 8);
};

// NuttX IOCTL encoding is `type | nr`, no Linux direction/size bits.
pub(super) const FBIOC_BASE: c_int = 0x2800;
pub(super) const FBIOGET_VIDEOINFO: c_int = FBIOC_BASE | 0x01;
pub(super) const FBIOGET_PLANEINFO: c_int = FBIOC_BASE | 0x02;
pub(super) const FBIO_UPDATE: c_int = FBIOC_BASE | 0x07;
pub(super) const FBIO_WAITFORVSYNC: c_int = FBIOC_BASE | 0x08;
pub(super) const FBIOSET_POWER: c_int = FBIOC_BASE | 0x14;
pub(super) const FBIOGET_POWER: c_int = FBIOC_BASE | 0x15;
pub(super) const FBIOPAN_DISPLAY: c_int = FBIOC_BASE | 0x18;

pub(super) const TSIOC_BASE: c_int = 0x0900;
pub(super) const TSIOC_GRAB: c_int = TSIOC_BASE | 0x0e;

pub(super) const FB_FMT_RGB16_565: u8 = 11;
pub(super) const FB_FMT_RGB24: u8 = 12;
pub(super) const FB_FMT_RGB32: u8 = 13;
pub(super) const FB_FMT_RGBA32: u8 = 21;

pub(super) const TOUCH_DOWN: u8 = 1;
pub(super) const TOUCH_MOVE: u8 = 2;
pub(super) const TOUCH_UP: u8 = 4;
pub(super) const TOUCH_ID_VALID: u8 = 8;
pub(super) const TOUCH_POS_VALID: u8 = 16;

pub(super) const KEYBOARD_PRESS: u32 = 0;
pub(super) const KEYBOARD_RELEASE: u32 = 1;

// 5 contacts = pinch + rotate; bigger boards rarely need more.
pub(super) const MAX_TOUCH_POINTS: usize = 5;

// touch_sample_s: 4-byte `npoints` + 4 pad aligning `point[0]`'s u64 to 8.
pub(super) const TOUCH_SAMPLE_HEADER: usize = 8;
