use alloc::{rc::Rc, string::String};

/// A static value or a shared, read-only getter passed between compositions.
///
/// Reading a getter inside a widget binding tracks its original reactive sources. Forwarding or cloning a prop creates no subscriptions or intermediate signals. Static clones use `T::clone`; dynamic clones share their getter without allocating.
pub struct Prop<T> {
    value: Value<T>,
}

enum Value<T> {
    Static(T),
    Read(Rc<dyn Fn() -> T>),
}

impl<T> Prop<T> {
    #[doc(hidden)]
    pub fn __mirui_is_static(&self) -> bool {
        matches!(self.value, Value::Static(_))
    }

    /// Retain a getter without evaluating it or registering a reactive consumer.
    ///
    /// Construction allocates one shared closure. Clones share that closure; each read evaluates it independently. Use `Computed` for cached results.
    pub fn new(read: impl Fn() -> T + 'static) -> Self {
        Self {
            value: Value::Read(Rc::new(read)),
        }
    }
}

impl<T: Clone> Prop<T> {
    /// Read the current value in the caller's reactive scope.
    pub fn get(&self) -> T {
        match &self.value {
            Value::Static(value) => value.clone(),
            Value::Read(read) => read(),
        }
    }
}

impl<T: Clone> Clone for Prop<T> {
    fn clone(&self) -> Self {
        Self {
            value: match &self.value {
                Value::Static(value) => Value::Static(value.clone()),
                Value::Read(read) => Value::Read(Rc::clone(read)),
            },
        }
    }
}

impl<T> From<T> for Prop<T> {
    fn from(value: T) -> Self {
        Self {
            value: Value::Static(value),
        }
    }
}

impl From<&str> for Prop<String> {
    fn from(value: &str) -> Self {
        String::from(value).into()
    }
}

impl<T: Default> Default for Prop<T> {
    fn default() -> Self {
        Self::from(T::default())
    }
}

impl<T: Clone> crate::core::model::SharedValue for Prop<T> {
    fn binding_owner(&self) -> crate::core::model::BindingOwner {
        // Opaque getters validate their captured sources when read.
        crate::core::model::BindingOwner::unbound()
    }
}
