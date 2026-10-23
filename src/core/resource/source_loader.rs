//! Byte recognition and decoding through the existing loader invocation pool.

use super::assets::{AssetSource, AssetSources};
use super::invocation::{CallLease, InstalledLoader, Loaded};
use super::manager_inner::ManagerInner;
use super::{
    AssetError, FormatMatch, HasProbe, LoadContext, LoadError, LoadInput, Loader, ProbeLoader,
};
use crate::core::cache::HasSize;
use alloc::{
    borrow::Cow,
    boxed::Box,
    rc::{Rc, Weak},
    vec::Vec,
};
use core::{
    cell::RefCell,
    task::{Context, Poll},
};

type SourceResolver = Box<dyn Fn(&str) -> Option<Cow<'static, str>>>;

/// A named byte source and its token-to-key resolver. Constructed by [`Loader::for_source`].
pub struct SourceBinding {
    name: &'static str,
    resolve: SourceResolver,
}

impl SourceBinding {
    pub(crate) fn new(
        name: &'static str,
        resolve: impl Fn(&str) -> Option<Cow<'static, str>> + 'static,
    ) -> Self {
        Self {
            name,
            resolve: Box::new(resolve),
        }
    }
}

/// A decoder bound to a named byte source. Reuses the decoder's state and optional metadata capability.
pub struct BoundLoader<L> {
    loader: L,
    binding: SourceBinding,
}

impl<L> BoundLoader<L> {
    pub(crate) fn new(loader: L, binding: SourceBinding) -> Self {
        Self { loader, binding }
    }
}

impl<T, L: Loader<T>> Loader<T> for BoundLoader<L> {
    type State = L::State;

    fn source_binding(&self) -> Option<&SourceBinding> {
        Some(&self.binding)
    }
    fn recognize(&self, input: LoadInput<'_>) -> FormatMatch {
        self.loader.recognize(input)
    }
    fn poll_load(
        &self,
        state: &mut Self::State,
        cx: &mut LoadContext<'_>,
    ) -> Poll<Result<T, LoadError>> {
        self.loader.poll_load(state, cx)
    }
    fn reset(&self, state: &mut Self::State) {
        self.loader.reset(state);
    }
}

impl<T: HasProbe, L: ProbeLoader<T>> ProbeLoader<T> for BoundLoader<L> {
    fn poll_probe(
        &self,
        state: &mut Self::State,
        cx: &mut LoadContext<'_>,
    ) -> Poll<Result<T::Meta, LoadError>> {
        self.loader.poll_probe(state, cx)
    }
}

pub(super) struct SourceLoader<T: HasSize + 'static> {
    manager: Weak<RefCell<ManagerInner<T>>>,
    sources: Rc<AssetSources>,
    bindings: Vec<SourceBinding>,
}

pub(super) struct SourceState<T: 'static> {
    source: Option<AssetSource>,
    decoder: Option<CallLease<T>>,
    candidate: Option<Rc<dyn InstalledLoader<T>>>,
    scan: usize,
    count: usize,
    required: usize,
    next_required: usize,
    generation: u64,
}
impl<T: 'static> Default for SourceState<T> {
    fn default() -> Self {
        Self {
            source: None,
            decoder: None,
            candidate: None,
            scan: 0,
            count: 0,
            required: 0,
            next_required: 0,
            generation: 0,
        }
    }
}

