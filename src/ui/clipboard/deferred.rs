use super::{ClipboardDispatch, ClipboardRead};
use alloc::{boxed::Box, sync::Arc};
use core::fmt;
use std::sync::Mutex;

pub(super) type ReadSource = Box<dyn FnOnce(ClipboardRead) -> ClipboardDispatch + Send>;

#[derive(Clone)]
pub(super) struct DeferredRead(Arc<Mutex<Option<ReadSource>>>);

impl DeferredRead {
    pub(super) fn new(
        source: impl FnOnce(ClipboardRead) -> ClipboardDispatch + Send + 'static,
    ) -> Self {
        Self(Arc::new(Mutex::new(Some(Box::new(source)))))
    }

    pub(super) fn take(&self) -> Option<ReadSource> {
        self.0.lock().ok()?.take()
    }
}

impl fmt::Debug for DeferredRead {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DeferredRead")
            .finish_non_exhaustive()
    }
}
