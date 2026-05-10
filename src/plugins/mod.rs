pub mod fps_summary;
#[cfg(feature = "std")]
pub mod std_clock;

pub use fps_summary::FpsSummaryPlugin;
#[cfg(feature = "std")]
pub use std_clock::StdInstantClockPlugin;