impl<T: HasSize + 'static> SourceLoader<T> {
    pub fn new(
        manager: Weak<RefCell<ManagerInner<T>>>,
        sources: Rc<AssetSources>,
        bindings: Vec<SourceBinding>,
    ) -> Result<Self, AssetError> {
        for binding in &bindings {
            if !sources.contains(binding.name) {
                return Err(AssetError::NotFound);
            }
        }
        let inner = manager.upgrade().ok_or(AssetError::InvalidSource)?;
        let count = inner.borrow().loader_count();
        for index in 0..count {
            let loader = inner.borrow().source_decoder(index);
            if loader
                .source_binding()
                .is_some_and(|binding| !sources.contains(binding.name))
            {
                return Err(AssetError::NotFound);
            }
        }
        Ok(Self {
            manager,
            sources,
            bindings,
        })
    }

    fn poll(
        &self,
        state: &mut SourceState<T>,
        cx: &mut LoadContext<'_>,
        probe: bool,
    ) -> Poll<Result<Loaded<T>, LoadError>> {
        let manager = self.manager.upgrade().ok_or(AssetError::InvalidSource)?;
        if state.source.is_none() {
            let (generation, count) = manager
                .borrow()
                .source_request(cx.token())
                .ok_or(AssetError::InvalidSource)?;
            let mut resolved = None;
            for binding in &self.bindings {
                if let Some(key) = (binding.resolve)(cx.token()) {
                    if resolved.is_some() {
                        return Poll::Ready(Err(LoadError::Failed("ambiguous resource source")));
                    }
                    resolved = Some((binding.name, key));
                }
            }
            for index in 0..count {
                let loader = manager.borrow().source_decoder(index);
                if let Some(binding) = loader.source_binding()
                    && let Some(key) = (binding.resolve)(cx.token())
                {
                    if resolved.is_some() {
                        return Poll::Ready(Err(LoadError::Failed("ambiguous resource source")));
                    }
                    resolved = Some((binding.name, key));
                    state.candidate = Some(loader);
                    state.scan = count;
                }
            }
            let (name, key) = resolved.ok_or(LoadError::NotMine)?;
            state.generation = generation;
            state.count = count;
            state.source = Some(self.sources.open(name, key)?);
        }
        let source = state.source.as_mut().unwrap();
        let waker = cx.waker().clone();
        let mut task_cx = Context::from_waker(&waker);
        core::task::ready!(source.poll_open(&mut task_cx))?;
        while state.decoder.is_none() {
            if !cx.consume_work() {
                return Poll::Pending;
            }
            core::task::ready!(source.poll_prefix(state.required, &mut task_cx, &mut 32))?;
            let (prefix, eof) = source.prefix();
            while state.scan < state.count {
                if !cx.consume_work() {
                    return Poll::Pending;
                }
                let decoder = manager.borrow().source_decoder(state.scan);
                state.scan += 1;
                if decoder.source_binding().is_some() {
                    continue;
                }
                match decoder.recognize(LoadInput::Bytes {
                    token: cx.token(),
                    prefix,
                    eof,
                }) {
                    FormatMatch::NoMatch => {}
                    FormatMatch::Match => {
                        if state.candidate.is_some() {
                            return Poll::Ready(Err(LoadError::Failed(
                                "ambiguous resource format",
                            )));
                        }
                        state.candidate = Some(decoder);
                    }
                    FormatMatch::Need(length) => {
                        if eof {
                            return Poll::Ready(Err(AssetError::Truncated.into()));
                        }
                        if length <= prefix.len() {
                            return Poll::Ready(Err(AssetError::NoProgress.into()));
                        }
                        state.next_required = state.next_required.max(length);
                    }
                }
            }
            if state.next_required != 0 {
                state.required = core::mem::take(&mut state.next_required);
                state.scan = 0;
                state.candidate = None;
                continue;
            }
            let decoder = state
                .candidate
                .take()
                .ok_or(LoadError::Failed("unrecognized resource format"))?;
            if probe && !decoder.has_probe() {
                return Poll::Ready(Err(LoadError::NotMine));
            }
            let task = decoder
                .take(cx.token(), state.generation, probe)
                .ok_or(AssetError::Capacity)?;
            state.decoder = Some(CallLease {
                owner: decoder,
                task: Some(task),
                retained: false,
            });
        }
        let decoder = state.decoder.as_mut().unwrap().task.as_mut().unwrap();
        decoder.call_mut().probe = probe;
        decoder.poll_source(cx.waker(), source).map(|result| {
            result.map_err(|error| match error {
                LoadError::NotMine => LoadError::Failed("decoder rejected recognized resource"),
                other => other,
            })
        })
    }
}

impl<T: HasSize + 'static> Loader<T> for SourceLoader<T> {
    type State = SourceState<T>;
    fn recognize(&self, input: LoadInput<'_>) -> FormatMatch {
        match input {
            LoadInput::Token(_) => FormatMatch::Match,
            LoadInput::Bytes { .. } => FormatMatch::NoMatch,
        }
    }
    fn poll_load(
        &self,
        state: &mut Self::State,
        cx: &mut LoadContext<'_>,
    ) -> Poll<Result<T, LoadError>> {
        self.poll(state, cx, false).map(|result| {
            result.and_then(|loaded| match loaded {
                Loaded::Value(value) => Ok(value),
                Loaded::Meta(_) => Err(AssetError::InvalidSource.into()),
            })
        })
    }
    fn reset(&self, state: &mut Self::State) {
        let previous = core::mem::take(state);
        drop(previous.decoder);
        drop(previous.source);
    }
}

impl<T: HasSize + HasProbe + 'static> ProbeLoader<T> for SourceLoader<T> {
    fn poll_probe(
        &self,
        state: &mut Self::State,
        cx: &mut LoadContext<'_>,
    ) -> Poll<Result<T::Meta, LoadError>> {
        self.poll(state, cx, true).map(|result| {
            result.and_then(|loaded| match loaded {
                Loaded::Meta(meta) => meta
                    .downcast::<T::Meta>()
                    .map(|meta| *meta)
                    .map_err(|_| AssetError::InvalidSource.into()),
                Loaded::Value(_) => Err(AssetError::InvalidSource.into()),
            })
        })
    }
}
