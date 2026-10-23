//! Prepared byte-source sessions shared by typed resource managers.

use super::blocks::BlockPool;
use super::load_context::{LoadBudget, ReadStream};
use super::source_read::SourceRead;
use super::wake::TaskWake;
use super::{AssetError, AssetProvider, AssetReader, SourceInfo};
use crate::core::cache::MaxSize;
use alloc::{borrow::Cow, boxed::Box, rc::Rc, vec::Vec};
use core::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

/// Shared byte-cache and prepared request limits. Static resource registrations do not use this storage.
#[derive(Clone, Copy, Debug)]
pub struct AssetCacheConfig {
    block_size: usize,
    budget: MaxSize,
    max_in_flight: usize,
    requests: usize,
    key_bytes: usize,
    prefix_bytes: usize,
}

impl Default for AssetCacheConfig {
    fn default() -> Self {
        Self::new()
    }
}
impl AssetCacheConfig {
    pub const fn new() -> Self {
        Self {
            block_size: 1024,
            budget: MaxSize::Bytes(4096),
            max_in_flight: 2,
            requests: 4,
            key_bytes: 1024,
            prefix_bytes: 256,
        }
    }
    pub const fn block_size(mut self, bytes: usize) -> Self {
        self.block_size = bytes;
        self
    }
    pub const fn budget(mut self, budget: MaxSize) -> Self {
        self.budget = budget;
        self
    }
    pub const fn max_in_flight(mut self, count: usize) -> Self {
        self.max_in_flight = count;
        self
    }
    /// Prepared open/read sessions per registered provider, including retiring operations.
    pub const fn max_requests(mut self, count: usize) -> Self {
        self.requests = count;
        self
    }
    pub const fn max_key_bytes(mut self, bytes: usize) -> Self {
        self.key_bytes = bytes;
        self
    }
    pub const fn probe_bytes(mut self, bytes: usize) -> Self {
        self.prefix_bytes = bytes;
        self
    }
}

trait Session {
    fn start(&mut self, key: Cow<'static, str>);
    fn poll_open(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), AssetError>>;
    fn info(&self) -> Option<SourceInfo>;
    fn mapped(&self) -> Result<Option<&[u8]>, AssetError>;
    fn poll_read(
        &mut self,
        offset: u64,
        dst: &mut [u8],
        cx: &mut Context<'_>,
        steps: &mut usize,
    ) -> Poll<Result<usize, AssetError>>;
    fn poll_prefix(
        &mut self,
        length: usize,
        cx: &mut Context<'_>,
        steps: &mut usize,
    ) -> Poll<Result<(), AssetError>>;
    fn prefix(&self) -> (&[u8], bool);
    fn poll_retire(&mut self, cx: &mut Context<'_>, steps: &mut usize) -> Poll<()>;
    fn reset(&mut self);
    fn has_pending(&self) -> bool;
    fn wake(&self) -> &TaskWake;
    fn retiring(&self) -> bool;
    fn set_retiring(&mut self);
}

type ReadSlot<R> = Option<Pin<Box<Option<<R as AssetReader>::Read>>>>;

struct OpenPoll<'a, F> {
    slot: Pin<&'a mut Option<F>>,
    returned: bool,
}
impl<F> Drop for OpenPoll<'_, F> {
    fn drop(&mut self) {
        if !self.returned {
            self.slot.as_mut().set(None);
        }
    }
}
fn poll_open<F: Future>(slot: Pin<&mut Option<F>>, cx: &mut Context<'_>) -> Poll<F::Output> {
    let mut guard = OpenPoll {
        slot,
        returned: false,
    };
    let result = guard
        .slot
        .as_mut()
        .as_pin_mut()
        .expect("pending open")
        .poll(cx);
    if result.is_ready() {
        guard.slot.as_mut().set(None);
    }
    guard.returned = true;
    result
}

struct TypedSession<P: AssetProvider> {
    provider: Rc<P>,
    pool: Rc<RefCell<BlockPool>>,
    opening: Pin<Box<Option<P::Open>>>,
    read_slot: ReadSlot<P::Reader>,
    reader: Option<SourceRead<P::Reader>>,
    prefix: Box<[u8]>,
    filled: usize,
    eof: bool,
    wake: TaskWake,
    retiring: bool,
}

