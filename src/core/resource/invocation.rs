//! Type-preserving loader state prepared at registration and leased for calls.

use super::load_context::ReadStream;
use super::load_context::{ExactRead, LoadBudget};
use super::wake::TaskWake;
use super::{FormatMatch, HasProbe, LoadContext, LoadError, LoadInput, Loader, ProbeLoader};
use alloc::{borrow::Cow, boxed::Box, rc::Rc, vec::Vec};
use core::{
    any::Any,
    cell::RefCell,
    task::{Poll, Waker},
};

pub(super) enum Loaded<T> {
    Value(T),
    Meta(Box<dyn Any>),
}

pub(super) struct CallState {
    pub token: Option<Cow<'static, str>>,
    pub generation: u64,
    pub probe: bool,
    pub exact: ExactRead,
    pub wake: TaskWake,
    pub meta: Option<Result<Box<dyn Any>, LoadError>>,
}

pub(super) trait Invocation<T>: 'static {
    fn call(&self) -> &CallState;
    fn call_mut(&mut self) -> &mut CallState;
    fn poll(&mut self, parent: &Waker) -> Poll<Result<Loaded<T>, LoadError>>;
    fn poll_source(
        &mut self,
        parent: &Waker,
        source: &mut dyn ReadStream,
    ) -> Poll<Result<Loaded<T>, LoadError>>;
    fn clone_meta(&self) -> Option<Result<Box<dyn Any>, LoadError>>;
    fn reset(&mut self);
}

pub(super) trait InstalledLoader<T>: 'static {
    fn recognize(&self, input: LoadInput<'_>) -> FormatMatch;
    fn has_probe(&self) -> bool;
    fn take(&self, token: &str, generation: u64, probe: bool) -> Option<Box<dyn Invocation<T>>>;
    fn capacity(&self) -> usize;
    fn take_ready(&self, slot: usize, parent: &Waker) -> Option<Box<dyn Invocation<T>>>;
    fn put(&self, task: Box<dyn Invocation<T>>);
    fn take_meta(&self, token: &str) -> Option<Box<dyn Invocation<T>>>;
    fn invalidate(&self, token: &str);
}

// The concrete adapter below keeps the resource type in its probe function, avoiding type-name inspection.
type TypedProbe<T, L> = fn(
    &L,
    &mut <L as Loader<T>>::State,
    &mut LoadContext<'_>,
) -> Poll<Result<Box<dyn Any>, LoadError>>;
type CloneMeta = fn(&dyn Any) -> Box<dyn Any>;
type Slots<T> = RefCell<Vec<Option<Box<dyn Invocation<T>>>>>;

struct Task<T, L: Loader<T>> {
    loader: Rc<L>,
    state: L::State,
    call: CallState,
    probe: Option<TypedProbe<T, L>>,
    clone_meta: Option<CloneMeta>,
}

impl<T: 'static, L: Loader<T>> Invocation<T> for Task<T, L> {
    fn call(&self) -> &CallState {
        &self.call
    }
    fn call_mut(&mut self) -> &mut CallState {
        &mut self.call
    }
    fn poll(&mut self, parent: &Waker) -> Poll<Result<Loaded<T>, LoadError>> {
        self.poll_on(parent, None)
    }
    fn poll_source(
        &mut self,
        parent: &Waker,
        source: &mut dyn ReadStream,
    ) -> Poll<Result<Loaded<T>, LoadError>> {
        self.poll_on(parent, Some(source))
    }
    fn clone_meta(&self) -> Option<Result<Box<dyn Any>, LoadError>> {
        let clone = self.clone_meta?;
        Some(
            self.call
                .meta
                .as_ref()?
                .as_ref()
                .map(|meta| clone(meta.as_ref()))
                .map_err(|error| *error),
        )
    }
    fn reset(&mut self) {
        self.call.token = None;
        self.call.meta = None;
        self.call.exact = ExactRead::default();
        self.loader.reset(&mut self.state);
    }
}

