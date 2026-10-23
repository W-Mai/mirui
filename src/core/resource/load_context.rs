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
            if self.budget.steps == 0 || self.budget.bytes == 0 {
                self.waker.wake_by_ref();
                return Poll::Pending;
            }
            self.budget.steps -= 1;
            let count = (dst.len() - done).min(self.budget.bytes);
            let mut cx = Context::from_waker(self.waker);
            let n = core::task::ready!(source.poll_read(
                offset + done as u64,
                &mut dst[done..done + count],
                &mut cx,
                &mut self.budget.steps
            ))?;
            if n == 0 {
                return Poll::Ready(Err(AssetError::Truncated.into()));
            }
            if n > count {
                return Poll::Ready(Err(AssetError::InvalidSource.into()));
            }
            done += n;
            self.budget.bytes -= n;
            self.read.active = Some((offset, dst.len(), done));
        }
        self.read.active = None;
        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
