use super::source_read::SourceRead;
use super::{AssetError, AssetReader, AssetSource, LoadError, SourceInfo};
use core::task::{Context, Poll, Waker};

pub(super) trait ReadStream: 'static {
    fn info(&self) -> Option<SourceInfo>;
    fn take_source(&mut self) -> Result<AssetSource, AssetError> {
        Err(AssetError::Unsupported)
    }
    fn poll_read(
        &mut self,
        offset: u64,
        dst: &mut [u8],
        cx: &mut Context<'_>,
        steps: &mut usize,
    ) -> Poll<Result<usize, AssetError>>;
}

impl<R: AssetReader> ReadStream for SourceRead<R> {
    fn info(&self) -> Option<SourceInfo> {
        Some(self.info())
    }
    fn poll_read(
        &mut self,
        offset: u64,
        dst: &mut [u8],
        cx: &mut Context<'_>,
        steps: &mut usize,
    ) -> Poll<Result<usize, AssetError>> {
        self.poll_read(offset, dst, cx, steps)
    }
}

#[derive(Default)]
pub(super) struct ExactRead {
    active: Option<(u64, usize, usize)>,
}

pub(super) struct LoadBudget {
    pub steps: usize,
    pub bytes: usize,
}

impl Default for LoadBudget {
    fn default() -> Self {
        Self {
            steps: 32,
            bytes: 4096,
        }
    }
}

/// Short-lived access to a loader's token, source and execution allowance.
///
/// `poll_read_exact` may fill a prefix before returning `Pending`. Retain that destination's contents and resume the same offset and length until completion. No destination reference is retained after a call; physical I/O owns separate resource buffers.
pub struct LoadContext<'a> {
    token: &'a str,
    waker: &'a Waker,
    source: Option<&'a mut dyn ReadStream>,
    read: &'a mut ExactRead,
    budget: &'a mut LoadBudget,
}

impl<'a> LoadContext<'a> {
    pub(super) fn new(
        token: &'a str,
        waker: &'a Waker,
        source: Option<&'a mut dyn ReadStream>,
        read: &'a mut ExactRead,
        budget: &'a mut LoadBudget,
    ) -> Self {
        Self {
            token,
            waker,
            source,
            read,
            budget,
        }
    }

    pub fn token(&self) -> &str {
        self.token
    }
    pub fn waker(&self) -> &Waker {
        self.waker
    }
    pub fn source_info(&self) -> Option<SourceInfo> {
        self.source.as_ref().and_then(|source| source.info())
    }

    /// Transfer the opened source into decoder state or the completed resource without reopening or copying it. Finish any `poll_read_exact` operation before taking the source. Further reads through this context fail; the returned owner can be polled independently and retains the prepared provider session until dropped.
    pub fn take_source(&mut self) -> Result<AssetSource, LoadError> {
        if self.read.active.is_some() {
            return Err(AssetError::InvalidSource.into());
        }
        self.source
            .as_mut()
            .ok_or(AssetError::Unsupported)?
            .take_source()
            .map_err(Into::into)
    }

    /// Charge one bounded decoder operation. On exhaustion, schedule another poll and yield.
    pub fn consume_work(&mut self) -> bool {
        if self.budget.steps == 0 {
            self.waker.wake_by_ref();
            false
        } else {
            self.budget.steps -= 1;
            true
        }
    }

    /// Read a bounded prefix using this invocation's shared byte and work allowances. For a nonempty destination, zero means EOF; errors remain errors, including truncation reported by the provider. Empty reads do not establish EOF or perform I/O. A pending exact read must complete before switching to prefix reads.
    ///
    /// No destination borrow is retained. On `Pending`, retry the same offset; on `Ready(Ok(n))`, consume those bytes and advance by `n`. The source's positioning capabilities still apply.
    pub fn poll_read(&mut self, offset: u64, dst: &mut [u8]) -> Poll<Result<usize, LoadError>> {
        if self.read.active.is_some() {
            return Poll::Ready(Err(AssetError::InvalidSource.into()));
        }
        read_prefix(
            *self.source.as_mut().ok_or(AssetError::Unsupported)?,
            self.waker,
            self.budget,
            offset,
            dst,
        )
    }

    pub fn poll_read_exact(&mut self, offset: u64, dst: &mut [u8]) -> Poll<Result<(), LoadError>> {
        let length = u64::try_from(dst.len()).map_err(|_| AssetError::OutOfBounds)?;
        let end = offset.checked_add(length).ok_or(AssetError::OutOfBounds)?;
        let source = self.source.as_mut().ok_or(AssetError::Unsupported)?;
        if source
            .info()
            .ok_or(AssetError::InvalidSource)?
            .len
            .is_some_and(|len| end > len)
        {
            return Poll::Ready(Err(AssetError::Truncated.into()));
        }
        let (_, _, done) = self.read.active.get_or_insert((offset, dst.len(), 0));
        let mut done = *done;
        if self.read.active.map(|(at, len, _)| (at, len)) != Some((offset, dst.len())) {
            return Poll::Ready(Err(AssetError::InvalidSource.into()));
        }
        while done < dst.len() {
            let n = core::task::ready!(read_prefix(
                *source,
                self.waker,
                self.budget,
                offset + done as u64,
                &mut dst[done..],
            ))?;
            if n == 0 {
                return Poll::Ready(Err(AssetError::Truncated.into()));
            }
            done += n;
            self.read.active = Some((offset, dst.len(), done));
        }
        self.read.active = None;
        Poll::Ready(Ok(()))
    }
}

