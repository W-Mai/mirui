//! Bounded polling of one opened reader. External futures never run under a pool borrow.

use super::blocks::{Block, BlockKey, BlockPool, BlockWrite};
use super::{
    AssetBuffer, AssetError, AssetReader, ReadPosition, SourceAccess, SourceConsistency, SourceInfo,
};
use crate::core::cache::Handle;
use alloc::{boxed::Box, rc::Rc};
use core::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

struct Reading {
    write: BlockWrite,
    key: BlockKey,
    address: usize,
    filled: usize,
    end: usize,
}

pub(super) struct SourceRead<R: AssetReader> {
    reader: R,
    info: SourceInfo,
    identity: u64,
    pool: Option<Rc<RefCell<BlockPool>>>,
    future: Option<Pin<Box<Option<R::Read>>>>,
    reading: Option<Reading>,
    cursor: u64,
    // This lease also permits streaming when resident caching is disabled.
    last: Option<Handle<Block>>,
}

struct PollGuard<'a, R: AssetReader> {
    source: &'a mut SourceRead<R>,
    completed: bool,
}
impl<R: AssetReader> Drop for PollGuard<'_, R> {
    fn drop(&mut self) {
        if !self.completed {
            if let Some(reading) = self.source.reading.take() {
                self.source.pool().borrow_mut().abandon(reading.write);
            }
            if let Some(future) = &mut self.source.future {
                // A provider must make dropping its operation safe even while physical work remains.
                future.as_mut().set(None);
            }
        }
    }
}

impl<R: AssetReader> SourceRead<R> {
    #[cfg(test)]
    pub fn new(reader: R, pool: Option<Rc<RefCell<BlockPool>>>) -> Result<Self, AssetError> {
        let mut storage = reader.mapped().is_none().then(|| Box::pin(None));
        Self::with_storage(reader, pool, &mut storage)
    }

    pub fn with_storage(
        reader: R,
        pool: Option<Rc<RefCell<BlockPool>>>,
        storage: &mut Option<Pin<Box<Option<R::Read>>>>,
    ) -> Result<Self, AssetError> {
        let info = reader.info();
        if info.consistency != SourceConsistency::Stable {
            return Err(AssetError::Unsupported);
        }
        let mapped = reader.mapped();
        if let Some(bytes) = mapped {
            if info.len != u64::try_from(bytes.len()).ok() {
                return Err(AssetError::InvalidSource);
            }
        }
        if mapped.is_none() && pool.is_none() {
            return Err(AssetError::Capacity);
        }
        let identity = match &pool {
            Some(pool) => pool.borrow_mut().new_source()?,
            None => 0,
        };
        let future = if mapped.is_none() {
            Some(storage.take().ok_or(AssetError::Capacity)?)
        } else {
            None
        };
        Ok(Self {
            reader,
            info,
            identity,
            pool,
            future,
            reading: None,
            cursor: 0,
            last: None,
        })
    }

    pub fn into_storage(self) -> Option<Pin<Box<Option<R::Read>>>> {
        assert!(
            self.reading.is_none(),
            "physical read must retire before reusing its slot"
        );
        self.future
    }

    pub fn info(&self) -> SourceInfo {
        self.info
    }

    pub fn has_pending_read(&self) -> bool {
        self.reading.is_some()
    }

    fn pool(&self) -> &RefCell<BlockPool> {
        self.pool
            .as_ref()
            .expect("unmapped reader has block storage")
    }

    pub(super) fn mapped(&self) -> Result<Option<&[u8]>, AssetError> {
        let bytes = self.reader.mapped();
        if let Some(bytes) = bytes
            && self.info.len != u64::try_from(bytes.len()).ok()
        {
            return Err(AssetError::InvalidSource);
        }
        Ok(bytes)
    }

    fn copy_block(block: &Block, offset: u64, dst: &mut [u8]) -> Option<usize> {
        let start = usize::try_from(offset.checked_sub(block.key().offset)?).ok()?;
        let remaining = block.bytes().get(start..)?;
        if remaining.is_empty() && !block.eof() {
            return None;
        }
        let n = remaining.len().min(dst.len());
        dst[..n].copy_from_slice(&remaining[..n]);
        Some(n)
    }

