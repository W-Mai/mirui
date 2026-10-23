use super::source_read::SourceRead;
use super::{AssetError, AssetReader, LoadError, SourceInfo};
use core::task::{Context, Poll, Waker};

pub(super) trait ReadStream: 'static {
    fn info(&self) -> SourceInfo;
    fn poll_read(
        &mut self,
        offset: u64,
        dst: &mut [u8],
        cx: &mut Context<'_>,
        steps: &mut usize,
    ) -> Poll<Result<usize, AssetError>>;
}

impl<R: AssetReader> ReadStream for SourceRead<R> {
    fn info(&self) -> SourceInfo {
        self.info()
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
        self.source.as_ref().map(|source| source.info())
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
        if source.info().len.is_some_and(|len| end > len) {
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