fn read_prefix(
    source: &mut dyn ReadStream,
    waker: &Waker,
    budget: &mut LoadBudget,
    offset: u64,
    dst: &mut [u8],
) -> Poll<Result<usize, LoadError>> {
    let length = u64::try_from(dst.len()).map_err(|_| AssetError::OutOfBounds)?;
    offset.checked_add(length).ok_or(AssetError::OutOfBounds)?;
    if dst.is_empty() {
        return Poll::Ready(Ok(0));
    }
    if budget.steps == 0 || budget.bytes == 0 {
        waker.wake_by_ref();
        return Poll::Pending;
    }
    budget.steps -= 1;
    let count = dst.len().min(budget.bytes);
    let n = core::task::ready!(source.poll_read(
        offset,
        &mut dst[..count],
        &mut Context::from_waker(waker),
        &mut budget.steps,
    ))?;
    if n > count {
        return Poll::Ready(Err(AssetError::InvalidSource.into()));
    }
    budget.bytes -= n;
    Poll::Ready(Ok(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_reads_share_exact_read_allowances_and_preserve_pending_progress() {
        let mut source = SourceRead::new(b"abcdefghij".as_slice(), None).unwrap();
        let mut read = ExactRead::default();
        let mut budget = LoadBudget { steps: 8, bytes: 3 };
        let mut bytes = [0; 8];
        let mut exact = [0; 3];
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                let mut cx = LoadContext::new(
                    "data",
                    Waker::noop(),
                    Some(&mut source),
                    &mut read,
                    &mut budget,
                );
                assert_eq!(cx.poll_read(0, &mut bytes), Poll::Ready(Ok(3)));
                assert_eq!(&bytes[..3], b"abc");
                assert_eq!(&bytes[3..], &[0; 5]);
                assert!(cx.poll_read(3, &mut bytes).is_pending());
                assert_eq!(cx.poll_read(u64::MAX, &mut []), Poll::Ready(Ok(0)));
                assert_eq!(
                    cx.poll_read(u64::MAX, &mut bytes),
                    Poll::Ready(Err(AssetError::OutOfBounds.into()))
                );
                let mut budget = LoadBudget { steps: 8, bytes: 2 };
                let mut cx = LoadContext::new(
                    "data",
                    Waker::noop(),
                    Some(&mut source),
                    &mut read,
                    &mut budget,
                );
                assert!(cx.poll_read_exact(3, &mut exact).is_pending());
                assert_eq!(&exact[..2], b"de");
                assert_eq!(
                    cx.poll_read(5, &mut bytes),
                    Poll::Ready(Err(AssetError::InvalidSource.into()))
                );
                let mut budget = LoadBudget { steps: 8, bytes: 8 };
                let mut cx = LoadContext::new(
                    "data",
                    Waker::noop(),
                    Some(&mut source),
                    &mut read,
                    &mut budget,
                );
                assert_eq!(cx.poll_read_exact(3, &mut exact), Poll::Ready(Ok(())));
                assert_eq!(&exact, b"def");
                assert_eq!(cx.poll_read(9, &mut bytes), Poll::Ready(Ok(1)));
                assert_eq!(bytes[0], b'j');
                assert_eq!(cx.poll_read(10, &mut bytes), Poll::Ready(Ok(0)));
                assert_eq!(
                    cx.poll_read(11, &mut bytes),
                    Poll::Ready(Err(AssetError::OutOfBounds.into()))
                );
            }),
            0
        );
    }

    #[test]
    fn unknown_length_prefix_reads_do_not_turn_source_errors_into_eof() {
        struct Stream {
            result: Poll<Result<usize, AssetError>>,
        }
        impl ReadStream for Stream {
            fn info(&self) -> Option<SourceInfo> {
                Some(SourceInfo {
                    len: None,
                    access: super::super::SourceAccess::Random,
                    consistency: super::super::SourceConsistency::Stable,
                })
            }
            fn poll_read(
                &mut self,
                _: u64,
                _: &mut [u8],
                cx: &mut Context<'_>,
                _: &mut usize,
            ) -> Poll<Result<usize, AssetError>> {
                if self.result.is_pending() {
                    cx.waker().wake_by_ref();
                }
                self.result
            }
        }
        for result in [
            Poll::Pending,
            Poll::Ready(Ok(0)),
            Poll::Ready(Ok(1)),
            Poll::Ready(Err(AssetError::Truncated)),
            Poll::Ready(Err(AssetError::Io("read failed"))),
        ] {
            let mut source = Stream { result };
            let mut read = ExactRead::default();
            let mut budget = LoadBudget::default();
            let mut cx = LoadContext::new(
                "data",
                Waker::noop(),
                Some(&mut source),
                &mut read,
                &mut budget,
            );
            assert_eq!(
                cx.poll_read(42, &mut [0]),
                result.map(|result| result.map_err(Into::into))
            );
        }
        let mut source = Stream {
            result: Poll::Ready(Ok(2)),
        };
        let mut read = ExactRead::default();
        let mut budget = LoadBudget::default();
        let mut cx = LoadContext::new(
            "data",
            Waker::noop(),
            Some(&mut source),
            &mut read,
            &mut budget,
        );
        assert_eq!(
            cx.poll_read(42, &mut [0]),
            Poll::Ready(Err(AssetError::InvalidSource.into()))
        );
    }

    #[test]
    fn source_transfer_reuses_storage_and_rejects_incomplete_exact_reads() {
        use super::super::{AssetCacheConfig, AssetSources};
        static BYTES: [u8; 8192] = [7; 8192];
        let mut sources = AssetSources::new(AssetCacheConfig::new().max_requests(1)).unwrap();
        sources
            .register("files", |_| core::future::ready(Ok(BYTES.as_slice())))
            .unwrap();
        let mut source = sources.open("files", "large".into()).unwrap();
        let mut task = Context::from_waker(Waker::noop());
        assert_eq!(source.poll_open(&mut task), Poll::Ready(Ok(())));
        let mut read = ExactRead::default();
        let mut budget = LoadBudget { steps: 2, bytes: 1 };
        let mut header = [0; 2];
        {
            let mut cx = LoadContext::new(
                "large",
                Waker::noop(),
                Some(&mut source),
                &mut read,
                &mut budget,
            );
            assert!(cx.poll_read_exact(0, &mut header).is_pending());
            assert!(matches!(
                cx.take_source(),
                Err(LoadError::Source(AssetError::InvalidSource))
            ));
        }
        budget.bytes = 1;
        budget.steps = 2;
        let mut cx = LoadContext::new(
            "large",
            Waker::noop(),
            Some(&mut source),
            &mut read,
            &mut budget,
        );
        assert_eq!(cx.poll_read_exact(0, &mut header), Poll::Ready(Ok(())));
        let mut retained = None;
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                retained = Some(cx.take_source().unwrap());
            }),
            0
        );
        assert_eq!(cx.source_info(), None);
        assert!(matches!(
            sources.open("files", "other".into()),
            Err(AssetError::Capacity)
        ));
        let mut retained = retained.unwrap();
        let mut bytes = [0; 8192];
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                assert_eq!(
                    retained.poll_read(0, &mut bytes, &mut task),
                    Poll::Ready(Ok(4096))
                );
            }),
            0
        );
        assert_eq!(&bytes[..4096], &BYTES[..4096]);
        assert_eq!(bytes[4096], 0);
        assert_eq!(
            retained.poll_read(u64::MAX, &mut bytes, &mut task),
            Poll::Ready(Err(AssetError::OutOfBounds))
        );
        drop(retained);
        assert!(sources.open("files", "other".into()).is_ok());
    }

    #[test]
    fn exact_reads_resume_bounded_prefixes_and_reject_changed_requests() {
        let mut source = SourceRead::new(b"abcdefghij".as_slice(), None).unwrap();
        let mut state = ExactRead::default();
        let mut dst = [0; 7];
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                let mut budget = LoadBudget { steps: 8, bytes: 3 };
                let mut cx = LoadContext::new(
                    "name",
                    Waker::noop(),
                    Some(&mut source),
                    &mut state,
                    &mut budget,
                );
                assert_eq!(cx.token(), "name");
                assert_eq!(cx.source_info().unwrap().len, Some(10));
                assert!(cx.waker().will_wake(Waker::noop()));
                assert!(cx.consume_work());
                assert!(cx.poll_read_exact(1, &mut dst).is_pending());
                assert_eq!(&dst[..3], b"bcd");
                assert_eq!(
                    cx.poll_read_exact(2, &mut dst),
                    Poll::Ready(Err(LoadError::Source(AssetError::InvalidSource)))
                );
                let mut budget = LoadBudget { steps: 8, bytes: 8 };
                let mut cx = LoadContext::new(
                    "name",
                    Waker::noop(),
                    Some(&mut source),
                    &mut state,
                    &mut budget,
                );
                assert_eq!(cx.poll_read_exact(1, &mut dst), Poll::Ready(Ok(())));
                assert_eq!(&dst, b"bcdefgh");
                assert_eq!(
                    cx.poll_read_exact(6, &mut dst),
                    Poll::Ready(Err(LoadError::Source(AssetError::Truncated)))
                );
                assert_eq!(cx.poll_read_exact(10, &mut []), Poll::Ready(Ok(())));
            }),
            0
        );
        assert!(
            source
                .poll_retire(&mut Context::from_waker(Waker::noop()), &mut 1)
                .is_ready()
        );
    }
}
