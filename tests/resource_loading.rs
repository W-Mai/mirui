#![cfg(feature = "std")]

use mirui::core::cache::{HasSize, MaxSize};
use mirui::core::resource::{
    FormatMatch, LoadContext, LoadError, LoadInput, Loader, ResourceManager,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::task::{Context, Poll, Wake, Waker};

#[derive(Debug, PartialEq)]
struct Asset(u32);
impl HasSize for Asset {
    fn cache_size(&self) -> usize {
        4
    }
}

#[derive(Default)]
struct Completion {
    result: Option<u32>,
    wake: Option<Waker>,
}
struct ExternalLoader(Arc<Mutex<Completion>>);
impl Loader<Asset> for ExternalLoader {
    type State = ();
    fn recognize(&self, input: LoadInput<'_>) -> FormatMatch {
        match input {
            LoadInput::Token("remote") => FormatMatch::Match,
            _ => FormatMatch::NoMatch,
        }
    }
    fn poll_load(&self, _: &mut (), cx: &mut LoadContext<'_>) -> Poll<Result<Asset, LoadError>> {
        let mut completion = self.0.lock().unwrap();
        if let Some(value) = completion.result {
            return Poll::Ready(Ok(Asset(value)));
        }
        completion.wake = Some(cx.waker().clone());
        Poll::Pending
    }
}

#[derive(Default)]
struct Executor(AtomicUsize);
impl Wake for Executor {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn external_completion_wakes_the_owner_without_publishing_from_the_worker() {
    let completion = Arc::new(Mutex::new(Completion::default()));
    let executor = Arc::new(Executor::default());
    let waker = Waker::from(executor.clone());
    let mut cx = Context::from_waker(&waker);
    let manager = ResourceManager::new(MaxSize::Count(8), Asset(0));
    manager.add_loader(ExternalLoader(completion.clone()));
    let resource = manager.load("remote");
    assert_eq!(resource.get().0, 0);
    manager.poll_pending(&mut cx);
    executor.0.store(0, Ordering::SeqCst);
    std::thread::spawn(move || {
        let wake = {
            let mut completion = completion.lock().unwrap();
            completion.result = Some(123);
            completion.wake.take().unwrap()
        };
        wake.wake();
    })
    .join()
    .unwrap();
    assert_eq!(executor.0.load(Ordering::SeqCst), 1);
    assert_eq!(resource.get().0, 0);
    manager.poll_pending(&mut cx);
    assert_eq!(resource.get().0, 123);
    executor.0.store(0, Ordering::SeqCst);
    for _ in 0..8 {
        manager.poll_pending(&mut cx);
    }
    assert_eq!(executor.0.load(Ordering::SeqCst), 0);
}