    /// Return a bounded prefix, retaining no borrow of the caller's destination across Pending.
    pub fn poll_read(
        &mut self,
        offset: u64,
        dst: &mut [u8],
        cx: &mut Context<'_>,
        steps: &mut usize,
    ) -> Poll<Result<usize, AssetError>> {
        let mut guard = PollGuard {
            source: self,
            completed: false,
        };
        let result = guard.source.poll_read_inner(offset, dst, cx, steps);
        guard.completed = true;
        result
    }

    fn poll_read_inner(
        &mut self,
        offset: u64,
        dst: &mut [u8],
        cx: &mut Context<'_>,
        steps: &mut usize,
    ) -> Poll<Result<usize, AssetError>> {
        let length = u64::try_from(dst.len()).map_err(|_| AssetError::OutOfBounds)?;
        offset.checked_add(length).ok_or(AssetError::OutOfBounds)?;
        if dst.is_empty() {
            return Poll::Ready(Ok(0));
        }
        if self.info.len.is_some_and(|len| offset > len) {
            return Poll::Ready(Err(AssetError::OutOfBounds));
        }
        if let Some(bytes) = self.mapped()? {
            let start = usize::try_from(offset).map_err(|_| AssetError::OutOfBounds)?;
            let remaining = bytes.get(start..).ok_or(AssetError::OutOfBounds)?;
            let n = remaining.len().min(dst.len());
            dst[..n].copy_from_slice(&remaining[..n]);
            return Poll::Ready(Ok(n));
        }
        if self.future.is_none() {
            return Poll::Ready(Err(AssetError::InvalidSource));
        }
        if let Some(block) = &self.last {
            if let Some(n) = Self::copy_block(block, offset, dst) {
                return Poll::Ready(Ok(n));
            }
        }
        self.last = None;
        let cached = self.pool().borrow_mut().acquire(self.identity, offset);
        if let Some(block) = cached {
            let n = Self::copy_block(&block, offset, dst).ok_or(AssetError::InvalidSource)?;
            self.last = Some(block);
            return Poll::Ready(Ok(n));
        }
        if self.reading.is_none() {
            if self.info.len == Some(offset) {
                return Poll::Ready(Ok(0));
            }
            if *steps == 0 {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            *steps -= 1;
            let size = self.pool().borrow().block_size();
            let start = match self.info.access {
                SourceAccess::Random => {
                    offset - offset % u64::try_from(size).map_err(|_| AssetError::OutOfBounds)?
                }
                SourceAccess::Sequential if offset == self.cursor => offset,
                SourceAccess::Sequential => return Poll::Ready(Err(AssetError::Unsupported)),
            };
            let key = BlockKey {
                source: self.identity,
                offset: start,
            };
            let end = usize::try_from(
                (self.info.len.unwrap_or(u64::MAX) - start)
                    .min(u64::try_from(size).map_err(|_| AssetError::OutOfBounds)?),
            )
            .map_err(|_| AssetError::OutOfBounds)?;
            let (write, mut buffer) = self.pool().borrow_mut().begin(key)?;
            buffer.end = end;
            let reading = Reading {
                write,
                key,
                address: buffer.bytes.as_ptr() as usize,
                filled: 0,
                end: buffer.end,
            };
            self.reading = Some(reading);
            self.start_read(buffer);
        }
        let block = core::task::ready!(self.poll_physical(cx, steps, false))?
            .expect("active read publishes a block");
        let n = Self::copy_block(&block, offset, dst).ok_or(AssetError::InvalidSource)?;
        self.last = Some(block);
        Poll::Ready(Ok(n))
    }

    fn start_read(&mut self, buffer: AssetBuffer) {
        let reading = self.reading.as_ref().unwrap();
        let position = match self.info.access {
            SourceAccess::Random => ReadPosition::At(reading.key.offset + reading.filled as u64),
            SourceAccess::Sequential => ReadPosition::Next,
        };
        let future = self.reader.read(position, buffer);
        self.future.as_mut().unwrap().as_mut().set(Some(future));
    }

    fn poll_physical(
        &mut self,
        cx: &mut Context<'_>,
        steps: &mut usize,
        retiring: bool,
    ) -> Poll<Result<Option<Handle<Block>>, AssetError>> {
        loop {
            if *steps == 0 {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            *steps -= 1;
            let future = self.future.as_mut().unwrap();
            let complete = core::task::ready!(future.as_mut().as_pin_mut().unwrap().poll(cx));
            future.as_mut().set(None);
            let mut reading = self.reading.take().unwrap();
            let mut buffer = complete.buffer;
            if retiring {
                return Poll::Ready(
                    self.pool()
                        .borrow_mut()
                        .abort(reading.write, buffer)
                        .map(|()| None),
                );
            }
            let result = complete.result.and_then(|progress| {
                if buffer.bytes.as_ptr() as usize != reading.address
                    || buffer.start != reading.filled
                    || buffer.end != reading.end
                    || progress.filled > buffer.len()
                {
                    return Err(AssetError::InvalidSource);
                }
                if progress.filled == 0 && !progress.eof {
                    return Err(AssetError::NoProgress);
                }
                reading.filled += progress.filled;
                let end = reading
                    .key
                    .offset
                    .checked_add(
                        u64::try_from(reading.filled).map_err(|_| AssetError::OutOfBounds)?,
                    )
                    .ok_or(AssetError::OutOfBounds)?;
                if let Some(len) = self.info.len {
                    if end > len {
                        return Err(AssetError::InvalidSource);
                    }
                    if progress.eof && end < len {
                        return Err(AssetError::Truncated);
                    }
                }
                self.cursor = end;
                Ok(progress.eof || self.info.len == Some(end))
            });
            match result {
                Ok(false) if reading.filled < reading.end => {
                    buffer.start = reading.filled;
                    self.reading = Some(reading);
                    self.start_read(buffer);
                }
                result => {
                    return Poll::Ready(
                        self.pool()
                            .borrow_mut()
                            .finish(
                                reading.write,
                                buffer,
                                result.map(|eof| (reading.filled, eof)),
                            )
                            .map(Some),
                    );
                }
            }
        }
    }

    /// Retire physical work after logical cancellation without publishing its data.
    pub fn poll_retire(&mut self, cx: &mut Context<'_>, steps: &mut usize) -> Poll<()> {
        self.last = None;
        if self.reading.is_none() {
            return Poll::Ready(());
        }
        let mut guard = PollGuard {
            source: self,
            completed: false,
        };
        let result = guard.source.poll_physical(cx, steps, true);
        guard.completed = true;
        match result {
            Poll::Pending => Poll::Pending,
            Poll::Ready(_) => Poll::Ready(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{ReadComplete, ReadProgress};
    use super::*;
    use crate::core::cache::MaxSize;
    use core::{cell::Cell, marker::PhantomPinned, task::Waker};

    #[test]
    fn unwound_provider_does_not_repoll_or_fabricate_replacement_payload() {
        struct Panics;
        impl AssetReader for Panics {
            type Read = core::future::Ready<ReadComplete>;
            fn info(&self) -> SourceInfo {
                SourceInfo {
                    len: Some(4),
                    access: SourceAccess::Random,
                    consistency: SourceConsistency::Stable,
                }
            }
            fn read(&self, _: ReadPosition, _: AssetBuffer) -> Self::Read {
                panic!("provider failed before returning its operation");
            }
        }
        let pool = Rc::new(RefCell::new(
            BlockPool::new(4, MaxSize::Count(2), 1).unwrap(),
        ));
        let mut reader = SourceRead::new(Panics, Some(pool.clone())).unwrap();
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                reader.poll_read(
                    0,
                    &mut [0; 1],
                    &mut Context::from_waker(Waker::noop()),
                    &mut 8,
                )
            }))
            .is_err()
        );
        assert!(!reader.has_pending_read());
        assert!(
            reader
                .poll_retire(&mut Context::from_waker(Waker::noop()), &mut 8)
                .is_ready()
        );
        let key = BlockKey {
            source: 2,
            offset: 0,
        };
        let (write, buffer) = pool.borrow_mut().begin(key).unwrap();
        pool.borrow_mut().abort(write, buffer).unwrap();
        // One prepared block remains usable; the missing one cannot be silently replaced.
        let (write, buffer) = pool.borrow_mut().begin(key).unwrap();
        assert!(matches!(
            pool.borrow_mut().begin(key),
            Err(AssetError::Capacity)
        ));
        pool.borrow_mut().abort(write, buffer).unwrap();
    }

    struct Driver {
        bytes: &'static [u8],
        ready: Cell<bool>,
        short: usize,
        calls: Cell<usize>,
        cursor: Cell<usize>,
        wake: RefCell<Option<Waker>>,
        pool: Rc<RefCell<BlockPool>>,
    }

    struct Reader {
        driver: Rc<Driver>,
        info: SourceInfo,
    }

    struct ReadingFuture {
        driver: Rc<Driver>,
        buffer: RefCell<Option<AssetBuffer>>,
        offset: usize,
        address: Cell<usize>,
        _pin: PhantomPinned,
    }

    impl Future for ReadingFuture {
        type Output = ReadComplete;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let this = self.as_ref().get_ref();
            let address = this as *const Self as usize;
            let original = this.address.replace(address);
            assert!(
                original == 0 || original == address,
                "future moved while pinned"
            );
            assert!(
                this.driver.pool.try_borrow_mut().is_ok(),
                "provider polled under pool borrow"
            );
            this.driver.wake.replace(Some(cx.waker().clone()));
            if !this.driver.ready.get() {
                return Poll::Pending;
            }
            let mut buffer = this.buffer.borrow_mut().take().unwrap();
            let remaining = &this.driver.bytes[this.offset..];
            let filled = remaining.len().min(buffer.len()).min(this.driver.short);
            buffer.as_mut_slice()[..filled].copy_from_slice(&remaining[..filled]);
            this.driver.cursor.set(this.offset + filled);
            Poll::Ready(ReadComplete {
                buffer,
                result: Ok(ReadProgress {
                    filled,
                    eof: filled == remaining.len(),
                }),
            })
        }
    }