impl<P: AssetProvider> Session for TypedSession<P> {
    fn start(&mut self, key: Cow<'static, str>) {
        self.opening.as_mut().set(Some(self.provider.open(key)));
    }
    fn poll_open(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), AssetError>> {
        if self.reader.is_some() {
            return Poll::Ready(Ok(()));
        }
        if self.opening.as_ref().get_ref().is_none() {
            return Poll::Ready(Err(AssetError::InvalidSource));
        }
        let result = core::task::ready!(poll_open(self.opening.as_mut(), cx));
        let reader = result?;
        self.reader = Some(SourceRead::with_storage(
            reader,
            Some(self.pool.clone()),
            &mut self.read_slot,
        )?);
        Poll::Ready(Ok(()))
    }
    fn info(&self) -> Option<SourceInfo> {
        self.reader.as_ref().map(SourceRead::info)
    }
    fn mapped(&self) -> Result<Option<&[u8]>, AssetError> {
        self.reader
            .as_ref()
            .ok_or(AssetError::InvalidSource)?
            .mapped()
    }
    fn poll_read(
        &mut self,
        offset: u64,
        dst: &mut [u8],
        cx: &mut Context<'_>,
        steps: &mut usize,
    ) -> Poll<Result<usize, AssetError>> {
        let length = u64::try_from(dst.len()).map_err(|_| AssetError::OutOfBounds)?;
        offset.checked_add(length).ok_or(AssetError::OutOfBounds)?;
        if let Ok(start) = usize::try_from(offset) {
            if start < self.filled {
                let count = dst.len().min(self.filled - start);
                dst[..count].copy_from_slice(&self.prefix[start..start + count]);
                return Poll::Ready(Ok(count));
            }
        }
        self.reader
            .as_mut()
            .ok_or(AssetError::InvalidSource)?
            .poll_read(offset, dst, cx, steps)
    }
    fn poll_prefix(
        &mut self,
        length: usize,
        cx: &mut Context<'_>,
        steps: &mut usize,
    ) -> Poll<Result<(), AssetError>> {
        if length > self.prefix.len() {
            return Poll::Ready(Err(AssetError::Capacity));
        }
        let reader = self.reader.as_mut().ok_or(AssetError::InvalidSource)?;
        let target = reader
            .info()
            .len
            .and_then(|len| usize::try_from(len).ok())
            .map_or(length, |len| len.min(length));
        while self.filled < target && !self.eof {
            if *steps == 0 {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            *steps -= 1;
            let count = core::task::ready!(reader.poll_read(
                self.filled as u64,
                &mut self.prefix[self.filled..target],
                cx,
                steps
            ))?;
            if count > target - self.filled {
                return Poll::Ready(Err(AssetError::InvalidSource));
            }
            self.filled += count;
            self.eof = count == 0;
        }
        self.eof |= reader.info().len == Some(self.filled as u64);
        Poll::Ready(Ok(()))
    }
    fn prefix(&self) -> (&[u8], bool) {
        (&self.prefix[..self.filled], self.eof)
    }
    fn poll_retire(&mut self, cx: &mut Context<'_>, steps: &mut usize) -> Poll<()> {
        if self.opening.as_ref().get_ref().is_some() {
            let result = core::task::ready!(poll_open(self.opening.as_mut(), cx));
            drop(result);
        }
        if let Some(reader) = &mut self.reader {
            core::task::ready!(reader.poll_retire(cx, steps));
        }
        Poll::Ready(())
    }
    fn reset(&mut self) {
        if let Some(reader) = self.reader.take() {
            if let Some(storage) = reader.into_storage() {
                self.read_slot = Some(storage);
            }
        }
        self.filled = 0;
        self.eof = false;
        self.retiring = false;
    }
    fn has_pending(&self) -> bool {
        self.opening.as_ref().get_ref().is_some()
            || self
                .reader
                .as_ref()
                .is_some_and(SourceRead::has_pending_read)
    }
    fn wake(&self) -> &TaskWake {
        &self.wake
    }
    fn retiring(&self) -> bool {
        self.retiring
    }
    fn set_retiring(&mut self) {
        self.retiring = true;
    }
}

type SessionSlots = RefCell<Vec<Option<Box<dyn Session>>>>;

struct ProviderSlots {
    sessions: SessionSlots,
    wake: Rc<TaskWake>,
    key_bytes: usize,
}

impl ProviderSlots {
    fn take(self: &Rc<Self>, key: Cow<'static, str>) -> Result<AssetSource, AssetError> {
        if key.len() > self.key_bytes {
            return Err(AssetError::Capacity);
        }
        let session = {
            let mut sessions = self.sessions.borrow_mut();
            let slot = sessions
                .iter_mut()
                .find(|slot| slot.as_ref().is_some_and(|session| !session.retiring()))
                .ok_or(AssetError::Capacity)?;
            slot.take().unwrap()
        };
        let mut source = AssetSource {
            provider: self.clone(),
            session: Some(session),
        };
        source.session.as_mut().unwrap().start(key);
        Ok(source)
    }
    fn put(&self, session: Box<dyn Session>) {
        let mut slots = self.sessions.borrow_mut();
        *slots
            .iter_mut()
            .find(|slot| slot.is_none())
            .expect("leased source slot") = Some(session);
    }
    fn poll_retired(&self, parent: &Waker) {
        let count = self.sessions.borrow().len();
        for index in 0..count {
            let session = {
                let mut slots = self.sessions.borrow_mut();
                if !slots[index]
                    .as_ref()
                    .is_some_and(|session| session.retiring())
                {
                    continue;
                }
                slots[index].take().unwrap()
            };
            let mut lease = Retiring {
                provider: self,
                session: Some(session),
            };
            let session = lease.session.as_mut().unwrap();
            let ready = session.wake().take_ready();
            let waker = session.wake().register(parent).clone();
            if ready
                && session
                    .poll_retire(&mut Context::from_waker(&waker), &mut 32)
                    .is_ready()
            {
                session.reset();
            }
        }
    }
}

struct Retiring<'a> {
    provider: &'a ProviderSlots,
    session: Option<Box<dyn Session>>,
}
impl Drop for Retiring<'_> {
    fn drop(&mut self) {
        self.provider.put(self.session.take().unwrap());
    }
}

