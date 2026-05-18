pub mod fps_summary;
pub mod perf_report;
#[cfg(feature = "std")]
pub mod std_clock;

pub use fps_summary::FpsSummaryPlugin;
pub use perf_report::{PerfReport, PerfReportPlugin, SystemPerfSnapshot, SystemStat};
#[cfg(feature = "std")]
pub use std_clock::StdInstantClockPlugin;