    impl AssetReader for Reader {
        type Read = ReadingFuture;
        fn info(&self) -> SourceInfo {
            self.info
        }
        fn read(&self, position: ReadPosition, buffer: AssetBuffer) -> Self::Read {
            assert!(self.driver.pool.try_borrow_mut().is_ok());
            self.driver.calls.set(self.driver.calls.get() + 1);
            let offset = match position {
                ReadPosition::At(n) => usize::try_from(n).unwrap(),
                ReadPosition::Next => self.driver.cursor.get(),
            };
            ReadingFuture {
                driver: self.driver.clone(),
                buffer: RefCell::new(Some(buffer)),
                offset,
                address: Cell::new(0),
                _pin: PhantomPinned,
            }
        }
    }

    fn setup(short: usize, known: bool, access: SourceAccess) -> (Rc<Driver>, SourceRead<Reader>) {
        let pool = Rc::new(RefCell::new(
            BlockPool::new(4, MaxSize::Bytes(8), 1).unwrap(),
        ));
        let driver = Rc::new(Driver {
            bytes: b"abcdefghij",
            ready: Cell::new(false),
            short,
            calls: Cell::new(0),
            cursor: Cell::new(0),
            wake: RefCell::new(None),
            pool: pool.clone(),
        });
        let reader = Reader {
            driver: driver.clone(),
            info: SourceInfo {
                len: known.then_some(10),
                access,
                consistency: SourceConsistency::Stable,
            },
        };
        (driver, SourceRead::new(reader, Some(pool)).unwrap())
    }

