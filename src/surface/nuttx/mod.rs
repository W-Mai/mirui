#![cfg(all(feature = "nuttx", target_os = "nuttx"))]

// NuttX RTOS backend. Shaped like `crate::surface::linux` but the NuttX
// kernel API differs from Linux fbdev / evdev in struct layout and IOCTL
// numbers, so the device structs and constants are not shared.

mod fb;
mod ioctl;