impl<T: 'static, L: Loader<T>> Task<T, L> {
    fn poll_on(
        &mut self,
        parent: &Waker,
        source: Option<&mut dyn ReadStream>,
    ) -> Poll<Result<Loaded<T>, LoadError>> {
        let waker = self.call.wake.register(parent).clone();
        let mut budget = LoadBudget {
            steps: 32,
            bytes: 4096,
        };
        let mut cx = LoadContext::new(
            self.call.token.as_deref().unwrap(),
            &waker,
            source,
            &mut self.call.exact,
            &mut budget,
        );
        if self.call.probe {
            match self.probe {
                Some(probe) => probe(&self.loader, &mut self.state, &mut cx)
                    .map(|result| result.map(Loaded::Meta)),
                None => Poll::Ready(Err(LoadError::NotMine)),
            }
        } else {
            self.loader
                .poll_load(&mut self.state, &mut cx)
                .map(|result| result.map(Loaded::Value))
        }
    }
}

struct Adapter<T, L: Loader<T>> {
    loader: Rc<L>,
    slots: Slots<T>,
    has_probe: bool,
}

struct SlotGuard<'a, T: 'static> {
    slots: &'a Slots<T>,
    task: Option<Box<dyn Invocation<T>>>,
}
impl<T: 'static> Drop for SlotGuard<'_, T> {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            let mut slots = self.slots.borrow_mut();
            *slots
                .iter_mut()
                .find(|slot| slot.is_none())
                .expect("leased loader slot") = Some(task);
        }
    }
}

impl<T: 'static, L: Loader<T>> InstalledLoader<T> for Adapter<T, L> {
    fn recognize(&self, input: LoadInput<'_>) -> FormatMatch {
        self.loader.recognize(input)
    }
    fn has_probe(&self) -> bool {
        self.has_probe
    }
    fn take(&self, token: &str, generation: u64, probe: bool) -> Option<Box<dyn Invocation<T>>> {
        let mut slots = self.slots.borrow_mut();
        let slot = slots
            .iter()
            .position(|slot| {
                slot.as_ref().is_some_and(|task| {
                    task.call().token.as_deref() == Some(token)
                        && (task.call().generation == generation || task.call().meta.is_some())
                })
            })
            .or_else(|| {
                slots.iter().position(|slot| {
                    slot.as_ref()
                        .is_some_and(|task| task.call().token.is_none())
                })
            })
            .or_else(|| {
                slots
                    .iter()
                    .position(|slot| slot.as_ref().is_some_and(|task| task.call().meta.is_some()))
            })?;
        let task = slots[slot].take();
        drop(slots);
        let mut guard = SlotGuard {
            slots: &self.slots,
            task,
        };
        let task = guard.task.as_mut().unwrap();
        if task
            .call()
            .token
            .as_deref()
            .is_some_and(|previous| previous != token)
            || task.call().meta.as_ref().is_some_and(Result::is_err)
        {
            task.reset();
        }
        let state = task.call_mut();
        if state.token.is_none() {
            state.token = Some(Cow::Owned(token.into()));
        }
        state.generation = generation;
        state.probe = probe;
        state.meta = None;
        state.wake.take_ready();
        guard.task.take()
    }
    fn capacity(&self) -> usize {
        self.slots.borrow().len()
    }
    fn take_ready(&self, slot: usize, parent: &Waker) -> Option<Box<dyn Invocation<T>>> {
        let task = self.slots.borrow_mut()[slot].take()?;
        let mut guard = SlotGuard {
            slots: &self.slots,
            task: Some(task),
        };
        let task = guard.task.as_ref().unwrap();
        let state = task.call();
        if state.token.is_some() && state.meta.is_none() {
            let ready = state.wake.take_ready();
            state.wake.register(parent);
            if ready {
                return guard.task.take();
            }
        }
        None
    }
    fn put(&self, task: Box<dyn Invocation<T>>) {
        let mut slots = self.slots.borrow_mut();
        let slot = slots
            .iter_mut()
            .find(|slot| slot.is_none())
            .expect("leased loader slot");
        *slot = Some(task);
    }
    fn take_meta(&self, token: &str) -> Option<Box<dyn Invocation<T>>> {
        let slot = self.slots.borrow().iter().position(|slot| {
            slot.as_ref().is_some_and(|task| {
                task.call().token.as_deref() == Some(token) && task.call().meta.is_some()
            })
        })?;
        self.slots.borrow_mut()[slot].take()
    }
    fn invalidate(&self, token: &str) {
        let count = self.capacity();
        for slot in 0..count {
            let task = {
                let mut slots = self.slots.borrow_mut();
                if !slots[slot]
                    .as_ref()
                    .is_some_and(|task| task.call().token.as_deref() == Some(token))
                {
                    continue;
                }
                slots[slot].take()
            };
            if let Some(task) = task {
                let mut guard = SlotGuard {
                    slots: &self.slots,
                    task: Some(task),
                };
                let task = guard.task.as_mut().unwrap();
                // Metadata has no physical operation in flight. Pending calls are retired by the manager.
                if task.call().meta.is_some() {
                    task.reset();
                } else {
                    task.call().wake.schedule();
                }
            }
        }
    }
}

