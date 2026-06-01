pub mod budget;
pub mod cache_report;
pub mod fps_summary;
pub mod frame_rate;
pub mod input_feedback;
pub mod perf_report;
#[cfg(feature = "std")]
pub mod std_clock;

pub use budget::{BudgetReportPlugin, BudgetViolation};
pub use cache_report::{CacheReport, CacheReportPlugin};
pub use fps_summary::{FpsSummary, FpsSummaryPlugin};
pub use frame_rate::FrameRateCapPlugin;
pub use input_feedback::InputFeedbackPlugin;
pub use perf_report::{
    PerfReport, PerfReportPlugin, PerfettoLineSink, SystemPerfSnapshot, SystemStat,
};
#[cfg(feature = "std")]
pub use std_clock::StdInstantClockPlugin;