/// One opened byte object retained independently of the path used to open it.
///
/// Obtained with [`super::LoadContext::take_source`]. This owner keeps its original provider, read session and cache alive; moving it does not move pending I/O. Dropping it retires submitted reads through the same prepared source storage.
pub struct AssetSource {
    provider: Rc<ProviderSlots>,
    session: Option<Box<dyn Session>>,
}

impl AssetSource {
    /// Borrow the opened reader's complete immutable mapping without I/O or copying. `None` means reads are required; invalid mappings return an error. The borrow cannot outlive this source owner, which retains the provider and session.
    pub fn mapped(&self) -> Result<Option<&[u8]>, AssetError> {
        self.session
            .as_ref()
            .ok_or(AssetError::InvalidSource)?
            .mapped()
    }

    /// The opened object's length, access and consistency guarantees.
    pub fn info(&self) -> Option<SourceInfo> {
        self.session.as_ref().and_then(|session| session.info())
    }

    /// Read a bounded prefix of `dst`, returning its initialized byte count. `Pending` retains no destination borrow. Resume with the same offset until ready, then advance by the returned count; zero means EOF for a nonempty destination. Reads use the original shared cache and execution allowance, and never reopen the provider key.
    pub fn poll_read(
        &mut self,
        offset: u64,
        dst: &mut [u8],
        cx: &mut Context<'_>,
    ) -> Poll<Result<usize, AssetError>> {
        let length = u64::try_from(dst.len()).map_err(|_| AssetError::OutOfBounds)?;
        offset.checked_add(length).ok_or(AssetError::OutOfBounds)?;
        let mut budget = LoadBudget::default();
        let count = dst.len().min(budget.bytes);
        ReadStream::poll_read(self, offset, &mut dst[..count], cx, &mut budget.steps)
    }

