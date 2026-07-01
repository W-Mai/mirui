#[cfg(feature = "std")]
pub mod ringbuf;
#[cfg(feature = "std")]
pub mod stderr;

#[cfg(feature = "std")]
pub use ringbuf::{LoggedRecord, RingBufferHandle, RingBufferSink};
#[cfg(feature = "std")]
pub use stderr::StderrSink;
