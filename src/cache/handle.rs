use core::cell::Cell;

#[cfg(not(feature = "sync-cache"))]
use alloc::rc::Rc as EntryRcImpl;
#[cfg(feature = "sync-cache")]
use alloc::sync::Arc as EntryRcImpl;

pub(crate) type EntryRc<V> = EntryRcImpl<CacheEntry<V>>;

pub struct CacheEntry<V> {
    pub(crate) payload: V,
    pub(crate) is_invalid: Cell<bool>,
}

impl<V> CacheEntry<V> {
    #[allow(dead_code)]
    pub(crate) fn new(payload: V) -> Self {
        Self {
            payload,
            is_invalid: Cell::new(false),
        }
    }
}

pub struct Handle<V> {
    pub(crate) inner: EntryRc<V>,
}

impl<V> Handle<V> {
    #[allow(dead_code)]
    pub(crate) fn from_rc(inner: EntryRc<V>) -> Self {
        Self { inner }
    }

    pub fn get(&self) -> &V {
        &self.inner.payload
    }

    pub fn is_invalid(&self) -> bool {
        self.inner.is_invalid.get()
    }

    pub fn ref_count(&self) -> usize {
        EntryRcImpl::strong_count(&self.inner)
    }
}

impl<V> core::ops::Deref for Handle<V> {
    type Target = V;
    fn deref(&self) -> &V {
        &self.inner.payload
    }
}

impl<V> Clone for Handle<V> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<V: core::fmt::Debug> core::fmt::Debug for Handle<V> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Handle")
            .field("payload", &self.inner.payload)
            .field("is_invalid", &self.is_invalid())
            .field("ref_count", &self.ref_count())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;

    #[test]
    fn handle_clone_shares_payload_and_bumps_ref_count() {
        let entry = EntryRcImpl::new(CacheEntry::new(String::from("hi")));
        let h1 = Handle::from_rc(entry);
        assert_eq!(h1.ref_count(), 1);
        let h2 = h1.clone();
        assert_eq!(h1.ref_count(), 2);
        assert_eq!(h2.ref_count(), 2);
        assert_eq!(&*h1, "hi");
        assert_eq!(&*h2, "hi");
        drop(h2);
        assert_eq!(h1.ref_count(), 1);
    }

    #[test]
    fn invalid_flag_is_shared_across_clones() {
        let entry = EntryRcImpl::new(CacheEntry::new(42u32));
        let h1 = Handle::from_rc(entry);
        let h2 = h1.clone();
        assert!(!h1.is_invalid());
        h1.inner.is_invalid.set(true);
        assert!(h1.is_invalid());
        assert!(h2.is_invalid());
    }
}