    pub(super) fn poll_open(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), AssetError>> {
        self.session
            .as_mut()
            .ok_or(AssetError::InvalidSource)?
            .poll_open(cx)
    }
    pub(super) fn poll_prefix(
        &mut self,
        length: usize,
        cx: &mut Context<'_>,
        steps: &mut usize,
    ) -> Poll<Result<(), AssetError>> {
        self.session
            .as_mut()
            .unwrap()
            .poll_prefix(length, cx, steps)
    }
    pub(super) fn prefix(&self) -> (&[u8], bool) {
        self.session.as_ref().unwrap().prefix()
    }
}
impl ReadStream for AssetSource {
    fn info(&self) -> Option<SourceInfo> {
        self.info()
    }
    fn take_source(&mut self) -> Result<AssetSource, AssetError> {
        let session = self.session.as_ref().ok_or(AssetError::InvalidSource)?;
        if session.info().is_none() || session.has_pending() {
            return Err(AssetError::InvalidSource);
        }
        Ok(Self {
            provider: self.provider.clone(),
            session: self.session.take(),
        })
    }
    fn poll_read(
        &mut self,
        offset: u64,
        dst: &mut [u8],
        cx: &mut Context<'_>,
        steps: &mut usize,
    ) -> Poll<Result<usize, AssetError>> {
        self.session
            .as_mut()
            .ok_or(AssetError::InvalidSource)?
            .poll_read(offset, dst, cx, steps)
    }
}
impl Drop for AssetSource {
    fn drop(&mut self) {
        let Some(mut session) = self.session.take() else {
            return;
        };
        let retiring = session.has_pending();
        if retiring {
            session.set_retiring();
            session.wake().schedule();
        } else {
            session.reset();
        }
        self.provider.put(session);
        if retiring {
            self.provider.wake.schedule();
        }
    }
}

