use super::*;

#[model]
struct CfgLeaf {
    #[cfg(any())]
    child: model!(owned MissingType),
    #[observe]
    value: u8,
}

#[model]
impl CfgLeaf {
    fn increment(&mut self) {
        self.value += 1;
    }
}

#[test]
fn disabled_owned_fields_do_not_reference_unavailable_models() {
    let mut app = App::headless(16, 16);
    let leaf = app.add_model(CfgLeaf { value: 1 });
    leaf.increment();
    assert_eq!(leaf.value(), 2);
}

#[model]
struct Value<T: Copy + Eq + 'static> {
    #[observe]
    value: T,
}

mod methods {
    use super::*;

    #[model]
    impl<T: Copy + Eq + 'static> Value<T> {
        pub(super) fn new(value: T) -> Self {
            Self { value }
        }
        pub(super) fn replace(&mut self, other: Self) {
            self.value = other.value;
        }

        pub(super) fn duplicate(&self) -> Self {
            Self::new(self.value)
        }

        pub(super) fn copy_pair(&self, other: Option<Self>) -> (Self, Option<Self>) {
            (self.duplicate(), other)
        }

        pub(super) fn map_copy<U>(&self, map: impl FnOnce(Self) -> U) -> U {
            map(self.duplicate())
        }
    }
}

#[model]
struct GenericParent<T: Model> {
    child: model!(owned T),
    #[cfg(any())]
    absent: model!(owned NotDefined),
    #[cfg(feature = "std")]
    present: model!(owned Counter),
}

#[model]
impl<T: Model> GenericParent<T> {
    fn new(child: T) -> Self {
        Self {
            child: child.into(),
            #[cfg(feature = "std")]
            present: Counter::new(0).into(),
        }
    }
}

#[model]
struct CopyParent<T: Copy + Eq + 'static> {
    child: model!(owned Value<T>),
}

#[model]
impl<T: Copy + Eq + 'static> CopyParent<T> {
    fn copy_child(&mut self) {
        let copy: Value<T> = self.child.duplicate();
        self.child.replace(copy);
        let (copy, peer): (Value<T>, Option<Value<T>>) = self.child.copy_pair(None);
        self.child.replace(peer.unwrap_or(copy));
    }
}

#[test]
fn generic_owned_fields_preserve_bounds_cfg_and_original_self_types() {
    let mut app = App::headless(16, 16);
    let parent = app.add_model(GenericParent::new(Value::new(4_u16)));
    let child = parent.child();
    child.replace(Value::new(9));
    assert_eq!(child.value(), 9);
    let copy: Value<u16> = child.duplicate();
    assert_eq!(copy.value, 9);
    let (copy, peer): (Value<u16>, Option<Value<u16>>) = child.copy_pair(Some(Value::new(4)));
    assert_eq!((copy.value, peer.unwrap().value), (9, 4));
    let mapped: u32 = child.map_copy(|value: Value<u16>| u32::from(value.value));
    assert_eq!(mapped, 9);
    #[cfg(feature = "std")]
    {
        parent.present().add(1);
        assert_eq!(parent.present().value(), 1);
    }

    let parent = app.add_model(CopyParent {
        child: Value::new(7_u8).into(),
    });
    parent.copy_child();
    assert_eq!(parent.child().value(), 7);
}

#[model]
pub struct PublicParent {
    child: model!(owned Counter),
}

#[model]
impl PublicParent {}

#[test]
fn public_parents_can_keep_child_types_private() {
    let mut app = App::headless(16, 16);
    let parent = app.add_model(PublicParent {
        child: Counter::new(1).into(),
    });
    parent.child().add(1);
    assert_eq!(parent.child().value(), 2);
}
