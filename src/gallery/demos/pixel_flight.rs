//! A pixel flight scene with a wrapped height field and layered aircraft.

pub mod jet;
pub mod world;

pub const VIEWPORT: (u16, u16) = (1280, 720);
pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_least(480, 320);
