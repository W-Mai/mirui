//! Byte sources with explicit access, consistency and asynchronous ownership.

use crate::core::threading::{Share, Transfer};
use alloc::borrow::Cow;
use alloc::boxed::Box;
use core::future::{Future, Ready, ready};

/// Failure to open or read a byte source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetError {
    NotFound,
    Unsupported,
    OutOfBounds,
    Capacity,
    InvalidSource,
    NoProgress,
    Truncated,
    Io(&'static str),
}

/// Positioning supported by one opened object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceAccess {
    Random,
    Sequential,
}

/// Content consistency across reads of one opened object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceConsistency {
    /// All reads refer to the same immutable content version.
    Stable,
    /// The source cannot guarantee a consistent version across reads.
    Unverified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceInfo {
    /// `None` means unknown, not an empty object.
    pub len: Option<u64>,
    pub access: SourceAccess,
    pub consistency: SourceConsistency,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadPosition {
    At(u64),
    Next,
}

/// An exclusive fixed-address buffer issued by resource storage.
///
/// Moving this value does not move its bytes. A read future owns the buffer until completion, including when its logical consumer is cancelled. Providers must stop hardware access before dropping a buffer, or transfer ownership to their physical completion record. The buffer has byte alignment; devices needing stronger alignment must use their own admitted staging.
pub struct AssetBuffer {
    pub(super) bytes: Box<[u8]>,
    pub(super) start: usize,
    pub(super) end: usize,
}

impl AssetBuffer {
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.bytes[self.start..self.end]
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.bytes[self.start..self.end]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadProgress {
    /// Number of valid bytes written into the returned buffer.
    pub filled: usize,
    /// Whether the provider has established the end of the source.
    pub eof: bool,
}

/// Completion always returns buffer ownership, including on failure.
pub struct ReadComplete {
    pub buffer: AssetBuffer,
    pub result: Result<ReadProgress, AssetError>,
}

/// Opens one long-lived byte object. Keys belong to the provider's namespace.
///
/// Futures must make bounded, non-blocking progress when polled. Blocking files or decoders require a separate execution facility; wrapping a blocking call in an async block does not satisfy this contract.
/// With shared resource storage enabled, providers and readers must be Send + Sync, while operation futures only need Send. Polling a session still requires exclusive access.
pub trait AssetProvider: Share + 'static {
    type Reader: AssetReader;
    type Open: Future<Output = Result<Self::Reader, AssetError>> + Transfer + 'static;

    fn open(&self, key: Cow<'static, str>) -> Self::Open;
}

/// Reads one opened object without reopening its key for each range.
///
/// `Random` readers accept `At`; `Sequential` readers accept `Next` and are advanced serially. Short reads are valid. A nonempty read returning zero bytes without EOF is an error, not a pending operation. Delayed operations return `Poll::Pending` and register the supplied waker before checking completion again.
///
/// Read futures must retain their session and buffer without borrowing the temporary reader reference. Logical cancellation does not prove that physical I/O has stopped. Dropping a future must remain memory-safe while hardware or workers may still be completing it.
pub trait AssetReader: Share + 'static {
    type Read: Future<Output = ReadComplete> + Transfer + 'static;

    fn info(&self) -> SourceInfo;
    fn read(&self, position: ReadPosition, buffer: AssetBuffer) -> Self::Read;

    /// Borrow the entire immutable object, with a length matching `info().len`.
    ///
    /// The slice is tied to the reader, not to cache-manager storage. Returning `None` means reads are required, not that the object is empty.
    fn mapped(&self) -> Option<&[u8]> {
        None
    }
}

impl<R, F, O> AssetProvider for F
where
    R: AssetReader,
    F: Fn(Cow<'static, str>) -> O + Share + 'static,
    O: Future<Output = Result<R, AssetError>> + Transfer + 'static,
{
    type Reader = R;
    type Open = O;

    fn open(&self, key: Cow<'static, str>) -> Self::Open {
        self(key)
    }
}

impl AssetReader for &'static [u8] {
    type Read = Ready<ReadComplete>;

    fn info(&self) -> SourceInfo {
        SourceInfo {
            len: Some(self.len() as u64),
            access: SourceAccess::Random,
            consistency: SourceConsistency::Stable,
        }
    }

    fn mapped(&self) -> Option<&[u8]> {
        Some(self)
    }

    fn read(&self, position: ReadPosition, mut buffer: AssetBuffer) -> Self::Read {
        let result = (|| {
            let ReadPosition::At(offset) = position else {
                return Err(AssetError::Unsupported);
            };
            let offset = usize::try_from(offset).map_err(|_| AssetError::OutOfBounds)?;
            let remaining = self.get(offset..).ok_or(AssetError::OutOfBounds)?;
            let filled = remaining.len().min(buffer.len());
            buffer.as_mut_slice()[..filled].copy_from_slice(&remaining[..filled]);
            Ok(ReadProgress {
                filled,
                eof: filled == remaining.len(),
            })
        })();
        ready(ReadComplete { buffer, result })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::source_cells::{Cell, Rc, RefCell};
    use alloc::vec;
    use core::pin::pin;
    use core::task::{Context, Poll, Waker};

    fn buffer(len: usize) -> AssetBuffer {
        AssetBuffer {
            bytes: vec![0; len].into_boxed_slice(),
            start: 0,
            end: len,
        }
    }

    fn complete(reader: &'static [u8], offset: ReadPosition, buffer: AssetBuffer) -> ReadComplete {
        let mut read = pin!(reader.read(offset, buffer));
        let Poll::Ready(result) = read.as_mut().poll(&mut Context::from_waker(Waker::noop()))
        else {
            panic!("static source cannot suspend");
        };
        result
    }

    #[test]
    fn mapped_source_and_owned_reads_preserve_storage_and_boundaries() {
        let source: &'static [u8] = b"abcdef";
        assert_eq!(source.mapped().unwrap().as_ptr(), source.as_ptr());
        assert_eq!(source.info().len, Some(6));
        let buffer = buffer(4);
        let address = buffer.as_slice().as_ptr();
        let mut result = None;
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                result = Some(complete(source, ReadPosition::At(3), buffer));
            }),
            0
        );
        let result = result.unwrap();
        assert_eq!(
            result.result,
            Ok(ReadProgress {
                filled: 3,
                eof: true
            })
        );
        assert_eq!(result.buffer.as_slice().as_ptr(), address);
        assert_eq!(&result.buffer.as_slice()[..3], b"def");
        let result = complete(source, ReadPosition::At(0), result.buffer);
        assert_eq!(
            result.result,
            Ok(ReadProgress {
                filled: 4,
                eof: false
            })
        );
        let result = complete(source, ReadPosition::At(u64::MAX), result.buffer);
        assert_eq!(result.result, Err(AssetError::OutOfBounds));
        assert_eq!(result.buffer.as_slice().as_ptr(), address);
        let result = complete(source, ReadPosition::Next, result.buffer);
        assert_eq!(result.result, Err(AssetError::Unsupported));
        let result = complete(source, ReadPosition::At(6), result.buffer);
        assert_eq!(
            result.result,
            Ok(ReadProgress {
                filled: 0,
                eof: true
            })
        );
    }

    #[test]
    fn a_read_can_outlive_the_provider_and_keep_the_same_buffer() {
        fn assert_send<T: Send>() {}
        assert_send::<AssetBuffer>();
        let provider = |key: Cow<'static, str>| {
            ready(match key.as_ref() {
                "asset" => Ok(b"abc".as_slice()),
                _ => Err(AssetError::NotFound),
            })
        };
        let mut opening = pin!(provider.open("asset".into()));
        let Poll::Ready(Ok(reader)) = opening
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        else {
            panic!("immediate open");
        };
        let buffer = buffer(3);
        let address = buffer.as_slice().as_ptr();
        let result = complete(reader, ReadPosition::At(0), buffer);
        assert_eq!(result.buffer.as_slice().as_ptr(), address);
        assert_eq!(result.buffer.as_slice(), b"abc");
    }

    #[test]
    fn pending_read_retains_session_and_buffer_until_physical_completion() {
        struct Session {
            complete: Cell<bool>,
            wake: RefCell<Option<Waker>>,
        }
        struct Reader(Rc<Session>);
        struct Read {
            session: Rc<Session>,
            buffer: Option<AssetBuffer>,
        }
        impl Future for Read {
            type Output = ReadComplete;

            fn poll(
                mut self: core::pin::Pin<&mut Self>,
                cx: &mut Context<'_>,
            ) -> Poll<Self::Output> {
                self.session.wake.replace(Some(cx.waker().clone()));
                if !self.session.complete.get() {
                    return Poll::Pending;
                }
                let mut buffer = self.buffer.take().expect("poll after completion");
                buffer.as_mut_slice()[0] = 42;
                Poll::Ready(ReadComplete {
                    buffer,
                    result: Ok(ReadProgress {
                        filled: 1,
                        eof: true,
                    }),
                })
            }
        }
        impl AssetReader for Reader {
            type Read = Read;

            fn info(&self) -> SourceInfo {
                SourceInfo {
                    len: None,
                    access: SourceAccess::Sequential,
                    consistency: SourceConsistency::Stable,
                }
            }

            fn read(&self, position: ReadPosition, buffer: AssetBuffer) -> Read {
                assert_eq!(position, ReadPosition::Next);
                Read {
                    session: self.0.clone(),
                    buffer: Some(buffer),
                }
            }
        }
        let session = Rc::new(Session {
            complete: Cell::new(false),
            wake: RefCell::new(None),
        });
        let reader = Reader(session.clone());
        let buffer = buffer(4);
        let address = buffer.as_slice().as_ptr();
        let mut read = pin!(reader.read(ReadPosition::Next, buffer));
        drop(reader);
        let weak = Rc::downgrade(&session);
        let mut cx = Context::from_waker(Waker::noop());
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                assert!(read.as_mut().poll(&mut cx).is_pending());
                assert!(read.as_mut().poll(&mut cx).is_pending());
            }),
            0
        );
        session.complete.set(true);
        session.wake.borrow_mut().take().unwrap().wake();
        drop(session);
        assert!(weak.upgrade().is_some());
        let Poll::Ready(result) = read.as_mut().poll(&mut cx) else {
            panic!("completion missing")
        };
        assert_eq!(result.buffer.as_slice().as_ptr(), address);
        assert_eq!(result.buffer.as_slice()[0], 42);
        assert_eq!(
            result.result,
            Ok(ReadProgress {
                filled: 1,
                eof: true
            })
        );
        // The operation itself, not the temporary reader, owns the session.
        assert_eq!(Rc::strong_count(&weak.upgrade().unwrap()), 2);
    }
}