pub(crate) struct AssetSources {
    entries: Vec<(&'static str, Rc<ProviderSlots>)>,
    config: AssetCacheConfig,
    pool: Rc<RefCell<BlockPool>>,
    wake: Rc<TaskWake>,
}
impl AssetSources {
    pub fn max_requests(&self) -> usize {
        self.config.requests
    }
    pub fn contains(&self, name: &str) -> bool {
        self.entries.iter().any(|(key, _)| *key == name)
    }
    pub fn new(config: AssetCacheConfig) -> Result<Self, AssetError> {
        if config.requests == 0 || config.key_bytes == 0 || config.prefix_bytes == 0 {
            return Err(AssetError::Capacity);
        }
        Ok(Self {
            entries: Vec::new(),
            config,
            pool: Rc::new(RefCell::new(BlockPool::new(
                config.block_size,
                config.budget,
                config.max_in_flight,
            )?)),
            wake: Rc::new(TaskWake::new()),
        })
    }
    pub fn register<P: AssetProvider>(
        &mut self,
        name: &'static str,
        provider: P,
    ) -> Result<(), AssetError> {
        if self.entries.iter().any(|(key, _)| *key == name) {
            return Err(AssetError::InvalidSource);
        }
        let provider = Rc::new(provider);
        let mut slots: Vec<Option<Box<dyn Session>>> = Vec::new();
        slots
            .try_reserve_exact(self.config.requests)
            .map_err(|_| AssetError::Capacity)?;
        for _ in 0..self.config.requests {
            let mut prefix = Vec::new();
            prefix
                .try_reserve_exact(self.config.prefix_bytes)
                .map_err(|_| AssetError::Capacity)?;
            prefix.resize(self.config.prefix_bytes, 0);
            slots.push(Some(Box::new(TypedSession {
                provider: provider.clone(),
                pool: self.pool.clone(),
                opening: Box::pin(None),
                read_slot: Some(Box::pin(None)),
                reader: None,
                prefix: prefix.into_boxed_slice(),
                filled: 0,
                eof: false,
                wake: TaskWake::new(),
                retiring: false,
            })));
        }
        self.entries.push((
            name,
            Rc::new(ProviderSlots {
                sessions: RefCell::new(slots),
                wake: self.wake.clone(),
                key_bytes: self.config.key_bytes,
            }),
        ));
        Ok(())
    }
    pub(super) fn open(
        &self,
        name: &str,
        key: Cow<'static, str>,
    ) -> Result<AssetSource, AssetError> {
        self.entries
            .iter()
            .find(|(key, _)| *key == name)
            .ok_or(AssetError::NotFound)?
            .1
            .take(key)
    }
    pub fn poll_pending(&self, cx: &mut Context<'_>) {
        let ready = self.wake.take_ready();
        self.wake.register(cx.waker());
        if !ready {
            return;
        }
        for (_, provider) in &self.entries {
            provider.poll_retired(self.wake.waker());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::{cell::Cell, future::Ready};

    #[derive(Default)]
    struct Opening {
        ready: Cell<bool>,
        polls: Cell<usize>,
        wake: RefCell<Option<Waker>>,
    }
    struct Open(Rc<Opening>);
    impl Future for Open {
        type Output = Result<&'static [u8], AssetError>;
        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            self.0.polls.set(self.0.polls.get() + 1);
            self.0.wake.replace(Some(cx.waker().clone()));
            if self.0.ready.get() {
                Poll::Ready(Ok(b"abcdef"))
            } else {
                Poll::Pending
            }
        }
    }

    fn config() -> AssetCacheConfig {
        AssetCacheConfig::new()
            .block_size(4)
            .budget(MaxSize::Count(2))
            .max_in_flight(1)
            .max_requests(1)
            .max_key_bytes(8)
            .probe_bytes(4)
    }

    #[test]
    fn unwound_open_is_removed_before_the_session_returns_to_its_pool() {
        struct Fails;
        impl Future for Fails {
            type Output = Result<&'static [u8], AssetError>;
            fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
                panic!("open failed");
            }
        }
        let mut sources = AssetSources::new(config()).unwrap();
        sources.register("files", |_| Fails).unwrap();
        let mut source = sources.open("files", "a".into()).unwrap();
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                source.poll_open(&mut Context::from_waker(Waker::noop()))
            }))
            .is_err()
        );
        drop(source);
        sources.poll_pending(&mut Context::from_waker(Waker::noop()));
        assert!(sources.open("files", "b".into()).is_ok());
    }

    #[test]
    fn pending_open_retirement_preserves_prepared_capacity_and_sleeps_until_woken() {
        let mut sources = AssetSources::new(config()).unwrap();
        let state = Rc::new(Opening::default());
        let producer = state.clone();
        sources
            .register("files", move |_| Open(producer.clone()))
            .unwrap();
        let mut source = sources.open("files", "a".into()).unwrap();
        let mut cx = Context::from_waker(Waker::noop());
        assert!(source.poll_open(&mut cx).is_pending());
        drop(source);
        sources.poll_pending(&mut cx);
        assert!(matches!(
            sources.open("files", "b".into()),
            Err(AssetError::Capacity)
        ));
        let polled = state.polls.get();
        for _ in 0..5 {
            sources.poll_pending(&mut cx);
        }
        assert_eq!(state.polls.get(), polled);
        state.ready.set(true);
        state.wake.borrow_mut().take().unwrap().wake();
        assert_eq!(
            crate::test_support::tracked_allocations(|| sources.poll_pending(&mut cx)),
            0
        );
        let mut source = sources.open("files", "b".into()).unwrap();
        assert_eq!(source.poll_open(&mut cx), Poll::Ready(Ok(())));
    }

    #[test]
    fn mapped_prefix_and_session_reuse_stay_bounded_without_poll_allocations() {
        let mut sources = AssetSources::new(config()).unwrap();
        fn open(_: Cow<'static, str>) -> Ready<Result<&'static [u8], AssetError>> {
            core::future::ready(Ok(b"abcdef"))
        }
        sources.register("rom", open).unwrap();
        assert_eq!(
            sources.register("rom", open),
            Err(AssetError::InvalidSource)
        );
        assert!(matches!(
            sources.open("rom", "too-long-key".into()),
            Err(AssetError::Capacity)
        ));
        let mut cx = Context::from_waker(Waker::noop());
        let allocations = crate::test_support::tracked_allocations(|| {
            for _ in 0..8 {
                let mut source = sources.open("rom", "a".into()).unwrap();
                assert_eq!(source.poll_open(&mut cx), Poll::Ready(Ok(())));
                assert_eq!(
                    source.poll_prefix(5, &mut cx, &mut 8),
                    Poll::Ready(Err(AssetError::Capacity))
                );
                assert_eq!(source.poll_prefix(4, &mut cx, &mut 8), Poll::Ready(Ok(())));
                assert_eq!(source.prefix(), (b"abcd".as_slice(), false));
                let mut bytes = [0; 2];
                assert_eq!(source.poll_read(4, &mut bytes, &mut cx), Poll::Ready(Ok(2)));
                assert_eq!(&bytes, b"ef");
            }
        });
        assert_eq!(allocations, 0);
    }
}
