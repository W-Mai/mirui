use super::*;

/// Input-source registration. Callback storage is prepared once per App.
pub struct InputMethod<C = ()> {
    callbacks: Option<C>,
}
impl InputMethod<()> {
    pub const fn native() -> Self {
        Self { callbacks: None }
    }
    pub fn software<C: InputMethodCallbacks>(callbacks: C) -> InputMethod<C> {
        InputMethod {
            callbacks: Some(callbacks),
        }
    }
}

#[doc(hidden)]
#[derive(Default)]
pub struct PreparedMethod {
    changed: Option<PreparedEvent<TextInputChanged, AppContext>>,
    key: Option<PreparedEvent<Key, AppContext>>,
}

#[doc(hidden)]
#[derive(Debug)]
pub enum InputMethodError {
    Callback(runtime::RuntimeInstallError),
    DuplicateCallback,
    MissingStateCallback,
    MissingDeliveryCapacity,
    ReserveFailed,
    Busy,
}

mod sealed {
    pub trait Callbacks {}
}
#[doc(hidden)]
pub trait InputMethodCallbacks: sealed::Callbacks {
    fn prepare(
        self,
        world: &mut World,
        output: &mut PreparedMethod,
    ) -> Result<(), InputMethodError>;
}

impl sealed::Callbacks for () {}
impl InputMethodCallbacks for () {
    fn prepare(self, _: &mut World, _: &mut PreparedMethod) -> Result<(), InputMethodError> {
        Ok(())
    }
}
macro_rules! callback {
    ($event:ty, $field:ident) => {
        impl sealed::Callbacks for EventCallback<$event, AppContext> {}
        impl InputMethodCallbacks for EventCallback<$event, AppContext> {
            fn prepare(
                self,
                world: &mut World,
                output: &mut PreparedMethod,
            ) -> Result<(), InputMethodError> {
                if output.$field.is_some() {
                    return Err(InputMethodError::DuplicateCallback);
                }
                output.$field = Some(
                    runtime::prepare_shared_callback(world, self)
                        .map_err(InputMethodError::Callback)?,
                );
                Ok(())
            }
        }
    };
}
callback!(TextInputChanged, changed);
callback!(Key, key);
macro_rules! tuple {
    ($($name:ident),+) => {
        impl<$($name: InputMethodCallbacks),+> sealed::Callbacks for ($($name,)+) {}
        impl<$($name: InputMethodCallbacks),+> InputMethodCallbacks for ($($name,)+) {
            fn prepare(self, world: &mut World, output: &mut PreparedMethod) -> Result<(), InputMethodError> {
                #[allow(non_snake_case)]
                let ($($name,)+) = self;
                $($name.prepare(world, output)?;)+
                Ok(())
            }
        }
    };
}
tuple!(A);
tuple!(A, B);

impl<B: crate::surface::Surface, F: crate::app::RendererFactory<B>> crate::app::App<B, F> {
    /// Replace the App's input method. Invalid registrations leave the current provider installed.
    pub fn with_input_method<C: InputMethodCallbacks>(
        &mut self,
        method: InputMethod<C>,
    ) -> &mut Self {
        let unavailable = self.text_input_unavailable();
        if let Err(error) = install(&mut self.world, method, unavailable) {
            crate::warn!(target: "mirui::input", "input-method registration rejected: {:?}", error);
        }
        self
    }
}

fn install<C: InputMethodCallbacks>(
    world: &mut World,
    method: InputMethod<C>,
    unavailable: u8,
) -> Result<(), InputMethodError> {
    let existing = service(world);
    if existing
        .as_ref()
        .is_some_and(|service| service.delivering.get())
    {
        return Err(InputMethodError::Busy);
    }
    runtime::initialize(world);
    let mut prepared = PreparedMethod::default();
    let software = method.callbacks.is_some();
    if let Some(callbacks) = method.callbacks {
        callbacks.prepare(world, &mut prepared)?;
        if prepared.changed.is_none() {
            return Err(InputMethodError::MissingStateCallback);
        }
    }
    let limits = runtime::delivery_limits(world);
    let capacity = limits.commands.map_or(0, CommandCapacity::record_count);
    if software && (capacity == 0 || limits.flush_limit.unwrap_or(0) == 0) {
        return Err(InputMethodError::MissingDeliveryCapacity);
    }
    let Some(service) = existing.or_else(|| software.then(|| Rc::new(Service::new(unavailable))))
    else {
        return Ok(());
    };
    if software {
        let mut state = service.state.borrow_mut();
        if state.pending.capacity() < capacity {
            let additional = capacity.saturating_sub(state.pending.len());
            state
                .pending
                .try_reserve_exact(additional)
                .map_err(|_| InputMethodError::ReserveFailed)?;
        }
    }
    let mut retired = service.state.borrow_mut().active.take();
    if let Some(active) = &mut retired {
        end(world, active, CompositionEndReason::Interrupted);
    }
    let old_callback = service.state.borrow().changed.clone();
    if let Some(callback) = old_callback
        && service.state.borrow().published != Some(None)
        && let Err(error) = callback.invoke(world, &TextInputChanged { state: None }, ())
    {
        crate::warn!(target: "mirui::input", "input-method notification skipped: {:?}", error);
    }
    let mut state = service.state.borrow_mut();
    state.pending.clear();
    if !software {
        state.pending = VecDeque::new();
    }
    state.unavailable = unavailable;
    state.capacity = if software { capacity } else { 0 };
    state.changed = prepared.changed;
    state.key = prepared.key;
    state.mode = if software {
        TextInputMode::Software
    } else {
        TextInputMode::Native
    };
    state.published = None;
    drop(state);
    world.insert_resource(service);
    runtime::freeze_shared_callbacks(world);
    flush(world);
    Ok(())
}