    #[test]
    fn pending_short_reads_reuse_pinned_future_and_preserve_requested_offsets() {
        let (driver, mut source) = setup(2, false, SourceAccess::Random);
        assert_eq!(source.info().len, None);
        let mut cx = Context::from_waker(Waker::noop());
        let mut dst = [0; 6];
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                assert!(source.poll_read(1, &mut dst, &mut cx, &mut 8).is_pending());
                assert!(source.poll_read(1, &mut dst, &mut cx, &mut 8).is_pending());
                assert_eq!(driver.calls.get(), 1);
                driver.ready.set(true);
                driver.wake.borrow_mut().take().unwrap().wake();
                assert_eq!(
                    source.poll_read(1, &mut dst, &mut cx, &mut 8),
                    Poll::Ready(Ok(3))
                );
                assert_eq!(&dst[..3], b"bcd");
                assert_eq!(driver.calls.get(), 2);
                assert_eq!(
                    source.poll_read(2, &mut dst[..1], &mut cx, &mut 8),
                    Poll::Ready(Ok(1))
                );
                assert_eq!(driver.calls.get(), 2);
                assert_eq!(
                    source.poll_read(4, &mut dst, &mut cx, &mut 8),
                    Poll::Ready(Ok(4))
                );
                assert_eq!(&dst[..4], b"efgh");
                assert_eq!(
                    source.poll_read(8, &mut dst, &mut cx, &mut 8),
                    Poll::Ready(Ok(2))
                );
                assert_eq!(&dst[..2], b"ij");
                assert_eq!(
                    source.poll_read(10, &mut dst, &mut cx, &mut 8),
                    Poll::Ready(Ok(0))
                );
            }),
            0
        );
    }

    #[test]
    fn cancellation_drains_only_submitted_io_and_does_not_publish_or_continue_short_reads() {
        let (driver, mut source) = setup(1, true, SourceAccess::Random);
        let mut cx = Context::from_waker(Waker::noop());
        assert!(
            source
                .poll_read(0, &mut [0; 4], &mut cx, &mut 8)
                .is_pending()
        );
        assert!(source.poll_retire(&mut cx, &mut 8).is_pending());
        assert!(
            driver
                .pool
                .borrow_mut()
                .begin(BlockKey {
                    source: 9,
                    offset: 0
                })
                .is_err()
        );
        driver.ready.set(true);
        assert!(source.poll_retire(&mut cx, &mut 8).is_ready());
        assert_eq!(driver.calls.get(), 1);
        assert!(
            driver
                .pool
                .borrow_mut()
                .acquire(source.identity, 0)
                .is_none()
        );
        let (write, bytes) = driver
            .pool
            .borrow_mut()
            .begin(BlockKey {
                source: 9,
                offset: 0,
            })
            .unwrap();
        driver.pool.borrow_mut().abort(write, bytes).unwrap();
    }

    #[test]
    fn work_quota_errors_and_sequential_access_are_explicit() {
        let (driver, mut source) = setup(1, true, SourceAccess::Sequential);
        driver.ready.set(true);
        let mut cx = Context::from_waker(Waker::noop());
        assert!(
            source
                .poll_read(0, &mut [0; 4], &mut cx, &mut 2)
                .is_pending()
        );
        assert_eq!(driver.calls.get(), 2);
        assert_eq!(
            source.poll_read(0, &mut [0; 4], &mut cx, &mut 8),
            Poll::Ready(Ok(4))
        );
        assert_eq!(
            source.poll_read(8, &mut [0; 1], &mut cx, &mut 8),
            Poll::Ready(Err(AssetError::Unsupported))
        );
        assert_eq!(
            source.poll_read(u64::MAX, &mut [0; 1], &mut cx, &mut 8),
            Poll::Ready(Err(AssetError::OutOfBounds))
        );
        let (driver, mut source) = setup(0, true, SourceAccess::Random);
        driver.ready.set(true);
        assert_eq!(
            source.poll_read(0, &mut [0; 4], &mut cx, &mut 8),
            Poll::Ready(Err(AssetError::NoProgress))
        );
        assert!(source.poll_retire(&mut cx, &mut 8).is_ready());
    }

    #[test]
    fn mapped_sources_need_neither_block_storage_nor_future_allocation() {
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                let mut source = SourceRead::new(b"mapped".as_slice(), None).unwrap();
                let mut cx = Context::from_waker(Waker::noop());
                let mut bytes = [0; 3];
                assert_eq!(
                    source.poll_read(1, &mut bytes, &mut cx, &mut 8),
                    Poll::Ready(Ok(3))
                );
                assert_eq!(&bytes, b"app");
                assert!(source.future.is_none());
                assert!(source.pool.is_none());
                assert_eq!(
                    source.poll_read(6, &mut bytes, &mut cx, &mut 8),
                    Poll::Ready(Ok(0))
                );
            }),
            0
        );
    }
}