fn install<T: 'static, L: Loader<T>>(
    loader: L,
    capacity: usize,
    probe: Option<TypedProbe<T, L>>,
    clone_meta: Option<CloneMeta>,
) -> Rc<dyn InstalledLoader<T>> {
    let loader = Rc::new(loader);
    let mut slots: Vec<Option<Box<dyn Invocation<T>>>> = Vec::with_capacity(capacity);
    for _ in 0..capacity {
        slots.push(Some(Box::new(Task {
            loader: loader.clone(),
            state: L::State::default(),
            call: CallState {
                token: None,
                generation: 0,
                probe: false,
                exact: ExactRead::default(),
                wake: TaskWake::new(),
                meta: None,
            },
            probe,
            clone_meta,
        })));
    }
    Rc::new(Adapter {
        loader,
        slots: RefCell::new(slots),
        has_probe: probe.is_some(),
    })
}

pub(super) fn install_loader<T: 'static>(
    loader: impl Loader<T>,
    capacity: usize,
) -> Rc<dyn InstalledLoader<T>> {
    install(loader, capacity, None, None)
}

pub(super) fn install_probed_loader<T: HasProbe + 'static, L: ProbeLoader<T>>(
    loader: L,
    capacity: usize,
) -> Rc<dyn InstalledLoader<T>> {
    install(
        loader,
        capacity,
        Some(|loader, state, cx| {
            loader
                .poll_probe(state, cx)
                .map(|result| result.map(|meta| Box::new(meta) as Box<dyn Any>))
        }),
        Some(|meta| {
            Box::new(
                meta.downcast_ref::<T::Meta>()
                    .expect("registered metadata type")
                    .clone(),
            )
        }),
    )
}

/// Returns the leased slot even if an external callback unwinds.
pub(super) struct CallLease<T: 'static> {
    pub owner: Rc<dyn InstalledLoader<T>>,
    pub task: Option<Box<dyn Invocation<T>>>,
    pub retained: bool,
}

impl<T: 'static> Drop for CallLease<T> {
    fn drop(&mut self) {
        struct Return<'a, T: 'static> {
            owner: &'a Rc<dyn InstalledLoader<T>>,
            task: Option<Box<dyn Invocation<T>>>,
        }
        impl<T: 'static> Drop for Return<'_, T> {
            fn drop(&mut self) {
                self.owner.put(self.task.take().unwrap());
            }
        }
        let mut returning = Return {
            owner: &self.owner,
            task: self.task.take(),
        };
        if !self.retained {
            returning.task.as_mut().unwrap().reset();
        }
    }
}
